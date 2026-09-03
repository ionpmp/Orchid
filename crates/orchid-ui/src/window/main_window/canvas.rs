//! Widget canvas: drag, resize, group, and close handlers.

use std::sync::Arc;

use slint::ComponentHandle;
use slint::Model;
use slint::SharedString;
use slint::VecModel;
use tracing::warn;
use uuid::Uuid;

use orchid_storage::WidgetSize;
use orchid_widgets::layout::PixelBounds;
use orchid_widgets::layout::ViewportSize;
use orchid_widgets::WidgetPayload;

use crate::error::{Result, UiError};
use crate::slint_generated::{AppState, WidgetCloseConfirmDialog, WidgetFrameModel};
use crate::window::spawn;

use super::{empty_close_confirm_dialog, MainWindowController};

pub(super) struct ResizeInteraction {
    instance_id: Uuid,
    corner: String,
    start: PixelBounds,
    press_canvas: (f32, f32),
}

impl MainWindowController {
    pub(super) fn on_widget_close(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        if self.viewer_text_unsaved(u) {
            self.show_viewer_unsaved_close_confirm(u);
            return;
        }
        self.finish_widget_close(u);
    }

    pub(super) fn on_widget_close_confirm_save(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = orchid_widgets::builtin::viewer::text_save(u).await {
                warn!(?e, "viewer text save on close");
                if let Some(c) = t.upgrade() {
                    c.show_viewer_unsaved_close_confirm(u);
                }
                return;
            }
            if let Some(c) = t.upgrade() {
                c.clear_close_confirm_overlay(u);
                c.finish_widget_close(u);
            }
        });
    }

    pub(super) fn on_widget_close_confirm_discard(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        self.clear_close_confirm_overlay(u);
        self.finish_widget_close(u);
    }

    pub(super) fn on_widget_close_confirm_cancel(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        self.clear_close_confirm_overlay(u);
    }

    pub(super) fn viewer_text_unsaved(&self, id: Uuid) -> bool {
        let Ok(iref) = self.widget_manager.get_instance(id) else {
            return false;
        };
        if iref.type_id != orchid_widgets::builtin::viewer::TYPE_ID {
            return false;
        }
        let cache = self.widget_manager.snapshot_cache();
        let Some(ws) = cache.get(id) else {
            return false;
        };
        let WidgetPayload::Viewer(v) = &ws.payload else {
            return false;
        };
        matches!(
            &v.snapshot,
            orchid_viewers::ViewerSnapshot::Text(s) if !s.read_only && s.dirty
        ) || matches!(
            &v.snapshot,
            orchid_viewers::ViewerSnapshot::Document(s) if s.dirty
        )
    }

    pub(super) fn show_viewer_unsaved_close_confirm(&self, id: Uuid) {
        let dlg = WidgetCloseConfirmDialog {
            visible: true,
            title: self.locale.tr("viewer-text-unsaved-title").into(),
            message: self.locale.tr("viewer-text-unsaved-body").into(),
            save_label: self.locale.tr("viewer-text-save").into(),
            discard_label: self.locale.tr("viewer-text-discard").into(),
            cancel_label: self.locale.tr("fm-rename-cancel").into(),
        };
        self.close_confirm_overlays.write().insert(id, dlg);
        self.patch_frame_close_confirm(id);
    }

    pub(super) fn clear_close_confirm_overlay(self: &Arc<Self>, id: Uuid) {
        self.close_confirm_overlays.write().remove(&id);
        self.patch_frame_close_confirm(id);
    }

    pub(super) fn patch_frame_close_confirm(&self, id: Uuid) {
        let dlg = self
            .close_confirm_overlays
            .read()
            .get(&id)
            .cloned()
            .unwrap_or_else(empty_close_confirm_dialog);
        let needle = id.to_string();
        for model in [&self.workspace_widgets, &self.workspace_floating_widgets] {
            let Some(v) = model.as_any().downcast_ref::<VecModel<WidgetFrameModel>>() else {
                continue;
            };
            for r in 0..v.row_count() {
                let Some(mut row) = v.row_data(r) else {
                    continue;
                };
                if row.instance_id.as_str() == needle.as_str() {
                    row.close_confirm = dlg.clone();
                    v.set_row_data(r, row);
                    break;
                }
            }
        }
    }

    pub(super) fn finish_widget_close(self: &Arc<Self>, u: Uuid) {
        self.close_confirm_overlays.write().remove(&u);
        self.settings_dialog_overlays.write().remove(&u);
        let wm = self.widget_manager.clone();
        let gm = self.group_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Some(group) = gm.find_for_instance(u) {
                if group.members.len() <= 2 {
                    if let Ok(released) = gm.dissolve_group(group.id).await {
                        for mid in released {
                            if let Ok(inst) = wm.get_instance(mid) {
                                *inst.group_id.write() = None;
                            }
                        }
                    }
                } else if let Err(e) = gm.remove_from_group(group.id, u).await {
                    warn!(?e, "group remove on close");
                }
            }
            if let Err(e) = wm.close(u).await {
                warn!(?e, "close");
            }
            if let Some(c) = t.upgrade() {
                if c.fm_focus.lock().is_some_and(|(fm_id, _)| fm_id == u) {
                    *c.fm_focus.lock() = None;
                }
                c.fm_overlays.write().remove(&u);
                c.close_confirm_overlays.write().remove(&u);
                c.drag_offset.lock().remove(&u);
                c.drag_start_bounds.lock().remove(&u);
                c.drag_grab.lock().remove(&u);
                c.resize_override.lock().remove(&u);
                c.search_selection.write().remove(&u);
                c.password_toasts.write().remove(&u);
                c.password_autofocus_pending.write().remove(&u);
                c.password_add_dialogs.write().remove(&u);
                c.floating_z_stack.lock().retain(|id| *id != u);
                c.schedule_rebuild();
            }
        });
    }

    pub(super) fn on_widget_activate(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        self.bring_floating_to_front(u);
    }

    pub(super) fn on_widget_drag_started(
        self: &Arc<Self>,
        id: &SharedString,
        grab_lx: f32,
        grab_ly: f32,
    ) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        self.drag_grab.lock().insert(u, (grab_lx, grab_ly));
        if self.is_floating_window(u) {
            if let Ok(inst) = self.widget_manager.get_instance(u) {
                if let Some(b) = inst.floating_bounds() {
                    self.drag_start_bounds.lock().insert(u, b);
                    self.raise_floating(u);
                }
            }
            return;
        }
        if let (Ok(w), Ok(_)) = (
            self.workspace_manager.active(),
            self.widget_manager.get_instance(u),
        ) {
            let inst = Self::docked_instances(&self.widget_manager.instances_for_workspace(w.id));
            let (vw, vh) = *self.canvas_size.lock();
            self.layout_engine.grow_grid_to_fit_instances(w.id, &inst);
            for pl in self
                .layout_engine
                .snapshot(
                    w.id,
                    &inst,
                    ViewportSize {
                        width_px: vw,
                        height_px: vh,
                    },
                )
                .cells
            {
                if pl.instance_id == u {
                    self.drag_start_bounds.lock().insert(u, pl.bounds);
                    return;
                }
            }
        }
    }

    pub(super) fn on_widget_drag_moved(
        self: &Arc<Self>,
        id: &SharedString,
        canvas_x: f32,
        canvas_y: f32,
    ) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        self.apply_drag_frame_preview(u, canvas_x, canvas_y);
    }

    fn frame_model_for_instance(&self, instance: Uuid) -> Option<&VecModel<WidgetFrameModel>> {
        let floating = self.is_floating_window(instance);
        let model = if floating {
            &self.workspace_floating_widgets
        } else {
            &self.workspace_widgets
        };
        model.as_any().downcast_ref::<VecModel<WidgetFrameModel>>()
    }

    /// O(1) update of the dragged widget's `x`/`y` in the Slint model (no full rebuild).
    pub(super) fn apply_drag_frame_preview(
        self: &Arc<Self>,
        instance: Uuid,
        canvas_x: f32,
        canvas_y: f32,
    ) {
        let Some((gx, gy)) = self.drag_grab.lock().get(&instance).copied() else {
            self.schedule_rebuild();
            return;
        };
        let Some(start) = self.drag_start_bounds.lock().get(&instance).copied() else {
            self.schedule_rebuild();
            return;
        };
        let fx = canvas_x - gx;
        let fy = canvas_y - gy;
        *self
            .drag_offset
            .lock()
            .entry(instance)
            .or_insert((0.0, 0.0)) = (fx - start.x, fy - start.y);

        let floating = self.is_floating_window(instance);
        let (snap_bounds, placement_valid) = if floating {
            self.floating_drag_snap_preview(instance, fx, fy)
        } else {
            self.drag_snap_preview(instance, fx, fy)
        };

        let Some(v) = self.frame_model_for_instance(instance) else {
            self.schedule_rebuild();
            return;
        };
        let needle = instance.to_string();
        for r in 0..v.row_count() {
            let Some(mut row) = v.row_data(r) else {
                continue;
            };
            if row.instance_id.as_str() != needle.as_str() {
                continue;
            }
            row.x = fx;
            row.y = fy;
            row.z_order = 10_000;
            row.placement_valid = placement_valid;
            if let Some(sb) = snap_bounds {
                row.snap_visible = true;
                row.snap_x = sb.x;
                row.snap_y = sb.y;
                row.snap_width = sb.width;
                row.snap_height = sb.height;
            } else {
                row.snap_visible = false;
            }
            v.set_row_data(r, row);
            if !floating {
                self.sync_canvas_scroll_extent();
            }
            return;
        }
        self.schedule_rebuild();
    }

    /// Snapped cell bounds + whether that placement is free of collisions.
    pub(super) fn drag_snap_preview(
        self: &Arc<Self>,
        instance: Uuid,
        top_left_x: f32,
        top_left_y: f32,
    ) -> (Option<PixelBounds>, bool) {
        let Ok(w) = self.workspace_manager.active() else {
            return (None, true);
        };
        let Ok(inst) = self.widget_manager.get_instance(instance) else {
            return (None, true);
        };
        let size = *inst.size.read();
        let (vw, vh) = *self.canvas_size.lock();
        let viewport = ViewportSize {
            width_px: vw,
            height_px: vh,
        };
        let le = &self.layout_engine;
        let pos = le.placement_from_content_top_left(viewport, top_left_x, top_left_y, size);
        let (pos, size) = le.snap(pos, size);
        let all = Self::docked_instances(&self.widget_manager.instances_for_workspace(w.id));
        let valid = le.can_place(w.id, instance, pos, size, &all).is_ok();
        let bounds = le.pixel_bounds_for(pos, size, viewport);
        (Some(bounds), valid)
    }

    /// Floating drag: pointer is viewport-relative; snap ghost also viewport-relative.
    pub(super) fn floating_drag_snap_preview(
        self: &Arc<Self>,
        instance: Uuid,
        viewport_x: f32,
        viewport_y: f32,
    ) -> (Option<PixelBounds>, bool) {
        let Ok(w) = self.workspace_manager.active() else {
            *self.snap_zone.lock() = None;
            return (None, true);
        };
        let Ok(inst) = self.widget_manager.get_instance(instance) else {
            *self.snap_zone.lock() = None;
            return (None, true);
        };
        let size = *inst.size.read();
        let (vw, vh) = *self.canvas_size.lock();
        let (sx, sy) = *self.canvas_scroll.lock();
        let viewport = ViewportSize {
            width_px: vw,
            height_px: vh,
        };
        let content_x = viewport_x + sx;
        let content_y = viewport_y + sy;
        let le = &self.layout_engine;
        let pos = le.placement_from_content_top_left(viewport, content_x, content_y, size);
        let (pos, size) = le.snap(pos, size);
        let all = Self::docked_instances(&self.widget_manager.instances_for_workspace(w.id));
        let valid = le.can_place(w.id, instance, pos, size, &all).is_ok();
        if valid {
            *self.snap_zone.lock() = None;
            let content_bounds = le.pixel_bounds_for(pos, size, viewport);
            let viewport_bounds = PixelBounds {
                x: content_bounds.x - sx,
                y: content_bounds.y - sy,
                width: content_bounds.width,
                height: content_bounds.height,
            };
            return (Some(viewport_bounds), true);
        }
        // Edge snap when not over a free grid cell.
        let cx = viewport_x
            + self
                .drag_start_bounds
                .lock()
                .get(&instance)
                .map(|b| b.width * 0.5)
                .unwrap_or(0.0);
        if let Some(snap) = self.snap_bounds_for_pointer(cx, viewport_y) {
            *self.snap_zone.lock() = Some(snap.bounds);
            self.patch_workspace_snap_zone();
            return (Some(snap.bounds), true);
        }
        *self.snap_zone.lock() = None;
        self.patch_workspace_snap_zone();
        (None, true)
    }

    pub(super) fn on_widget_drag_ended(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        // Keep drag offset in place until the async path commits (or bails) so
        // `rebuild` during a failed can_place still shows the pre-commit drag, not
        // a one-frame jump with stale math.
        let (off, start) = {
            let doff = self.drag_offset.lock();
            let ds = self.drag_start_bounds.lock();
            (doff.get(&u).copied(), ds.get(&u).copied())
        };

        if self.is_floating_window(u) {
            match (off, start) {
                (Some(o), Some(s)) if o.0.abs() > 0.5 || o.1.abs() > 0.5 => {
                    self.finish_floating_drag(u, s, o);
                }
                _ => {
                    // Title-bar click (no drag): clear grab and bring to front.
                    self.drag_offset.lock().remove(&u);
                    self.drag_start_bounds.lock().remove(&u);
                    self.drag_grab.lock().remove(&u);
                    self.bring_floating_to_front(u);
                }
            }
            return;
        }

        let (off, start) = match (off, start) {
            (Some(o), Some(s)) => (o, s),
            _ => return,
        };

        let wm = self.widget_manager.clone();
        let le = self.layout_engine.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local(async move {
            let end_drag = |c: &Arc<MainWindowController>| {
                c.drag_offset.lock().remove(&u);
                c.drag_start_bounds.lock().remove(&u);
                c.drag_grab.lock().remove(&u);
            };
            let Some(c) = t.upgrade() else {
                return;
            };
            let w = match c.workspace_manager.active() {
                Ok(w) => w,
                Err(_) => {
                    if let Some(c) = t.upgrade() {
                        end_drag(&c);
                        c.schedule_rebuild();
                    }
                    return;
                }
            };
            let (vw, vh) = *c.canvas_size.lock();
            let new_x = start.x + off.0;
            let new_y = start.y + off.1;
            let inst = match wm.get_instance(u) {
                Ok(i) => i,
                Err(_) => {
                    if let Some(c) = t.upgrade() {
                        end_drag(&c);
                        c.schedule_rebuild();
                    }
                    return;
                }
            };
            let size = *inst.size.read();
            let viewport = ViewportSize {
                width_px: vw,
                height_px: vh,
            };
            let pos = le.placement_from_content_top_left(viewport, new_x, new_y, size);
            let all = Self::docked_instances(&c.widget_manager.instances_for_workspace(w.id));

            // Drop onto another widget's header → form / join a group.
            if let Some(target_id) = c.find_group_drop_target(u, new_x, new_y, start.width) {
                if let Err(e) = c.form_or_join_group(w.id, u, target_id, pos, size).await {
                    warn!(?e, "group form");
                }
                if let Some(c) = t.upgrade() {
                    end_drag(&c);
                    c.schedule_rebuild();
                }
                return;
            }

            if le.can_place(w.id, u, pos, size, &all).is_err() {
                if let Some(c) = t.upgrade() {
                    end_drag(&c);
                    c.push_notification(
                        &c.locale.tr("workspace-placement-blocked-title"),
                        &c.locale.tr("workspace-placement-blocked-body"),
                        2,
                    );
                    c.schedule_rebuild();
                }
                return;
            }
            let (pos, _) = le.snap(pos, size);

            // Alt+drop away from another header → detach this member from its group.
            let alt_detach = c
                .keyboard_modifiers
                .lock()
                .contains(slint::winit_030::winit::keyboard::ModifiersState::ALT);
            if alt_detach {
                if let Some(group) = c.group_manager.find_for_instance(u) {
                    if group.members.len() >= 2 {
                        if group.members.len() <= 2 {
                            let _ = c.dissolve_group_internal(group.id).await;
                        } else {
                            let _ = c.group_manager.remove_from_group(group.id, u).await;
                            if let Ok(inst) = wm.get_instance(u) {
                                *inst.group_id.write() = None;
                            }
                        }
                        if let Err(e) = wm.move_to(u, pos).await {
                            warn!(?e, "move after ungroup");
                        }
                        if let Some(c) = t.upgrade() {
                            end_drag(&c);
                            c.schedule_rebuild();
                        }
                        return;
                    }
                }
            }

            if let Err(e) = wm.move_to(u, pos).await {
                warn!(?e, "move");
            }
            // Keep group slot + sibling members aligned when dragging the active tab.
            if let Some(group) = c.group_manager.find_for_instance(u) {
                if group.active_instance() == Some(u) {
                    let _ = c.group_manager.update_slot(group.id, pos, size).await;
                    for mid in &group.members {
                        if *mid != u {
                            let _ = wm.move_and_resize(*mid, pos, size).await;
                        }
                    }
                }
            }
            if let Some(c) = t.upgrade() {
                end_drag(&c);
                c.schedule_rebuild();
            }
        });
    }

    fn finish_floating_drag(self: &Arc<Self>, u: Uuid, start: PixelBounds, off: (f32, f32)) {
        let new_x = start.x + off.0;
        let new_y = start.y + off.1;
        let wm = self.widget_manager.clone();
        let le = self.layout_engine.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local(async move {
            let end_drag = |c: &Arc<MainWindowController>| {
                c.drag_offset.lock().remove(&u);
                c.drag_start_bounds.lock().remove(&u);
                c.drag_grab.lock().remove(&u);
            };
            let Some(c) = t.upgrade() else {
                return;
            };
            let w = match c.workspace_manager.active() {
                Ok(w) => w,
                Err(_) => {
                    end_drag(&c);
                    c.schedule_rebuild();
                    return;
                }
            };
            let Ok(inst) = wm.get_instance(u) else {
                end_drag(&c);
                c.schedule_rebuild();
                return;
            };
            let size = *inst.size.read();
            let (vw, vh) = *c.canvas_size.lock();
            let (sx, sy) = *c.canvas_scroll.lock();
            let viewport = ViewportSize {
                width_px: vw,
                height_px: vh,
            };
            let content_x = new_x + sx;
            let content_y = new_y + sy;
            let pos = le.placement_from_content_top_left(viewport, content_x, content_y, size);
            let (pos, size) = le.snap(pos, size);
            let all = Self::docked_instances(&c.widget_manager.instances_for_workspace(w.id));
            if le.can_place(w.id, u, pos, size, &all).is_ok() {
                if let Err(e) = wm.move_and_resize(u, pos, size).await {
                    warn!(?e, "floating dock move/resize");
                }
                if let Err(e) = wm.dock_to_grid(u).await {
                    warn!(?e, "floating dock placement");
                }
                c.floating_z_stack.lock().retain(|id| *id != u);
                *c.snap_zone.lock() = None;
                end_drag(&c);
                c.schedule_rebuild();
                return;
            }
            // Snap zones when not docking to a free grid cell.
            if let Some(snap) = c.snap_bounds_for_pointer(new_x + start.width * 0.5, new_y) {
                if let Err(e) = match snap.kind {
                    SnapKind::Maximize => wm.maximize_window(u, c.maximized_window_bounds()).await,
                    SnapKind::LeftHalf | SnapKind::RightHalf => {
                        wm.set_floating_bounds(u, snap.bounds).await
                    }
                } {
                    warn!(?e, "floating snap apply");
                }
                *c.snap_zone.lock() = None;
                end_drag(&c);
                c.schedule_rebuild();
                return;
            }
            // Stay floating at the drop position.
            let dropped = PixelBounds {
                x: new_x,
                y: new_y,
                width: start.width,
                height: start.height,
            };
            if let Err(e) = wm.set_floating_bounds(u, dropped).await {
                warn!(?e, "floating move");
            }
            *c.snap_zone.lock() = None;
            end_drag(&c);
            c.schedule_rebuild();
        });
    }

    /// Header hit-test: pointer near another frame's title bar → group drop target.
    pub(super) fn find_group_drop_target(
        self: &Arc<Self>,
        dragged: Uuid,
        drop_x: f32,
        drop_y: f32,
        dragged_width: f32,
    ) -> Option<Uuid> {
        let Ok(w) = self.workspace_manager.active() else {
            return None;
        };
        let (vw, vh) = *self.canvas_size.lock();
        let instances = Self::docked_instances(&self.widget_manager.instances_for_workspace(w.id));
        let snap = self.layout_engine.snapshot(
            w.id,
            &instances,
            ViewportSize {
                width_px: vw,
                height_px: vh,
            },
        );
        let cx = drop_x + dragged_width * 0.5;
        let cy = drop_y + Self::WIDGET_FRAME_HEADER_PX * 0.5;
        for pl in &snap.cells {
            if pl.instance_id == dragged {
                continue;
            }
            // Skip hidden (non-active) group members.
            if let Some(g) = self.group_manager.find_for_instance(pl.instance_id) {
                if g.members.len() >= 2 && g.active_instance() != Some(pl.instance_id) {
                    continue;
                }
            }
            let header = PixelBounds {
                x: pl.bounds.x,
                y: pl.bounds.y,
                width: pl.bounds.width,
                height: Self::WIDGET_FRAME_HEADER_PX,
            };
            if cx >= header.x
                && cx <= header.x + header.width
                && cy >= header.y
                && cy <= header.y + header.height
            {
                return Some(pl.instance_id);
            }
        }
        None
    }

    pub(super) async fn form_or_join_group(
        self: &Arc<Self>,
        workspace_id: Uuid,
        dragged: Uuid,
        target: Uuid,
        pos: orchid_storage::GridPosition,
        size: WidgetSize,
    ) -> Result<()> {
        if dragged == target {
            return Ok(());
        }
        // Already grouped together — nothing to do.
        if let (Some(ga), Some(gb)) = (
            self.group_manager.find_for_instance(dragged),
            self.group_manager.find_for_instance(target),
        ) {
            if ga.id == gb.id {
                return Ok(());
            }
        }

        if let Some(target_group) = self.group_manager.find_for_instance(target) {
            // Leave previous group if any.
            if let Some(prev) = self.group_manager.find_for_instance(dragged) {
                let _ = self.group_manager.remove_from_group(prev.id, dragged).await;
                if let Ok(inst) = self.widget_manager.get_instance(dragged) {
                    *inst.group_id.write() = None;
                }
                if prev.members.len() <= 2 {
                    let _ = self.dissolve_group_internal(prev.id).await;
                }
            }
            self.group_manager
                .add_to_group(target_group.id, dragged)
                .await
                .map_err(|e| UiError::Slint(format!("add to group: {e}")))?;
            if let Ok(inst) = self.widget_manager.get_instance(dragged) {
                *inst.group_id.write() = Some(target_group.id);
            }
            let slot_pos = target_group.position;
            let slot_size = target_group.size;
            let _ = self
                .widget_manager
                .move_and_resize(dragged, slot_pos, slot_size)
                .await;
            let _ = self
                .group_manager
                .switch_active(target_group.id, dragged)
                .await;
            return Ok(());
        }

        // Target is ungrouped — create a new group.
        if let Some(prev) = self.group_manager.find_for_instance(dragged) {
            let _ = self.group_manager.remove_from_group(prev.id, dragged).await;
            if let Ok(inst) = self.widget_manager.get_instance(dragged) {
                *inst.group_id.write() = None;
            }
            if prev.members.len() <= 2 {
                let _ = self.dissolve_group_internal(prev.id).await;
            }
        }
        let target_inst = self
            .widget_manager
            .get_instance(target)
            .map_err(|e| UiError::Slint(format!("{e}")))?;
        let slot_pos = *target_inst.position.read();
        let slot_size = *target_inst.size.read();
        let _ = pos;
        let _ = size;
        let gid = self
            .group_manager
            .create_group(workspace_id, vec![target, dragged], slot_pos, slot_size)
            .await
            .map_err(|e| UiError::Slint(format!("create group: {e}")))?;
        for mid in [target, dragged] {
            if let Ok(inst) = self.widget_manager.get_instance(mid) {
                *inst.group_id.write() = Some(gid);
            }
            let _ = self
                .widget_manager
                .move_and_resize(mid, slot_pos, slot_size)
                .await;
        }
        let _ = self.group_manager.switch_active(gid, dragged).await;
        Ok(())
    }

    pub(super) async fn dissolve_group_internal(self: &Arc<Self>, group_id: Uuid) -> Result<()> {
        let members = self
            .group_manager
            .dissolve_group(group_id)
            .await
            .map_err(|e| UiError::Slint(format!("dissolve group: {e}")))?;
        let Ok(w) = self.workspace_manager.active() else {
            return Ok(());
        };
        for mid in members {
            if let Ok(inst) = self.widget_manager.get_instance(mid) {
                *inst.group_id.write() = None;
            }
            let all = self.widget_manager.instances_for_workspace(w.id);
            if let Ok(inst) = self.widget_manager.get_instance(mid) {
                let size = *inst.size.read();
                if let Ok(pos) = self
                    .layout_engine
                    .auto_place_excluding_with_growth(w.id, size, &all, mid)
                {
                    let _ = self.widget_manager.move_to(mid, pos).await;
                }
            }
        }
        Ok(())
    }

    pub(super) fn on_group_tab_clicked(
        self: &Arc<Self>,
        group_id: &SharedString,
        member_id: &SharedString,
    ) {
        let Ok(gid) = Uuid::parse_str(group_id.as_str()) else {
            return;
        };
        let Ok(mid) = Uuid::parse_str(member_id.as_str()) else {
            return;
        };
        let gm = self.group_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local(async move {
            if let Err(e) = gm.switch_active(gid, mid).await {
                warn!(?e, "group switch_active");
            }
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }

    pub(super) fn on_group_tab_closed(
        self: &Arc<Self>,
        group_id: &SharedString,
        member_id: &SharedString,
    ) {
        let Ok(gid) = Uuid::parse_str(group_id.as_str()) else {
            return;
        };
        let Ok(mid) = Uuid::parse_str(member_id.as_str()) else {
            return;
        };
        let t = Arc::downgrade(self);
        spawn::spawn_local(async move {
            let Some(c) = t.upgrade() else {
                return;
            };
            let Ok(group) = c.group_manager.get(gid) else {
                return;
            };
            if !group.members.contains(&mid) {
                return;
            }
            // Closing a tab removes the member from the stack and re-places it;
            // the widget itself stays open (unlike the frame × which destroys it).
            if group.members.len() <= 2 {
                if let Err(e) = c.dissolve_group_internal(gid).await {
                    warn!(?e, "group dissolve on tab close");
                }
            } else {
                if let Err(e) = c.group_manager.remove_from_group(gid, mid).await {
                    warn!(?e, "group remove_from_group");
                    return;
                }
                if let Ok(inst) = c.widget_manager.get_instance(mid) {
                    *inst.group_id.write() = None;
                }
                if let Ok(w) = c.workspace_manager.active() {
                    let all = c.widget_manager.instances_for_workspace(w.id);
                    if let Ok(inst) = c.widget_manager.get_instance(mid) {
                        let size = *inst.size.read();
                        if let Ok(pos) = c
                            .layout_engine
                            .auto_place_excluding_with_growth(w.id, size, &all, mid)
                        {
                            let _ = c.widget_manager.move_to(mid, pos).await;
                        }
                    }
                }
            }
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }

    pub(super) fn on_group_tab_move(
        self: &Arc<Self>,
        group_id: &SharedString,
        member_id: &SharedString,
        delta: i32,
    ) {
        let Ok(gid) = Uuid::parse_str(group_id.as_str()) else {
            return;
        };
        let Ok(mid) = Uuid::parse_str(member_id.as_str()) else {
            return;
        };
        let gm = self.group_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local(async move {
            let Ok(group) = gm.get(gid) else {
                return;
            };
            let Some(from) = group.members.iter().position(|m| *m == mid) else {
                return;
            };
            let to = from as i32 + delta;
            if to < 0 || to as usize >= group.members.len() {
                return;
            }
            if let Err(e) = gm.reorder_members(gid, from, to as usize).await {
                warn!(?e, "group reorder");
            }
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }

    pub(super) fn on_group_dissolve_clicked(self: &Arc<Self>, group_id: &SharedString) {
        let Ok(gid) = Uuid::parse_str(group_id.as_str()) else {
            return;
        };
        if gid.is_nil() {
            return;
        }
        let t = Arc::downgrade(self);
        spawn::spawn_local(async move {
            let Some(c) = t.upgrade() else {
                return;
            };
            if let Err(e) = c.dissolve_group_internal(gid).await {
                warn!(?e, "group dissolve");
            }
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }

    pub(super) fn on_widget_resize_started(
        self: &Arc<Self>,
        id: &SharedString,
        corner: &SharedString,
        press_x: f32,
        press_y: f32,
    ) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        if self.is_floating_window(u) {
            if let Ok(inst) = self.widget_manager.get_instance(u) {
                if inst.window_state() == Some(orchid_storage::WindowState::Maximized) {
                    return;
                }
                if let Some(b) = inst.floating_bounds() {
                    self.raise_floating(u);
                    *self.resize_state.lock() = Some(ResizeInteraction {
                        instance_id: u,
                        corner: corner.to_string(),
                        start: b,
                        press_canvas: (press_x, press_y),
                    });
                }
            }
            return;
        }
        if let (Ok(w), Ok(_)) = (
            self.workspace_manager.active(),
            self.widget_manager.get_instance(u),
        ) {
            let (vw, vh) = *self.canvas_size.lock();
            let inst = Self::docked_instances(&self.widget_manager.instances_for_workspace(w.id));
            self.layout_engine.grow_grid_to_fit_instances(w.id, &inst);
            for pl in self
                .layout_engine
                .snapshot(
                    w.id,
                    &inst,
                    ViewportSize {
                        width_px: vw,
                        height_px: vh,
                    },
                )
                .cells
            {
                if pl.instance_id == u {
                    *self.resize_state.lock() = Some(ResizeInteraction {
                        instance_id: u,
                        corner: corner.to_string(),
                        start: pl.bounds,
                        press_canvas: (press_x, press_y),
                    });
                    return;
                }
            }
        }
    }

    pub(super) fn on_widget_resize_moved(
        self: &Arc<Self>,
        id: &SharedString,
        canvas_x: f32,
        canvas_y: f32,
    ) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let st = self.resize_state.lock();
        if let Some(s) = st.as_ref() {
            if s.instance_id != u {
                return;
            }
            let dcx = canvas_x - s.press_canvas.0;
            let dcy = canvas_y - s.press_canvas.1;
            let mut b = s.start;
            match s.corner.as_str() {
                "se" => {
                    b.width = (b.width + dcx).max(40.0);
                    b.height = (b.height + dcy).max(40.0);
                }
                "sw" => {
                    b.x += dcx;
                    b.width = (b.width - dcx).max(40.0);
                    b.height = (b.height + dcy).max(40.0);
                }
                "ne" => {
                    b.y += dcy;
                    b.width = (b.width + dcx).max(40.0);
                    b.height = (b.height - dcy).max(40.0);
                }
                "nw" => {
                    b.x += dcx;
                    b.y += dcy;
                    b.width = (b.width - dcx).max(40.0);
                    b.height = (b.height - dcy).max(40.0);
                }
                _ => {}
            }
            drop(st);
            self.resize_override.lock().insert(u, b);
            self.apply_resize_frame_preview(u, b);
        }
    }

    /// O(1) update of a frame's bounds during live resize (no full `rebuild_workspace_model`).
    pub(super) fn apply_resize_frame_preview(self: &Arc<Self>, instance: Uuid, b: PixelBounds) {
        let floating = self.is_floating_window(instance);
        let (snap_bounds, placement_valid) = if floating {
            // Free pixel resize while floating — no grid snap required.
            (None, true)
        } else {
            self.resize_snap_preview(instance, &b)
        };
        let Some(v) = self.frame_model_for_instance(instance) else {
            self.schedule_rebuild();
            return;
        };
        let needle = instance.to_string();
        for r in 0..v.row_count() {
            let Some(mut row) = v.row_data(r) else {
                continue;
            };
            if row.instance_id.as_str() != needle.as_str() {
                continue;
            }
            row.x = b.x;
            row.y = b.y;
            row.width = b.width;
            row.height = b.height;
            row.z_order = 10_000;
            row.placement_valid = placement_valid;
            if let Some(sb) = snap_bounds {
                row.snap_visible = true;
                row.snap_x = sb.x;
                row.snap_y = sb.y;
                row.snap_width = sb.width;
                row.snap_height = sb.height;
            } else {
                row.snap_visible = false;
            }
            v.set_row_data(r, row);
            if !floating {
                self.sync_canvas_scroll_extent();
            }
            return;
        }
        self.schedule_rebuild();
    }

    pub(super) fn resize_snap_preview(
        self: &Arc<Self>,
        instance: Uuid,
        bounds: &PixelBounds,
    ) -> (Option<PixelBounds>, bool) {
        let Ok(w) = self.workspace_manager.active() else {
            return (None, true);
        };
        let (vw, vh) = *self.canvas_size.lock();
        let viewport = ViewportSize {
            width_px: vw,
            height_px: vh,
        };
        let le = &self.layout_engine;
        let (pos, size) = le.placement_from_free_bounds(bounds, viewport);
        let all = Self::docked_instances(&self.widget_manager.instances_for_workspace(w.id));
        let valid = le.can_place(w.id, instance, pos, size, &all).is_ok();
        let snapped = le.pixel_bounds_for(pos, size, viewport);
        (Some(snapped), valid)
    }

    /// Keep the Flickable scroll extent in sync while drag/resize previews move frames
    /// beyond the last committed layout bounds.
    pub(super) fn sync_canvas_scroll_extent(self: &Arc<Self>) {
        let (vw, vh) = *self.canvas_size.lock();
        let mut content_w = vw;
        let mut content_h = vh;
        let Some(v) = self
            .workspace_widgets
            .as_any()
            .downcast_ref::<VecModel<WidgetFrameModel>>()
        else {
            return;
        };
        for r in 0..v.row_count() {
            let Some(row) = v.row_data(r) else {
                continue;
            };
            content_w = content_w.max(row.x + row.width);
            content_h = content_h.max(row.y + row.height);
        }
        let app_g = self.window.global::<AppState>();
        let mut ws = app_g.get_workspace();
        if (ws.canvas_content_width - content_w).abs() > 0.5
            || (ws.canvas_content_height - content_h).abs() > 0.5
        {
            ws.canvas_content_width = content_w;
            ws.canvas_content_height = content_h;
            app_g.set_workspace(ws);
        }
    }

    pub(super) fn on_widget_resize_ended(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let _ = self.resize_state.lock().take();
        let Some(pb) = self.resize_override.lock().remove(&u) else {
            return;
        };

        if self.is_floating_window(u) {
            let wm = self.widget_manager.clone();
            let t = Arc::downgrade(self);
            spawn::spawn_local(async move {
                if let Err(e) = wm.set_floating_bounds(u, pb).await {
                    warn!(?e, "floating resize");
                }
                if let Some(c) = t.upgrade() {
                    c.schedule_rebuild();
                }
            });
            return;
        }

        let wm = self.widget_manager.clone();
        let le = self.layout_engine.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local(async move {
            let Some(c) = t.upgrade() else { return };
            if c.workspace_manager.active().is_err() {
                return;
            }
            let (vw, vh) = *c.canvas_size.lock();
            let viewport = ViewportSize {
                width_px: vw,
                height_px: vh,
            };
            let (new_pos, new_size) = le.placement_from_free_bounds(&pb, viewport);
            let ws_id = match c.workspace_manager.active() {
                Ok(w) => w.id,
                Err(_) => {
                    c.schedule_rebuild();
                    return;
                }
            };
            let all = Self::docked_instances(&c.widget_manager.instances_for_workspace(ws_id));
            if le.can_place(ws_id, u, new_pos, new_size, &all).is_err() {
                c.push_notification(
                    &c.locale.tr("workspace-placement-blocked-title"),
                    &c.locale.tr("workspace-placement-blocked-body"),
                    2,
                );
                c.schedule_rebuild();
                return;
            }
            if let Err(e) = wm.move_and_resize(u, new_pos, new_size).await {
                warn!(?e, "resize");
            }
            if let Some(group) = c.group_manager.find_for_instance(u) {
                if group.active_instance() == Some(u) {
                    let _ = c
                        .group_manager
                        .update_slot(group.id, new_pos, new_size)
                        .await;
                    for mid in &group.members {
                        if *mid != u {
                            let _ = wm.move_and_resize(*mid, new_pos, new_size).await;
                        }
                    }
                }
            }
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }

    /// Dock a floating window onto the canvas grid (auto-place into a free cell).
    pub(super) fn on_widget_dock(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        if !self.is_windowed(u) {
            return;
        }
        let Ok(w) = self.workspace_manager.active() else {
            return;
        };
        let wm = self.widget_manager.clone();
        let le = self.layout_engine.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local(async move {
            let Ok(inst) = wm.get_instance(u) else {
                return;
            };
            if !inst.is_windowed() {
                return;
            }
            // Prefer min size when docking so Large floating viewers fit the grid.
            let preferred = MainWindowController::minimal_widget_size(&wm, &inst.type_id);
            let all = MainWindowController::docked_instances(&wm.instances_for_workspace(w.id));
            let pos = match le.auto_place_excluding_with_growth(w.id, preferred, &all, u) {
                Ok(p) => p,
                Err(e) => {
                    warn!(?e, "dock: no free cell");
                    if let Some(c) = t.upgrade() {
                        c.push_notification(
                            &c.locale.tr("window-dock-failed-title"),
                            &c.locale.tr("window-dock-failed-body"),
                            1,
                        );
                    }
                    return;
                }
            };
            if let Err(e) = wm.move_and_resize(u, pos, preferred).await {
                warn!(?e, "dock: move/resize");
                return;
            }
            if let Err(e) = wm.dock_to_grid(u).await {
                warn!(?e, "dock: dock_to_grid");
                return;
            }
            if let Some(c) = t.upgrade() {
                c.floating_z_stack.lock().retain(|id| *id != u);
                c.schedule_rebuild();
            }
        });
    }

    pub(super) fn on_widget_undock(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        if self.is_windowed(u) {
            return;
        }
        let Ok(w) = self.workspace_manager.active() else {
            return;
        };
        let windowed = self
            .widget_manager
            .instances_for_workspace(w.id)
            .iter()
            .filter(|i| i.is_windowed())
            .count();
        if windowed >= Self::FLOATING_WINDOW_CAP {
            self.push_notification(
                &self.locale.tr("window-float-capped-title"),
                &self.locale.tr_args(
                    "window-float-capped-body",
                    &orchid_i18n::FluentArgs::new()
                        .with("cap", Self::FLOATING_WINDOW_CAP.to_string()),
                ),
                1,
            );
            return;
        }
        // Detach from group before floating.
        if let Some(group) = self.group_manager.find_for_instance(u) {
            let gm = self.group_manager.clone();
            let t = Arc::downgrade(self);
            spawn::spawn_local(async move {
                let _ = gm.remove_from_group(group.id, u).await;
                if let Some(c) = t.upgrade() {
                    c.undock_instance(u);
                }
            });
            return;
        }
        self.undock_instance(u);
    }

    fn undock_instance(self: &Arc<Self>, u: Uuid) {
        let bounds = self.default_floating_bounds();
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local(async move {
            if let Err(e) = wm.undock_to_floating(u, bounds).await {
                warn!(?e, "undock");
                return;
            }
            if let Some(c) = t.upgrade() {
                c.raise_floating(u);
                c.schedule_rebuild();
            }
        });
    }

    pub(super) fn on_widget_minimize(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local(async move {
            if let Err(e) = wm.minimize_window(u).await {
                warn!(?e, "minimize");
                return;
            }
            if let Some(c) = t.upgrade() {
                c.floating_z_stack.lock().retain(|id| *id != u);
                c.schedule_rebuild();
            }
        });
    }

    pub(super) fn on_widget_maximize_toggle(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let max_bounds = self.maximized_window_bounds();
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local(async move {
            if let Err(e) = wm.toggle_maximize_window(u, max_bounds).await {
                warn!(?e, "maximize toggle");
                return;
            }
            if let Some(c) = t.upgrade() {
                c.raise_floating(u);
                c.schedule_rebuild();
            }
        });
    }

    pub(super) fn on_window_taskbar_activate(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local(async move {
            let Ok(inst) = wm.get_instance(u) else {
                return;
            };
            if inst.window_state() == Some(orchid_storage::WindowState::Minimized) {
                if let Err(e) = wm.restore_window(u).await {
                    warn!(?e, "restore from taskbar");
                    return;
                }
            }
            if let Some(c) = t.upgrade() {
                c.raise_floating(u);
                c.schedule_rebuild();
            }
        });
    }

    pub(super) fn cycle_floating_windows(self: &Arc<Self>, reverse: bool) {
        let Ok(w) = self.workspace_manager.active() else {
            return;
        };
        let mut ids: Vec<Uuid> = self
            .widget_manager
            .instances_for_workspace(w.id)
            .into_iter()
            .filter(|i| i.is_windowed())
            .map(|i| i.id)
            .collect();
        if ids.is_empty() {
            return;
        }
        // Prefer z-stack order for visible, then remaining minimized.
        let stack = self.floating_z_stack.lock().clone();
        ids.sort_by_key(|id| {
            stack
                .iter()
                .position(|x| x == id)
                .map(|i| i as i32)
                .unwrap_or(-1)
        });
        let current = stack.last().copied();
        let idx = current
            .and_then(|c| ids.iter().position(|x| *x == c))
            .unwrap_or(0);
        let next = if reverse {
            if idx == 0 {
                ids.len() - 1
            } else {
                idx - 1
            }
        } else {
            (idx + 1) % ids.len()
        };
        let target = ids[next];
        self.on_window_taskbar_activate(&target.to_string().into());
    }

    fn snap_bounds_for_pointer(&self, pointer_x: f32, pointer_y: f32) -> Option<SnapTarget> {
        let (vw, vh) = *self.canvas_size.lock();
        let th = Self::SNAP_EDGE_THRESHOLD_PX;
        let work_h = (vh - Self::WINDOW_TASKBAR_HEIGHT_PX).max(80.0);
        if pointer_y <= th {
            return Some(SnapTarget {
                kind: SnapKind::Maximize,
                bounds: self.maximized_window_bounds(),
            });
        }
        if pointer_x <= th {
            return Some(SnapTarget {
                kind: SnapKind::LeftHalf,
                bounds: PixelBounds {
                    x: 0.0,
                    y: 0.0,
                    width: (vw * 0.5).max(120.0),
                    height: work_h,
                },
            });
        }
        if pointer_x >= vw - th {
            let width = (vw * 0.5).max(120.0);
            return Some(SnapTarget {
                kind: SnapKind::RightHalf,
                bounds: PixelBounds {
                    x: (vw - width).max(0.0),
                    y: 0.0,
                    width,
                    height: work_h,
                },
            });
        }
        None
    }

    fn patch_workspace_snap_zone(&self) {
        let app_g = self.window.global::<crate::slint_generated::AppState>();
        let mut ws = app_g.get_workspace();
        let snap = *self.snap_zone.lock();
        ws.snap_zone_visible = snap.is_some();
        ws.snap_zone_x = snap.map(|b| b.x).unwrap_or(0.0);
        ws.snap_zone_y = snap.map(|b| b.y).unwrap_or(0.0);
        ws.snap_zone_width = snap.map(|b| b.width).unwrap_or(0.0);
        ws.snap_zone_height = snap.map(|b| b.height).unwrap_or(0.0);
        app_g.set_workspace(ws);
    }
}

#[derive(Debug, Clone, Copy)]
enum SnapKind {
    LeftHalf,
    RightHalf,
    Maximize,
}

#[derive(Debug, Clone, Copy)]
struct SnapTarget {
    kind: SnapKind,
    bounds: PixelBounds,
}
