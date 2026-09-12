//! File-manager handlers for [`MainWindowController`].

#![allow(unused_imports)]

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use secrecy::ExposeSecret;
use slint::Model;
use slint::ModelRc;
use slint::SharedString;
use slint::VecModel;
use tracing::{debug, warn};
use uuid::Uuid;

use orchid_storage::LifecycleState;
use orchid_widgets::layout::PixelBounds;
use orchid_widgets::{CreateWidgetRequest, WidgetPayload};

use crate::slint_generated::{
    FmConfirmDialog, FmConflictDialog, FmPassphraseState, FmPathSuggest, FmRenameState, FmTagState,
    WidgetFrameModel,
};
use crate::window::errors::{fm_localized_error, is_passphrase_retryable};
use crate::window::models::{
    build_context_menu, build_managed_policy_state, empty_confirm_dialog, empty_conflict_dialog,
    empty_context_menu, empty_find_state, empty_fm_overlays, empty_managed_policy_state,
    empty_passphrase_state, empty_rename_state, empty_tag_state, fm_grid_rebase_slack,
    fm_grid_visible_range, fm_grid_window, fm_list_visible_range, fm_list_window,
    fm_passphrase_dialog_labels, fm_window_covers, patch_fm_selection, sync_fm_path_suggestions,
    FileManagerOverlays, FmViewport, FM_LIST_REBASE_SLACK,
};
use crate::window::spawn;

use super::super::{open_file_associations, open_with_application_picker, MainWindowController};
use super::{default_fm_overlays, parse_select_filter_commit};

impl MainWindowController {
    pub(in crate::window::main_window) fn fm_pane_at_point(
        &self,
        inst: Uuid,
        content_x: f32,
        bounds: PixelBounds,
    ) -> u8 {
        let dual = self
            .widget_manager
            .snapshot_cache()
            .get(inst)
            .and_then(|s| match &s.payload {
                WidgetPayload::FileManager(fm) => Some(fm.dual_pane),
                _ => None,
            })
            .unwrap_or(false);
        if !dual {
            return (*self.fm_focus.lock())
                .map(|(_, p)| p)
                .unwrap_or_else(|| self.fm_active_pane(inst));
        }
        let local_x = content_x - bounds.x;
        if local_x < bounds.width / 2.0 {
            0
        } else {
            1
        }
    }

    pub(in crate::window::main_window) fn fm_drop_target(&self) -> Option<(Uuid, u8)> {
        if let (Some((cx, cy)), Ok(w)) = (
            *self.last_canvas_pointer.lock(),
            self.workspace_manager.active(),
        ) {
            let (vw, vh) = *self.canvas_size.lock();
            let instances = self.widget_manager.instances_for_workspace(w.id);
            self.layout_engine
                .grow_grid_to_fit_instances(w.id, &instances);
            let snap = self.layout_engine.snapshot(
                w.id,
                &instances,
                orchid_widgets::ViewportSize {
                    width_px: vw,
                    height_px: vh,
                },
            );
            let off = self.drag_offset.lock();
            for pl in snap.cells.iter().rev() {
                let mut b = pl.bounds;
                if let Some((dx, dy)) = off.get(&pl.instance_id) {
                    b.x += dx;
                    b.y += dy;
                }
                if cx < b.x || cy < b.y || cx >= b.x + b.width || cy >= b.y + b.height {
                    continue;
                }
                if let Ok(inst) = self.widget_manager.get_instance(pl.instance_id) {
                    if inst.type_id == "file-manager" {
                        let content_top = b.y + Self::WIDGET_FRAME_HEADER_PX;
                        if cy < content_top {
                            continue;
                        }
                        let pane = self.fm_pane_at_point(pl.instance_id, cx, b);
                        return Some((pl.instance_id, pane));
                    }
                }
            }
        }
        (*self.fm_focus.lock()).or_else(|| {
            self.find_active_fm()
                .map(|id| (id, self.fm_active_pane(id)))
        })
    }

    pub(in crate::window::main_window) fn pointer_over_viewer_content(&self) -> bool {
        self.viewer_content_at_pointer().is_some()
    }

    /// Viewer instance under the canvas pointer (content area, below header).
    pub(in crate::window::main_window) fn viewer_content_at_pointer(&self) -> Option<Uuid> {
        let Some((cx, cy)) = *self.last_canvas_pointer.lock() else {
            return None;
        };
        let (inst, bounds) =
            self.widget_bounds_at_canvas_point(cx, cy, orchid_widgets::builtin::viewer::TYPE_ID)?;
        let content_top = bounds.y + Self::WIDGET_FRAME_HEADER_PX;
        if cy >= content_top && cy < bounds.y + bounds.height {
            Some(inst)
        } else {
            None
        }
    }

    /// Audio Player instance under the canvas pointer (content area, below header).
    pub(in crate::window::main_window) fn audio_player_content_at_pointer(&self) -> Option<Uuid> {
        let Some((cx, cy)) = *self.last_canvas_pointer.lock() else {
            return None;
        };
        let (inst, bounds) = self.widget_bounds_at_canvas_point(
            cx,
            cy,
            orchid_widgets::builtin::audio_player::TYPE_ID,
        )?;
        let content_top = bounds.y + Self::WIDGET_FRAME_HEADER_PX;
        if cy >= content_top && cy < bounds.y + bounds.height {
            Some(inst)
        } else {
            None
        }
    }

    /// Video Player instance under the canvas pointer (content area, below header).
    pub(in crate::window::main_window) fn video_player_content_at_pointer(&self) -> Option<Uuid> {
        let Some((cx, cy)) = *self.last_canvas_pointer.lock() else {
            return None;
        };
        let (inst, bounds) = self.widget_bounds_at_canvas_point(
            cx,
            cy,
            orchid_widgets::builtin::video_player::TYPE_ID,
        )?;
        let content_top = bounds.y + Self::WIDGET_FRAME_HEADER_PX;
        if cy >= content_top && cy < bounds.y + bounds.height {
            Some(inst)
        } else {
            None
        }
    }

    pub(in crate::window::main_window) fn fm_open_paths_in_viewer(
        self: &Arc<Self>,
        paths: Vec<String>,
    ) {
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let mut opened = 0usize;
            let mut skipped = 0usize;
            for p in paths {
                let Ok(fp) = orchid_fs::FsPath::new(&p) else {
                    continue;
                };
                if fp.scheme() == "virtual" {
                    continue;
                }
                let os = std::path::Path::new(&p);
                if os.is_dir() {
                    continue;
                }
                if !os.is_file() {
                    continue;
                }
                if opened >= Self::VIEWER_MULTI_OPEN_CAP {
                    skipped += 1;
                    continue;
                }
                // Multi-file open: one viewer per path; rebuild once after the batch.
                // Cap counts only newly created viewers (focus of already-open does not).
                match Self::open_in_viewer_for_controller(tw.clone(), fp, false, false).await {
                    Ok((_, true)) => {
                        opened += 1;
                    }
                    Ok((_, false)) => {}
                    Err(_) => {}
                }
            }
            if let Some(c) = tw.upgrade() {
                if skipped > 0 {
                    let title = c.locale.tr("widget-viewer-name");
                    let args = orchid_i18n::FluentArgs::new()
                        .with("opened", opened.to_string())
                        .with("skipped", skipped.to_string())
                        .with("cap", Self::VIEWER_MULTI_OPEN_CAP.to_string());
                    let body = c.locale.tr_args("viewer-multi-open-capped", &args);
                    c.push_notification(&title, &body, 2);
                }
                c.schedule_rebuild();
            }
        });
    }

    pub(in crate::window::main_window) fn fm_dispatch_drag_transfer(
        self: &Arc<Self>,
        source_inst: Uuid,
        target_inst: Uuid,
        paths: Vec<String>,
        dest: String,
        copy: bool,
    ) {
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let result = if copy {
                orchid_widgets::builtin::file_manager::copy_paths_to_directory(
                    target_inst,
                    paths,
                    &dest,
                )
                .await
            } else {
                orchid_widgets::builtin::file_manager::move_paths_to_directory(
                    target_inst,
                    paths,
                    &dest,
                )
                .await
            };
            match result {
                Ok(outcome) => {
                    if let Some(c) = tw.upgrade() {
                        c.apply_fm_action_outcome(target_inst, outcome);
                    }
                }
                Err(e) => {
                    warn!(?e, dest = %dest, copy, "fm drag drop");
                    if let Some(c) = tw.upgrade() {
                        c.notify_fm_action_failed(&e);
                    }
                }
            }
            if source_inst != target_inst {
                let _ = orchid_widgets::builtin::file_manager::refresh_instance(source_inst).await;
            }
            if let Some(c) = tw.upgrade() {
                let _ = c.widget_manager.refresh_snapshot_cache(target_inst).await;
                if source_inst != target_inst {
                    let _ = c.widget_manager.refresh_snapshot_cache(source_inst).await;
                    c.fm_patch_or_rebuild(source_inst);
                }
                c.fm_patch_or_rebuild(target_inst);
            }
        });
    }

    pub(in crate::window::main_window) fn fm_resolve_move_dest(
        &self,
        source_inst: Uuid,
        hinted_dest: Option<String>,
    ) -> Option<(Uuid, String)> {
        let hinted = hinted_dest.filter(|d| !d.is_empty() && !d.starts_with("virtual:"));
        let drop_target = self.fm_drop_target();
        match (hinted, drop_target) {
            (Some(dest), Some((fm, _pane))) if fm == source_inst => Some((source_inst, dest)),
            (Some(dest), _) => {
                let fm = drop_target.map(|(f, _)| f).unwrap_or(source_inst);
                Some((fm, dest))
            }
            (None, Some((fm, pane))) => {
                let path = self.fm_active_tab_path(fm, pane)?;
                if path.is_empty() || path.starts_with("virtual:") {
                    return None;
                }
                Some((fm, path))
            }
            (None, None) => None,
        }
    }

    pub(in crate::window::main_window) fn fm_complete_drag_drop(
        self: &Arc<Self>,
        source_inst: Uuid,
        hinted_dest: Option<String>,
    ) {
        let paths = {
            let over = self.fm_overlays.read();
            over.get(&source_inst)
                .filter(|e| e.drag_active)
                .map(|e| e.drag_paths.clone())
                .unwrap_or_default()
        };
        if paths.is_empty() {
            self.clear_fm_drag(source_inst);
            self.fm_patch_or_rebuild(source_inst);
            return;
        }
        if self.pointer_over_viewer_content() {
            self.clear_fm_drag(source_inst);
            self.fm_patch_or_rebuild(source_inst);
            self.fm_open_paths_in_viewer(paths);
            return;
        }
        let Some((target_inst, dest)) = self.fm_resolve_move_dest(source_inst, hinted_dest) else {
            self.clear_fm_drag(source_inst);
            self.fm_patch_or_rebuild(source_inst);
            return;
        };
        self.clear_fm_drag(source_inst);
        self.fm_patch_or_rebuild(source_inst);
        let copy = self
            .keyboard_modifiers
            .lock()
            .contains(slint::winit_030::winit::keyboard::ModifiersState::CONTROL);
        self.fm_dispatch_drag_transfer(source_inst, target_inst, paths, dest, copy);
    }

    pub(in crate::window::main_window) fn clear_fm_drag(&self, inst: Uuid) {
        let mut over = self.fm_overlays.write();
        if let Some(entry) = over.get_mut(&inst) {
            entry.drag_active = false;
            entry.drag_paths.clear();
            entry.drag_drop_target.clear();
            entry.drag_target_pane = -1;
        }
    }

    pub(in crate::window::main_window) fn queue_os_file_drop(self: &Arc<Self>, path: String) {
        let generation = {
            let mut batch = self.os_drop_batch.lock();
            batch.paths.push(path);
            batch.generation += 1;
            batch.generation
        };
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            let Some(c) = tw.upgrade() else {
                return;
            };
            let paths = {
                let mut batch = c.os_drop_batch.lock();
                if batch.generation != generation {
                    return;
                }
                std::mem::take(&mut batch.paths)
            };
            if paths.is_empty() {
                return;
            }
            c.on_os_files_dropped(paths);
        });
    }

    pub(in crate::window::main_window) fn on_os_files_dropped(
        self: &Arc<Self>,
        paths: Vec<String>,
    ) {
        // Prefer opening into the viewer under the pointer (media / documents / images).
        if let Some(viewer_id) = self.viewer_content_at_pointer() {
            self.open_os_paths_on_viewer(viewer_id, paths);
            return;
        }
        // Audio Player: folders → library roots, audio files → enqueue.
        if let Some(audio_id) = self.audio_player_content_at_pointer() {
            if orchid_widgets::builtin::audio_player::ingest_os_paths(audio_id, &paths) {
                return;
            }
        }
        // Video Player: folders → library roots, video files → enqueue.
        if let Some(video_id) = self.video_player_content_at_pointer() {
            if orchid_widgets::builtin::video_player::ingest_os_paths(video_id, &paths) {
                return;
            }
        }
        let Some((inst, pane)) = self.fm_drop_target() else {
            return;
        };
        let dest = self.fm_active_tab_path(inst, pane);
        let Some(dest) = dest.filter(|d| !d.is_empty() && !d.starts_with("virtual:")) else {
            return;
        };
        self.set_fm_focus(inst, pane);
        let copy = self
            .keyboard_modifiers
            .lock()
            .contains(slint::winit_030::winit::keyboard::ModifiersState::CONTROL);
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let result = if copy {
                orchid_widgets::builtin::file_manager::copy_paths_to_directory(inst, paths, &dest)
                    .await
            } else {
                orchid_widgets::builtin::file_manager::move_paths_to_directory(inst, paths, &dest)
                    .await
            };
            match result {
                Ok(outcome) => {
                    if let Some(c) = tw.upgrade() {
                        c.apply_fm_action_outcome(inst, outcome);
                    }
                }
                Err(e) => {
                    warn!(?e, dest = %dest, copy, "fm os file drop");
                    if let Some(c) = tw.upgrade() {
                        c.notify_fm_action_failed(&e);
                    }
                }
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    /// OS / Explorer drop onto a viewer: replace the hovered viewer with the first
    /// file, open additional files in new viewers (same cap as FM multi-open).
    pub(in crate::window::main_window) fn open_os_paths_on_viewer(
        self: &Arc<Self>,
        viewer_id: Uuid,
        paths: Vec<String>,
    ) {
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let mut files = Vec::new();
            for p in paths {
                let Ok(fp) = orchid_fs::FsPath::new(&p) else {
                    continue;
                };
                if fp.scheme() == "virtual" {
                    continue;
                }
                let os = std::path::Path::new(&p);
                if !os.is_file() {
                    continue;
                }
                files.push(fp);
            }
            if files.is_empty() {
                return;
            }
            let mut iter = files.into_iter();
            let Some(first) = iter.next() else {
                return;
            };
            if let Err(e) =
                orchid_widgets::builtin::viewer::open_path(viewer_id, first.clone()).await
            {
                warn!(?e, "viewer os drop: open first");
            } else if let Some(c) = tw.upgrade() {
                c.recent_files.touch(&first, Some(&c.bus));
            }
            let mut opened = 1usize;
            let mut skipped = 0usize;
            for fp in iter {
                if opened >= Self::VIEWER_MULTI_OPEN_CAP {
                    skipped += 1;
                    continue;
                }
                match Self::open_in_viewer_for_controller(tw.clone(), fp, false, false).await {
                    Ok((_, true)) => opened += 1,
                    Ok((_, false)) => {}
                    Err(_) => {}
                }
            }
            if let Some(c) = tw.upgrade() {
                if skipped > 0 {
                    let title = c.locale.tr("widget-viewer-name");
                    let args = orchid_i18n::FluentArgs::new()
                        .with("opened", opened.to_string())
                        .with("skipped", skipped.to_string())
                        .with("cap", Self::VIEWER_MULTI_OPEN_CAP.to_string());
                    let body = c.locale.tr_args("viewer-multi-open-capped", &args);
                    c.push_notification(&title, &body, 2);
                }
                c.schedule_rebuild();
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_entry_drag_start(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        path: &SharedString,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        self.set_fm_focus(inst, p);
        let pressed = path.to_string();
        let mut paths = self.fm_selected_paths(inst, p);
        if !pressed.is_empty() && !paths.iter().any(|existing| existing == &pressed) {
            // Selection snapshot can lag the pointer-down that started the gesture.
            paths.push(pressed);
        }
        if paths.is_empty() {
            return;
        }
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(default_fm_overlays);
        entry.drag_active = true;
        entry.drag_paths = paths;
        entry.drag_drop_target.clear();
        entry.drag_target_pane = pane;
        drop(over);
        self.fm_patch_or_rebuild(inst);
    }

    pub(in crate::window::main_window) fn on_fm_entry_drag_hover(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        folder: &SharedString,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        self.set_fm_drag_hover(inst, pane, folder.to_string());
    }

    pub(in crate::window::main_window) fn set_fm_drag_hover(
        self: &Arc<Self>,
        inst: Uuid,
        pane: i32,
        folder: String,
    ) {
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(default_fm_overlays);
        if !entry.drag_active {
            return;
        }
        if entry.drag_drop_target == folder && entry.drag_target_pane == pane {
            return;
        }
        entry.drag_drop_target = folder;
        entry.drag_target_pane = pane;
        drop(over);
        self.fm_patch_or_rebuild(inst);
    }

    pub(in crate::window::main_window) fn clear_fm_drag_hover_to_pane(
        self: &Arc<Self>,
        inst: Uuid,
        pane: i32,
    ) {
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(default_fm_overlays);
        if !entry.drag_active {
            return;
        }
        entry.drag_drop_target.clear();
        entry.drag_target_pane = pane;
        drop(over);
        self.fm_patch_or_rebuild(inst);
    }

    pub(in crate::window::main_window) fn on_fm_entry_drag_scroll(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        mouse_x: f32,
        mouse_y: f32,
        viewport_y: f32,
        width: f32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let drag_active = self
            .fm_overlays
            .read()
            .get(&inst)
            .map(|o| o.drag_active)
            .unwrap_or(false);
        if !drag_active {
            return;
        }
        let p = pane.max(0) as u8;
        if let Some(path) =
            self.fm_drag_hover_path_at_pointer(inst, p, mouse_x, mouse_y, viewport_y, width)
        {
            self.set_fm_drag_hover(inst, pane, path);
        } else {
            self.clear_fm_drag_hover_to_pane(inst, pane);
        }
    }

    pub(in crate::window::main_window) fn fm_drag_hover_path_at_pointer(
        &self,
        inst: Uuid,
        pane: u8,
        mouse_x: f32,
        mouse_y: f32,
        viewport_y: f32,
        width: f32,
    ) -> Option<String> {
        let snap = self.widget_manager.snapshot_cache().get(inst)?;
        let fm = match &snap.payload {
            WidgetPayload::FileManager(fm) => fm,
            _ => return None,
        };
        let pp = fm.panes.get(pane as usize)?;
        let tab = pp.tabs.get(pp.active_tab as usize)?;
        let offset = tab.entries_offset as usize;
        let content_y = mouse_y + viewport_y;

        use orchid_widgets::FmViewMode::*;
        match tab.view_mode {
            List => {
                let row = (content_y / 28.0).floor() as usize;
                tab.entries
                    .get(row.checked_sub(offset)?)
                    .filter(|e| e.is_dir)
                    .map(|e| e.path.clone())
            }
            Details => {
                if content_y < 28.0 {
                    return None;
                }
                let row = ((content_y - 28.0) / 28.0).floor() as usize;
                tab.entries
                    .get(row.checked_sub(offset)?)
                    .filter(|e| e.is_dir)
                    .map(|e| e.path.clone())
            }
            Icons | Gallery => {
                let large = tab.view_mode == Gallery;
                let tile_spacing = 8.0;
                let tile_size = if large { 220.0 } else { 100.0 };
                let tile_height = if large { 240.0 } else { 120.0 };
                let columns = ((width - tile_spacing) / (tile_size + tile_spacing))
                    .floor()
                    .max(1.0) as usize;
                let col = ((mouse_x - tile_spacing) / (tile_size + tile_spacing)).floor() as i32;
                let row =
                    ((content_y - tile_spacing) / (tile_height + tile_spacing)).floor() as i32;
                if col < 0 || row < 0 {
                    return None;
                }
                let idx = row as usize * columns + col as usize;
                tab.entries
                    .get(idx.checked_sub(offset)?)
                    .filter(|e| e.is_dir)
                    .map(|e| e.path.clone())
            }
        }
    }

    pub(in crate::window::main_window) fn on_fm_entry_drag_drop(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        folder: &SharedString,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let folder_path = folder.to_string();
        self.fm_complete_drag_drop(inst, Some(folder_path));
    }

    pub(in crate::window::main_window) fn on_fm_pane_drag_hover(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(default_fm_overlays);
        if !entry.drag_active {
            return;
        }
        entry.drag_drop_target.clear();
        entry.drag_target_pane = pane;
        drop(over);
        self.fm_patch_or_rebuild(inst);
    }

    pub(in crate::window::main_window) fn on_fm_drop_on_current_dir(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(source) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        self.set_fm_focus(source, p);
        self.fm_complete_drag_drop(source, None);
    }

    pub(in crate::window::main_window) fn on_fm_entry_drag_cancel(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        self.clear_fm_drag(inst);
        self.fm_patch_or_rebuild(inst);
    }
}
