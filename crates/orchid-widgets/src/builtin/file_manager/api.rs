//! Navigation, selection, and configuration entry points.

use super::*;

/// Show a passphrase failure on the file-manager status bar.
pub fn report_passphrase_error(instance_id: Uuid, message: String) -> WidgetResult<()> {
    live_inner(instance_id)?.set_passphrase_error(message);
    Ok(())
}

/// Clear the passphrase failure toast.
pub fn clear_passphrase_error(instance_id: Uuid) -> WidgetResult<()> {
    live_inner(instance_id)?.clear_passphrase_error();
    Ok(())
}

/// Options for [`run_action`].
#[derive(Debug, Clone, Copy, Default)]
pub struct RunActionOpts {
    /// When true, destructive actions skip confirmation (user already confirmed).
    pub skip_confirm: bool,
}

/// Build context-menu descriptors for a right-click on `context_path` in `pane`.
pub fn context_menu_for(
    instance_id: Uuid,
    pane: u8,
    context_path: &str,
) -> WidgetResult<(Vec<ContextMenuItem>, Vec<String>, Option<ContextMenuInfo>)> {
    let inner = live_inner(instance_id)?;
    if context_path.is_empty() {
        inner.deselect_all_in_pane(pane);
    }
    let (entries, target_paths, entry_count, selection_count, can_create) = {
        let state = inner.state.lock();
        let tab = active_tab_ref(&state, pane)?;
        let tab_id = tab.id;
        let selection = tab.selection.selected_paths();
        let entries = inner
            .entries_by_tab
            .read()
            .get(&tab_id)
            .cloned()
            .unwrap_or_else(|| Arc::new(Vec::new()));
        let entry_count = inner.filtered_paths_for_tab(tab).len();
        let selection_count = tab.selection.count();
        let can_create = !is_virtual(&tab.path);
        let target_paths = if context_path.is_empty() {
            Vec::new()
        } else if selection.iter().any(|p| p == context_path) {
            selection
        } else {
            vec![context_path.to_string()]
        };
        (
            entries,
            target_paths,
            entry_count,
            selection_count,
            can_create,
        )
    };
    let selected_entries: Vec<orchid_fs::FsEntry> = target_paths
        .iter()
        .filter_map(|p| entries.iter().find(|e| e.path.as_str() == p))
        .cloned()
        .collect();
    let mut tag_union = std::collections::BTreeSet::new();
    for e in &selected_entries {
        for t in &e.metadata.extended.tags {
            tag_union.insert(t.clone());
        }
    }
    let inputs = ContextMenuInputs {
        clipboard_has_contents: inner.deps.clipboard.can_paste(),
        can_undo: inner.can_undo(),
        can_redo: inner.can_redo(),
        all_encrypted: selected_entries
            .iter()
            .all(|e| e.metadata.extended.is_encrypted),
        any_encrypted: selected_entries
            .iter()
            .any(|e| e.metadata.extended.is_encrypted),
        all_managed: selected_entries
            .iter()
            .all(|e| e.metadata.extended.is_managed),
        all_starred: selected_entries.iter().all(|e| e.metadata.extended.starred),
        any_starred: selected_entries.iter().any(|e| e.metadata.extended.starred),
        known_tags: inner
            .deps
            .tag_manager
            .all_tags()
            .unwrap_or_default()
            .into_iter()
            .take(12)
            .collect(),
        tags_on_selection: tag_union.into_iter().take(12).collect(),
        entry_count,
        selection_count,
        managed_policy_available: target_paths
            .iter()
            .any(|p| inner.managed_root_for_path(p).is_some()),
        can_create,
        dual_pane: inner.state.lock().right_pane.is_some(),
        in_archive: inner
            .state
            .lock()
            .active_pane()
            .active_tab()
            .path
            .is_archive(),
        in_recycle: {
            let tab_path = inner
                .state
                .lock()
                .active_pane()
                .active_tab()
                .path
                .as_str()
                .to_string();
            orchid_fs::is_recycle_listing(&tab_path)
                || target_paths.iter().any(|p| orchid_fs::is_recycle_item(p))
        },
        selection_has_audio: selected_entries.iter().any(|e| {
            e.metadata.kind == orchid_fs::FsEntryKind::File
                && e.name.rsplit('.').next().is_some_and(|ext| {
                    crate::builtin::audio_player::library::is_audio_extension(
                        &ext.to_ascii_lowercase(),
                    )
                })
        }),
        selection_has_video: selected_entries.iter().any(|e| {
            e.metadata.kind == orchid_fs::FsEntryKind::File
                && e.name.rsplit('.').next().is_some_and(|ext| {
                    crate::builtin::video_player::library::is_video_extension(
                        &ext.to_ascii_lowercase(),
                    )
                })
        }),
    };
    let fmt_locale = inner.deps.orchid_config.read().locale.clone();
    let info = info_for_selection(&selected_entries, &inner.deps.locale, &fmt_locale);
    Ok((
        build_for_selection(&selected_entries, inputs),
        target_paths,
        info,
    ))
}

/// Select `context_path` when it is not already part of the current selection.
pub async fn focus_context_target(
    instance_id: Uuid,
    pane: u8,
    context_path: &str,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let needs_select = {
        let state = inner.state.lock();
        let tab = active_tab_ref(&state, pane)?;
        !tab.selection.is_selected(context_path)
    };
    if needs_select {
        select_entry(instance_id, pane, context_path, SelectionMode::Single).await?;
    }
    Ok(())
}

pub(crate) fn active_tab_ref(state: &FileManagerState, pane: u8) -> WidgetResult<&TabState> {
    if pane == 1 {
        if let Some(r) = state.right_pane.as_ref() {
            return Ok(r.active_tab());
        }
    }
    Ok(state.left_pane.active_tab())
}

pub(crate) fn active_tab_path(inner: &FileManagerInner) -> String {
    let state = inner.state.lock();
    let pane = match state.active_pane {
        ActivePane::Left => 0,
        ActivePane::Right => 1,
    };
    active_tab_ref(&state, pane)
        .map(|t| t.path.as_str().to_string())
        .unwrap_or_default()
}

/// Navigate the given `pane` (0 left, 1 right) to `path`.
pub async fn navigate(instance_id: Uuid, pane: u8, path: orchid_fs::FsPath) -> WidgetResult<()> {
    navigate_inner(instance_id, pane, path, true).await
}

pub(crate) async fn navigate_inner(
    instance_id: Uuid,
    pane: u8,
    path: orchid_fs::FsPath,
    publish: bool,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let tab = {
        let mut state = inner.state.lock();
        let changed = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.active_tab_mut().navigate_to(path)
            } else {
                state.left_pane.active_tab_mut().navigate_to(path)
            }
        } else {
            state.left_pane.active_tab_mut().navigate_to(path)
        };
        inner.reset_pane_viewport(pane);
        let tab = active_tab_ref(&state, pane)?.clone();
        if changed {
            inner.record_visit(&tab.path);
        }
        tab
    };
    inner
        .refresh_tabs_with_opts(
            &[tab],
            RefreshOpts {
                publish,
                indicate_loading: true,
            },
        )
        .await;
    Ok(())
}

/// Back in history for `pane`.
pub async fn navigate_back(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let tab = {
        let mut state = inner.state.lock();
        let changed = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.active_tab_mut().back()
            } else {
                state.left_pane.active_tab_mut().back()
            }
        } else {
            state.left_pane.active_tab_mut().back()
        };
        if !changed {
            return Ok(());
        }
        inner.reset_pane_viewport(pane);
        let tab = active_tab_ref(&state, pane)?.clone();
        inner.record_visit(&tab.path);
        tab
    };
    inner
        .refresh_tabs_with_opts(
            &[tab],
            RefreshOpts {
                publish: true,
                indicate_loading: true,
            },
        )
        .await;
    Ok(())
}

/// Forward in history for `pane`.
pub async fn navigate_forward(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let tab = {
        let mut state = inner.state.lock();
        let changed = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.active_tab_mut().forward()
            } else {
                state.left_pane.active_tab_mut().forward()
            }
        } else {
            state.left_pane.active_tab_mut().forward()
        };
        if !changed {
            return Ok(());
        }
        inner.reset_pane_viewport(pane);
        let tab = active_tab_ref(&state, pane)?.clone();
        inner.record_visit(&tab.path);
        tab
    };
    inner
        .refresh_tabs_with_opts(
            &[tab],
            RefreshOpts {
                publish: true,
                indicate_loading: true,
            },
        )
        .await;
    Ok(())
}

/// Up to parent folder for `pane`.
pub async fn navigate_up(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let parent = {
        let state = inner.state.lock();
        let tab = if pane == 1 {
            state
                .right_pane
                .as_ref()
                .unwrap_or(&state.left_pane)
                .active_tab()
        } else {
            state.left_pane.active_tab()
        };
        tab.path.parent()
    };
    if let Some(p) = parent {
        navigate(instance_id, pane, p).await?;
    }
    Ok(())
}

/// Jump the active tab to the user's home directory (or a local root fallback).
pub async fn navigate_home(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    navigate(instance_id, pane, default_initial_path()).await
}

/// Directory names under the typed address, filtered by the trailing prefix.
#[must_use]
pub async fn complete_path(instance_id: Uuid, typed: &str) -> Vec<PathCompleteItem> {
    let Ok(inner) = live_inner(instance_id) else {
        return Vec::new();
    };
    let Some((parent, prefix)) = complete_parent_and_prefix(typed) else {
        return Vec::new();
    };
    let show_hidden = inner.config.read().show_hidden;
    let result = inner.navigator.navigate(&parent, show_hidden).await;
    let prefix_l = prefix.to_lowercase();
    let mut items: Vec<PathCompleteItem> = result
        .entries
        .into_iter()
        .filter(|e| matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory))
        .filter(|e| prefix_l.is_empty() || e.name.to_lowercase().starts_with(&prefix_l))
        .map(|e| PathCompleteItem {
            path: e.path.as_str().to_string(),
            label: e.name,
        })
        .collect();
    items.sort_by_key(|a| a.label.to_lowercase());
    items.truncate(12);
    items
}

/// Switch to tab by string id.
pub async fn switch_to_tab(instance_id: Uuid, pane: u8, tab_id: &str) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let want = Uuid::parse_str(tab_id)
        .map_err(|_| WidgetError::InvalidStateForOperation("invalid tab id".into()))?;
    {
        let mut state = inner.state.lock();
        let prev_id = active_tab_ref(&state, pane).ok().map(|t| t.id);
        if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                if let Some(idx) = r.tabs.iter().position(|t| t.id == want) {
                    r.active_tab = idx;
                }
            } else if let Some(idx) = state.left_pane.tabs.iter().position(|t| t.id == want) {
                state.left_pane.active_tab = idx;
            }
        } else if let Some(idx) = state.left_pane.tabs.iter().position(|t| t.id == want) {
            state.left_pane.active_tab = idx;
        }
        let tab = active_tab_ref(&state, pane)?;
        if prev_id != Some(tab.id) {
            inner.record_visit(&tab.path);
        }
    }
    inner.reset_pane_viewport(pane);
    inner.publish_refresh();
    if let Ok(tab) = active_tab_ref(&inner.state.lock(), pane) {
        inner.spawn_view_decorations(tab.clone());
    }
    Ok(())
}

/// Close tab by id.
pub async fn close_tab(instance_id: Uuid, pane: u8, tab_id: &str) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let want = Uuid::parse_str(tab_id)
        .map_err(|_| WidgetError::InvalidStateForOperation("invalid tab id".into()))?;
    let mut closed = false;
    {
        let mut state = inner.state.lock();
        let target = if pane == 1 {
            state.right_pane.as_mut()
        } else {
            None
        };
        if let Some(r) = target {
            if r.tabs.len() <= 1 {
                return Ok(());
            }
            if let Some(idx) = r.tabs.iter().position(|t| t.id == want) {
                r.tabs.remove(idx);
                r.active_tab = r.active_tab.min(r.tabs.len().saturating_sub(1));
                closed = true;
            }
        } else {
            if state.left_pane.tabs.len() <= 1 {
                return Ok(());
            }
            if let Some(idx) = state.left_pane.tabs.iter().position(|t| t.id == want) {
                state.left_pane.tabs.remove(idx);
                state.left_pane.active_tab = state
                    .left_pane
                    .active_tab
                    .min(state.left_pane.tabs.len().saturating_sub(1));
                closed = true;
            }
        }
    }
    if closed {
        inner.drop_tab_watch(want);
    }
    inner.publish_refresh();
    Ok(())
}

/// Create a new tab in `pane`. When `path` is `None`, clones the current folder.
pub async fn new_tab(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    new_tab_at(instance_id, pane, None).await
}

/// Create a new tab in `pane` opened at `path` (or the current folder).
pub async fn new_tab_at(
    instance_id: Uuid,
    pane: u8,
    path: Option<orchid_fs::FsPath>,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let tab = {
        let cfg = inner.config.read().clone();
        let mut state = inner.state.lock();
        let dest = if pane == 1 && state.right_pane.is_some() {
            state.right_pane.as_mut().expect("right pane")
        } else {
            &mut state.left_pane
        };
        let path = path.unwrap_or_else(|| dest.active_tab().path.clone());
        dest.tabs
            .push(TabState::new(path, cfg.default_view_mode, cfg.sort_by));
        dest.active_tab = dest.tabs.len().saturating_sub(1);
        dest.active_tab().clone()
    };
    inner
        .refresh_tabs_with_opts(
            &[tab],
            RefreshOpts {
                publish: true,
                indicate_loading: true,
            },
        )
        .await;
    Ok(())
}

/// Open `path` (or the current folder) in the opposite pane, enabling dual-pane if needed.
pub async fn open_in_other_pane(
    instance_id: Uuid,
    src_pane: u8,
    path: Option<orchid_fs::FsPath>,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let dual = inner.config.read().dual_pane;
    if !dual {
        drop(inner);
        toggle_dual_pane(instance_id).await?;
    }
    let inner = live_inner(instance_id)?;
    let dest_pane: u8 = if src_pane == 1 { 0 } else { 1 };
    let target = {
        let state = inner.state.lock();
        if let Some(p) = path {
            p
        } else {
            let src = if src_pane == 1 {
                state
                    .right_pane
                    .as_ref()
                    .unwrap_or(&state.left_pane)
                    .active_tab()
            } else {
                state.left_pane.active_tab()
            };
            src.path.clone()
        }
    };
    drop(inner);
    navigate(instance_id, dest_pane, target).await?;
    switch_active_pane(instance_id, dest_pane).await
}

/// Flatten or restore the listing of nested files in `pane` (branch view).
pub async fn toggle_branch_view(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let tab = {
        let mut state = inner.state.lock();
        let tab = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.active_tab_mut()
            } else {
                state.left_pane.active_tab_mut()
            }
        } else {
            state.left_pane.active_tab_mut()
        };
        if is_virtual(&tab.path) {
            return Ok(());
        }
        tab.branch_view = !tab.branch_view;
        tab.selection.clear();
        inner.reset_pane_viewport(pane);
        tab.clone()
    };
    inner
        .refresh_tabs_with_opts(
            &[tab],
            RefreshOpts {
                publish: true,
                indicate_loading: true,
            },
        )
        .await;
    Ok(())
}

/// Jump `pane` to the drive / volume root of the current folder.
pub async fn navigate_drive_root(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let current = {
        let state = inner.state.lock();
        let tab = if pane == 1 {
            state
                .right_pane
                .as_ref()
                .unwrap_or(&state.left_pane)
                .active_tab()
        } else {
            state.left_pane.active_tab()
        };
        tab.path.clone()
    };
    let Some(root) = drive_root(&current) else {
        return Ok(());
    };
    if root == current {
        return Ok(());
    }
    navigate(instance_id, pane, root).await
}

/// Switch active pane focus.
pub async fn switch_active_pane(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut state = inner.state.lock();
        state.active_pane = if pane == 1 {
            ActivePane::Right
        } else {
            ActivePane::Left
        };
    }
    inner.publish_refresh();
    Ok(())
}

/// Shared file clipboard for a live file-manager instance.
#[must_use]
pub fn file_clipboard(instance_id: Uuid) -> Option<Arc<FileClipboard>> {
    FM_LIVE
        .get(&instance_id)
        .map(|inner| inner.deps.clipboard.clone())
}

/// Snapshot live file-manager config for the settings dialog.
#[must_use]
pub fn current_config(instance_id: Uuid) -> Option<FileManagerConfig> {
    FM_LIVE
        .get(&instance_id)
        .map(|inner| inner.config.read().clone())
}

/// Apply a settings mutation. For dual_pane changes, mirror the pane create/destroy logic from toggle_dual_pane.
pub async fn update_config(
    instance_id: Uuid,
    mutate: impl FnOnce(&mut FileManagerConfig),
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let before_dual = inner.config.read().dual_pane;
    let before_hidden = inner.config.read().show_hidden;
    {
        let mut cfg = inner.config.write();
        mutate(&mut cfg);
    }
    let after = inner.config.read().clone();
    if before_dual != after.dual_pane {
        let enabled = after.dual_pane;
        {
            let mut state = inner.state.lock();
            if enabled && state.right_pane.is_none() {
                let path = state.left_pane.active_tab().path.clone();
                state.right_pane = Some(PaneState::with_single_tab(TabState::new(
                    path,
                    after.default_view_mode,
                    after.sort_by,
                )));
            }
            if !enabled {
                state.right_pane = None;
                state.active_pane = ActivePane::Left;
            }
        }
        inner.publish_refresh();
    } else if before_hidden != after.show_hidden {
        inner.refresh_all_tabs().await;
    } else {
        inner.publish_refresh();
    }
    Ok(())
}

/// Toggle dual-pane configuration.
pub async fn toggle_dual_pane(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let enabled = {
        let mut cfg = inner.config.write();
        cfg.dual_pane = !cfg.dual_pane;
        cfg.dual_pane
    };
    {
        let cfg = inner.config.read().clone();
        let mut state = inner.state.lock();
        if enabled && state.right_pane.is_none() {
            let path = state.left_pane.active_tab().path.clone();
            state.right_pane = Some(PaneState::with_single_tab(TabState::new(
                path,
                cfg.default_view_mode,
                cfg.sort_by,
            )));
        }
        if !enabled {
            state.right_pane = None;
            state.active_pane = ActivePane::Left;
        }
    }
    inner.publish_refresh();
    Ok(())
}

/// Update the visible filtered-entry window for a pane (used by UI virtualization).
pub fn set_viewport_window(instance_id: Uuid, pane: u8, first: usize, end: usize) {
    if let Some(inner) = FM_LIVE.get(&instance_id) {
        let end = end.max(first);
        let prev = inner.viewport_by_pane.write().insert(pane, (first, end));
        if prev.is_some_and(|p| p != (first, end)) {
            if let Ok(tab) = active_tab_ref(&inner.state.lock(), pane) {
                inner.spawn_view_decorations(tab.clone());
            }
        }
    }
}

/// Drop a cached viewport slice so the next snapshot starts at the top.
pub fn reset_viewport_window(instance_id: Uuid, pane: u8) {
    if let Some(inner) = FM_LIVE.get(&instance_id) {
        inner.reset_pane_viewport(pane);
    }
}

/// Listings at or below this size are sent in full. Must match the UI
/// `FM_VIRTUALIZE_THRESHOLD` so a small folder never ships a mid-list slice
/// while Slint lays out `pad = 0` and expects every row.
pub(crate) const FM_VIRTUALIZE_THRESHOLD: usize = 80;

/// Map a stored virtualization window onto a listing of `len` entries.
///
/// A window that starts past the end (typical after navigating out of a long
/// folder) is treated as "show from the top" rather than an empty slice.
pub(crate) fn clamp_entry_window(
    stored: Option<(usize, usize)>,
    len: usize,
    default_window: usize,
) -> (usize, usize) {
    if len <= FM_VIRTUALIZE_THRESHOLD {
        return (0, len);
    }
    match stored {
        Some((first, end)) if first < len && end > first => (first, end.min(len)),
        _ => (0, default_window.min(len)),
    }
}

/// Whether hidden entries are listed in navigation results.
pub fn show_hidden(instance_id: Uuid) -> WidgetResult<bool> {
    Ok(live_inner(instance_id)?.config.read().show_hidden)
}

/// Toggle whether hidden entries are shown in listings.
pub async fn toggle_show_hidden(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut cfg = inner.config.write();
        cfg.show_hidden = !cfg.show_hidden;
    }
    inner.reset_pane_viewport(0);
    inner.reset_pane_viewport(1);
    inner.refresh_all_tabs().await;
    Ok(())
}

/// Toggle single-click vs double-click to open files.
pub async fn toggle_click_behavior(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut cfg = inner.config.write();
        cfg.click_behavior = match cfg.click_behavior {
            ClickBehavior::DoubleToOpen => ClickBehavior::SingleToOpen,
            ClickBehavior::SingleToOpen => ClickBehavior::DoubleToOpen,
        };
    }
    inner.publish_refresh();
    Ok(())
}

/// Cycle view mode for the active tab in `pane`.
pub async fn cycle_view_mode(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let tab = {
        let mut state = inner.state.lock();
        let tab = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.active_tab_mut()
            } else {
                state.left_pane.active_tab_mut()
            }
        } else {
            state.left_pane.active_tab_mut()
        };
        tab.view_mode = match tab.view_mode {
            ViewMode::Icons => ViewMode::List,
            ViewMode::List => ViewMode::Details,
            ViewMode::Details => ViewMode::Gallery,
            ViewMode::Gallery => ViewMode::Icons,
        };
        tab.clone()
    };
    inner.reset_pane_viewport(pane);
    inner.publish_refresh();
    inner.spawn_view_decorations(tab);
    Ok(())
}

/// Cycle the sort column for the active tab in `pane` (folders stay grouped first).
pub async fn cycle_sort(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let (tab_id, sort_by, descending) = {
        let mut state = inner.state.lock();
        let tab = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.active_tab_mut()
            } else {
                state.left_pane.active_tab_mut()
            }
        } else {
            state.left_pane.active_tab_mut()
        };
        tab.sort_by = next_sort_by(tab.sort_by);
        tab.sort_descending = false;
        (tab.id, tab.sort_by, tab.sort_descending)
    };
    inner.reset_pane_viewport(pane);
    inner.resort_tab_in_memory(tab_id, sort_by, descending);
    inner.publish_refresh();
    Ok(())
}

/// Set sort column for the active tab in `pane`; toggles direction when the column is unchanged.
pub async fn set_sort_column(instance_id: Uuid, pane: u8, column: u8) -> WidgetResult<()> {
    let sort_by = sort_by_from_u8(column)
        .ok_or_else(|| WidgetError::InvalidStateForOperation("invalid sort column".into()))?;
    let inner = live_inner(instance_id)?;
    let (tab_id, sort_by, descending) = {
        let mut state = inner.state.lock();
        let tab = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.active_tab_mut()
            } else {
                state.left_pane.active_tab_mut()
            }
        } else {
            state.left_pane.active_tab_mut()
        };
        if tab.sort_by == sort_by {
            tab.sort_descending = !tab.sort_descending;
        } else {
            tab.sort_by = sort_by;
            tab.sort_descending = false;
        }
        (tab.id, tab.sort_by, tab.sort_descending)
    };
    inner.reset_pane_viewport(pane);
    inner.resort_tab_in_memory(tab_id, sort_by, descending);
    inner.publish_refresh();
    Ok(())
}

/// Update quick filter text.
pub async fn set_quick_filter(instance_id: Uuid, pane: u8, q: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut state = inner.state.lock();
        let tab = if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                r.active_tab_mut()
            } else {
                state.left_pane.active_tab_mut()
            }
        } else {
            state.left_pane.active_tab_mut()
        };
        tab.quick_filter = q;
    }
    inner.reset_pane_viewport(pane);
    inner.publish_refresh();
    Ok(())
}

/// Select entry inside the active tab for `pane`.
///
/// Does **not** publish a snapshot refresh — the UI patches selection flags in
/// place. Callers that need a full rebuild must refresh explicitly.
pub async fn select_entry(
    instance_id: Uuid,
    pane: u8,
    path: &str,
    mode: SelectionMode,
) -> WidgetResult<()> {
    select_entry_sync(instance_id, pane, path, mode)
}

/// Same as [`select_entry`], callable from the UI thread without an async hop.
pub fn select_entry_sync(
    instance_id: Uuid,
    pane: u8,
    path: &str,
    mode: SelectionMode,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let (tab_id, ordered): (Uuid, Vec<String>) = {
        let state = inner.state.lock();
        let tab = if pane == 1 {
            state
                .right_pane
                .as_ref()
                .unwrap_or(&state.left_pane)
                .active_tab()
        } else {
            state.left_pane.active_tab()
        };
        // Honor the active quick filter so range selection matches the visible list.
        (tab.id, inner.filtered_paths_for_tab(tab))
    };
    {
        let mut state = inner.state.lock();
        if pane == 1 {
            if let Some(r) = state.right_pane.as_mut() {
                if let Some(t) = r.tabs.iter_mut().find(|t| t.id == tab_id) {
                    match mode {
                        SelectionMode::Single => t.selection.select_single(path),
                        SelectionMode::Toggle => t.selection.toggle(path),
                        SelectionMode::Range => t.selection.extend_to(&ordered, path),
                    }
                }
            } else if let Some(t) = state.left_pane.tabs.iter_mut().find(|t| t.id == tab_id) {
                match mode {
                    SelectionMode::Single => t.selection.select_single(path),
                    SelectionMode::Toggle => t.selection.toggle(path),
                    SelectionMode::Range => t.selection.extend_to(&ordered, path),
                }
            }
        } else if let Some(t) = state.left_pane.tabs.iter_mut().find(|t| t.id == tab_id) {
            match mode {
                SelectionMode::Single => t.selection.select_single(path),
                SelectionMode::Toggle => t.selection.toggle(path),
                SelectionMode::Range => t.selection.extend_to(&ordered, path),
            }
        }
    }
    Ok(())
}

/// Selected entries `(path, is_dir)` for the active tab in `pane` (live state).
#[must_use]
pub fn selected_entries(instance_id: Uuid, pane: u8) -> Vec<(String, bool)> {
    let Ok(inner) = live_inner(instance_id) else {
        return Vec::new();
    };
    let state = inner.state.lock();
    let Ok(tab) = active_tab_ref(&state, pane) else {
        return Vec::new();
    };
    let selected = tab.selection.selected_paths();
    if selected.is_empty() {
        return Vec::new();
    };
    let guard = inner.entries_by_tab.read();
    let Some(entries) = guard.get(&tab.id) else {
        return selected.into_iter().map(|p| (p, false)).collect();
    };
    selected
        .into_iter()
        .map(|path| {
            let is_dir = entries
                .iter()
                .find(|e| e.path.as_str() == path)
                .map(|e| matches!(e.metadata.kind, orchid_fs::FsEntryKind::Directory))
                .unwrap_or(false);
            (path, is_dir)
        })
        .collect()
}

/// `(selection_count, item_count, selected_bytes)` for the active tab in `pane`.
#[must_use]
pub fn selection_counts(instance_id: Uuid, pane: u8) -> Option<(u32, u32, u64)> {
    let inner = live_inner(instance_id).ok()?;
    let state = inner.state.lock();
    let tab = active_tab_ref(&state, pane).ok()?;
    let selection_count = tab.selection.count() as u32;
    let item_count = inner.filtered_paths_for_tab(tab).len() as u32;
    let bytes = inner.selection_bytes_in_tab(tab);
    Some((selection_count, item_count, bytes))
}

/// Rename `old_path` to `new_name` in the same directory.
pub async fn rename(instance_id: Uuid, old_path: &str, new_name: &str) -> WidgetResult<()> {
    if new_name.is_empty()
        || new_name.contains('/')
        || new_name.contains('\\')
        || new_name.contains(':')
    {
        return Err(WidgetError::InvalidStateForOperation(
            "fm-invalid-rename-target".into(),
        ));
    }
    let inner = live_inner(instance_id)?;
    let old = orchid_fs::FsPath::new(old_path).map_err(map_fs_error)?;
    let parent = old
        .parent()
        .ok_or_else(|| WidgetError::InvalidStateForOperation("fm-cannot-rename-root".into()))?;
    let new_path = parent.join(new_name);
    let provider = inner
        .deps
        .registry
        .for_path(&old)
        .ok_or_else(|| WidgetError::InvalidStateForOperation("fm-no-provider-path".into()))?;
    provider
        .rename(&old, &new_path)
        .await
        .map_err(map_fs_error)?;
    inner.record_undo(undo::FsUndoOp::Rename {
        pairs: vec![(old.as_str().to_string(), new_path.as_str().to_string())],
    });
    inner.refresh_all_tabs().await;
    Ok(())
}

/// Create a subfolder under `parent_path`.
pub async fn create_folder(instance_id: Uuid, parent_path: &str, name: &str) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let parent = orchid_fs::FsPath::new(parent_path).map_err(map_fs_error)?;
    if is_virtual(&parent) {
        return Err(WidgetError::InvalidStateForOperation(
            "fm-virtual-create-denied".into(),
        ));
    }
    inner.create_folder_at(&parent, name).await?;
    inner.refresh_all_tabs().await;
    Ok(())
}

/// Create an empty file under `parent_path`.
pub async fn create_file(instance_id: Uuid, parent_path: &str, name: &str) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let parent = orchid_fs::FsPath::new(parent_path).map_err(map_fs_error)?;
    if is_virtual(&parent) {
        return Err(WidgetError::InvalidStateForOperation(
            "fm-virtual-create-denied".into(),
        ));
    }
    inner.create_file_at(&parent, name).await?;
    inner.refresh_all_tabs().await;
    Ok(())
}

/// Open the new-folder dialog for `pane`'s current directory.
pub async fn request_new_folder(instance_id: Uuid, pane: u8) -> WidgetResult<ActionOutcome> {
    let inner = live_inner(instance_id)?;
    let parent = {
        let state = inner.state.lock();
        active_tab_ref(&state, pane)?.path.clone()
    };
    if is_virtual(&parent) {
        return Ok(ActionOutcome::Done);
    }
    Ok(ActionOutcome::NeedsCreateFolder {
        parent: parent.as_str().to_string(),
    })
}

/// Apply `tag` to every path in `paths`.
pub async fn add_tag_to_paths(
    instance_id: Uuid,
    paths: Vec<String>,
    tag: &str,
) -> WidgetResult<()> {
    let trimmed = tag.trim();
    if trimmed.is_empty() {
        return Err(WidgetError::InvalidStateForOperation("fm-empty-tag".into()));
    }
    let inner = live_inner(instance_id)?;
    let fps: Result<Vec<_>, _> = paths
        .iter()
        .map(|p| orchid_fs::FsPath::new(p).map_err(map_fs_error))
        .collect();
    let fps = fps?;
    let refs: Vec<&orchid_fs::FsPath> = fps.iter().collect();
    inner
        .deps
        .tag_manager
        .add_tag_many(&refs, trimmed)
        .map_err(map_fs_error)?;
    inner.refresh_all_tabs().await;
    Ok(())
}

/// Select every visible entry in `pane`'s active tab.
///
/// Selection-only — does not re-list the directory or publish a snapshot.
pub async fn select_all_in_pane(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    inner.select_all_in_pane(pane);
    Ok(())
}

/// Clear selection in `pane`'s active tab.
///
/// Selection-only — does not re-list the directory or publish a snapshot.
pub async fn deselect_all_in_pane(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    deselect_all_in_pane_sync(instance_id, pane)
}

/// Same as [`deselect_all_in_pane`], callable from the UI thread without an async hop.
pub fn deselect_all_in_pane_sync(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    inner.deselect_all_in_pane(pane);
    Ok(())
}

/// Active pane index (0 left, 1 right) for `instance_id`.
#[must_use]
pub fn focused_pane(instance_id: Uuid) -> Option<u8> {
    let inner = live_inner(instance_id).ok()?;
    let pane = inner.state.lock().active_pane;
    Some(match pane {
        ActivePane::Left => 0,
        ActivePane::Right => 1,
    })
}

/// Invert the visible selection in `pane`'s active tab.
pub async fn invert_selection_in_pane(instance_id: Uuid, pane: u8) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    inner.invert_selection_in_pane(pane);
    Ok(())
}

/// Apply a name/attribute mask to the visible listing.
pub async fn apply_select_filter(
    instance_id: Uuid,
    pane: u8,
    op: MaskOp,
    filter: SelectFilter,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    inner.apply_filter_in_pane(pane, op, &filter);
    Ok(())
}

/// Select listing indices `[from, to]` (inclusive). `additive` keeps the previous set.
///
/// `columns <= 1` selects a linear range (list / details). Otherwise the bounding
/// rectangle of the two tile indices in an icon grid is selected.
pub async fn select_index_range(
    instance_id: Uuid,
    pane: u8,
    from: i32,
    to: i32,
    additive: bool,
    columns: i32,
) -> WidgetResult<()> {
    select_index_range_sync(instance_id, pane, from, to, additive, columns)
}

/// Same as [`select_index_range`], but callable from the UI thread without an
/// async hop — marquee tracking needs the selection model updated before the
/// next pointer move is painted.
pub fn select_index_range_sync(
    instance_id: Uuid,
    pane: u8,
    from: i32,
    to: i32,
    additive: bool,
    columns: i32,
) -> WidgetResult<()> {
    if from < 0 || to < 0 {
        return Ok(());
    }
    let inner = live_inner(instance_id)?;
    inner.select_index_range_in_pane(
        pane,
        from as usize,
        to as usize,
        additive,
        columns.max(1) as usize,
    );
    Ok(())
}

/// Move selection by `delta` entries in `pane`'s active tab (filtered order).
/// With `extend`, grows a range from the anchor (Shift+arrows).
///
/// Selection-only — does not publish a snapshot refresh.
pub async fn select_relative(
    instance_id: Uuid,
    pane: u8,
    delta: i32,
    extend: bool,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    inner.move_selection_in_pane(pane, delta, extend);
    Ok(())
}

/// Apply a passphrase for encrypt or reveal after [`ActionOutcome::NeedsPassphrase`].
pub async fn apply_passphrase(
    instance_id: Uuid,
    paths: Vec<String>,
    passphrase: String,
    purpose: PassphrasePurpose,
) -> WidgetResult<ActionOutcome> {
    let inner = live_inner(instance_id)?;
    let outcome = match purpose {
        PassphrasePurpose::Encrypt => {
            inner.encrypt_paths(&paths, &passphrase).await?;
            inner.refresh_all_tabs().await;
            ActionOutcome::Done
        }
        PassphrasePurpose::Decrypt => {
            inner.decrypt_paths(&paths, &passphrase).await?;
            inner.refresh_all_tabs().await;
            ActionOutcome::Done
        }
        PassphrasePurpose::Reveal => {
            let revealed = inner.reveal_paths(&paths, &passphrase).await?;
            ActionOutcome::OpenExternally { paths: revealed }
        }
        PassphrasePurpose::RevealInViewer => {
            let revealed = inner.reveal_paths(&paths, &passphrase).await?;
            if let Some(path) = revealed.first() {
                ActionOutcome::OpenInViewer { path: path.clone() }
            } else {
                ActionOutcome::Done
            }
        }
        PassphrasePurpose::ArchiveCreate | PassphrasePurpose::ArchiveOpen => {
            archive::apply_passphrase(&inner, &paths, &passphrase, purpose).await?
        }
    };
    let _ = inner
        .deps
        .fm_passphrase_vault
        .save_passphrase(secrecy::SecretString::from(passphrase));
    Ok(outcome)
}

/// Current single- vs double-click open behaviour.
pub fn click_behavior(instance_id: Uuid) -> WidgetResult<ClickBehavior> {
    Ok(live_inner(instance_id)?.config.read().click_behavior)
}

/// Open a path from the listing (navigate directories, reveal or view files).
pub async fn open_path(
    instance_id: Uuid,
    pane: u8,
    path: &str,
    is_dir_hint: bool,
) -> WidgetResult<ActionOutcome> {
    let t0 = std::time::Instant::now();
    debug!(%path, is_dir_hint, pane, "fm open_path start");
    let inner = live_inner(instance_id)?;
    let fp = orchid_fs::FsPath::new(path).map_err(map_fs_error)?;

    if orchid_fs::is_recycle_item(path) {
        orchid_fs::restore_recycle(&[path.to_string()])
            .await
            .map_err(map_fs_error)?;
        inner.refresh_all_tabs().await;
        return Ok(ActionOutcome::Done);
    }

    let is_dir = entry_is_directory(&inner, &fp, is_dir_hint).await;
    debug!(%path, is_dir, elapsed_ms = t0.elapsed().as_millis(), "fm open_path classified");

    if is_dir {
        if inner.is_path_encrypted(&fp) {
            return Ok(ActionOutcome::NeedsPassphrase {
                paths: vec![path.to_string()],
                purpose: PassphrasePurpose::Reveal,
            });
        }
        navigate_inner(instance_id, pane, fp, false).await?;
        debug!(%path, elapsed_ms = t0.elapsed().as_millis(), "fm open_path navigated");
        return Ok(ActionOutcome::Done);
    }

    if inner.is_path_encrypted(&fp) {
        return Ok(ActionOutcome::NeedsPassphrase {
            paths: vec![path.to_string()],
            purpose: PassphrasePurpose::RevealInViewer,
        });
    }

    if let Some(outcome) = archive::open_if_archive(&inner, instance_id, pane, &fp).await {
        return outcome;
    }

    inner.record_recent(&fp);
    debug!(%path, elapsed_ms = t0.elapsed().as_millis(), "fm open_path -> viewer");
    Ok(ActionOutcome::OpenInViewer {
        path: path.to_string(),
    })
}

pub(crate) async fn entry_is_directory(
    inner: &FileManagerInner,
    fp: &orchid_fs::FsPath,
    is_dir_hint: bool,
) -> bool {
    if is_virtual(fp) {
        let raw = fp.as_str();
        if orchid_fs::is_recycle_item(raw) {
            return false;
        }
        return is_dir_hint
            || category_for_virtual_path(raw).is_some()
            || label_key_for_virtual_path(raw).is_some()
            || find::is_search_virtual(raw);
    }
    if let Some(provider) = inner.deps.registry.for_path(fp) {
        if let Ok(meta) = provider.metadata(fp).await {
            return matches!(meta.kind, orchid_fs::FsEntryKind::Directory);
        }
    }
    // Provider metadata can fail under load; fall back to OS + UI hint.
    if let Ok(local) = fp.to_local() {
        if local.is_dir() {
            return true;
        }
        if local.is_file() {
            return false;
        }
    }
    is_dir_hint
}

pub(crate) async fn folder_path_from_target(
    inner: &FileManagerInner,
    path: &str,
) -> Option<orchid_fs::FsPath> {
    let fp = orchid_fs::FsPath::new(path).ok()?;
    if entry_is_directory(inner, &fp, false).await {
        Some(fp)
    } else {
        fp.parent()
    }
}

/// Refresh every tab in a live file-manager instance.
pub async fn refresh_instance(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    inner.refresh_all_tabs().await;
    inner.publish_refresh();
    Ok(())
}

/// Move `sources` into directory `dest_dir` (drag-and-drop target).
pub async fn move_paths_to_directory(
    instance_id: Uuid,
    sources: Vec<String>,
    dest_dir: &str,
) -> WidgetResult<ActionOutcome> {
    transfer_into_directory(instance_id, sources, dest_dir, false).await
}

/// Copy `sources` into directory `dest_dir` (Ctrl+drag or Ctrl+OS drop).
pub async fn copy_paths_to_directory(
    instance_id: Uuid,
    sources: Vec<String>,
    dest_dir: &str,
) -> WidgetResult<ActionOutcome> {
    transfer_into_directory(instance_id, sources, dest_dir, true).await
}

pub(crate) async fn transfer_into_directory(
    instance_id: Uuid,
    sources: Vec<String>,
    dest_dir: &str,
    is_copy: bool,
) -> WidgetResult<ActionOutcome> {
    if sources.is_empty() {
        return Ok(ActionOutcome::Done);
    }
    let inner = live_inner(instance_id)?;
    let dest = orchid_fs::FsPath::new(dest_dir).map_err(map_fs_error)?;
    if let Some(provider) = inner.deps.registry.for_path(&dest) {
        let meta = provider.metadata(&dest).await.map_err(map_fs_error)?;
        if !matches!(meta.kind, orchid_fs::FsEntryKind::Directory) {
            return Err(WidgetError::InvalidStateForOperation(
                "fm-drop-not-directory".into(),
            ));
        }
    } else {
        return Err(WidgetError::InvalidStateForOperation(
            "fm-drop-unavailable".into(),
        ));
    }
    inner.transfer_paths(&sources, &dest, is_copy).await
}

/// Apply a batch-rename pattern to `paths`.
pub async fn apply_batch_rename(
    instance_id: Uuid,
    paths: Vec<String>,
    pattern: &str,
    find: &str,
    replace: &str,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let mut pairs = Vec::new();
    for (i, p) in paths.iter().enumerate() {
        let old = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
        let Some(name) = old.file_name() else {
            continue;
        };
        let new_name = batch_rename::apply_rename_pattern(name, i, pattern, find, replace);
        if new_name.is_empty() || new_name == name {
            continue;
        }
        let Some(parent) = old.parent() else {
            continue;
        };
        let dest = parent.join(&new_name);
        let Some(provider) = inner.deps.registry.for_path(&old) else {
            continue;
        };
        provider.rename(&old, &dest).await.map_err(map_fs_error)?;
        pairs.push((old.as_str().to_string(), dest.as_str().to_string()));
    }
    inner.record_undo(undo::FsUndoOp::Rename { pairs });
    inner.refresh_all_tabs().await;
    Ok(())
}

pub(crate) async fn create_link_in_pane(
    instance_id: Uuid,
    pane: u8,
    paths: &[String],
    kind: &str,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let dest_dir = {
        let state = inner.state.lock();
        let tab = if pane == 1 {
            state
                .right_pane
                .as_ref()
                .unwrap_or(&state.left_pane)
                .active_tab()
        } else {
            state.left_pane.active_tab()
        };
        tab.path.clone()
    };
    if is_virtual(&dest_dir) {
        return Err(WidgetError::InvalidStateForOperation(
            "fm-transfer-virtual-dest".into(),
        ));
    }
    let Some(target_s) = paths.first() else {
        return Ok(());
    };
    let target = orchid_fs::FsPath::new(target_s).map_err(map_fs_error)?;
    let base = target.file_name().unwrap_or("link");
    let suffix = match kind {
        "fs.link-hard" => "hardlink",
        "fs.link-junction" => "junction",
        _ => "link",
    };
    let (stem, ext) = batch_rename::split_stem_ext(base);
    let link_name = format!("{stem} - {suffix}{ext}");
    let link = dest_dir.join(&link_name);
    match kind {
        "fs.link-hard" => orchid_fs::create_hard_link(&link, &target)
            .await
            .map_err(map_fs_error)?,
        "fs.link-junction" => orchid_fs::create_junction(&link, &target)
            .await
            .map_err(map_fs_error)?,
        _ => orchid_fs::create_symlink(&link, &target)
            .await
            .map_err(map_fs_error)?,
    }
    Ok(())
}

/// Record a path in the recent-files list (files only).
pub async fn touch_recent(instance_id: Uuid, path: &str) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let fp = orchid_fs::FsPath::new(path).map_err(map_fs_error)?;
    inner.record_recent(&fp);
    Ok(())
}

/// Navigate to a virtual folder by sidebar id.
pub async fn navigate_virtual(instance_id: Uuid, pane: u8, virtual_id: &str) -> WidgetResult<()> {
    // Map the UI ids from the sidebar to internal virtual paths.
    let path = match virtual_id {
        "fav:recent" => orchid_fs::FsPath::new("virtual:recent").ok(),
        "fav:starred" => orchid_fs::FsPath::new("virtual:starred").ok(),
        "fav:tags" => orchid_fs::FsPath::new("virtual:tags").ok(),
        "cat:images" => orchid_fs::FsPath::new("virtual:categories/images").ok(),
        "cat:documents" => orchid_fs::FsPath::new("virtual:categories/documents").ok(),
        "cat:video" => orchid_fs::FsPath::new("virtual:categories/video").ok(),
        "cat:audio" => orchid_fs::FsPath::new("virtual:categories/audio").ok(),
        "cat:archives" => orchid_fs::FsPath::new("virtual:categories/archives").ok(),
        "fav:search" => orchid_fs::FsPath::new("virtual:search").ok(),
        "fav:recycle" => orchid_fs::FsPath::new("virtual:recycle").ok(),
        "net:places" => orchid_fs::FsPath::new("virtual:network").ok(),
        other if other.starts_with("net:") && other != "net:places" => {
            let idx = other
                .strip_prefix("net:")
                .and_then(|s| s.parse::<usize>().ok());
            let inner = live_inner(instance_id)?;
            idx.and_then(|i| {
                inner
                    .enabled_network_mounts()
                    .into_iter()
                    .nth(i)
                    .and_then(|m| orchid_fs::normalize_mount_uri(&m.uri))
                    .and_then(|uri| orchid_fs::FsPath::new(&uri).ok())
            })
        }
        other if other.starts_with("drive:") => other
            .strip_prefix("drive:")
            .and_then(|p| orchid_fs::FsPath::new(p).ok()),
        other if other.starts_with("managed:") => {
            let idx = other
                .strip_prefix("managed:")
                .and_then(|s| s.parse::<usize>().ok());
            let inner = live_inner(instance_id)?;
            let roots = inner.managed_roots.read();
            idx.and_then(|i| roots.get(i))
                .and_then(|p| orchid_fs::FsPath::new(p).ok())
        }
        other => {
            warn!(id = %other, "unknown virtual folder id");
            None
        }
    };
    if let Some(p) = path {
        navigate(instance_id, pane, p).await?;
    }
    Ok(())
}

/// Refresh every live file-manager instance (e.g. after config hot-reload).
pub async fn refresh_all_instances() {
    for entry in FM_LIVE.iter() {
        let inner = Arc::clone(entry.value());
        tokio::spawn(async move {
            inner.refresh_all_tabs().await;
        });
    }
}

/// Notify every live file-manager instance that managed ingest started.
pub fn notify_managed_ingest_started(path: &orchid_fs::FsPath) {
    for entry in FM_LIVE.iter() {
        entry.value().handle_managed_ingest_started(path);
    }
}

/// Notify every live file-manager instance that managed ingest failed.
pub fn notify_managed_ingest_failed(path: &orchid_fs::FsPath) {
    for entry in FM_LIVE.iter() {
        entry.value().handle_managed_ingest_failed(path);
    }
}

/// Notify every live file-manager instance that a managed file was ingested.
pub fn notify_managed_ingest(path: &orchid_fs::FsPath) {
    for entry in FM_LIVE.iter() {
        let inner = Arc::clone(entry.value());
        let path = path.clone();
        tokio::spawn(async move {
            inner.handle_managed_ingest(&path).await;
        });
    }
}

pub(crate) fn network_mount_display_name(
    m: &orchid_storage::NetworkMountConfig,
    uri: &str,
) -> String {
    if !m.name.trim().is_empty() {
        return m.name.trim().to_string();
    }
    orchid_fs::FsPath::new(uri)
        .ok()
        .and_then(|p| p.file_name().map(String::from))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| uri.to_string())
}
