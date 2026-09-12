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

use super::{open_file_associations, open_with_application_picker, MainWindowController};

mod dialogs;
mod drag;
mod nav;
mod outcome;

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
            if inst.type_id != "file-manager" {
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

    pub(super) fn reveal_folder_in_fm(self: &Arc<Self>, folder: orchid_fs::FsPath) {
        let pane = self.fm_focus.lock().map(|(_, p)| p).unwrap_or(0);
        let t = Arc::downgrade(self);
        spawn::spawn_local(async move {
            let Some(c) = t.upgrade() else {
                return;
            };
            let fm = if let Some(id) = c.find_active_fm() {
                id
            } else {
                let Ok(w) = c.workspace_manager.active() else {
                    return;
                };
                let size = Self::minimal_widget_size(&c.widget_manager, "file-manager");
                match c
                    .widget_manager
                    .create(CreateWidgetRequest {
                        type_id: "file-manager".into(),
                        workspace_id: w.id,
                        position: None,
                        size: Some(size),
                        initial_lifecycle: None,
                        config_bytes: None,
                    })
                    .await
                {
                    Ok(id) => id,
                    Err(e) => {
                        warn!(?e, "create file manager for image folder");
                        return;
                    }
                }
            };
            if let Err(e) = orchid_widgets::builtin::file_manager::navigate(fm, pane, folder).await
            {
                warn!(?e, "navigate file manager to image folder");
            }
            c.set_fm_focus(fm, pane);
            c.schedule_rebuild();
        });
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
            let state = *iref.lifecycle.read();
            if state == LifecycleState::Sleeping || state == LifecycleState::Unloaded {
                let wm = self.widget_manager.clone();
                spawn::spawn_local_compat(async move {
                    if let Err(e) = wm.change_lifecycle(inst, LifecycleState::Active).await {
                        warn!(?e, %inst, "fm wake to Active failed");
                    }
                });
            }
        }
    }

    fn fm_patch_or_rebuild(self: &Arc<Self>, inst: Uuid) {
        if !self.try_patch_file_manager_instance(inst) {
            self.schedule_rebuild();
        }
    }

    pub(super) async fn fm_refresh_ui(self: &Arc<Self>, inst: Uuid) {
        let _ = self.widget_manager.refresh_snapshot_cache(inst).await;
        // Prefer nested-model mutation. Replacing the workspace frame remounts
        // FileManagerView / TouchAreas and is what made selection jump.
        if self.try_patch_file_manager_instance(inst) {
            return;
        }
        if let Err(e) = self.patch_workspace_frames(&[inst]) {
            warn!(?e, "fm_refresh_ui patch");
            self.schedule_rebuild();
        }
    }

    fn reset_fm_pane_viewport(&self, inst: Uuid, pane: u8) {
        orchid_widgets::builtin::file_manager::reset_viewport_window(inst, pane);
        self.fm_viewport.lock().insert(
            (inst, pane),
            FmViewport {
                scroll_y: 0.0,
                view_h: 480.0,
                view_w: 640.0,
            },
        );
        self.fm_viewport_window.lock().remove(&(inst, pane));
        self.fm_viewport_pin_top.lock().insert((inst, pane));
    }

    /// Update selection highlighting without rebuilding the entry model / snapshot.
    pub(super) fn fm_refresh_selection_ui(self: &Arc<Self>, inst: Uuid, pane: u8) {
        if self.try_patch_fm_selection(inst, pane) {
            // Selecting only mutates widget state, so the cached snapshot still
            // carries the old highlight. Sync the cache quietly — a dirty frame
            // would re-patch from the snapshot on the next tick and race the
            // live rubber band.
            let wm = self.widget_manager.clone();
            spawn::spawn_local_compat(async move {
                let _ = wm.sync_snapshot_cache(inst).await;
            });
            return;
        }
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(super) fn try_patch_fm_selection(&self, inst: Uuid, pane: u8) -> bool {
        use std::collections::HashSet;
        let selected: HashSet<String> =
            orchid_widgets::builtin::file_manager::selected_entries(inst, pane)
                .into_iter()
                .map(|(p, _)| p)
                .collect();
        let Some((selection_count, item_count, selection_bytes)) =
            orchid_widgets::builtin::file_manager::selection_counts(inst, pane)
        else {
            return false;
        };
        let needle = inst.to_string();
        for model in [&self.workspace_widgets, &self.workspace_floating_widgets] {
            let Some(v) = model
                .as_any()
                .downcast_ref::<VecModel<crate::slint_generated::WidgetFrameModel>>()
            else {
                continue;
            };
            for r in 0..v.row_count() {
                let Some(mut row) = v.row_data(r) else {
                    continue;
                };
                if row.instance_id.as_str() != needle.as_str() {
                    continue;
                }
                // Patch nested VecModels in place — do not set_row_data on the
                // workspace frame (that remounts FileManagerView).
                return patch_fm_selection(
                    &mut row.file_manager,
                    pane,
                    &selected,
                    selection_count,
                    item_count,
                    selection_bytes,
                    &self.locale,
                );
            }
        }
        false
    }

    pub(super) fn widget_bounds_at_canvas_point(
        &self,
        content_x: f32,
        content_y: f32,
        type_id: &str,
    ) -> Option<(Uuid, orchid_widgets::PixelBounds)> {
        let w = self.workspace_manager.active().ok()?;
        let (vw, vh) = *self.canvas_size.lock();
        let (sx, sy) = *self.canvas_scroll.lock();
        let all = self.widget_manager.instances_for_workspace(w.id);
        let off = self.drag_offset.lock();

        // Floating windows sit above the canvas; hit-test them first (viewport → content).
        {
            let stack = self.floating_z_stack.lock().clone();
            for id in stack.iter().rev() {
                let Ok(inst) = self.widget_manager.get_instance(*id) else {
                    continue;
                };
                if inst.type_id != type_id {
                    continue;
                }
                let Some(mut b) = inst.floating_bounds() else {
                    continue;
                };
                if let Some((dx, dy)) = off.get(id) {
                    b.x += dx;
                    b.y += dy;
                }
                // Convert viewport-relative floating bounds to content space.
                let content_bounds = orchid_widgets::PixelBounds {
                    x: b.x + sx,
                    y: b.y + sy,
                    width: b.width,
                    height: b.height,
                };
                if content_x >= content_bounds.x
                    && content_y >= content_bounds.y
                    && content_x < content_bounds.x + content_bounds.width
                    && content_y < content_bounds.y + content_bounds.height
                {
                    return Some((*id, content_bounds));
                }
            }
        }

        let instances = Self::docked_instances(&all);
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

    pub(super) fn fm_selected_paths(&self, inst: Uuid, pane: u8) -> Vec<String> {
        self.fm_selected_entries(inst, pane)
            .into_iter()
            .map(|(path, _)| path)
            .collect()
    }

    pub(super) fn fm_selected_entries(&self, inst: Uuid, pane: u8) -> Vec<(String, bool)> {
        orchid_widgets::builtin::file_manager::selected_entries(inst, pane)
    }

    fn fm_selected_folder(&self, inst: Uuid, pane: u8) -> Option<orchid_fs::FsPath> {
        let (path, is_dir) = self.fm_selected_entries(inst, pane).into_iter().next()?;
        let fp = orchid_fs::FsPath::new(&path).ok()?;
        if is_dir {
            Some(fp)
        } else {
            fp.parent()
        }
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

    pub(super) fn spawn_fm_action(
        self: &Arc<Self>,
        inst: Uuid,
        action_id: &str,
        paths: Vec<String>,
    ) {
        let tw = Arc::downgrade(self);
        let action_id = action_id.to_string();
        spawn::spawn_local_compat(async move {
            let outcome =
                match orchid_widgets::builtin::file_manager::run_action(inst, &action_id, paths)
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
                if matches!(
                    action_id.as_str(),
                    "fs.copy" | "fs.cut" | "fs.copy-to-other" | "fs.move-to-other"
                ) {
                    if let Some(clip) = orchid_widgets::builtin::file_manager::file_clipboard(inst)
                    {
                        super::system_file_clipboard::push_file_clipboard(&clip);
                    }
                }
                c.apply_fm_action_outcome(inst, outcome);
            }
        });
    }
}

pub(super) fn default_fm_overlays() -> FileManagerOverlays {
    empty_fm_overlays()
}

pub(super) fn parse_select_filter_commit(
    raw: &str,
) -> orchid_widgets::builtin::file_manager::SelectFilter {
    use orchid_widgets::builtin::file_manager::{parse_byte_size, SelectFilter};
    if !raw.contains('\n') {
        return SelectFilter::name_mask(raw);
    }
    let mut parts = raw.split('\n');
    let pattern = parts.next().unwrap_or("*").to_string();
    let files = parts.next() != Some("0");
    let folders = parts.next() != Some("0");
    let min_size = parts.next().and_then(parse_byte_size);
    let max_size = parts.next().and_then(parse_byte_size);
    let hidden = parts.next() == Some("1");
    let readonly = parts.next() == Some("1");
    let newer_than_days = parts
        .next()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .filter(|d| *d > 0);
    SelectFilter {
        pattern,
        files,
        folders,
        min_size,
        max_size,
        hidden,
        readonly,
        newer_than_days,
    }
}
