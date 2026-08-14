//! Copy/move queue, pause, and overwrite-conflict handling.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::batch_rename::unique_numbered_name;
use super::clipboard::ClipboardOperation;
use super::is_virtual;
use super::live_inner;
use super::map_fs_error;
use super::ActionOutcome;
use super::FileManagerInner;
use crate::error::Result as WidgetResult;
use crate::error::WidgetError;

/// User choice for an existing destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictChoice {
    /// Replace the destination.
    Overwrite,
    /// Leave the destination unchanged.
    Skip,
    /// Keep the destination if it is newer than the source.
    OverwriteOlder,
    /// Append remaining bytes onto a shorter destination.
    Resume,
    /// Copy under a new name (`rename_to` or auto-numbered).
    Rename,
}

/// Copy knobs selected in the copy dialog / context menu.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransferOptions {
    /// BLAKE3 verify after each file.
    pub verify: bool,
    /// Skip destinations that are newer or same age.
    pub newer_only: bool,
    /// Create folders only.
    pub structure_only: bool,
}

#[derive(Debug, Clone)]
pub(super) struct TransferJob {
    pub sources: Vec<String>,
    pub dest_dir: orchid_fs::FsPath,
    pub is_copy: bool,
    pub options: TransferOptions,
    pub policy: Option<ConflictChoice>,
}

#[derive(Debug, Clone)]
pub(super) struct PendingTransfer {
    pub remaining: Vec<String>,
    pub dest_dir: orchid_fs::FsPath,
    pub is_copy: bool,
    pub options: TransferOptions,
}

#[derive(Debug)]
pub(super) struct TransferCtl {
    pub pause: Arc<AtomicBool>,
    pub cancel: parking_lot::RwLock<Option<CancellationToken>>,
    pub queue: parking_lot::Mutex<VecDeque<TransferJob>>,
    pub pending: parking_lot::Mutex<Option<PendingTransfer>>,
}

impl Default for TransferCtl {
    fn default() -> Self {
        Self {
            pause: Arc::new(AtomicBool::new(false)),
            cancel: parking_lot::RwLock::new(None),
            queue: parking_lot::Mutex::new(VecDeque::new()),
            pending: parking_lot::Mutex::new(None),
        }
    }
}

impl FileManagerInner {
    pub(super) async fn transfer_paths(
        self: &Arc<Self>,
        sources: &[String],
        dest_dir: &orchid_fs::FsPath,
        is_copy: bool,
    ) -> WidgetResult<ActionOutcome> {
        self.transfer_paths_opts(sources, dest_dir, is_copy, TransferOptions::default(), None)
            .await
    }

    pub(super) async fn transfer_paths_opts(
        self: &Arc<Self>,
        sources: &[String],
        dest_dir: &orchid_fs::FsPath,
        is_copy: bool,
        options: TransferOptions,
        policy: Option<ConflictChoice>,
    ) -> WidgetResult<ActionOutcome> {
        if is_virtual(dest_dir) {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-transfer-virtual-dest".into(),
            ));
        }
        if self.transfer.read().active {
            self.xfer.queue.lock().push_back(TransferJob {
                sources: sources.to_vec(),
                dest_dir: dest_dir.clone(),
                is_copy,
                options,
                policy,
            });
            self.transfer.write().queue_len = self.xfer.queue.lock().len() as u32;
            self.publish_refresh();
            return Ok(ActionOutcome::Done);
        }
        self.run_transfer_loop(TransferJob {
            sources: sources.to_vec(),
            dest_dir: dest_dir.clone(),
            is_copy,
            options,
            policy,
        })
        .await
    }

    async fn run_transfer_loop(
        self: &Arc<Self>,
        first: TransferJob,
    ) -> WidgetResult<ActionOutcome> {
        let mut next = Some(first);
        while let Some(job) = next {
            let outcome = self.run_transfer_job(job).await?;
            if !matches!(outcome, ActionOutcome::Done) {
                return Ok(outcome);
            }
            next = self.xfer.queue.lock().pop_front();
            self.transfer.write().queue_len = self.xfer.queue.lock().len() as u32;
        }
        Ok(ActionOutcome::Done)
    }

    async fn run_transfer_job(self: &Arc<Self>, job: TransferJob) -> WidgetResult<ActionOutcome> {
        self.begin_transfer(job.is_copy);
        self.xfer.pause.store(false, Ordering::Relaxed);
        let token = CancellationToken::new();
        *self.xfer.cancel.write() = Some(token.clone());
        let ctl = orchid_fs::TransferControl {
            cancel: Some(token),
            pause: Some(Arc::clone(&self.xfer.pause)),
        };
        let (sink, mut rx) = orchid_fs::ProgressSink::channel();
        let inner = Arc::clone(self);
        let progress_task = tokio::spawn(async move {
            while let Some(p) = rx.recv().await {
                inner.apply_transfer_progress(&p);
            }
        });

        let registry = self.deps.registry.as_ref();
        let dest_str = job.dest_dir.as_str();
        let mut result: WidgetResult<ActionOutcome> = Ok(ActionOutcome::Done);
        let mut idx = 0usize;
        while idx < job.sources.len() {
            let p = &job.sources[idx];
            let src = match orchid_fs::FsPath::new(p) {
                Ok(fp) => fp,
                Err(e) => {
                    result = Err(map_fs_error(e));
                    break;
                }
            };
            if &src == &job.dest_dir {
                idx += 1;
                continue;
            }
            let src_str = src.as_str();
            if dest_str.len() > src_str.len() {
                let rest = &dest_str[src_str.len()..];
                if rest.starts_with('/') || rest.starts_with('\\') {
                    idx += 1;
                    continue;
                }
            }
            let name = src.file_name().map(str::to_string).unwrap_or_else(|| {
                if job.is_copy {
                    "copy".into()
                } else {
                    "moved".into()
                }
            });
            let dest = match job.policy {
                Some(ConflictChoice::Rename) => {
                    let unique = unique_numbered_name(&name, |c| {
                        job.dest_dir
                            .join(c)
                            .to_local()
                            .map(|os| os.exists())
                            .unwrap_or(false)
                    });
                    job.dest_dir.join(&unique)
                }
                _ => job.dest_dir.join(&name),
            };
            if src == dest {
                idx += 1;
                continue;
            }

            let dest_exists = match registry.for_path(&dest) {
                Some(prov) => prov.exists(&dest).await.unwrap_or(false),
                None => false,
            };

            if dest_exists && job.policy.is_none() {
                let can_resume = dest_is_partial(registry, &src, &dest).await;
                drop(sink);
                let _ = progress_task.await;
                self.end_transfer();
                *self.xfer.pending.lock() = Some(PendingTransfer {
                    remaining: job.sources[idx..].to_vec(),
                    dest_dir: job.dest_dir,
                    is_copy: job.is_copy,
                    options: job.options,
                });
                return Ok(ActionOutcome::NeedsConflict {
                    source: src.as_str().to_string(),
                    dest: dest.as_str().to_string(),
                    dest_name: name,
                    can_resume,
                    is_copy: job.is_copy,
                });
            }
            if dest_exists && matches!(job.policy, Some(ConflictChoice::Skip)) {
                idx += 1;
                continue;
            }

            if let Err(e) = self
                .copy_or_move(
                    &src,
                    &dest,
                    job.is_copy,
                    job.options,
                    job.policy,
                    &sink,
                    ctl.clone(),
                )
                .await
            {
                let msg = e.to_string();
                if msg.to_lowercase().contains("already exists") {
                    drop(sink);
                    let _ = progress_task.await;
                    self.end_transfer();
                    *self.xfer.pending.lock() = Some(PendingTransfer {
                        remaining: job.sources[idx..].to_vec(),
                        dest_dir: job.dest_dir,
                        is_copy: job.is_copy,
                        options: job.options,
                    });
                    return Ok(ActionOutcome::NeedsConflict {
                        source: src.as_str().to_string(),
                        dest: dest.as_str().to_string(),
                        dest_name: name,
                        can_resume: dest_is_partial(registry, &src, &dest).await,
                        is_copy: job.is_copy,
                    });
                }
                result = Err(e);
                break;
            }
            idx += 1;
        }

        drop(sink);
        let _ = progress_task.await;
        self.end_transfer();
        if let Err(ref e) = result {
            self.set_transfer_notice(e.to_string());
        }
        if result.is_ok() {
            self.refresh_all_tabs().await;
        }
        result
    }

    async fn copy_or_move(
        &self,
        src: &orchid_fs::FsPath,
        dest: &orchid_fs::FsPath,
        is_copy: bool,
        options: TransferOptions,
        policy: Option<ConflictChoice>,
        sink: &orchid_fs::ProgressSink,
        ctl: orchid_fs::TransferControl,
    ) -> WidgetResult<()> {
        let registry = self.deps.registry.as_ref();
        let overwrite = matches!(
            policy,
            Some(ConflictChoice::Overwrite | ConflictChoice::OverwriteOlder)
        );
        let skip = matches!(policy, Some(ConflictChoice::Skip));
        let resume = matches!(policy, Some(ConflictChoice::Resume));
        let newer = options.newer_only || matches!(policy, Some(ConflictChoice::OverwriteOlder));
        let copy_opts = orchid_fs::CopyOptions {
            overwrite,
            skip_existing: skip,
            overwrite_older: newer,
            structure_only: options.structure_only,
            resume,
            verify_content_hash: options.verify,
            ..orchid_fs::CopyOptions::default()
        };
        if is_copy {
            orchid_fs::copy_with_control(registry, src, dest, copy_opts, Some(sink), ctl)
                .await
                .map_err(map_fs_error)
        } else if overwrite || skip || resume || newer {
            orchid_fs::copy_with_control(registry, src, dest, copy_opts, Some(sink), ctl)
                .await
                .map_err(map_fs_error)?;
            if let Some(provider) = registry.for_path(src) {
                provider.remove(src, true).await.map_err(map_fs_error)?;
            }
            Ok(())
        } else {
            orchid_fs::move_(registry, src, dest, Some(sink), ctl.cancel)
                .await
                .map_err(map_fs_error)
        }
    }

    pub(super) async fn paste_clipboard(self: &Arc<Self>) -> WidgetResult<ActionOutcome> {
        let dest_dir = {
            let state = self.state.lock();
            state.active_tab().path.clone()
        };
        if is_virtual(&dest_dir) {
            return Ok(ActionOutcome::Done);
        }
        let (sources, op) = self.deps.clipboard.paste(&dest_dir);
        if sources.is_empty() || op == ClipboardOperation::None {
            return Ok(ActionOutcome::Done);
        }
        let paths: Vec<String> = sources.iter().map(|p| p.as_str().to_string()).collect();
        let is_copy = op == ClipboardOperation::Copy;
        self.transfer_paths(&paths, &dest_dir, is_copy).await
    }
}

/// Pause the active copy/move.
pub fn pause_transfer(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    inner.xfer.pause.store(true, Ordering::Relaxed);
    inner.transfer.write().paused = true;
    inner.publish_refresh();
    Ok(())
}

/// Resume a paused copy/move.
pub fn resume_transfer(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    inner.xfer.pause.store(false, Ordering::Relaxed);
    inner.transfer.write().paused = false;
    inner.publish_refresh();
    Ok(())
}

/// Cancel the active copy/move and drop the queued jobs.
pub fn cancel_transfer(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    if let Some(token) = inner.xfer.cancel.read().as_ref() {
        token.cancel();
    }
    inner.xfer.pause.store(false, Ordering::Relaxed);
    inner.xfer.queue.lock().clear();
    inner.xfer.pending.lock().take();
    inner.transfer.write().paused = false;
    inner.transfer.write().queue_len = 0;
    inner.publish_refresh();
    Ok(())
}

/// Continue a transfer after the overwrite dialog.
pub async fn apply_conflict(
    instance_id: Uuid,
    choice: ConflictChoice,
    apply_all: bool,
    rename_to: Option<String>,
) -> WidgetResult<ActionOutcome> {
    let inner = live_inner(instance_id)?;
    let Some(pending) = inner.xfer.pending.lock().take() else {
        return Ok(ActionOutcome::Done);
    };
    if pending.remaining.is_empty() {
        return Ok(ActionOutcome::Done);
    }
    let mut sources = pending.remaining;
    let dest_dir = pending.dest_dir.clone();
    if matches!(choice, ConflictChoice::Rename) {
        if let Some(new_name) = rename_to {
            let src = sources.remove(0);
            let src_fp = orchid_fs::FsPath::new(&src).map_err(map_fs_error)?;
            let dest = dest_dir.join(&new_name);
            inner
                .copy_or_move(
                    &src_fp,
                    &dest,
                    pending.is_copy,
                    pending.options,
                    Some(ConflictChoice::Overwrite),
                    &{
                        let (sink, _rx) = orchid_fs::ProgressSink::channel();
                        sink
                    },
                    orchid_fs::TransferControl::default(),
                )
                .await?;
            if sources.is_empty() {
                inner.refresh_all_tabs().await;
                return Ok(ActionOutcome::Done);
            }
            return inner
                .transfer_paths_opts(&sources, &dest_dir, pending.is_copy, pending.options, None)
                .await;
        }
        if apply_all {
            return inner
                .transfer_paths_opts(
                    &sources,
                    &dest_dir,
                    pending.is_copy,
                    pending.options,
                    Some(ConflictChoice::Rename),
                )
                .await;
        }
    }
    if apply_all {
        inner
            .transfer_paths_opts(
                &sources,
                &dest_dir,
                pending.is_copy,
                pending.options,
                Some(choice),
            )
            .await
    } else {
        // Apply this choice to the first remaining path, then ask again.
        let first = sources[0].clone();
        let rest = sources[1..].to_vec();
        let first_out = inner
            .transfer_paths_opts(
                std::slice::from_ref(&first),
                &dest_dir,
                pending.is_copy,
                pending.options,
                Some(choice),
            )
            .await?;
        if !matches!(first_out, ActionOutcome::Done) {
            return Ok(first_out);
        }
        if rest.is_empty() {
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        inner
            .transfer_paths_opts(&rest, &dest_dir, pending.is_copy, pending.options, None)
            .await
    }
}

async fn dest_is_partial(
    registry: &orchid_fs::FsProviderRegistry,
    src: &orchid_fs::FsPath,
    dest: &orchid_fs::FsPath,
) -> bool {
    let Some(sp) = registry.for_path(src) else {
        return false;
    };
    let Some(dp) = registry.for_path(dest) else {
        return false;
    };
    let Ok(sm) = sp.metadata(src).await else {
        return false;
    };
    let Ok(dm) = dp.metadata(dest).await else {
        return false;
    };
    dm.size > 0 && dm.size < sm.size
}

fn other_pane_dir(inner: &FileManagerInner, src_pane: u8) -> Option<orchid_fs::FsPath> {
    let state = inner.state.lock();
    if src_pane == 1 {
        Some(state.left_pane.active_tab().path.clone())
    } else {
        state
            .right_pane
            .as_ref()
            .map(|p| p.active_tab().path.clone())
    }
}

/// Copy the selection into the opposite pane (clipboard if single-pane).
pub async fn copy_to_other_pane(
    instance_id: Uuid,
    src_pane: u8,
    paths: Vec<String>,
    options: TransferOptions,
    skip_confirm: bool,
) -> WidgetResult<ActionOutcome> {
    transfer_to_other_pane(instance_id, src_pane, paths, true, options, skip_confirm).await
}

/// Move the selection into the opposite pane (cut if single-pane).
pub async fn move_to_other_pane(
    instance_id: Uuid,
    src_pane: u8,
    paths: Vec<String>,
    options: TransferOptions,
    skip_confirm: bool,
) -> WidgetResult<ActionOutcome> {
    transfer_to_other_pane(instance_id, src_pane, paths, false, options, skip_confirm).await
}

async fn transfer_to_other_pane(
    instance_id: Uuid,
    src_pane: u8,
    paths: Vec<String>,
    is_copy: bool,
    options: TransferOptions,
    skip_confirm: bool,
) -> WidgetResult<ActionOutcome> {
    let inner = live_inner(instance_id)?;
    if paths.is_empty() {
        return Ok(ActionOutcome::Done);
    }
    let Some(dest) = other_pane_dir(&inner, src_pane) else {
        let fps: Vec<orchid_fs::FsPath> = paths
            .iter()
            .filter_map(|p| orchid_fs::FsPath::new(p).ok())
            .collect();
        if is_copy {
            inner.deps.clipboard.copy(fps);
        } else {
            inner.deps.clipboard.cut(fps);
        }
        inner.publish_refresh();
        return Ok(ActionOutcome::Done);
    };
    if !skip_confirm {
        return Ok(ActionOutcome::NeedsConfirmation {
            message: if is_copy {
                "fm-confirm-copy".into()
            } else {
                "fm-confirm-move".into()
            },
            action_id: if is_copy {
                "fs.copy-to-other".into()
            } else {
                "fs.move-to-other".into()
            },
            paths,
        });
    }
    inner
        .transfer_paths_opts(&paths, &dest, is_copy, options, None)
        .await
}
