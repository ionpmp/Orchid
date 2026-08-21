//! Floating windows, taskbar, and document/viewer opening logic for [`MainWindowController`].

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};
use std::time::Duration;
use tracing::warn;
use uuid::Uuid;

use orchid_storage::{WidgetSize, WindowState};
use orchid_widgets::layout::{PixelBounds, ViewportSize};
use orchid_widgets::{CreateWidgetRequest, PlacedWidget, SharedInstance};

use crate::error::{Result, UiError};
use crate::slint_generated::{WidgetFrameModel, WindowTaskbarItem};
use crate::window::spawn;

use super::{sync_vec_model, AddWidgetPlacement, MainWindowController};

impl MainWindowController {
    /// Instances that participate in the canvas grid (excludes windowed widgets).
    pub(super) fn docked_instances(instances: &[SharedInstance]) -> Vec<SharedInstance> {
        instances
            .iter()
            .filter(|i| !i.is_windowed())
            .cloned()
            .collect()
    }

    pub(super) fn is_floating_window(&self, id: Uuid) -> bool {
        self.widget_manager
            .get_instance(id)
            .map(|i| i.is_visible_floating())
            .unwrap_or(false)
    }

    pub(super) fn is_windowed(&self, id: Uuid) -> bool {
        self.widget_manager
            .get_instance(id)
            .map(|i| i.is_windowed())
            .unwrap_or(false)
    }

    pub(super) fn sync_floating_z_stack(&self, instances: &[SharedInstance]) {
        let live: HashSet<Uuid> = instances
            .iter()
            .filter(|i| i.is_visible_floating())
            .map(|i| i.id)
            .collect();
        let mut stack = self.floating_z_stack.lock();
        stack.retain(|id| live.contains(id));
        for id in &live {
            if !stack.contains(id) {
                stack.push(*id);
            }
        }
    }

    pub(super) fn raise_floating(&self, id: Uuid) {
        let mut stack = self.floating_z_stack.lock();
        stack.retain(|x| *x != id);
        stack.push(id);
    }

    /// Raise a floating window and refresh the overlay model so paint / hit-test
    /// order matches the stack (Slint `for` order; later = on top).
    pub(crate) fn bring_floating_to_front(self: &Arc<Self>, id: Uuid) {
        if !self.is_floating_window(id) {
            return;
        }
        let already_top = self.floating_z_stack.lock().last().copied() == Some(id);
        self.raise_floating(id);
        if already_top {
            return;
        }
        self.sync_floating_widgets_model();
    }

    /// Viewport bounds for a maximized floating window (above the taskbar).
    pub(super) fn maximized_window_bounds(&self) -> PixelBounds {
        let (vw, vh) = *self.canvas_size.lock();
        PixelBounds {
            x: 0.0,
            y: 0.0,
            width: vw.max(120.0),
            height: (vh - Self::WINDOW_TASKBAR_HEIGHT_PX).max(80.0),
        }
    }

    /// Rebuild only the floating overlay rows (no full workspace rebuild).
    pub(super) fn sync_floating_widgets_model(&self) {
        let Ok(w) = self.workspace_manager.active() else {
            return;
        };
        let all_instances = self.widget_manager.instances_for_workspace(w.id);
        self.sync_floating_z_stack(&all_instances);
        let off = self.drag_offset.lock().clone();
        let ro = self.resize_override.lock().clone();
        let floating_frames = self.build_floating_frames(&all_instances, &off, &ro);
        sync_vec_model(&self.workspace_floating_widgets, floating_frames);
    }

    pub(super) fn default_floating_bounds(&self) -> PixelBounds {
        let (vw, vh) = *self.canvas_size.lock();
        let view = ViewportSize {
            width_px: vw.max(320.0),
            height_px: vh.max(240.0),
        };
        let size = WidgetSize::Large;
        let cell = self.layout_engine.pixel_bounds_for(
            orchid_storage::GridPosition { col: 0, row: 0 },
            size,
            view,
        );
        let width = cell.width.max(320.0);
        let height = cell.height.max(240.0);
        // Stagger so a new floating viewer does not fully cover the previous one.
        let n = self.floating_z_stack.lock().len() as f32;
        let x = ((vw - width) * 0.5 + n * 48.0).max(16.0);
        let y = ((vh - height) * 0.5 + n * 36.0).max(16.0);
        PixelBounds {
            x,
            y,
            width,
            height,
        }
    }

    pub(super) fn build_floating_frames(
        &self,
        _instances: &[SharedInstance],
        off: &HashMap<Uuid, (f32, f32)>,
        ro: &HashMap<Uuid, PixelBounds>,
    ) -> Vec<WidgetFrameModel> {
        let stack = self.floating_z_stack.lock().clone();
        let mut frames = Vec::new();
        for (zi, id) in stack.iter().enumerate() {
            let Ok(iref) = self.widget_manager.get_instance(*id) else {
                continue;
            };
            let Some(bounds0) = iref.floating_bounds() else {
                continue;
            };
            let mut bounds = ro.get(id).copied().unwrap_or(bounds0);
            if let Some(o) = off.get(id) {
                bounds.x += o.0;
                bounds.y += o.1;
            }
            let pl = PlacedWidget {
                instance_id: *id,
                group_id: None,
                bounds,
                z_order: zi as u32,
            };
            let mut frame = self.build_widget_frame_for_placed(&pl, zi as i32, bounds, &iref);
            frame.is_floating = true;
            frame.window_state = match iref.window_state() {
                Some(WindowState::Maximized) => 1,
                Some(WindowState::Minimized) => 2,
                _ => 0,
            };
            frames.push(frame);
        }
        frames
    }

    pub(super) fn build_window_taskbar_items(
        &self,
        instances: &[SharedInstance],
    ) -> Vec<WindowTaskbarItem> {
        let top = self.floating_z_stack.lock().last().copied();
        let mut items = Vec::new();
        for inst in instances {
            if !inst.is_windowed() {
                continue;
            }
            let title = self
                .widget_manager
                .snapshot_cache()
                .get(inst.id)
                .map(|s| s.title.clone())
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| inst.type_id.clone());
            let state = inst.window_state().unwrap_or(WindowState::Normal);
            items.push(WindowTaskbarItem {
                instance_id: inst.id.to_string().into(),
                title: title.into(),
                type_id: inst.type_id.clone().into(),
                is_active: top == Some(inst.id) && state != WindowState::Minimized,
                is_minimized: state == WindowState::Minimized,
            });
        }
        items
    }

    pub(super) fn request_canvas_scroll_to(&self, bounds: PixelBounds) {
        let (vw, vh) = *self.canvas_size.lock();
        let (cur_x, cur_y) = *self.canvas_scroll.lock();
        let mut sx = cur_x;
        let mut sy = cur_y;
        if bounds.x < cur_x {
            sx = bounds.x.max(0.0);
        } else if bounds.x + bounds.width > cur_x + vw {
            sx = (bounds.x + bounds.width - vw).max(0.0);
        }
        if bounds.y < cur_y {
            sy = bounds.y.max(0.0);
        } else if bounds.y + bounds.height > cur_y + vh {
            sy = (bounds.y + bounds.height - vh).max(0.0);
        }
        *self.canvas_scroll.lock() = (sx, sy);
        self.canvas_scroll_gen
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub(super) fn focus_viewer(self: &Arc<Self>, id: Uuid) {
        if let Some(group) = self.group_manager.find_for_instance(id) {
            if group.members.len() >= 2 && group.active_instance() != Some(id) {
                let gm = self.group_manager.clone();
                let t = Arc::downgrade(self);
                spawn::spawn_local(async move {
                    let _ = gm.switch_active(group.id, id).await;
                    if let Some(c) = t.upgrade() {
                        c.focus_viewer_ui(id);
                        c.schedule_rebuild();
                    }
                });
                return;
            }
        }
        self.focus_viewer_ui(id);
        self.schedule_rebuild();
    }

    pub(super) fn focus_viewer_ui(&self, id: Uuid) {
        if self.is_floating_window(id) {
            // `focus_viewer` always follows with `schedule_rebuild`; stack raise is enough here.
            self.raise_floating(id);
            return;
        }
        let Ok(w) = self.workspace_manager.active() else {
            return;
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
        if let Some(pl) = snap.cells.iter().find(|c| c.instance_id == id) {
            self.request_canvas_scroll_to(pl.bounds);
        }
    }

    /// Create a sample `.docx` under [`Self::documents_dir`] and open it docked on the canvas.
    pub(super) fn spawn_open_document_editor(self: &Arc<Self>, placement: AddWidgetPlacement) {
        let ctrl = Arc::downgrade(self);
        let documents_dir = self.documents_dir.clone();
        spawn::spawn_local(async move {
            if let Err(e) = tokio::fs::create_dir_all(&documents_dir).await {
                warn!(?e, "document editor: create documents dir");
                return;
            }
            let path = super::next_untitled_docx_path(&documents_dir);
            if let Err(e) = orchid_viewers::create_sample_docx(&path).await {
                warn!(?e, path = %path.display(), "document editor: write sample docx");
                return;
            }
            let fs_path = match orchid_fs::FsPath::from_local(&path) {
                Ok(p) => p,
                Err(e) => {
                    warn!(?e, path = %path.display(), "document editor: FsPath");
                    return;
                }
            };
            if let Err(e) = Self::open_document_editor_on_canvas(ctrl, fs_path, placement).await {
                warn!(?e, "document editor: open on canvas");
            }
        });
    }

    /// Open a document in a new viewer widget placed on the workspace grid (not floating).
    pub(super) async fn open_document_editor_on_canvas(
        ctrl: Weak<MainWindowController>,
        path: orchid_fs::FsPath,
        placement: AddWidgetPlacement,
    ) -> Result<(Uuid, bool)> {
        let Some(c) = ctrl.upgrade() else {
            return Err(UiError::Slint("controller gone".into()));
        };
        let ws_id = c
            .workspace_manager
            .active()
            .map_err(|e| UiError::Slint(format!("no active workspace: {e}")))?
            .id;

        let viewer_ids: Vec<Uuid> = c
            .widget_manager
            .instances_for_workspace(ws_id)
            .into_iter()
            .filter(|i| i.type_id == orchid_widgets::builtin::viewer::TYPE_ID)
            .map(|i| i.id)
            .collect();

        if let Some(existing) =
            orchid_widgets::builtin::viewer::find_instance_for_path(&viewer_ids, &path)
        {
            c.recent_files.touch(&path, Some(&c.bus));
            c.focus_viewer(existing);
            return Ok((existing, false));
        }

        let size =
            Self::minimal_widget_size(&c.widget_manager, orchid_widgets::builtin::viewer::TYPE_ID);
        let id = c
            .widget_manager
            .create(CreateWidgetRequest {
                type_id: orchid_widgets::builtin::viewer::TYPE_ID.into(),
                workspace_id: ws_id,
                position: None,
                size: Some(size),
                initial_lifecycle: None,
                config_bytes: None,
            })
            .await
            .map_err(|e| UiError::Slint(format!("viewer create: {e}")))?;

        match placement {
            AddWidgetPlacement::AutoSlot => {
                Self::move_new_widget_to_free_slot(&c.layout_engine, &c.widget_manager, ws_id, id)
                    .await;
            }
            AddWidgetPlacement::CanvasPoint {
                content_x,
                content_y,
            } => {
                c.place_widget_at_canvas_point(ws_id, id, size, content_x, content_y)
                    .await;
            }
        }

        orchid_widgets::builtin::viewer::open_path(id, path.clone())
            .await
            .map_err(|e| UiError::Slint(format!("viewer open: {e}")))?;
        c.recent_files.touch(&path, Some(&c.bus));
        if let Some(c2) = ctrl.upgrade() {
            c2.schedule_rebuild();
        }
        Ok((id, true))
    }

    pub(super) async fn open_in_viewer_for_controller(
        ctrl: Weak<MainWindowController>,
        path: orchid_fs::FsPath,
        schedule_rebuild: bool,
        edit: bool,
    ) -> Result<(Uuid, bool)> {
        let Some(c) = ctrl.upgrade() else {
            return Err(UiError::Slint("controller gone".into()));
        };
        let ws_id = c
            .workspace_manager
            .active()
            .map_err(|e| UiError::Slint(format!("no active workspace: {e}")))?
            .id;

        let viewer_ids: Vec<Uuid> = c
            .widget_manager
            .instances_for_workspace(ws_id)
            .into_iter()
            .filter(|i| i.type_id == orchid_widgets::builtin::viewer::TYPE_ID)
            .map(|i| i.id)
            .collect();

        if let Some(existing) =
            orchid_widgets::builtin::viewer::find_instance_for_path(&viewer_ids, &path)
        {
            c.recent_files.touch(&path, Some(&c.bus));
            c.focus_viewer(existing);
            return Ok((existing, false));
        }

        let id = c
            .widget_manager
            .create(CreateWidgetRequest {
                type_id: orchid_widgets::builtin::viewer::TYPE_ID.into(),
                workspace_id: ws_id,
                position: None,
                size: None,
                initial_lifecycle: None,
                config_bytes: None,
            })
            .await
            .map_err(|e| UiError::Slint(format!("viewer create: {e}")))?;

        let bounds = c.default_floating_bounds();
        for _ in 0..50 {
            if c.widget_manager.get_instance(id).is_ok()
                && c.widget_manager
                    .undock_to_floating(id, bounds)
                    .await
                    .is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        c.raise_floating(id);

        let open = if edit {
            orchid_widgets::builtin::viewer::open_path_for_edit(id, path.clone()).await
        } else {
            orchid_widgets::builtin::viewer::open_path(id, path.clone()).await
        };
        open.map_err(|e| UiError::Slint(format!("viewer open: {e}")))?;
        c.recent_files.touch(&path, Some(&c.bus));
        if schedule_rebuild {
            if let Some(c2) = ctrl.upgrade() {
                c2.schedule_rebuild();
            }
        }
        Ok((id, true))
    }
}
