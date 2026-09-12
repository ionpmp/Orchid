//! Context-menu / toolbar action dispatch.

use super::*;

/// Dispatch a context-menu / toolbar action against `target_paths`.
pub async fn run_action(
    instance_id: Uuid,
    action_id: &str,
    target_paths: Vec<String>,
) -> WidgetResult<ActionOutcome> {
    run_action_with_opts(
        instance_id,
        action_id,
        target_paths,
        RunActionOpts::default(),
    )
    .await
}

/// Like [`run_action`] with extra flags (e.g. skip delete confirmation).
pub async fn run_action_with_opts(
    instance_id: Uuid,
    action_id: &str,
    target_paths: Vec<String>,
    opts: RunActionOpts,
) -> WidgetResult<ActionOutcome> {
    let inner = live_inner(instance_id)?;
    if let Some(tag) = action_id.strip_prefix("fs.tag-remove:") {
        if !tag.is_empty() {
            let fps: Result<Vec<_>, _> = target_paths
                .iter()
                .map(|p| orchid_fs::FsPath::new(p).map_err(map_fs_error))
                .collect();
            let fps = fps?;
            let refs: Vec<&orchid_fs::FsPath> = fps.iter().collect();
            inner
                .deps
                .tag_manager
                .remove_tag_many(&refs, tag)
                .map_err(map_fs_error)?;
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
    }
    if let Some(tag) = action_id.strip_prefix("fs.tag:") {
        if !tag.is_empty() {
            let fps: Result<Vec<_>, _> = target_paths
                .iter()
                .map(|p| orchid_fs::FsPath::new(p).map_err(map_fs_error))
                .collect();
            let fps = fps?;
            let refs: Vec<&orchid_fs::FsPath> = fps.iter().collect();
            inner
                .deps
                .tag_manager
                .add_tag_many(&refs, tag)
                .map_err(map_fs_error)?;
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
    }
    match action_id {
        "fs.open" => {
            let Some(p) = target_paths.first() else {
                return Ok(ActionOutcome::Done);
            };
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            let is_dir = entry_is_directory(&inner, &fp, false).await;
            return open_path(instance_id, pane, p, is_dir).await;
        }
        "fs.open-tab" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            let folder = if let Some(p) = target_paths.first() {
                folder_path_from_target(&inner, p).await
            } else {
                None
            };
            new_tab_at(instance_id, pane, folder).await?;
            return Ok(ActionOutcome::Done);
        }
        "fs.open-other-pane" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            let folder = if let Some(p) = target_paths.first() {
                folder_path_from_target(&inner, p).await
            } else {
                None
            };
            open_in_other_pane(instance_id, pane, folder).await?;
            return Ok(ActionOutcome::Done);
        }
        "fs.branch-view" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            toggle_branch_view(instance_id, pane).await?;
            return Ok(ActionOutcome::Done);
        }
        "fs.open-all" => {
            let mut files = Vec::new();
            for p in target_paths {
                let fp = orchid_fs::FsPath::new(&p).map_err(map_fs_error)?;
                // Skip directories even when provider metadata fails (OS fallback in helper).
                if entry_is_directory(&inner, &fp, false).await {
                    continue;
                }
                files.push(p);
            }
            if files.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            return Ok(ActionOutcome::OpenInViewerMany { paths: files });
        }
        "viewer.open" => {
            let Some(p) = target_paths.first() else {
                return Ok(ActionOutcome::Done);
            };
            let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            if inner.is_path_encrypted(&fp) {
                return Ok(ActionOutcome::NeedsPassphrase {
                    paths: vec![p.clone()],
                    purpose: PassphrasePurpose::RevealInViewer,
                });
            }
            return Ok(ActionOutcome::OpenInViewer { path: p.clone() });
        }
        "viewer.edit" => {
            let Some(p) = target_paths.first() else {
                return Ok(ActionOutcome::Done);
            };
            let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
            if inner.is_path_encrypted(&fp) {
                return Ok(ActionOutcome::NeedsPassphrase {
                    paths: vec![p.clone()],
                    purpose: PassphrasePurpose::RevealInViewer,
                });
            }
            return Ok(ActionOutcome::OpenInEditor { path: p.clone() });
        }
        "audio.play" => {
            let mut audio = Vec::new();
            for p in &target_paths {
                let path = std::path::Path::new(p);
                if !path.is_file() {
                    continue;
                }
                let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                    continue;
                };
                if crate::builtin::audio_player::library::is_audio_extension(
                    &ext.to_ascii_lowercase(),
                ) {
                    audio.push(p.clone());
                }
            }
            if audio.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            return Ok(ActionOutcome::PlayInAudioPlayer { paths: audio });
        }
        "audio.enqueue" => {
            let mut audio = Vec::new();
            for p in &target_paths {
                let path = std::path::Path::new(p);
                if !path.is_file() {
                    continue;
                }
                let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                    continue;
                };
                if crate::builtin::audio_player::library::is_audio_extension(
                    &ext.to_ascii_lowercase(),
                ) {
                    audio.push(p.clone());
                }
            }
            if audio.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            return Ok(ActionOutcome::EnqueueInAudioPlayer { paths: audio });
        }
        "video.play" => {
            let mut video = Vec::new();
            for p in &target_paths {
                let path = std::path::Path::new(p);
                if !path.is_file() {
                    continue;
                }
                let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                    continue;
                };
                if crate::builtin::video_player::library::is_video_extension(
                    &ext.to_ascii_lowercase(),
                ) {
                    video.push(p.clone());
                }
            }
            if video.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            return Ok(ActionOutcome::PlayInVideoPlayer { paths: video });
        }
        "video.enqueue" => {
            let mut video = Vec::new();
            for p in &target_paths {
                let path = std::path::Path::new(p);
                if !path.is_file() {
                    continue;
                }
                let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                    continue;
                };
                if crate::builtin::video_player::library::is_video_extension(
                    &ext.to_ascii_lowercase(),
                ) {
                    video.push(p.clone());
                }
            }
            if video.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            return Ok(ActionOutcome::EnqueueInVideoPlayer { paths: video });
        }
        "fs.file-assoc" => {
            let Some(p) = target_paths.first() else {
                return Ok(ActionOutcome::Done);
            };
            return Ok(ActionOutcome::OpenFileAssociations { path: p.clone() });
        }
        "fs.open-external" => {
            let mut files = Vec::new();
            for p in &target_paths {
                let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
                if let Some(provider) = inner.deps.registry.for_path(&fp) {
                    if let Ok(meta) = provider.metadata(&fp).await {
                        if matches!(meta.kind, orchid_fs::FsEntryKind::Directory) {
                            if inner.is_path_encrypted(&fp) {
                                return Ok(ActionOutcome::NeedsPassphrase {
                                    paths: vec![p.clone()],
                                    purpose: PassphrasePurpose::Reveal,
                                });
                            }
                            continue;
                        }
                    }
                }
                files.push(p.clone());
            }
            if files.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            if files.iter().all(|p| {
                orchid_fs::FsPath::new(p)
                    .map(|fp| inner.is_path_encrypted(&fp))
                    .unwrap_or(false)
            }) {
                return Ok(ActionOutcome::NeedsPassphrase {
                    paths: files,
                    purpose: PassphrasePurpose::Reveal,
                });
            }
            return Ok(ActionOutcome::OpenExternally { paths: files });
        }
        "fs.open-with" => {
            let mut files = Vec::new();
            for p in &target_paths {
                let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
                if let Some(provider) = inner.deps.registry.for_path(&fp) {
                    if let Ok(meta) = provider.metadata(&fp).await {
                        if matches!(meta.kind, orchid_fs::FsEntryKind::Directory) {
                            continue;
                        }
                    }
                }
                if inner.is_path_encrypted(&fp) {
                    return Ok(ActionOutcome::NeedsPassphrase {
                        paths: vec![p.clone()],
                        purpose: PassphrasePurpose::Reveal,
                    });
                }
                files.push(p.clone());
            }
            if files.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            return Ok(ActionOutcome::OpenWithPicker { paths: files });
        }
        "fs.copy" => {
            let paths: Vec<orchid_fs::FsPath> = target_paths
                .iter()
                .filter_map(|p| orchid_fs::FsPath::new(p).ok())
                .collect();
            inner.deps.clipboard.copy(paths);
        }
        "fs.cut" => {
            let paths: Vec<orchid_fs::FsPath> = target_paths
                .iter()
                .filter_map(|p| orchid_fs::FsPath::new(p).ok())
                .collect();
            inner.deps.clipboard.cut(paths);
        }
        "fs.paste" => {
            return inner.paste_clipboard().await;
        }
        "fs.undo" => {
            inner.undo_last().await?;
            return Ok(ActionOutcome::Done);
        }
        "fs.redo" => {
            inner.redo_last().await?;
            return Ok(ActionOutcome::Done);
        }
        "fs.rename" => {
            if target_paths.len() == 1 {
                let p = target_paths[0].clone();
                let current_name = p.rsplit('/').next().unwrap_or(p.as_str()).to_string();
                return Ok(ActionOutcome::NeedsRename {
                    path: p,
                    current_name,
                });
            }
            if target_paths.len() > 1 {
                return Ok(ActionOutcome::NeedsBatchRename {
                    paths: target_paths,
                });
            }
        }
        "fs.batch-rename" => {
            if !target_paths.is_empty() {
                return Ok(ActionOutcome::NeedsBatchRename {
                    paths: target_paths,
                });
            }
        }
        "fs.delete" | "fs.delete-recycle" | "fs.delete-permanent" => {
            if target_paths.iter().any(|p| orchid_fs::is_recycle_item(p)) {
                return recycle_purge_action(&inner, target_paths, opts.skip_confirm).await;
            }
            let cfg = inner.config.read().clone();
            let recycle = match action_id {
                "fs.delete-recycle" => true,
                "fs.delete-permanent" => false,
                _ => cfg.delete_to_recycle,
            };
            if cfg.confirm_delete && !target_paths.is_empty() && !opts.skip_confirm {
                let message = if recycle {
                    "fm-confirm-delete"
                } else {
                    "fm-confirm-delete-permanent"
                };
                return Ok(ActionOutcome::NeedsConfirmation {
                    message: message.into(),
                    action_id: action_id.to_string(),
                    paths: target_paths,
                });
            }
            inner.delete_paths(&target_paths, Some(recycle)).await?;
            if recycle {
                let items = undo::match_recycle_virtual_paths(&target_paths).await;
                inner.record_undo(undo::FsUndoOp::Recycle { items });
            }
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        "fs.recycle-restore" => {
            if target_paths.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            orchid_fs::restore_recycle(&target_paths)
                .await
                .map_err(map_fs_error)?;
            let restored: Vec<String> = target_paths
                .iter()
                .filter_map(|p| {
                    let orig = orchid_fs::recycle_original_path(p)?;
                    orchid_fs::FsPath::from_local(std::path::Path::new(&orig))
                        .ok()
                        .map(|fp| fp.as_str().to_string())
                })
                .collect();
            inner.record_undo(undo::FsUndoOp::Create {
                paths: restored,
                recycle_items: Vec::new(),
            });
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        "fs.recycle-purge" => {
            return recycle_purge_action(&inner, target_paths, opts.skip_confirm).await;
        }
        "fs.recycle-empty" => {
            if !opts.skip_confirm {
                return Ok(ActionOutcome::NeedsConfirmation {
                    message: "fm-confirm-recycle-empty".into(),
                    action_id: "fs.recycle-empty".into(),
                    paths: vec![orchid_fs::RECYCLE_PATH.to_string()],
                });
            }
            orchid_fs::empty_recycle().await.map_err(map_fs_error)?;
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        "fs.copy-to-other" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            return copy_to_other_pane(
                instance_id,
                pane,
                target_paths,
                TransferOptions::default(),
                opts.skip_confirm,
            )
            .await;
        }
        "fs.move-to-other" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            return move_to_other_pane(
                instance_id,
                pane,
                target_paths,
                TransferOptions::default(),
                opts.skip_confirm,
            )
            .await;
        }
        "fs.copy-verify" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            return copy_to_other_pane(
                instance_id,
                pane,
                target_paths,
                TransferOptions {
                    verify: true,
                    ..TransferOptions::default()
                },
                opts.skip_confirm,
            )
            .await;
        }
        "fs.copy-newer" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            return copy_to_other_pane(
                instance_id,
                pane,
                target_paths,
                TransferOptions {
                    newer_only: true,
                    ..TransferOptions::default()
                },
                opts.skip_confirm,
            )
            .await;
        }
        "fs.copy-structure" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            return copy_to_other_pane(
                instance_id,
                pane,
                target_paths,
                TransferOptions {
                    structure_only: true,
                    ..TransferOptions::default()
                },
                opts.skip_confirm,
            )
            .await;
        }
        "fs.link-symlink" | "fs.link-hard" | "fs.link-junction" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            create_link_in_pane(instance_id, pane, &target_paths, action_id).await?;
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        "fs.star" => {
            let fps: Result<Vec<_>, _> = target_paths
                .iter()
                .map(|p| orchid_fs::FsPath::new(p).map_err(map_fs_error))
                .collect();
            let fps = fps?;
            let refs: Vec<&orchid_fs::FsPath> = fps.iter().collect();
            inner
                .deps
                .tag_manager
                .set_starred_many(&refs, true)
                .map_err(map_fs_error)?;
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        "fs.unstar" => {
            let fps: Result<Vec<_>, _> = target_paths
                .iter()
                .map(|p| orchid_fs::FsPath::new(p).map_err(map_fs_error))
                .collect();
            let fps = fps?;
            let refs: Vec<&orchid_fs::FsPath> = fps.iter().collect();
            inner
                .deps
                .tag_manager
                .set_starred_many(&refs, false)
                .map_err(map_fs_error)?;
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        "fs.tag-add" => {
            if target_paths.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            return Ok(ActionOutcome::NeedsTag {
                paths: target_paths,
            });
        }
        "fs.select-all" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            inner.select_all_in_pane(pane);
            return Ok(ActionOutcome::Done);
        }
        "fs.deselect-all" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            inner.deselect_all_in_pane(pane);
            return Ok(ActionOutcome::Done);
        }
        "fs.select-more" => {
            return Ok(ActionOutcome::Done);
        }
        "fs.invert-selection" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            inner.invert_selection_in_pane(pane);
            return Ok(ActionOutcome::Done);
        }
        "fs.select-files" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            inner.apply_filter_in_pane(
                pane,
                MaskOp::Replace,
                &SelectFilter {
                    folders: false,
                    ..SelectFilter::default()
                },
            );
            return Ok(ActionOutcome::Done);
        }
        "fs.select-folders" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            inner.apply_filter_in_pane(
                pane,
                MaskOp::Replace,
                &SelectFilter {
                    files: false,
                    ..SelectFilter::default()
                },
            );
            return Ok(ActionOutcome::Done);
        }
        "fs.select-hidden" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            inner.apply_filter_in_pane(
                pane,
                MaskOp::Add,
                &SelectFilter {
                    hidden: true,
                    ..SelectFilter::default()
                },
            );
            return Ok(ActionOutcome::Done);
        }
        "fs.select-readonly" => {
            let pane = match inner.state.lock().active_pane {
                ActivePane::Left => 0,
                ActivePane::Right => 1,
            };
            inner.apply_filter_in_pane(
                pane,
                MaskOp::Add,
                &SelectFilter {
                    readonly: true,
                    ..SelectFilter::default()
                },
            );
            return Ok(ActionOutcome::Done);
        }
        "fs.select-mask" | "fs.select-mask-add" => {
            return Ok(ActionOutcome::NeedsSelectMask {
                op: MaskOp::Add,
                filter: false,
            });
        }
        "fs.select-mask-sub" => {
            return Ok(ActionOutcome::NeedsSelectMask {
                op: MaskOp::Subtract,
                filter: false,
            });
        }
        "fs.select-filter" => {
            return Ok(ActionOutcome::NeedsSelectMask {
                op: MaskOp::Replace,
                filter: true,
            });
        }
        "fs.find" => {
            let root = active_tab_path(&inner);
            return Ok(ActionOutcome::NeedsFindDialog { root });
        }
        "fs.find-duplicates" => {
            let root = orchid_fs::FsPath::new(active_tab_path(&inner)).map_err(map_fs_error)?;
            return find::run_find_duplicates(&inner, &root).await;
        }
        "fs.find-large" => {
            let root = active_tab_path(&inner);
            return Ok(ActionOutcome::NeedsToolPrompt {
                action_id: "fs.find-large".into(),
                paths: vec![root],
                proposed: "100MB".into(),
                title: inner.deps.locale.tr("fm-find-large-title"),
                hint: inner.deps.locale.tr("fm-find-large-hint"),
            });
        }
        "fs.new-folder" => {
            let parent = {
                let state = inner.state.lock();
                let pane = match state.active_pane {
                    ActivePane::Left => 0,
                    ActivePane::Right => 1,
                };
                active_tab_ref(&state, pane).map(|t| t.path.clone()).ok()
            };
            if let Some(parent) = parent {
                if !is_virtual(&parent) {
                    return Ok(ActionOutcome::NeedsCreateFolder {
                        parent: parent.as_str().to_string(),
                    });
                }
            }
            return Ok(ActionOutcome::Done);
        }
        "fs.new-file" => {
            let parent = {
                let state = inner.state.lock();
                let pane = match state.active_pane {
                    ActivePane::Left => 0,
                    ActivePane::Right => 1,
                };
                active_tab_ref(&state, pane).map(|t| t.path.clone()).ok()
            };
            if let Some(parent) = parent {
                if !is_virtual(&parent) {
                    return Ok(ActionOutcome::NeedsCreateFile {
                        parent: parent.as_str().to_string(),
                    });
                }
            }
            return Ok(ActionOutcome::Done);
        }
        "fs.color-label" => {
            // Parent row only opens the flyout submenu.
            return Ok(ActionOutcome::Done);
        }
        action_id if action_id.starts_with("fs.color-label:") => {
            let color = color_label_from_action_id(action_id);
            let fps: Result<Vec<_>, _> = target_paths
                .iter()
                .map(|p| orchid_fs::FsPath::new(p).map_err(map_fs_error))
                .collect();
            let fps = fps?;
            let refs: Vec<&orchid_fs::FsPath> = fps.iter().collect();
            inner
                .deps
                .tag_manager
                .set_color_many(&refs, color)
                .map_err(map_fs_error)?;
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        "fs.encrypt" => {
            if target_paths.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            return Ok(ActionOutcome::NeedsPassphrase {
                paths: target_paths,
                purpose: PassphrasePurpose::Encrypt,
            });
        }
        "fs.wrap-orchid" => {
            return wrap_orchid::run(&inner, &target_paths).await;
        }
        "fs.add-to-managed" => {
            inner.add_selection_to_managed(&target_paths).await?;
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        "fs.remove-from-managed" => {
            inner.remove_selection_from_managed(&target_paths).await?;
            inner.refresh_all_tabs().await;
            return Ok(ActionOutcome::Done);
        }
        "fs.managed-policy" => {
            let root = target_paths
                .iter()
                .find_map(|p| inner.managed_root_for_path(p))
                .ok_or_else(|| {
                    WidgetError::InvalidStateForOperation("fm-not-managed-folder".into())
                })?;
            let policy = inner.managed_policies.read().get(&root).cloned().flatten();
            return Ok(ActionOutcome::NeedsManagedPolicy { path: root, policy });
        }
        "fs.decrypt" => {
            if target_paths.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            return Ok(ActionOutcome::NeedsPassphrase {
                paths: target_paths,
                purpose: PassphrasePurpose::Decrypt,
            });
        }
        "fs.reveal" => {
            if target_paths.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            return Ok(ActionOutcome::NeedsPassphrase {
                paths: target_paths,
                purpose: PassphrasePurpose::Reveal,
            });
        }
        id if archive::is_archive_action(id) => {
            return archive::run(&inner, action_id, &target_paths, None).await;
        }
        id if is_tools_action(id) => {
            return tools::run(&inner, action_id, &target_paths, opts, None).await;
        }
        _ => {
            // Unknown actions: treat as done for MVP.
        }
    }
    inner.publish_refresh();
    Ok(ActionOutcome::Done)
}

pub(crate) fn is_tools_action(id: &str) -> bool {
    matches!(
        id,
        "fs.compare-dirs"
            | "fs.compare-dirs-bytes"
            | "fs.compare-files"
            | "fs.sync-to-other"
            | "fs.sync-from-other"
            | "fs.sync-both"
            | "fs.cloud-sync"
            | "fs.network-bookmark"
            | "fs.network-connect"
            | "fs.merge-to-other"
            | "fs.split"
            | "fs.join"
            | "fs.hash-md5"
            | "fs.hash-sha1"
            | "fs.hash-sha256"
            | "fs.hash-blake3"
            | "fs.hash-crc32"
            | "fs.hash-verify"
            | "fs.encode-base64"
            | "fs.decode-base64"
            | "fs.encode-uue"
            | "fs.decode-uue"
            | "fs.attr-readonly-on"
            | "fs.attr-readonly-off"
            | "fs.attr-hidden-on"
            | "fs.attr-hidden-off"
            | "fs.attr-system-on"
            | "fs.attr-system-off"
            | "fs.attr-archive-on"
            | "fs.attr-archive-off"
            | "fs.touch-now"
            | "fs.touch-set"
            | "fs.case-lower"
            | "fs.case-upper"
            | "fs.case-title"
            | "fs.chmod"
            | "fs.chown"
            | "fs.acl-view"
            | "fs.acl-grant"
            | "fs.acl-reset"
            | "fs.share"
            | "fs.share-view"
            | "fs.share-add"
            | "fs.share-remove"
            | "fs.share-os"
            | "fs.versions"
            | "fs.versions-view"
            | "fs.versions-restore"
            | "fs.versions-copy"
            | "fs.versions-os"
            | "fs.bitlocker"
            | "fs.bitlocker-view"
            | "fs.bitlocker-lock"
            | "fs.bitlocker-unlock"
            | "fs.bitlocker-os"
            | "fs.properties"
            | "fs.exif"
            | "fs.meta-edit"
            | "fs.meta-gps"
            | "fs.meta-date"
            | "fs.meta-date-shift"
            | "fs.meta-strip"
            | "fs.meta-strip-gps"
            | "fs.meta-copy"
            | "fs.meta-export-csv"
            | "fs.meta-export-xml"
            | "fs.meta-import-csv"
            | "fs.meta-template-save"
            | "fs.meta-template-apply"
            | "fs.image-resize"
            | "fs.image-canvas"
            | "fs.image-auto-straighten"
            | "fs.image-adjust"
            | "fs.image-auto-levels"
            | "fs.image-auto-contrast"
            | "fs.image-auto-color"
            | "fs.image-gray"
            | "fs.image-sepia"
            | "fs.image-invert"
            | "fs.image-filter"
            | "fs.image-sharpen"
            | "fs.image-blur"
            | "fs.image-despeckle"
            | "fs.image-cartoon"
            | "fs.image-sketch"
            | "fs.image-vignette"
            | "fs.image-redeye"
            | "fs.image-filter-save-look"
            | "fs.image-annotate"
            | "fs.image-watermark"
            | "fs.image-wm-image"
            | "fs.image-stamp"
            | "fs.image-convert"
            | "fs.image-rotate"
            | "fs.image-thumbs"
            | "fs.image-rename-tpl"
            | "fs.image-batch"
            | "fs.image-batch-preview"
            | "fs.image-batch-save"
            | "fs.image-batch-cancel"
            | "fs.image-compare"
            | "fs.image-pick"
            | "fs.image-diff"
            | "fs.image-composite"
            | "fs.image-pano"
            | "fs.image-hdr"
            | "fs.image-print"
            | "fs.image-print-preview"
            | "fs.image-print-sheet"
            | "fs.image-print-nup"
            | "fs.image-print-batch"
            | "fs.image-export"
            | "fs.image-save-as"
            | "fs.image-email"
            | "fs.image-share"
            | "fs.image-copy"
            | "fs.image-paste"
            | "fs.image-wallpaper"
            | "fs.image-screenshot"
            | "fs.image-ico"
            | "fs.image-favicon"
            | "fs.id3"
            | "fs.office-meta"
            | "fs.signature"
    )
}

/// Finish a tool prompt (`NeedsToolPrompt`) with the user's `input`.
pub async fn complete_tool(
    instance_id: Uuid,
    action_id: &str,
    paths: Vec<String>,
    input: &str,
) -> WidgetResult<ActionOutcome> {
    let inner = live_inner(instance_id)?;
    if action_id == "fs.find-large" {
        let root = paths
            .first()
            .cloned()
            .unwrap_or_else(|| active_tab_path(&inner));
        let min_size = parse_byte_size(input).unwrap_or(100 * 1024 * 1024);
        let fp = orchid_fs::FsPath::new(&root).map_err(map_fs_error)?;
        return find::run_find_large(&inner, &fp, min_size).await;
    }
    if archive::is_archive_action(action_id) {
        return archive::run(&inner, action_id, &paths, Some(input)).await;
    }
    tools::run(
        &inner,
        action_id,
        &paths,
        RunActionOpts { skip_confirm: true },
        Some(input),
    )
    .await
}

/// Run Find Files from the packed dialog payload.
pub async fn complete_find(instance_id: Uuid, packed: &str) -> WidgetResult<ActionOutcome> {
    let inner = live_inner(instance_id)?;
    let fallback = active_tab_path(&inner);
    let spec = find::FindSpec::unpack(packed, &fallback);
    find::run_find(&inner, spec).await
}
