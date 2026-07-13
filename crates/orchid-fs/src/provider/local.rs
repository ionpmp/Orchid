//! Concrete OS-filesystem provider.

use std::path::Path;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebounceEventResult};
use tokio::sync::mpsc;
use tracing::warn;

use crate::entry::{ExtendedAttributes, FsEntry, FsEntryKind, FsMetadata};
use crate::error::{FsError, Result};
use crate::path::{FsPath, SCHEME_LOCAL};
use crate::provider::{
    FsCapabilities, FsChange, FsChangeKind, FsProvider, FsWatcherHandle, ProviderId,
};

/// Default OS-filesystem provider.
pub struct LocalProvider {
    id: ProviderId,
}

impl Default for LocalProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalProvider {
    /// Construct with the canonical id `"local"`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: ProviderId::new("local"),
        }
    }

    /// Construct with an explicit id (useful for multi-mount setups).
    #[must_use]
    pub fn with_id(id: ProviderId) -> Self {
        Self { id }
    }
}

#[async_trait]
impl FsProvider for LocalProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn scheme(&self) -> &'static str {
        SCHEME_LOCAL
    }

    async fn list(&self, path: &FsPath) -> Result<Vec<FsEntry>> {
        let os_path = path.to_local()?;
        let mut rd = tokio::fs::read_dir(&os_path)
            .await
            .map_err(map_io(&os_path))?;
        let mut out = Vec::new();
        while let Some(entry) = rd.next_entry().await? {
            let entry_path = entry.path();
            let Ok(fs_path) = FsPath::from_local(&entry_path) else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            let metadata = os_metadata_to_fs(&entry_path).await?;
            out.push(FsEntry {
                path: fs_path,
                name,
                metadata,
            });
        }
        Ok(out)
    }

    async fn metadata(&self, path: &FsPath) -> Result<FsMetadata> {
        let os_path = path.to_local()?;
        os_metadata_to_fs(&os_path).await
    }

    async fn exists(&self, path: &FsPath) -> Result<bool> {
        let os_path = path.to_local()?;
        match tokio::fs::metadata(&os_path).await {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(FsError::Io(e)),
        }
    }

    async fn read(&self, path: &FsPath) -> Result<Vec<u8>> {
        let os_path = path.to_local()?;
        tokio::fs::read(&os_path).await.map_err(map_io(&os_path))
    }

    async fn read_stream(
        &self,
        path: &FsPath,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Unpin + Send>> {
        let os_path = path.to_local()?;
        let file = tokio::fs::File::open(&os_path)
            .await
            .map_err(map_io(&os_path))?;
        Ok(Box::new(file))
    }

    async fn write(&self, path: &FsPath, bytes: &[u8]) -> Result<()> {
        let os_path = path.to_local()?;
        if let Some(parent) = os_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&os_path, bytes)
            .await
            .map_err(map_io(&os_path))
    }

    async fn write_stream(
        &self,
        path: &FsPath,
    ) -> Result<Box<dyn tokio::io::AsyncWrite + Unpin + Send>> {
        let os_path = path.to_local()?;
        if let Some(parent) = os_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let file = tokio::fs::File::create(&os_path)
            .await
            .map_err(map_io(&os_path))?;
        Ok(Box::new(file))
    }

    async fn create_dir(&self, path: &FsPath, recursive: bool) -> Result<()> {
        let os_path = path.to_local()?;
        if recursive {
            tokio::fs::create_dir_all(&os_path)
                .await
                .map_err(map_io(&os_path))
        } else {
            tokio::fs::create_dir(&os_path)
                .await
                .map_err(map_io(&os_path))
        }
    }

    async fn rename(&self, from: &FsPath, to: &FsPath) -> Result<()> {
        let from_os = from.to_local()?;
        let to_os = to.to_local()?;
        if let Some(parent) = to_os.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::rename(&from_os, &to_os)
            .await
            .map_err(map_io(&from_os))
    }

    async fn remove(&self, path: &FsPath, recursive: bool) -> Result<()> {
        let os_path = path.to_local()?;
        let meta = tokio::fs::metadata(&os_path)
            .await
            .map_err(map_io(&os_path))?;
        if meta.is_dir() {
            if recursive {
                tokio::fs::remove_dir_all(&os_path)
                    .await
                    .map_err(map_io(&os_path))
            } else {
                tokio::fs::remove_dir(&os_path)
                    .await
                    .map_err(map_io(&os_path))
            }
        } else {
            tokio::fs::remove_file(&os_path)
                .await
                .map_err(map_io(&os_path))
        }
    }

    async fn watch(&self, path: &FsPath) -> Result<Option<Box<dyn FsWatcherHandle>>> {
        let os_path = path.to_local()?;
        let (tx, rx) = mpsc::unbounded_channel::<Vec<FsChange>>();
        let mut debouncer = new_debouncer(
            Duration::from_millis(300),
            None,
            move |res: DebounceEventResult| match res {
                Ok(events) => {
                    let mut batch: Vec<FsChange> = Vec::with_capacity(events.len());
                    let now = Utc::now();
                    for ev in events {
                        for p in &ev.paths {
                            let Ok(fs_path) = FsPath::from_local(p) else {
                                continue;
                            };
                            let kind = notify_kind_to_fs_change(&ev.kind);
                            if let Some(k) = kind {
                                batch.push(FsChange {
                                    path: fs_path,
                                    kind: k,
                                    timestamp: now,
                                });
                            }
                        }
                    }
                    if !batch.is_empty() {
                        let _ = tx.send(batch);
                    }
                }
                Err(errs) => {
                    for e in errs {
                        warn!(error = %e, "notify watcher error");
                    }
                }
            },
        )?;
        debouncer
            .watch(&os_path, RecursiveMode::Recursive)
            .map_err(FsError::Notify)?;
        let handle = LocalWatcherHandle {
            _debouncer: Some(debouncer),
            rx,
        };
        Ok(Some(Box::new(handle)))
    }

    fn capabilities(&self) -> FsCapabilities {
        FsCapabilities {
            supports_rename: true,
            supports_symlinks: true,
            supports_permissions: false,
            supports_extended_attrs: true,
            supports_native_watch: true,
            case_sensitive: false,
            supports_random_write: true,
        }
    }
}

struct LocalWatcherHandle {
    // Kept alive for the lifetime of the handle so the underlying watcher
    // keeps receiving OS events.
    _debouncer: Option<
        notify_debouncer_full::Debouncer<
            notify::RecommendedWatcher,
            notify_debouncer_full::RecommendedCache,
        >,
    >,
    rx: mpsc::UnboundedReceiver<Vec<FsChange>>,
}

#[async_trait]
impl FsWatcherHandle for LocalWatcherHandle {
    async fn recv(&mut self) -> Option<Vec<FsChange>> {
        self.rx.recv().await
    }

    async fn shutdown(mut self: Box<Self>) {
        // Dropping the debouncer stops the background thread it owns.
        self._debouncer.take();
    }
}

fn notify_kind_to_fs_change(kind: &notify::EventKind) -> Option<FsChangeKind> {
    use notify::event::{CreateKind, EventKind, ModifyKind, RemoveKind, RenameMode};
    match kind {
        EventKind::Create(
            CreateKind::Any | CreateKind::File | CreateKind::Folder | CreateKind::Other,
        ) => Some(FsChangeKind::Created),
        EventKind::Modify(ModifyKind::Name(RenameMode::Both | RenameMode::To)) => {
            // We don't know the `from` path from this event alone; the
            // aggregated `FileWatcher` may correlate To/From later. For the
            // per-provider stream, represent it as a regular create.
            Some(FsChangeKind::Created)
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => Some(FsChangeKind::Deleted),
        EventKind::Modify(_) => Some(FsChangeKind::Modified),
        EventKind::Remove(
            RemoveKind::Any | RemoveKind::File | RemoveKind::Folder | RemoveKind::Other,
        ) => Some(FsChangeKind::Deleted),
        _ => None,
    }
}

async fn os_metadata_to_fs(path: &Path) -> Result<FsMetadata> {
    let std_meta = tokio::fs::metadata(path).await.map_err(map_io(path))?;
    let kind = if std_meta.is_dir() {
        FsEntryKind::Directory
    } else if std_meta.is_symlink() {
        FsEntryKind::Symlink
    } else if std_meta.is_file() {
        FsEntryKind::File
    } else {
        FsEntryKind::Other
    };

    #[cfg(windows)]
    let (readonly, hidden, system) = {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_READONLY: u32 = 0x0000_0001;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x0000_0002;
        const FILE_ATTRIBUTE_SYSTEM: u32 = 0x0000_0004;
        let attrs = std_meta.file_attributes();
        (
            (attrs & FILE_ATTRIBUTE_READONLY) != 0,
            (attrs & FILE_ATTRIBUTE_HIDDEN) != 0,
            (attrs & FILE_ATTRIBUTE_SYSTEM) != 0,
        )
    };
    #[cfg(not(windows))]
    let (readonly, hidden, system) = {
        let ro = std_meta.permissions().readonly();
        let hidden = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.'));
        (ro, hidden, false)
    };

    let mime = {
        let fs_path = FsPath::from_local(path).ok();
        if let Some(p) = fs_path {
            if matches!(kind, FsEntryKind::File) {
                crate::mime::guess_mime(&p, None).await
            } else {
                None
            }
        } else {
            None
        }
    };

    Ok(FsMetadata {
        kind,
        size: if matches!(kind, FsEntryKind::File) {
            std_meta.len()
        } else {
            0
        },
        created: system_time_to_utc(std_meta.created().ok()),
        modified: system_time_to_utc(std_meta.modified().ok()),
        accessed: system_time_to_utc(std_meta.accessed().ok()),
        readonly,
        hidden,
        system,
        mime,
        extended: ExtendedAttributes::default(),
    })
}

fn system_time_to_utc(t: Option<SystemTime>) -> Option<DateTime<Utc>> {
    let t = t?;
    let dur = t.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    DateTime::<Utc>::from_timestamp(dur.as_secs() as i64, dur.subsec_nanos())
}

fn map_io(path: &Path) -> impl FnOnce(std::io::Error) -> FsError + '_ {
    let path_str = path.display().to_string();
    move |e| match e.kind() {
        std::io::ErrorKind::NotFound => FsError::NotFound(path_str),
        std::io::ErrorKind::PermissionDenied => FsError::PermissionDenied(path_str),
        std::io::ErrorKind::AlreadyExists => FsError::AlreadyExists(path_str),
        _ => FsError::Io(e),
    }
}
