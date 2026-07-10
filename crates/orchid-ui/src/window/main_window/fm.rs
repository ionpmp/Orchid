//! File-manager handlers for [`MainWindowController`].

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
use orchid_widgets::WidgetPayload;

use crate::window::errors::{
    fm_localized_error, is_passphrase_retryable,
};
use crate::window::spawn;
use crate::window::models::{
    build_context_menu, build_managed_policy_state,
    empty_confirm_dialog, empty_context_menu, empty_managed_policy_state, empty_passphrase_state, empty_rename_state, empty_tag_state,
    fm_passphrase_dialog_labels, FileManagerOverlays,
};
use crate::slint_generated::{
    FmConfirmDialog,
    FmRenameState, FmTagState, FmPassphraseState,
};


use super::{MainWindowController, open_with_application_picker};

impl MainWindowController {
        pub(super) fn notify_fm_action_failed(self: &Arc<Self>, err: &impl std::fmt::Display) {
        let title = self.locale.tr("widget-fm-name");
        let reason = fm_localized_error(&self.locale, &err.to_string());
        let body = self.locale.tr_args(
            "fm-action-failed",
            &orchid_i18n::FluentArgs::new().with("reason", reason),
        );
        self.push_notification(&title, &body, 3);
    }

        pub(super) fn sync_fm_transfer_notifications(self: &Arc<Self>) {
        let mut transfer_error: Option<String> = None;
        for inst in self.widget_manager.list_instances() {
            if inst.type_id != "file_manager" {
                continue;
            }
            let Some(snap) = self.widget_manager.snapshot_cache().get(inst.id) else {
                continue;
            };
            if let WidgetPayload::FileManager(fm) = &snap.payload {
                if fm.transfer_error.is_some() {
                    transfer_error = fm.transfer_error.clone();
                    break;
                }
            }
        }
        let mut last = self.last_fm_transfer_error.lock();
        match &transfer_error {
            None => *last = None,
            Some(err) if last.as_deref() == Some(err.as_str()) => {}
            Some(err) => {
                let title = self.locale.tr("widget-fm-name");
                let body = self.locale.tr_args(
                    "fm-transfer-failed",
                    &orchid_i18n::FluentArgs::new()
                        .with("reason", fm_localized_error(&self.locale, err)),
                );
                self.push_notification(&title, &body, 3);
                *last = Some(err.clone());
            }
        }
    }
        pub(super) fn drain_fm_ingest_failure_notification(self: &Arc<Self>) {
        let Some(name) = self.fm_ingest_failure_pending.lock().take() else {
            return;
        };
        let title = self.locale.tr("widget-fm-name");
        let body = self.locale.tr_args(
            "fm-ingest-failed",
            &orchid_i18n::FluentArgs::new().with("name", name.as_str()),
        );
        self.push_notification(&title, &body, 3);
    }
        pub(super) fn set_fm_focus(&self, inst: Uuid, pane: u8) {
        *self.fm_focus.lock() = Some((inst, pane));
    }
        pub(super) fn fm_instances_on_active_workspace(&self) -> Vec<Uuid> {
        let Ok(w) = self.workspace_manager.active() else {
            return Vec::new();
        };
        self.widget_manager
            .instances_for_workspace(w.id)
            .into_iter()
            .filter(|inst| inst.type_id == "file-manager")
            .map(|inst| inst.id)
            .collect()
    }
        pub(super) fn find_active_fm(&self) -> Option<Uuid> {
        let fm_ids = self.fm_instances_on_active_workspace();
        if fm_ids.is_empty() {
            *self.fm_focus.lock() = None;
            return None;
        }
        if let Some((id, _)) = *self.fm_focus.lock() {
            if fm_ids.contains(&id) {
                return Some(id);
            }
        }
        Some(fm_ids[0])
    }
        pub(super) fn fm_prepare_instance(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: Option<u8>,
    ) -> Option<Uuid> {
        let Ok(inst) = Uuid::parse_str(fm_id.as_str()) else {
            return None;
        };
        if !self.fm_instances_on_active_workspace().contains(&inst) {
            return None;
        }
        if let Some(p) = pane {
            self.set_fm_focus(inst, p);
        }
        self.fm_wake_instance(inst);
        Some(inst)
    }
        pub(super) fn fm_wake_instance(self: &Arc<Self>, inst: Uuid) {
        self.widget_manager.touch(inst);
        if let Ok(iref) = self.widget_manager.get_instance(inst) {
            if *iref.lifecycle.read() == LifecycleState::Sleeping {
                let wm = self.widget_manager.clone();
                spawn::spawn_local_compat(async move {
                    let _ = wm.change_lifecycle(inst, LifecycleState::Active).await;
                });
            }
        }
    }
        pub(super) async fn fm_refresh_ui(self: &Arc<Self>, inst: Uuid) {
        let _ = self.widget_manager.refresh_snapshot_cache(inst).await;
        self.schedule_rebuild();
    }
        pub(super) fn widget_bounds_at_canvas_point(
        &self,
        content_x: f32,
        content_y: f32,
        type_id: &str,
    ) -> Option<(Uuid, orchid_widgets::PixelBounds)> {
        let w = self.workspace_manager.active().ok()?;
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
            if content_x < b.x
                || content_y < b.y
                || content_x >= b.x + b.width
                || content_y >= b.y + b.height
            {
                continue;
            }
            if let Ok(inst) = self.widget_manager.get_instance(pl.instance_id) {
                if inst.type_id == type_id {
                    return Some((pl.instance_id, b));
                }
            }
        }
        None
    }
        pub(super) fn fm_pane_at_point(&self, inst: Uuid, content_x: f32, bounds: PixelBounds) -> u8 {
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
        pub(super) fn fm_drop_target(&self) -> Option<(Uuid, u8)> {
        if let (Some((cx, cy)), Ok(w)) =
            (*self.last_canvas_pointer.lock(), self.workspace_manager.active())
        {
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
        self.fm_focus
            .lock()
            .clone()
            .or_else(|| {
                self.find_active_fm()
                    .map(|id| (id, self.fm_active_pane(id)))
            })
    }
        pub(super) fn pointer_over_viewer_content(&self) -> bool {
        let Some((cx, cy)) = self.last_canvas_pointer.lock().clone() else {
            return false;
        };
        let Some((_inst, bounds)) = self.widget_bounds_at_canvas_point(
            cx,
            cy,
            orchid_widgets::builtin::viewer::TYPE_ID,
        ) else {
            return false;
        };
        let content_top = bounds.y + Self::WIDGET_FRAME_HEADER_PX;
        cy >= content_top && cy < bounds.y + bounds.height
    }
        pub(super) fn fm_open_paths_in_viewer(self: &Arc<Self>, paths: Vec<String>) {
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
                if Self::open_in_viewer_for_controller(tw.clone(), fp, false, false)
                    .await
                    .is_ok()
                {
                    opened += 1;
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
        pub(super) fn fm_dispatch_drag_transfer(
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
            if let Err(e) = result {
                warn!(?e, dest = %dest, copy, "fm drag drop");
                if let Some(c) = tw.upgrade() {
                    c.notify_fm_action_failed(&e);
                }
            }
            if source_inst != target_inst {
                let _ =
                    orchid_widgets::builtin::file_manager::refresh_instance(source_inst).await;
            }
            if let Some(c) = tw.upgrade() {
                let _ = c.widget_manager.refresh_snapshot_cache(target_inst).await;
                if source_inst != target_inst {
                    let _ = c.widget_manager.refresh_snapshot_cache(source_inst).await;
                }
                c.schedule_rebuild();
            }
        });
    }
        pub(super) fn fm_resolve_move_dest(
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
        pub(super) fn fm_complete_drag_drop(self: &Arc<Self>, source_inst: Uuid, hinted_dest: Option<String>) {
        let paths = {
            let over = self.fm_overlays.read();
            over.get(&source_inst)
                .filter(|e| e.drag_active)
                .map(|e| e.drag_paths.clone())
                .unwrap_or_default()
        };
        if paths.is_empty() {
            self.clear_fm_drag(source_inst);
            self.schedule_rebuild();
            return;
        }
        if self.pointer_over_viewer_content() {
            self.clear_fm_drag(source_inst);
            self.schedule_rebuild();
            self.fm_open_paths_in_viewer(paths);
            return;
        }
        let Some((target_inst, dest)) = self.fm_resolve_move_dest(source_inst, hinted_dest) else {
            self.clear_fm_drag(source_inst);
            self.schedule_rebuild();
            return;
        };
        self.clear_fm_drag(source_inst);
        self.schedule_rebuild();
        let copy = self
            .keyboard_modifiers
            .lock()
            .contains(slint::winit_030::winit::keyboard::ModifiersState::CONTROL);
        self.fm_dispatch_drag_transfer(source_inst, target_inst, paths, dest, copy);
    }
        pub(super) fn ensure_fm_overlays(&self, inst: Uuid) -> FileManagerOverlays {
        self.fm_overlays
            .read()
            .get(&inst)
            .cloned()
            .unwrap_or_else(|| FileManagerOverlays {
                context_menu: empty_context_menu(),
                confirm_dialog: empty_confirm_dialog(),
                rename: empty_rename_state(),
                tag: empty_tag_state(),
                tag_paths: Vec::new(),
                passphrase: empty_passphrase_state(),
                managed_policy: empty_managed_policy_state(),
                passphrase_paths: Vec::new(),
                passphrase_purpose: None,
                create_folder_parent: None,
                drag_active: false,
                drag_paths: Vec::new(),
                drag_drop_target: String::new(),
                drag_target_pane: -1,
            })
    }
        pub(super) fn clear_fm_drag(&self, inst: Uuid) {
        let mut over = self.fm_overlays.write();
        if let Some(entry) = over.get_mut(&inst) {
            entry.drag_active = false;
            entry.drag_paths.clear();
            entry.drag_drop_target.clear();
            entry.drag_target_pane = -1;
        }
    }
        pub(super) fn fm_active_tab_path(&self, inst: Uuid, pane: u8) -> Option<String> {
        let cache = self.widget_manager.snapshot_cache();
        let snap = cache.get(inst).map(|s| (*s).clone())?;
        let WidgetPayload::FileManager(fm) = &snap.payload else {
            return None;
        };
        let pane_idx = usize::from(pane.min(1));
        let pane = fm.panes.get(pane_idx)?;
        let tab = pane.tabs.get(pane.active_tab as usize)?;
        Some(tab.path_display.clone())
    }
        pub(super) fn fm_active_pane(&self, inst: Uuid) -> u8 {
        let cache = self.widget_manager.snapshot_cache();
        cache
            .get(inst)
            .and_then(|s| match &s.payload {
                WidgetPayload::FileManager(fm) => Some(fm.active_pane),
                _ => None,
            })
            .unwrap_or(0)
    }
        pub(super) fn queue_os_file_drop(self: &Arc<Self>, path: String) {
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
        pub(super) fn on_os_files_dropped(self: &Arc<Self>, paths: Vec<String>) {
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
                orchid_widgets::builtin::file_manager::copy_paths_to_directory(
                    inst,
                    paths,
                    &dest,
                )
                .await
            } else {
                orchid_widgets::builtin::file_manager::move_paths_to_directory(
                    inst,
                    paths,
                    &dest,
                )
                .await
            };
            if let Err(e) = result {
                warn!(?e, dest = %dest, copy, "fm os file drop");
                if let Some(c) = tw.upgrade() {
                    c.notify_fm_action_failed(&e);
                }
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

        pub(super) fn fm_selected_paths(&self, inst: Uuid, pane: u8) -> Vec<String> {
        self.fm_selected_entries(inst, pane)
            .into_iter()
            .map(|(path, _)| path)
            .collect()
    }

        pub(super) fn fm_selected_entries(&self, inst: Uuid, pane: u8) -> Vec<(String, bool)> {
        let cache = self.widget_manager.snapshot_cache();
        let Some(snap) = cache.get(inst).map(|s| (*s).clone()) else {
            return Vec::new();
        };
        let WidgetPayload::FileManager(fm) = &snap.payload else {
            return Vec::new();
        };
        let pane_idx = usize::from(pane.min(1));
        let Some(pane) = fm.panes.get(pane_idx) else {
            return Vec::new();
        };
        let Some(tab) = pane.tabs.get(pane.active_tab as usize) else {
            return Vec::new();
        };
        tab.entries
            .iter()
            .filter(|e| e.is_selected)
            .map(|e| (e.path.clone(), e.is_dir))
            .collect()
    }

        pub(super) fn fm_entry_is_dir(&self, inst: Uuid, pane: u8, path: &str) -> bool {
        let cache = self.widget_manager.snapshot_cache();
        let Some(snap) = cache.get(inst) else {
            return false;
        };
        let WidgetPayload::FileManager(fm) = &snap.payload else {
            return false;
        };
        let pane_idx = usize::from(pane.min(1));
        let Some(pane) = fm.panes.get(pane_idx) else {
            return false;
        };
        let Some(tab) = pane.tabs.get(pane.active_tab as usize) else {
            return false;
        };
        tab.entries
            .iter()
            .find(|e| e.path == path)
            .map(|e| e.is_dir)
            .unwrap_or(false)
    }

        pub(super) fn on_fm_sidebar_clicked(self: &Arc<Self>, fm_id: &SharedString, id: &SharedString) {
        let item_id = id.to_string();
        if item_id.starts_with("section:") {
            return;
        }
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let pane = {
            let cache = self.widget_manager.snapshot_cache();
            cache
                .get(inst)
                .and_then(|s| match &s.payload {
                    WidgetPayload::FileManager(fm) => Some(fm.active_pane),
                    _ => None,
                })
                .unwrap_or(0)
        };
        let tw = Arc::downgrade(self);
        let wm = self.widget_manager.clone();
        spawn::spawn_bg_then_local(
            async move {
                if let Err(e) =
                    orchid_widgets::builtin::file_manager::navigate_virtual(inst, pane, &item_id)
                        .await
                {
                    warn!(?e, "fm sidebar navigation");
                }
                let _ = wm.refresh_snapshot_cache(inst).await;
            },
            move |()| async move {
                if let Some(c) = tw.upgrade() {
                    c.schedule_rebuild();
                }
            },
        );
    }
        pub(super) fn on_fm_toggle_dual_pane(self: &Arc<Self>, fm_id: &SharedString) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::toggle_dual_pane(inst).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }
        pub(super) fn on_fm_toggle_show_hidden(self: &Arc<Self>, fm_id: &SharedString) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::toggle_show_hidden(inst).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }
        pub(super) fn on_fm_toggle_click_behavior(self: &Arc<Self>, fm_id: &SharedString) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::toggle_click_behavior(inst).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }
        pub(super) fn on_fm_open_selected(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let entries = self.fm_selected_entries(inst, p);
        let Some((path, is_dir)) = entries.first() else {
            return;
        };
        self.fm_dispatch_open(inst, p, path.clone(), *is_dir);
    }
        pub(super) fn on_fm_entry_drag_start(self: &Arc<Self>, fm_id: &SharedString, pane: i32, _path: &SharedString) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        self.set_fm_focus(inst, p);
        let paths = self.fm_selected_paths(inst, p);
        if paths.is_empty() {
            return;
        }
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
        entry.drag_active = true;
        entry.drag_paths = paths;
        entry.drag_drop_target.clear();
        entry.drag_target_pane = pane;
        drop(over);
        self.schedule_rebuild();
    }
        pub(super) fn on_fm_entry_drag_hover(self: &Arc<Self>, fm_id: &SharedString, pane: i32, folder: &SharedString) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        self.set_fm_drag_hover(inst, pane, folder.to_string());
    }
        pub(super) fn set_fm_drag_hover(self: &Arc<Self>, inst: Uuid, pane: i32, folder: String) {
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
        if !entry.drag_active {
            return;
        }
        entry.drag_drop_target = folder;
        entry.drag_target_pane = pane;
        drop(over);
        self.schedule_rebuild();
    }
        pub(super) fn clear_fm_drag_hover_to_pane(self: &Arc<Self>, inst: Uuid, pane: i32) {
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
        if !entry.drag_active {
            return;
        }
        entry.drag_drop_target.clear();
        entry.drag_target_pane = pane;
        drop(over);
        self.schedule_rebuild();
    }
        pub(super) fn on_fm_entry_drag_scroll(
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
        if let Some(path) = self.fm_drag_hover_path_at_pointer(
            inst,
            p,
            mouse_x,
            mouse_y,
            viewport_y,
            width,
        ) {
            self.set_fm_drag_hover(inst, pane, path);
        } else {
            self.clear_fm_drag_hover_to_pane(inst, pane);
        }
    }
        pub(super) fn fm_drag_hover_path_at_pointer(
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
        let content_y = mouse_y + viewport_y;

        use orchid_widgets::FmViewMode::*;
        match tab.view_mode {
            List => {
                let row = (content_y / 28.0).floor() as usize;
                tab.entries.get(row).filter(|e| e.is_dir).map(|e| e.path.clone())
            }
            Details => {
                if content_y < 28.0 {
                    return None;
                }
                let row = ((content_y - 28.0) / 28.0).floor() as usize;
                tab.entries.get(row).filter(|e| e.is_dir).map(|e| e.path.clone())
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
                let row = ((content_y - tile_spacing) / (tile_height + tile_spacing)).floor() as i32;
                if col < 0 || row < 0 {
                    return None;
                }
                let idx = row as usize * columns + col as usize;
                tab.entries
                    .get(idx)
                    .filter(|e| e.is_dir)
                    .map(|e| e.path.clone())
            }
        }
    }
        pub(super) fn on_fm_entry_drag_drop(self: &Arc<Self>, fm_id: &SharedString, pane: i32, folder: &SharedString) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let folder_path = folder.to_string();
        self.fm_complete_drag_drop(inst, Some(folder_path));
    }
        pub(super) fn on_fm_pane_drag_hover(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
        if !entry.drag_active {
            return;
        }
        entry.drag_drop_target.clear();
        entry.drag_target_pane = pane;
        drop(over);
        self.schedule_rebuild();
    }
        pub(super) fn on_fm_drop_on_current_dir(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(source) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        self.set_fm_focus(source, p);
        self.fm_complete_drag_drop(source, None);
    }
        pub(super) fn on_fm_entry_drag_cancel(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        self.clear_fm_drag(inst);
        self.schedule_rebuild();
    }
        pub(super) fn on_fm_pane_clicked(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        self.set_fm_focus(inst, p);
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::switch_active_pane(inst, p).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }
        pub(super) fn on_fm_tab_clicked(self: &Arc<Self>, fm_id: &SharedString, pane: i32, tab_id: &SharedString) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let tab = tab_id.to_string();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::switch_to_tab(inst, p, &tab).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }
        pub(super) fn on_fm_tab_closed(self: &Arc<Self>, fm_id: &SharedString, pane: i32, tab_id: &SharedString) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let tab = tab_id.to_string();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::close_tab(inst, p, &tab).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }
        pub(super) fn on_fm_tab_new(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::new_tab(inst, p).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }
        pub(super) fn on_fm_new_folder(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let outcome = match orchid_widgets::builtin::file_manager::request_new_folder(inst, p).await {
                Ok(o) => o,
                Err(e) => {
                    warn!(?e, "fm new folder");
                    if let Some(c) = tw.upgrade() {
                        c.notify_fm_action_failed(&e);
                    }
                    return;
                }
            };
            if let Some(c) = tw.upgrade() {
                c.apply_fm_action_outcome(inst, outcome);
            }
        });
    }
        pub(super) fn on_fm_nav_back(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let tw = Arc::downgrade(self);
        let wm = self.widget_manager.clone();
        spawn::spawn_bg_then_local(
            async move {
                let _ = orchid_widgets::builtin::file_manager::navigate_back(inst, p).await;
                let _ = wm.refresh_snapshot_cache(inst).await;
            },
            move |()| async move {
                if let Some(c) = tw.upgrade() {
                    c.schedule_rebuild();
                }
            },
        );
    }
        pub(super) fn on_fm_nav_forward(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let tw = Arc::downgrade(self);
        let wm = self.widget_manager.clone();
        spawn::spawn_bg_then_local(
            async move {
                let _ = orchid_widgets::builtin::file_manager::navigate_forward(inst, p).await;
                let _ = wm.refresh_snapshot_cache(inst).await;
            },
            move |()| async move {
                if let Some(c) = tw.upgrade() {
                    c.schedule_rebuild();
                }
            },
        );
    }
        pub(super) fn on_fm_nav_up(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let tw = Arc::downgrade(self);
        let wm = self.widget_manager.clone();
        spawn::spawn_bg_then_local(
            async move {
                let _ = orchid_widgets::builtin::file_manager::navigate_up(inst, p).await;
                let _ = wm.refresh_snapshot_cache(inst).await;
            },
            move |()| async move {
                if let Some(c) = tw.upgrade() {
                    c.schedule_rebuild();
                }
            },
        );
    }
        pub(super) fn on_fm_nav_home(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::navigate_home(inst, p).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }
        pub(super) fn on_fm_breadcrumb_clicked(self: &Arc<Self>, fm_id: &SharedString, pane: i32, path: &SharedString) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let raw = path.to_string();
        let Ok(fs_path) = orchid_fs::FsPath::new(raw) else {
            return;
        };
        let tw = Arc::downgrade(self);
        let wm = self.widget_manager.clone();
        spawn::spawn_bg_then_local(
            async move {
                let _ = orchid_widgets::builtin::file_manager::navigate(inst, p, fs_path).await;
                let _ = wm.refresh_snapshot_cache(inst).await;
            },
            move |()| async move {
                if let Some(c) = tw.upgrade() {
                    c.schedule_rebuild();
                }
            },
        );
    }
        pub(super) fn on_fm_view_mode_cycle(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::cycle_view_mode(inst, p).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }
        pub(super) fn on_fm_sort_cycle(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::cycle_sort(inst, p).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }
        pub(super) fn on_fm_sort_column_clicked(self: &Arc<Self>, fm_id: &SharedString, pane: i32, column: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let col = column.max(0).min(3) as u8;
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::set_sort_column(inst, p, col).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }
        pub(super) fn on_fm_quick_filter_changed(self: &Arc<Self>, fm_id: &SharedString, pane: i32, q: &SharedString) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let query = q.to_string();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::set_quick_filter(inst, p, query).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }
        pub(super) fn on_fm_entry_clicked(self: &Arc<Self>, fm_id: &SharedString, pane: i32, path: &SharedString, ctrl: bool) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        self.set_fm_focus(inst, p);
        let ps = path.to_string();
        let ps_for_select = ps.clone();
        let mode = if ctrl {
            orchid_widgets::builtin::file_manager::SelectionMode::Toggle
        } else {
            orchid_widgets::builtin::file_manager::SelectionMode::Single
        };
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ =
                orchid_widgets::builtin::file_manager::select_entry(inst, p, &ps_for_select, mode)
                    .await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });

        let behavior = orchid_widgets::builtin::file_manager::click_behavior(inst)
            .unwrap_or(orchid_widgets::builtin::file_manager::ClickBehavior::DoubleToOpen);
        if behavior != orchid_widgets::builtin::file_manager::ClickBehavior::SingleToOpen {
            return;
        }
        let is_dir = self.fm_entry_is_dir(inst, p, &ps);
        self.fm_dispatch_open(inst, p, ps, is_dir);
    }
        pub(super) fn on_fm_entry_shift_clicked(self: &Arc<Self>, fm_id: &SharedString, pane: i32, path: &SharedString) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let ps = path.to_string();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::select_entry(
                inst,
                p,
                &ps,
                orchid_widgets::builtin::file_manager::SelectionMode::Range,
            )
            .await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }
        pub(super) fn on_fm_entry_double_clicked(self: &Arc<Self>, fm_id: &SharedString, pane: i32, path: &SharedString, is_dir: bool) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let raw = path.to_string();
        let behavior = orchid_widgets::builtin::file_manager::click_behavior(inst)
            .unwrap_or(orchid_widgets::builtin::file_manager::ClickBehavior::DoubleToOpen);
        if is_dir {
            self.fm_dispatch_open(inst, p, raw, true);
            return;
        }
        if behavior == orchid_widgets::builtin::file_manager::ClickBehavior::DoubleToOpen {
            self.fm_dispatch_open(inst, p, raw, false);
        }
    }
        pub(super) fn fm_dispatch_open(self: &Arc<Self>, inst: Uuid, pane: u8, path: String, is_dir: bool) {
        let tw = Arc::downgrade(self);
        let wm = self.widget_manager.clone();
        debug!(%path, is_dir, pane, %inst, "fm_dispatch_open");
        spawn::spawn_bg_then_local(
            async move {
                let t0 = Instant::now();
                let outcome =
                    orchid_widgets::builtin::file_manager::open_path(inst, pane, &path, is_dir)
                        .await;
                let elapsed_ms = t0.elapsed().as_millis();
                match &outcome {
                    Ok(_) => debug!(%path, elapsed_ms, "fm_dispatch_open ok"),
                    Err(e) => warn!(?e, %path, elapsed_ms, "fm_dispatch_open err"),
                }
                let _ = wm.refresh_snapshot_cache(inst).await;
                (path, outcome)
            },
            move |(path, outcome)| async move {
                let Some(c) = tw.upgrade() else {
                    return;
                };
                match outcome {
                    Ok(o) => {
                        c.apply_fm_action_outcome(inst, o);
                        c.schedule_rebuild();
                    }
                    Err(e) => {
                        warn!(?e, path = %path, "fm open path");
                        c.notify_fm_action_failed(&e);
                    }
                }
            },
        );
    }
        pub(super) fn on_fm_entry_context(self: &Arc<Self>, fm_id: &SharedString, pane: i32, path: &SharedString, x: f32, y: f32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let target = path.to_string();
        let (actions, target_paths) = match orchid_widgets::builtin::file_manager::context_menu_for(
            inst,
            p,
            &target,
        ) {
            Ok(v) => v,
            Err(e) => {
                warn!(?e, "fm context menu");
                return;
            }
        };
        let menu = build_context_menu(&actions, &target_paths, x, y, &self.locale);
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
        entry.context_menu = menu;
        drop(over);
        self.schedule_rebuild();

        if target.is_empty() {
            return;
        }

        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) =
                orchid_widgets::builtin::file_manager::focus_context_target(inst, p, &target).await
            {
                warn!(?e, "fm context focus");
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }
        pub(super) fn on_fm_context_action(self: &Arc<Self>, fm_id: &SharedString, action_id: &SharedString, paths: &ModelRc<SharedString>) {
        let id = action_id.to_string();
        let path_vec: Vec<String> = (0..paths.row_count())
            .filter_map(|i| paths.row_data(i))
            .map(|s| s.to_string())
            .collect();
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let outcome = match orchid_widgets::builtin::file_manager::run_action_with_opts(
                inst,
                &id,
                path_vec.clone(),
                orchid_widgets::builtin::file_manager::RunActionOpts::default(),
            )
            .await
            {
                Ok(o) => o,
                Err(e) => {
                    warn!(?e, "fm action");
                    if let Some(c) = tw.upgrade() {
                        c.notify_fm_action_failed(&e);
                    }
                    return;
                }
            };

            if let Some(c) = tw.upgrade() {
                c.apply_fm_action_outcome(inst, outcome);
            }
        });
    }
        pub(super) fn apply_fm_action_outcome(
        self: &Arc<Self>,
        inst: Uuid,
        outcome: orchid_widgets::builtin::file_manager::ActionOutcome,
    ) {
        match outcome {
            orchid_widgets::builtin::file_manager::ActionOutcome::Done => {
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
                entry.context_menu = empty_context_menu();
                entry.confirm_dialog = empty_confirm_dialog();
                entry.rename = empty_rename_state();
                entry.tag = empty_tag_state();
                entry.tag_paths.clear();
                entry.create_folder_parent = None;
                drop(over);
                self.schedule_rebuild();
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::NeedsConfirmation {
                message,
                action_id,
                paths,
            } => {
                let n = paths.len();
                let message_text = if message == "fm-confirm-delete"
                    || message == "fm-confirm-delete-permanent"
                {
                    self.locale.tr_args(
                        &message,
                        &orchid_i18n::FluentArgs::new().with("n", n.to_string()),
                    )
                } else {
                    message
                };
                let dlg = FmConfirmDialog {
                    visible: true,
                    title: self.locale.tr("fm-confirm-title").into(),
                    message: message_text.into(),
                    confirm_label: self.locale.tr("action-confirm-yes").into(),
                    cancel_label: self.locale.tr("action-confirm-no").into(),
                    pending_action: action_id.into(),
                    pending_paths: ModelRc::new(VecModel::from(
                        paths.into_iter().map(SharedString::from).collect::<Vec<_>>(),
                    )),
                };
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
                entry.confirm_dialog = dlg;
                entry.context_menu = empty_context_menu();
                drop(over);
                self.schedule_rebuild();
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::NeedsRename { path, current_name } => {
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
                entry.create_folder_parent = None;
                entry.rename = FmRenameState {
                    active: true,
                    path: path.into(),
                    proposed_name: current_name.into(),
                    title: self.locale.tr("fm-rename-title").into(),
                    ok_label: self.locale.tr("fm-rename-ok").into(),
                    cancel_label: self.locale.tr("fm-rename-cancel").into(),
                };
                entry.context_menu = empty_context_menu();
                drop(over);
                self.schedule_rebuild();
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::NeedsCreateFolder { parent } => {
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
                entry.create_folder_parent = Some(parent);
                entry.rename = FmRenameState {
                    active: true,
                    path: SharedString::new(),
                    proposed_name: self.locale.tr("fm-action-new-folder").into(),
                    title: self.locale.tr("fm-action-new-folder").into(),
                    ok_label: self.locale.tr("fm-rename-ok").into(),
                    cancel_label: self.locale.tr("fm-rename-cancel").into(),
                };
                entry.context_menu = empty_context_menu();
                drop(over);
                self.schedule_rebuild();
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::NeedsTag { paths } => {
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
                entry.tag_paths = paths;
                entry.tag = FmTagState {
                    active: true,
                    proposed_tag: SharedString::new(),
                    title: self.locale.tr("fm-tag-add-title").into(),
                    ok_label: self.locale.tr("fm-rename-ok").into(),
                    cancel_label: self.locale.tr("fm-rename-cancel").into(),
                };
                entry.context_menu = empty_context_menu();
                drop(over);
                self.schedule_rebuild();
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::NeedsPassphrase {
                paths,
                purpose,
            } => {
                let (title, hint, ok_label) = fm_passphrase_dialog_labels(self.locale.as_ref(), purpose);
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
                entry.passphrase_paths = paths;
                entry.passphrase_purpose = Some(purpose);
                entry.passphrase = FmPassphraseState {
                    active: true,
                    proposed_passphrase: SharedString::new(),
                    title: title.into(),
                    hint: hint.into(),
                    ok_label: ok_label.into(),
                    cancel_label: self.locale.tr("fm-rename-cancel").into(),
                    biometric_available: self.fm_passphrase_vault.biometric_unlock_available(),
                    biometric_label: self.locale.tr("fm-passphrase-biometric").into(),
                };
                if let Err(e) =
                    orchid_widgets::builtin::file_manager::clear_passphrase_error(inst)
                {
                    warn!(?e, "fm clear passphrase error");
                }
                entry.context_menu = empty_context_menu();
                drop(over);
                self.schedule_rebuild();
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::NeedsManagedPolicy {
                path,
                policy,
            } => {
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
                entry.managed_policy =
                    build_managed_policy_state(self.locale.as_ref(), &path, policy.as_ref());
                entry.context_menu = empty_context_menu();
                drop(over);
                self.schedule_rebuild();
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::OpenInViewer { path } => {
                let Ok(fs_path) = orchid_fs::FsPath::new(&path) else {
                    warn!(path = %path, "open in viewer: invalid path");
                    return;
                };
                let tw2 = Arc::downgrade(self);
                spawn::spawn_local_compat(async move {
                    let _ = MainWindowController::open_in_viewer_for_controller(
                        tw2, fs_path, true, true,
                    )
                    .await;
                });
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::OpenInViewerMany { paths } => {
                let tw2 = Arc::downgrade(self);
                spawn::spawn_local_compat(async move {
                    let mut opened = 0usize;
                    let mut skipped = 0usize;
                    for path in paths {
                        let Ok(fs_path) = orchid_fs::FsPath::new(&path) else {
                            continue;
                        };
                        if opened >= MainWindowController::VIEWER_MULTI_OPEN_CAP {
                            skipped += 1;
                            continue;
                        }
                        // One widget per path; rebuild once after the batch.
                        if MainWindowController::open_in_viewer_for_controller(
                            tw2.clone(),
                            fs_path,
                            false,
                            false,
                        )
                        .await
                        .is_ok()
                        {
                            opened += 1;
                        }
                    }
                    if let Some(c) = tw2.upgrade() {
                        if skipped > 0 {
                            let title = c.locale.tr("widget-viewer-name");
                            let args = orchid_i18n::FluentArgs::new()
                                .with("opened", opened.to_string())
                                .with("skipped", skipped.to_string())
                                .with(
                                    "cap",
                                    MainWindowController::VIEWER_MULTI_OPEN_CAP.to_string(),
                                );
                            let body = c.locale.tr_args("viewer-multi-open-capped", &args);
                            c.push_notification(&title, &body, 2);
                        }
                        c.schedule_rebuild();
                    }
                });
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::OpenWithPicker { paths } => {
                for path in paths {
                    let open_path = match orchid_fs::FsPath::new(&path) {
                        Ok(fp) => fp
                            .to_local()
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or(path),
                        Err(_) => path,
                    };
                    if let Err(e) = open_with_application_picker(&open_path) {
                        warn!(?e, path = %open_path, "open with picker");
                    }
                }
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::OpenExternally { paths } => {
                for path in paths {
                    let open_path = match orchid_fs::FsPath::new(&path) {
                        Ok(fp) => fp
                            .to_local()
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or(path),
                        Err(_) => path,
                    };
                    if let Err(e) = opener::open(&open_path) {
                        warn!(?e, path = %open_path, "open file externally");
                    }
                }
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::ShowInfo { title, message } => {
                let title_text = if title == "fm-properties-title" {
                    self.locale.tr("fm-properties-title")
                } else {
                    title
                };
                let dlg = FmConfirmDialog {
                    visible: true,
                    title: title_text.into(),
                    message: message.into(),
                    confirm_label: self.locale.tr("fm-info-close").into(),
                    cancel_label: SharedString::new(),
                    pending_action: SharedString::new(),
                    pending_paths: ModelRc::new(VecModel::default()),
                };
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
                entry.confirm_dialog = dlg;
                entry.context_menu = empty_context_menu();
                drop(over);
                self.schedule_rebuild();
            }
        }
    }
        pub(super) fn on_fm_context_dismiss(self: &Arc<Self>, fm_id: &SharedString) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
        entry.context_menu = empty_context_menu();
        drop(over);
        self.schedule_rebuild();
    }
        pub(super) fn on_fm_confirm_yes(self: &Arc<Self>, fm_id: &SharedString) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let overlay = self.fm_overlays.read().get(&inst).cloned();
        let Some(over) = overlay else {
            return;
        };
        let action = over.confirm_dialog.pending_action.to_string();
        if action.is_empty() {
            let mut over = self.fm_overlays.write();
            let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
            entry.confirm_dialog = empty_confirm_dialog();
            drop(over);
            self.schedule_rebuild();
            return;
        }
        let path_vec: Vec<String> = (0..over.confirm_dialog.pending_paths.row_count())
            .filter_map(|i| over.confirm_dialog.pending_paths.row_data(i))
            .map(|s| s.to_string())
            .collect();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let outcome = match orchid_widgets::builtin::file_manager::run_action_with_opts(
                inst,
                &action,
                path_vec,
                orchid_widgets::builtin::file_manager::RunActionOpts {
                    skip_confirm: true,
                },
            )
            .await
            {
                Ok(o) => o,
                Err(e) => {
                    warn!(?e, "fm confirm action");
                    if let Some(c) = tw.upgrade() {
                        c.notify_fm_action_failed(&e);
                    }
                    return;
                }
            };
            if let Some(c) = tw.upgrade() {
                match outcome {
                    orchid_widgets::builtin::file_manager::ActionOutcome::Done => {
                        let mut over = c.fm_overlays.write();
                        let entry = over.entry(inst).or_insert_with(|| c.ensure_fm_overlays(inst));
                        entry.confirm_dialog = empty_confirm_dialog();
                        entry.context_menu = empty_context_menu();
                        drop(over);
                        c.schedule_rebuild();
                    }
                    other => {
                        warn!(?other, "unexpected outcome after fm confirm");
                    }
                }
            }
        });
    }
        pub(super) fn on_fm_confirm_no(self: &Arc<Self>, fm_id: &SharedString) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
        entry.confirm_dialog = empty_confirm_dialog();
        drop(over);
        self.schedule_rebuild();
    }
        pub(super) fn on_fm_rename_commit(self: &Arc<Self>, fm_id: &SharedString, old_path: &SharedString, new_name: &SharedString) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let create_parent = self
            .fm_overlays
            .read()
            .get(&inst)
            .and_then(|o| o.create_folder_parent.clone());
        if let Some(parent) = create_parent {
            let newn = new_name.to_string();
            let tw = Arc::downgrade(self);
            spawn::spawn_local_compat(async move {
                if let Err(e) =
                    orchid_widgets::builtin::file_manager::create_folder(inst, &parent, &newn).await
                {
                    warn!(?e, "fm create folder");
                    if let Some(c) = tw.upgrade() {
                        c.notify_fm_action_failed(&e);
                    }
                }
                if let Some(c) = tw.upgrade() {
                    let mut over = c.fm_overlays.write();
                    let entry = over.entry(inst).or_insert_with(|| c.ensure_fm_overlays(inst));
                    entry.rename = empty_rename_state();
                    entry.create_folder_parent = None;
                    drop(over);
                    c.schedule_rebuild();
                }
            });
            return;
        }
        let old = old_path.to_string();
        let newn = new_name.to_string();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = orchid_widgets::builtin::file_manager::rename(inst, &old, &newn).await {
                warn!(?e, "fm rename");
                if let Some(c) = tw.upgrade() {
                    c.notify_fm_action_failed(&e);
                }
            }
            if let Some(c) = tw.upgrade() {
                let mut over = c.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(|| c.ensure_fm_overlays(inst));
                entry.rename = empty_rename_state();
                drop(over);
                c.schedule_rebuild();
            }
        });
    }
        pub(super) fn on_fm_rename_cancel(self: &Arc<Self>, fm_id: &SharedString) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
        entry.rename = empty_rename_state();
        entry.create_folder_parent = None;
        drop(over);
        self.schedule_rebuild();
    }
        pub(super) fn on_fm_tag_commit(self: &Arc<Self>, fm_id: &SharedString, tag: &SharedString) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let paths = self
            .fm_overlays
            .read()
            .get(&inst)
            .map(|o| o.tag_paths.clone())
            .unwrap_or_default();
        let tag_str = tag.to_string();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ =
                orchid_widgets::builtin::file_manager::add_tag_to_paths(inst, paths, &tag_str).await;
            if let Some(c) = tw.upgrade() {
                let mut over = c.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(|| c.ensure_fm_overlays(inst));
                entry.tag = empty_tag_state();
                entry.tag_paths.clear();
                drop(over);
                c.schedule_rebuild();
            }
        });
    }
        pub(super) fn on_fm_tag_cancel(self: &Arc<Self>, fm_id: &SharedString) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
        entry.tag = empty_tag_state();
        entry.tag_paths.clear();
        drop(over);
        self.schedule_rebuild();
    }
        pub(super) fn on_fm_passphrase_commit(self: &Arc<Self>, fm_id: &SharedString, passphrase: &SharedString) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let overlay = self.fm_overlays.read().get(&inst).cloned();
        let Some(over) = overlay else {
            return;
        };
        let pw = passphrase.to_string();
        if pw.trim().is_empty() {
            if let Err(e) = orchid_widgets::builtin::file_manager::report_passphrase_error(
                inst,
                "passphrase required".into(),
            ) {
                warn!(?e, "fm passphrase empty");
            }
            self.schedule_rebuild();
            return;
        }
        let purpose = over
            .passphrase_purpose
            .unwrap_or(orchid_widgets::builtin::file_manager::PassphrasePurpose::Encrypt);
        let paths = over.passphrase_paths.clone();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let outcome = match orchid_widgets::builtin::file_manager::apply_passphrase(
                inst,
                paths,
                pw,
                purpose,
            )
            .await
            {
                Ok(o) => o,
                Err(e) => {
                    let msg = e.to_string();
                    warn!(?e, "fm passphrase");
                    if let Some(c) = tw.upgrade() {
                        if let Err(report) =
                            orchid_widgets::builtin::file_manager::report_passphrase_error(
                                inst,
                                msg.clone(),
                            )
                        {
                            warn!(?report, "fm passphrase error report");
                        }
                        if !is_passphrase_retryable(&msg) {
                            c.clear_fm_passphrase_overlay(inst);
                        }
                        c.schedule_rebuild();
                    }
                    return;
                }
            };
            if let Some(c) = tw.upgrade() {
                c.clear_fm_passphrase_overlay(inst);
                c.apply_fm_action_outcome(inst, outcome);
            }
        });
    }
        pub(super) fn on_fm_passphrase_cancel(self: &Arc<Self>, fm_id: &SharedString) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        self.clear_fm_passphrase_overlay(inst);
        self.schedule_rebuild();
    }
        pub(super) fn on_fm_managed_policy_close(self: &Arc<Self>, fm_id: &SharedString) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let mut over = self.fm_overlays.write();
        if let Some(entry) = over.get_mut(&inst) {
            entry.managed_policy = empty_managed_policy_state();
            entry.context_menu = empty_context_menu();
        }
        drop(over);
        self.schedule_rebuild();
    }
        pub(super) fn on_fm_passphrase_biometric(self: &Arc<Self>, fm_id: &SharedString) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let overlay = self.fm_overlays.read().get(&inst).cloned();
        let Some(over) = overlay else {
            return;
        };
        let purpose = over
            .passphrase_purpose
            .unwrap_or(orchid_widgets::builtin::file_manager::PassphrasePurpose::Reveal);
        let paths = over.passphrase_paths.clone();
        let prompt = self.locale.tr("fm-passphrase-biometric-prompt");
        let passphrase = match self
            .fm_passphrase_vault
            .load_passphrase_after_biometric(&prompt)
        {
            Ok(p) => p.expose_secret().to_string(),
            Err(e) => {
                let msg = e.to_string();
                warn!(?e, "fm passphrase biometric");
                if let Err(report) =
                    orchid_widgets::builtin::file_manager::report_passphrase_error(inst, msg.clone())
                {
                    warn!(?report, "fm passphrase error report");
                }
                self.schedule_rebuild();
                return;
            }
        };
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let outcome = match orchid_widgets::builtin::file_manager::apply_passphrase(
                inst,
                paths,
                passphrase,
                purpose,
            )
            .await
            {
                Ok(o) => o,
                Err(e) => {
                    let msg = e.to_string();
                    warn!(?e, "fm passphrase biometric apply");
                    if let Some(c) = tw.upgrade() {
                        if let Err(report) =
                            orchid_widgets::builtin::file_manager::report_passphrase_error(
                                inst,
                                msg.clone(),
                            )
                        {
                            warn!(?report, "fm passphrase error report");
                        }
                        if !is_passphrase_retryable(&msg) {
                            c.clear_fm_passphrase_overlay(inst);
                        }
                        c.schedule_rebuild();
                    }
                    return;
                }
            };
            if let Some(c) = tw.upgrade() {
                c.clear_fm_passphrase_overlay(inst);
                c.apply_fm_action_outcome(inst, outcome);
            }
        });
    }
        pub(super) fn clear_fm_passphrase_overlay(self: &Arc<Self>, inst: Uuid) {
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
        entry.passphrase = empty_passphrase_state();
        entry.passphrase_paths.clear();
        entry.passphrase_purpose = None;
        drop(over);
        if let Err(e) = orchid_widgets::builtin::file_manager::clear_passphrase_error(inst) {
            warn!(?e, "fm clear passphrase error");
        }
        self.schedule_rebuild();
    }
        pub(super) fn on_fm_select_all(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) =
                orchid_widgets::builtin::file_manager::select_all_in_pane(inst, p).await
            {
                warn!(?e, "fm select all");
                return;
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }
        pub(super) fn on_fm_deselect_all(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) =
                orchid_widgets::builtin::file_manager::deselect_all_in_pane(inst, p).await
            {
                warn!(?e, "fm deselect all");
                return;
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }
        pub(super) fn on_fm_delete_selected(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let paths = self.fm_selected_paths(inst, pane.max(0) as u8);
        if paths.is_empty() {
            return;
        }
        self.spawn_fm_action(inst, "fs.delete", paths);
    }
        pub(super) fn on_fm_copy_selected(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let paths = self.fm_selected_paths(inst, pane.max(0) as u8);
        if paths.is_empty() {
            return;
        }
        self.spawn_fm_action(inst, "fs.copy", paths);
    }
        pub(super) fn on_fm_paste_clipboard(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        self.spawn_fm_action(inst, "fs.paste", Vec::new());
    }
        pub(super) fn on_fm_rename_selected(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let paths = self.fm_selected_paths(inst, pane.max(0) as u8);
        if paths.len() != 1 {
            return;
        }
        self.spawn_fm_action(inst, "fs.rename", paths);
    }
        pub(super) fn spawn_fm_action(self: &Arc<Self>, inst: Uuid, action_id: &str, paths: Vec<String>) {
        let tw = Arc::downgrade(self);
        let action_id = action_id.to_string();
        spawn::spawn_local_compat(async move {
            let outcome = match orchid_widgets::builtin::file_manager::run_action(
                inst,
                &action_id,
                paths,
            )
            .await
            {
                Ok(o) => o,
                Err(e) => {
                    warn!(?e, action_id = %action_id, "fm action");
                    if let Some(c) = tw.upgrade() {
                        c.notify_fm_action_failed(&e);
                    }
                    return;
                }
            };
            if let Some(c) = tw.upgrade() {
                c.apply_fm_action_outcome(inst, outcome);
            }
        });
    }
}
