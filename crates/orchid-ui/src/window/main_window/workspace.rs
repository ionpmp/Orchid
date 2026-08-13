//! Workspace model rebuilding and frame patching handlers for [`MainWindowController`].

use slint::{ComponentHandle, Image, Model, ModelRc, SharedString, VecModel};
use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, warn};
use uuid::Uuid;

use orchid_i18n::LocaleManager;
use orchid_storage::{WidgetSize, WindowState};
use orchid_widgets::layout::{PixelBounds, ViewportSize};
use orchid_widgets::{PlacedWidget, SharedInstance, WidgetPayload};

use crate::error::{Result, UiError};
use crate::slint_generated::{
    AppState, CalculatorModel, CalendarModel, ClockModel, FileManagerModel, GroupTabModel,
    JyotishModel, MediaModel, MoonModel, NotesModel, PasswordModel, ProcessesModel,
    RecentFilesModel, RssModel, SearchModel, SettingsFieldRow, SystemModel, TerminalCellModel,
    ViewerModel, WeatherModel, WidgetFrameModel, WorkspaceModel, WorkspaceSummary,
};
use crate::window::models::{
    blank_terminal, build_calculator_model, build_calendar_model, build_clock_model,
    build_file_manager_model, build_jyotish_model, build_media_model, build_moon_model,
    build_notes_model, build_password_model, build_processes_model, build_recent_files_model,
    build_rss_model, build_search_model, build_system_model, build_terminal_divider_models,
    build_terminal_tab_models, build_viewer_model, build_weather_model,
    default_terminal_divider_models, default_terminal_pane_models, default_terminal_tab_models,
    empty_calculator_model, empty_calendar_model, empty_clock_model, empty_confirm_dialog,
    empty_context_menu, empty_file_manager_model, empty_jyotish_model, empty_managed_policy_state,
    empty_media_model, empty_moon_model, empty_notes_model, empty_passphrase_state,
    empty_password_model, empty_processes_confirm, empty_processes_model, empty_recent_files_model,
    empty_rename_state, empty_rss_model, empty_search_model, empty_system_model, empty_tag_state,
    empty_terminal_cells, empty_viewer_model, empty_weather_model, patch_calculator_model,
    patch_clock_model, patch_file_manager_model, patch_media_model, patch_password_model,
    patch_processes_model, patch_recent_files_model, patch_search_model, patch_system_model,
    widget_has_settings, FileManagerOverlays, PasswordAddDialogOverlay,
};

use super::{sync_vec_model, MainWindowController};

impl MainWindowController {
    /// Patch Slint `WidgetFrameModel` rows for instances whose [`WidgetSnapshotCache`] data changed
    /// without a layout canvas / scale / workspace event (e.g. terminal text at ~30Hz).
    pub(super) fn patch_workspace_frames(self: &Arc<Self>, ids: &[Uuid]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        for id in ids {
            self.drain_weather_notice(*id);
            self.drain_clock_notice(*id);
        }
        let unique: HashSet<Uuid> = ids.iter().copied().collect();
        let w = self
            .workspace_manager
            .active()
            .map_err(|e| UiError::Slint(format!("{e}")))?;
        let (vw, vh) = *self.canvas_size.lock();
        let instances = self.widget_manager.instances_for_workspace(w.id);
        self.layout_engine
            .grow_grid_to_fit_instances(w.id, &instances);
        let view = ViewportSize {
            width_px: vw,
            height_px: vh,
        };
        let snap = self.layout_engine.snapshot(w.id, &instances, view);
        let off = self.drag_offset.lock().clone();
        let ro = self.resize_override.lock().clone();
        let v = self
            .workspace_widgets
            .as_any()
            .downcast_ref::<VecModel<WidgetFrameModel>>()
            .expect("workspace widgets must be VecModel-backed");
        let mut need_floating_sync = false;
        for id in &unique {
            // Floating windows live in a separate model; patch content in place
            // when possible, otherwise rebuild the floating overlay.
            if self.is_floating_window(*id) {
                let Ok(iref) = self.widget_manager.get_instance(*id) else {
                    need_floating_sync = true;
                    continue;
                };
                if iref.type_id == orchid_widgets::builtin::processes::TYPE_ID
                    && self.try_patch_processes_row(
                        &self.workspace_floating_widgets,
                        *id,
                        None,
                        None,
                    )
                {
                    continue;
                }
                if iref.type_id == orchid_widgets::builtin::system::TYPE_ID
                    && self.try_patch_system_row(&self.workspace_floating_widgets, *id, None, None)
                {
                    continue;
                }
                if self.try_patch_viewer_row(
                    &self.workspace_floating_widgets,
                    *id,
                    None,
                    None,
                    true,
                ) {
                    continue;
                }
                if iref.type_id == orchid_widgets::builtin::file_manager::TYPE_ID
                    && self.try_patch_file_manager_row(
                        &self.workspace_floating_widgets,
                        *id,
                        None,
                        None,
                    )
                {
                    continue;
                }
                if self.try_patch_common_content_row(
                    &self.workspace_floating_widgets,
                    *id,
                    iref.type_id.as_str(),
                    None,
                    None,
                ) {
                    continue;
                }
                need_floating_sync = true;
                continue;
            }
            let Some((idx, pl)) = snap
                .cells
                .iter()
                .enumerate()
                .find(|(_, c)| c.instance_id == *id)
            else {
                continue;
            };
            let mut bounds = pl.bounds;
            if let Some(o) = off.get(id) {
                bounds.x += o.0;
                bounds.y += o.1;
            }
            if let Some(ov) = ro.get(id) {
                bounds = *ov;
            }
            let Ok(iref) = self.widget_manager.get_instance(*id) else {
                continue;
            };
            if iref.type_id == "terminal" && !ro.contains_key(id) {
                let cw = bounds.width.max(1.0);
                let ch = (bounds.height - Self::WIDGET_FRAME_HEADER_PX - Self::TERMINAL_TAB_BAR_PX)
                    .max(1.0);
                let _ = self.resize_terminal_pty_to_content(*id, cw, ch);
            }
            // Fast paths to update content in place
            if iref.type_id == orchid_widgets::builtin::viewer::TYPE_ID
                && self.try_patch_viewer_row(
                    &self.workspace_widgets,
                    *id,
                    Some(bounds),
                    Some(idx as i32),
                    false,
                )
            {
                continue;
            }
            if iref.type_id == orchid_widgets::builtin::processes::TYPE_ID
                && self.try_patch_processes_row(
                    &self.workspace_widgets,
                    *id,
                    Some(bounds),
                    Some(idx as i32),
                )
            {
                continue;
            }
            if iref.type_id == orchid_widgets::builtin::system::TYPE_ID
                && self.try_patch_system_row(
                    &self.workspace_widgets,
                    *id,
                    Some(bounds),
                    Some(idx as i32),
                )
            {
                continue;
            }
            if iref.type_id == orchid_widgets::builtin::file_manager::TYPE_ID
                && self.try_patch_file_manager_row(
                    &self.workspace_widgets,
                    *id,
                    Some(bounds),
                    Some(idx as i32),
                )
            {
                continue;
            }
            if self.try_patch_common_content_row(
                &self.workspace_widgets,
                *id,
                iref.type_id.as_str(),
                Some(bounds),
                Some(idx as i32),
            ) {
                continue;
            }
            let new_row = self.build_widget_frame_for_placed(pl, idx as i32, bounds, &iref);
            let needle = id.to_string();
            for r in 0..v.row_count() {
                let Some(row) = v.row_data(r) else {
                    continue;
                };
                if row.instance_id.as_str() == needle.as_str() {
                    v.set_row_data(r, new_row);
                    break;
                }
            }
        }
        if need_floating_sync {
            self.sync_floating_widgets_model();
        }
        Ok(())
    }

    /// Patch an existing system frame row without replacing nested ModelRcs.
    pub(super) fn try_patch_system_row(
        &self,
        model: &ModelRc<WidgetFrameModel>,
        id: Uuid,
        bounds: Option<PixelBounds>,
        z_order: Option<i32>,
    ) -> bool {
        let cache = self.widget_manager.snapshot_cache();
        let Some(ws) = cache.get(id) else {
            return false;
        };
        let WidgetPayload::SystemIndicators(p) = &ws.payload else {
            return false;
        };
        let Some(v) = model.as_any().downcast_ref::<VecModel<WidgetFrameModel>>() else {
            return false;
        };
        let needle = id.to_string();
        for r in 0..v.row_count() {
            let Some(mut row) = v.row_data(r) else {
                continue;
            };
            if row.instance_id.as_str() != needle.as_str() {
                continue;
            }
            patch_system_model(&mut row.system, p, &self.locale);
            let mut need_frame = false;
            if let Some(b) = bounds {
                if row.x != b.x || row.y != b.y || row.width != b.width || row.height != b.height {
                    row.x = b.x;
                    row.y = b.y;
                    row.width = b.width;
                    row.height = b.height;
                    need_frame = true;
                }
            }
            if let Some(z) = z_order {
                if row.z_order != z {
                    row.z_order = z;
                    need_frame = true;
                }
            }
            let title: SharedString = ws.title.clone().into();
            if row.title != title {
                row.title = title;
                need_frame = true;
            }
            if need_frame {
                let (group_id, group_tabs) = self.build_group_tab_models(id);
                row.group_id = group_id;
                row.group_tabs = group_tabs;
                v.set_row_data(r, row);
            }
            return true;
        }
        false
    }

    /// Patch an existing processes frame row without rebuilding sibling models.
    pub(super) fn try_patch_processes_row(
        &self,
        model: &ModelRc<WidgetFrameModel>,
        id: Uuid,
        bounds: Option<PixelBounds>,
        z_order: Option<i32>,
    ) -> bool {
        let cache = self.widget_manager.snapshot_cache();
        let Some(ws) = cache.get(id) else {
            return false;
        };
        let WidgetPayload::Processes(p) = &ws.payload else {
            return false;
        };
        let Some(v) = model.as_any().downcast_ref::<VecModel<WidgetFrameModel>>() else {
            return false;
        };
        let needle = id.to_string();
        for r in 0..v.row_count() {
            let Some(mut row) = v.row_data(r) else {
                continue;
            };
            if row.instance_id.as_str() != needle.as_str() {
                continue;
            }
            let (ctx_vis, ctx_x, ctx_y) = self
                .processes_context
                .read()
                .get(&id)
                .copied()
                .unwrap_or((false, 0.0, 0.0));
            let confirm = self
                .processes_confirm
                .read()
                .get(&id)
                .cloned()
                .unwrap_or_else(empty_processes_confirm);
            let patch = patch_processes_model(
                &mut row.processes,
                p,
                &self.locale,
                ctx_vis,
                ctx_x,
                ctx_y,
                confirm,
            );
            let mut need_frame = patch.needs_frame_write;
            if let Some(b) = bounds {
                if row.x != b.x || row.y != b.y || row.width != b.width || row.height != b.height {
                    row.x = b.x;
                    row.y = b.y;
                    row.width = b.width;
                    row.height = b.height;
                    need_frame = true;
                }
            }
            if let Some(z) = z_order {
                if row.z_order != z {
                    row.z_order = z;
                    need_frame = true;
                }
            }
            let title: SharedString = ws.title.clone().into();
            if row.title != title {
                row.title = title;
                need_frame = true;
            }
            let (group_id, group_tabs) = self.build_group_tab_models(id);
            if need_frame {
                row.group_id = group_id;
                row.group_tabs = group_tabs;
                v.set_row_data(r, row);
            }
            return true;
        }
        false
    }

    pub(super) fn try_patch_file_manager_instance(&self, id: Uuid) -> bool {
        if self.is_floating_window(id) {
            self.try_patch_file_manager_row(&self.workspace_floating_widgets, id, None, None)
        } else {
            self.try_patch_file_manager_row(&self.workspace_widgets, id, None, None)
        }
    }

    /// Patch FM listing/chrome without replacing the workspace frame row.
    pub(super) fn try_patch_file_manager_row(
        &self,
        model: &ModelRc<WidgetFrameModel>,
        id: Uuid,
        bounds: Option<PixelBounds>,
        z_order: Option<i32>,
    ) -> bool {
        let cache = self.widget_manager.snapshot_cache();
        let Some(ws) = cache.get(id) else {
            return false;
        };
        let WidgetPayload::FileManager(p) = &ws.payload else {
            return false;
        };
        let Some(v) = model.as_any().downcast_ref::<VecModel<WidgetFrameModel>>() else {
            return false;
        };
        let needle = id.to_string();
        for r in 0..v.row_count() {
            let Some(mut row) = v.row_data(r) else {
                continue;
            };
            if row.instance_id.as_str() != needle.as_str() {
                continue;
            }
            let overlays = self
                .fm_overlays
                .read()
                .get(&id)
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
                    create_item_is_file: false,
                    drag_active: false,
                    drag_paths: Vec::new(),
                    drag_drop_target: String::new(),
                    drag_target_pane: -1,
                });
            let mut need_frame = patch_file_manager_model(
                &mut row.file_manager,
                p,
                overlays,
                id,
                &self.locale,
                false,
                &self.fm_viewport.lock(),
            );
            if let Some(b) = bounds {
                if row.x != b.x || row.y != b.y || row.width != b.width || row.height != b.height {
                    row.x = b.x;
                    row.y = b.y;
                    row.width = b.width;
                    row.height = b.height;
                    need_frame = true;
                }
            }
            if let Some(z) = z_order {
                if row.z_order != z {
                    row.z_order = z;
                    need_frame = true;
                }
            }
            let title: SharedString = ws.title.clone().into();
            if row.title != title {
                row.title = title;
                need_frame = true;
            }
            if need_frame {
                let (group_id, group_tabs) = self.build_group_tab_models(id);
                row.group_id = group_id;
                row.group_tabs = group_tabs;
                v.set_row_data(r, row);
            }
            return true;
        }
        false
    }

    /// Patch an existing viewer frame row without rebuilding empty sibling models.
    pub(super) fn try_patch_viewer_row(
        &self,
        model: &ModelRc<WidgetFrameModel>,
        id: Uuid,
        bounds: Option<PixelBounds>,
        z_order: Option<i32>,
        is_floating: bool,
    ) -> bool {
        let cache = self.widget_manager.snapshot_cache();
        let Some(ws) = cache.get(id) else {
            return false;
        };
        let WidgetPayload::Viewer(vp) = &ws.payload else {
            return false;
        };
        let Some(v) = model.as_any().downcast_ref::<VecModel<WidgetFrameModel>>() else {
            return false;
        };
        let needle = id.to_string();
        for r in 0..v.row_count() {
            let Some(mut row) = v.row_data(r) else {
                continue;
            };
            if row.instance_id.as_str() != needle.as_str() {
                continue;
            }
            if let Some(b) = bounds {
                row.x = b.x;
                row.y = b.y;
                row.width = b.width;
                row.height = b.height;
            }
            if let Some(z) = z_order {
                row.z_order = z;
            }
            row.is_floating = is_floating;
            row.title = ws.title.clone().into();
            row.viewer = build_viewer_model(vp, &self.locale);
            let (group_id, group_tabs) = self.build_group_tab_models(id);
            row.group_id = group_id;
            row.group_tabs = group_tabs;
            v.set_row_data(r, row);
            return true;
        }
        false
    }

    /// In-place content patch for high-frequency widgets (clock / media / password /
    /// search / recent / calculator). Keeps nested `ModelRc` handles alive.
    pub(super) fn try_patch_common_content_row(
        &self,
        model: &ModelRc<WidgetFrameModel>,
        id: Uuid,
        type_id: &str,
        bounds: Option<PixelBounds>,
        z_order: Option<i32>,
    ) -> bool {
        let cache = self.widget_manager.snapshot_cache();
        let Some(ws) = cache.get(id) else {
            return false;
        };
        let Some(v) = model.as_any().downcast_ref::<VecModel<WidgetFrameModel>>() else {
            return false;
        };
        let needle = id.to_string();
        for r in 0..v.row_count() {
            let Some(mut row) = v.row_data(r) else {
                continue;
            };
            if row.instance_id.as_str() != needle.as_str() {
                continue;
            }
            let patched = match (type_id, &ws.payload) {
                (orchid_widgets::builtin::clock::TYPE_ID, WidgetPayload::Clock(p)) => {
                    patch_clock_model(&mut row.clock, p, &self.locale);
                    true
                }
                (orchid_widgets::builtin::media::TYPE_ID, WidgetPayload::MediaPlayer(p)) => {
                    patch_media_model(&mut row.media, p, &self.locale);
                    true
                }
                (orchid_widgets::builtin::password::TYPE_ID, WidgetPayload::PasswordManager(p)) => {
                    let toast = self.password_toasts.read().get(&id).cloned();
                    let autofocus = self
                        .password_autofocus_pending
                        .read()
                        .get(&id)
                        .copied()
                        .unwrap_or(false);
                    if autofocus {
                        self.password_autofocus_pending.write().remove(&id);
                    }
                    let add_dialog = self
                        .password_add_dialogs
                        .read()
                        .get(&id)
                        .cloned()
                        .unwrap_or_default();
                    patch_password_model(
                        &mut row.password,
                        p,
                        toast,
                        autofocus,
                        add_dialog,
                        &self.locale,
                    );
                    true
                }
                (orchid_widgets::builtin::search::TYPE_ID, WidgetPayload::UniversalSearch(p)) => {
                    let selected = self.search_selection.read().get(&id).copied().unwrap_or(-1);
                    let request_autofocus = matches!(
                        *self.search_autofocus_pending.lock(),
                        Some(pending) if pending == id
                    );
                    patch_search_model(
                        &mut row.search,
                        p,
                        &self.locale,
                        selected,
                        request_autofocus,
                    );
                    true
                }
                (orchid_widgets::builtin::recent_files::TYPE_ID, WidgetPayload::RecentFiles(p)) => {
                    patch_recent_files_model(&mut row.recent_files, p);
                    true
                }
                (orchid_widgets::builtin::calculator::TYPE_ID, WidgetPayload::Calculator(p)) => {
                    patch_calculator_model(&mut row.calculator, p, &self.locale);
                    true
                }
                _ => false,
            };
            if !patched {
                return false;
            }
            if let Some(b) = bounds {
                row.x = b.x;
                row.y = b.y;
                row.width = b.width;
                row.height = b.height;
            }
            if let Some(z) = z_order {
                row.z_order = z;
            }
            row.title = ws.title.clone().into();
            let (group_id, group_tabs) = self.build_group_tab_models(id);
            row.group_id = group_id;
            row.group_tabs = group_tabs;
            v.set_row_data(r, row);
            return true;
        }
        false
    }

    pub(super) fn build_widget_frame_for_placed(
        &self,
        pl: &PlacedWidget,
        z_order: i32,
        bounds: PixelBounds,
        iref: &SharedInstance,
    ) -> WidgetFrameModel {
        let type_s: SharedString = iref.type_id.clone().into();
        let cache = self.widget_manager.snapshot_cache();
        let cached = cache.get(pl.instance_id);
        let (
            title,
            tcols,
            trows,
            tcells,
            tpix,
            tcc,
            tcr,
            tcvis,
            weather_model,
            moon_model,
            jyotish_model,
            clock_model,
            system_model,
            processes_model,
            calculator_model,
            notes_model,
            calendar_model,
            rss_model,
            search_model,
            media_model,
            password_model,
            viewer_model,
            recent_files_model,
            file_manager_model,
        ) = if let Some(ws) = cached.as_deref() {
            let tstr: SharedString = ws.title.clone().into();
            match &ws.payload {
                WidgetPayload::Terminal(t) => {
                    let img = self.raster_terminal_payload(t);
                    (
                        tstr,
                        i32::from(t.cols),
                        i32::from(t.rows),
                        empty_terminal_cells(),
                        img,
                        i32::from(t.cursor_col),
                        i32::from(t.cursor_row),
                        t.cursor_visible,
                        empty_weather_model(&self.locale),
                        empty_moon_model(&self.locale),
                        empty_jyotish_model(&self.locale),
                        empty_clock_model(&self.locale),
                        empty_system_model(&self.locale),
                        empty_processes_model(&self.locale),
                        empty_calculator_model(&self.locale),
                        empty_notes_model(&self.locale),
                        empty_calendar_model(&self.locale),
                        empty_rss_model(&self.locale),
                        empty_search_model(&self.locale),
                        empty_media_model(&self.locale),
                        empty_password_model(&self.locale),
                        empty_viewer_model(&self.locale),
                        empty_recent_files_model(&self.locale),
                        empty_file_manager_model(&self.locale),
                    )
                }
                WidgetPayload::Weather(w) => (
                    tstr,
                    80,
                    24,
                    blank_terminal(80, 24),
                    Image::default(),
                    0,
                    0,
                    true,
                    build_weather_model(w, &self.locale),
                    empty_moon_model(&self.locale),
                    empty_jyotish_model(&self.locale),
                    empty_clock_model(&self.locale),
                    empty_system_model(&self.locale),
                    empty_processes_model(&self.locale),
                    empty_calculator_model(&self.locale),
                    empty_notes_model(&self.locale),
                    empty_calendar_model(&self.locale),
                    empty_rss_model(&self.locale),
                    empty_search_model(&self.locale),
                    empty_media_model(&self.locale),
                    empty_password_model(&self.locale),
                    empty_viewer_model(&self.locale),
                    empty_recent_files_model(&self.locale),
                    empty_file_manager_model(&self.locale),
                ),
                WidgetPayload::Moon(m) => (
                    tstr,
                    80,
                    24,
                    blank_terminal(80, 24),
                    Image::default(),
                    0,
                    0,
                    true,
                    empty_weather_model(&self.locale),
                    build_moon_model(m, &self.locale),
                    empty_jyotish_model(&self.locale),
                    empty_clock_model(&self.locale),
                    empty_system_model(&self.locale),
                    empty_processes_model(&self.locale),
                    empty_calculator_model(&self.locale),
                    empty_notes_model(&self.locale),
                    empty_calendar_model(&self.locale),
                    empty_rss_model(&self.locale),
                    empty_search_model(&self.locale),
                    empty_media_model(&self.locale),
                    empty_password_model(&self.locale),
                    empty_viewer_model(&self.locale),
                    empty_recent_files_model(&self.locale),
                    empty_file_manager_model(&self.locale),
                ),
                WidgetPayload::Jyotish(j) => (
                    tstr,
                    80,
                    24,
                    blank_terminal(80, 24),
                    Image::default(),
                    0,
                    0,
                    true,
                    empty_weather_model(&self.locale),
                    empty_moon_model(&self.locale),
                    build_jyotish_model(j.as_ref(), &self.locale),
                    empty_clock_model(&self.locale),
                    empty_system_model(&self.locale),
                    empty_processes_model(&self.locale),
                    empty_calculator_model(&self.locale),
                    empty_notes_model(&self.locale),
                    empty_calendar_model(&self.locale),
                    empty_rss_model(&self.locale),
                    empty_search_model(&self.locale),
                    empty_media_model(&self.locale),
                    empty_password_model(&self.locale),
                    empty_viewer_model(&self.locale),
                    empty_recent_files_model(&self.locale),
                    empty_file_manager_model(&self.locale),
                ),
                WidgetPayload::Clock(c) => (
                    tstr,
                    80,
                    24,
                    blank_terminal(80, 24),
                    Image::default(),
                    0,
                    0,
                    true,
                    empty_weather_model(&self.locale),
                    empty_moon_model(&self.locale),
                    empty_jyotish_model(&self.locale),
                    build_clock_model(c, &self.locale),
                    empty_system_model(&self.locale),
                    empty_processes_model(&self.locale),
                    empty_calculator_model(&self.locale),
                    empty_notes_model(&self.locale),
                    empty_calendar_model(&self.locale),
                    empty_rss_model(&self.locale),
                    empty_search_model(&self.locale),
                    empty_media_model(&self.locale),
                    empty_password_model(&self.locale),
                    empty_viewer_model(&self.locale),
                    empty_recent_files_model(&self.locale),
                    empty_file_manager_model(&self.locale),
                ),
                WidgetPayload::SystemIndicators(s) => (
                    tstr,
                    80,
                    24,
                    blank_terminal(80, 24),
                    Image::default(),
                    0,
                    0,
                    true,
                    empty_weather_model(&self.locale),
                    empty_moon_model(&self.locale),
                    empty_jyotish_model(&self.locale),
                    empty_clock_model(&self.locale),
                    build_system_model(s, &self.locale),
                    empty_processes_model(&self.locale),
                    empty_calculator_model(&self.locale),
                    empty_notes_model(&self.locale),
                    empty_calendar_model(&self.locale),
                    empty_rss_model(&self.locale),
                    empty_search_model(&self.locale),
                    empty_media_model(&self.locale),
                    empty_password_model(&self.locale),
                    empty_viewer_model(&self.locale),
                    empty_recent_files_model(&self.locale),
                    empty_file_manager_model(&self.locale),
                ),
                WidgetPayload::Processes(p) => {
                    let (ctx_vis, ctx_x, ctx_y) = self
                        .processes_context
                        .read()
                        .get(&pl.instance_id)
                        .copied()
                        .unwrap_or((false, 0.0, 0.0));
                    let confirm = self
                        .processes_confirm
                        .read()
                        .get(&pl.instance_id)
                        .cloned()
                        .unwrap_or_else(empty_processes_confirm);
                    (
                        tstr,
                        80,
                        24,
                        blank_terminal(80, 24),
                        Image::default(),
                        0,
                        0,
                        true,
                        empty_weather_model(&self.locale),
                        empty_moon_model(&self.locale),
                        empty_jyotish_model(&self.locale),
                        empty_clock_model(&self.locale),
                        empty_system_model(&self.locale),
                        build_processes_model(p, &self.locale, ctx_vis, ctx_x, ctx_y, confirm),
                        empty_calculator_model(&self.locale),
                        empty_notes_model(&self.locale),
                        empty_calendar_model(&self.locale),
                        empty_rss_model(&self.locale),
                        empty_search_model(&self.locale),
                        empty_media_model(&self.locale),
                        empty_password_model(&self.locale),
                        empty_viewer_model(&self.locale),
                        empty_recent_files_model(&self.locale),
                        empty_file_manager_model(&self.locale),
                    )
                }
                WidgetPayload::Calculator(p) => (
                    tstr,
                    80,
                    24,
                    blank_terminal(80, 24),
                    Image::default(),
                    0,
                    0,
                    true,
                    empty_weather_model(&self.locale),
                    empty_moon_model(&self.locale),
                    empty_jyotish_model(&self.locale),
                    empty_clock_model(&self.locale),
                    empty_system_model(&self.locale),
                    empty_processes_model(&self.locale),
                    build_calculator_model(p, &self.locale),
                    empty_notes_model(&self.locale),
                    empty_calendar_model(&self.locale),
                    empty_rss_model(&self.locale),
                    empty_search_model(&self.locale),
                    empty_media_model(&self.locale),
                    empty_password_model(&self.locale),
                    empty_viewer_model(&self.locale),
                    empty_recent_files_model(&self.locale),
                    empty_file_manager_model(&self.locale),
                ),
                WidgetPayload::Notes(p) => (
                    tstr,
                    80,
                    24,
                    blank_terminal(80, 24),
                    Image::default(),
                    0,
                    0,
                    true,
                    empty_weather_model(&self.locale),
                    empty_moon_model(&self.locale),
                    empty_jyotish_model(&self.locale),
                    empty_clock_model(&self.locale),
                    empty_system_model(&self.locale),
                    empty_processes_model(&self.locale),
                    empty_calculator_model(&self.locale),
                    build_notes_model(p, &self.locale),
                    empty_calendar_model(&self.locale),
                    empty_rss_model(&self.locale),
                    empty_search_model(&self.locale),
                    empty_media_model(&self.locale),
                    empty_password_model(&self.locale),
                    empty_viewer_model(&self.locale),
                    empty_recent_files_model(&self.locale),
                    empty_file_manager_model(&self.locale),
                ),
                WidgetPayload::Calendar(p) => (
                    tstr,
                    80,
                    24,
                    blank_terminal(80, 24),
                    Image::default(),
                    0,
                    0,
                    true,
                    empty_weather_model(&self.locale),
                    empty_moon_model(&self.locale),
                    empty_jyotish_model(&self.locale),
                    empty_clock_model(&self.locale),
                    empty_system_model(&self.locale),
                    empty_processes_model(&self.locale),
                    empty_calculator_model(&self.locale),
                    empty_notes_model(&self.locale),
                    build_calendar_model(p, &self.locale),
                    empty_rss_model(&self.locale),
                    empty_search_model(&self.locale),
                    empty_media_model(&self.locale),
                    empty_password_model(&self.locale),
                    empty_viewer_model(&self.locale),
                    empty_recent_files_model(&self.locale),
                    empty_file_manager_model(&self.locale),
                ),
                WidgetPayload::RssFeed(r) => (
                    tstr,
                    80,
                    24,
                    blank_terminal(80, 24),
                    Image::default(),
                    0,
                    0,
                    true,
                    empty_weather_model(&self.locale),
                    empty_moon_model(&self.locale),
                    empty_jyotish_model(&self.locale),
                    empty_clock_model(&self.locale),
                    empty_system_model(&self.locale),
                    empty_processes_model(&self.locale),
                    empty_calculator_model(&self.locale),
                    empty_notes_model(&self.locale),
                    empty_calendar_model(&self.locale),
                    build_rss_model(r, &self.locale),
                    empty_search_model(&self.locale),
                    empty_media_model(&self.locale),
                    empty_password_model(&self.locale),
                    empty_viewer_model(&self.locale),
                    empty_recent_files_model(&self.locale),
                    empty_file_manager_model(&self.locale),
                ),
                WidgetPayload::UniversalSearch(s) => {
                    let selected = self
                        .search_selection
                        .read()
                        .get(&pl.instance_id)
                        .copied()
                        .unwrap_or(if s.candidates.is_empty() { -1 } else { 0 });
                    let request_autofocus = matches!(
                        *self.search_autofocus_pending.lock(),
                        Some(id) if id == pl.instance_id
                    );
                    (
                        tstr,
                        80,
                        24,
                        blank_terminal(80, 24),
                        Image::default(),
                        0,
                        0,
                        true,
                        empty_weather_model(&self.locale),
                        empty_moon_model(&self.locale),
                        empty_jyotish_model(&self.locale),
                        empty_clock_model(&self.locale),
                        empty_system_model(&self.locale),
                        empty_processes_model(&self.locale),
                        empty_calculator_model(&self.locale),
                        empty_notes_model(&self.locale),
                        empty_calendar_model(&self.locale),
                        empty_rss_model(&self.locale),
                        build_search_model(s, &self.locale, selected, request_autofocus),
                        empty_media_model(&self.locale),
                        empty_password_model(&self.locale),
                        empty_viewer_model(&self.locale),
                        empty_recent_files_model(&self.locale),
                        empty_file_manager_model(&self.locale),
                    )
                }
                WidgetPayload::MediaPlayer(m) => (
                    tstr,
                    80,
                    24,
                    blank_terminal(80, 24),
                    Image::default(),
                    0,
                    0,
                    true,
                    empty_weather_model(&self.locale),
                    empty_moon_model(&self.locale),
                    empty_jyotish_model(&self.locale),
                    empty_clock_model(&self.locale),
                    empty_system_model(&self.locale),
                    empty_processes_model(&self.locale),
                    empty_calculator_model(&self.locale),
                    empty_notes_model(&self.locale),
                    empty_calendar_model(&self.locale),
                    empty_rss_model(&self.locale),
                    empty_search_model(&self.locale),
                    build_media_model(m, &self.locale),
                    empty_password_model(&self.locale),
                    empty_viewer_model(&self.locale),
                    empty_recent_files_model(&self.locale),
                    empty_file_manager_model(&self.locale),
                ),
                WidgetPayload::PasswordManager(p) => {
                    let toast = self.password_toasts.read().get(&pl.instance_id).cloned();
                    let autofocus = self
                        .password_autofocus_pending
                        .read()
                        .get(&pl.instance_id)
                        .copied()
                        .unwrap_or(false);
                    if autofocus {
                        self.password_autofocus_pending
                            .write()
                            .remove(&pl.instance_id);
                    }
                    let add_dialog = self
                        .password_add_dialogs
                        .read()
                        .get(&pl.instance_id)
                        .cloned()
                        .unwrap_or_default();
                    if add_dialog.request_autofocus {
                        self.password_add_dialogs.write().insert(
                            pl.instance_id,
                            PasswordAddDialogOverlay {
                                request_autofocus: false,
                                ..add_dialog.clone()
                            },
                        );
                    }
                    (
                        tstr,
                        80,
                        24,
                        blank_terminal(80, 24),
                        Image::default(),
                        0,
                        0,
                        true,
                        empty_weather_model(&self.locale),
                        empty_moon_model(&self.locale),
                        empty_jyotish_model(&self.locale),
                        empty_clock_model(&self.locale),
                        empty_system_model(&self.locale),
                        empty_processes_model(&self.locale),
                        empty_calculator_model(&self.locale),
                        empty_notes_model(&self.locale),
                        empty_calendar_model(&self.locale),
                        empty_rss_model(&self.locale),
                        empty_search_model(&self.locale),
                        empty_media_model(&self.locale),
                        build_password_model(p, toast, autofocus, add_dialog, &self.locale),
                        empty_viewer_model(&self.locale),
                        empty_recent_files_model(&self.locale),
                        empty_file_manager_model(&self.locale),
                    )
                }
                WidgetPayload::Viewer(v) => (
                    tstr,
                    80,
                    24,
                    blank_terminal(80, 24),
                    Image::default(),
                    0,
                    0,
                    true,
                    empty_weather_model(&self.locale),
                    empty_moon_model(&self.locale),
                    empty_jyotish_model(&self.locale),
                    empty_clock_model(&self.locale),
                    empty_system_model(&self.locale),
                    empty_processes_model(&self.locale),
                    empty_calculator_model(&self.locale),
                    empty_notes_model(&self.locale),
                    empty_calendar_model(&self.locale),
                    empty_rss_model(&self.locale),
                    empty_search_model(&self.locale),
                    empty_media_model(&self.locale),
                    empty_password_model(&self.locale),
                    build_viewer_model(v, &self.locale),
                    empty_recent_files_model(&self.locale),
                    empty_file_manager_model(&self.locale),
                ),
                WidgetPayload::RecentFiles(r) => (
                    tstr,
                    80,
                    24,
                    blank_terminal(80, 24),
                    Image::default(),
                    0,
                    0,
                    true,
                    empty_weather_model(&self.locale),
                    empty_moon_model(&self.locale),
                    empty_jyotish_model(&self.locale),
                    empty_clock_model(&self.locale),
                    empty_system_model(&self.locale),
                    empty_processes_model(&self.locale),
                    empty_calculator_model(&self.locale),
                    empty_notes_model(&self.locale),
                    empty_calendar_model(&self.locale),
                    empty_rss_model(&self.locale),
                    empty_search_model(&self.locale),
                    empty_media_model(&self.locale),
                    empty_password_model(&self.locale),
                    empty_viewer_model(&self.locale),
                    build_recent_files_model(r, &self.locale),
                    empty_file_manager_model(&self.locale),
                ),
                WidgetPayload::FileManager(fm) => {
                    let overlays = self
                        .fm_overlays
                        .read()
                        .get(&pl.instance_id)
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
                            create_item_is_file: false,
                            drag_active: false,
                            drag_paths: Vec::new(),
                            drag_drop_target: String::new(),
                            drag_target_pane: -1,
                        });
                    (
                        tstr,
                        80,
                        24,
                        blank_terminal(80, 24),
                        Image::default(),
                        0,
                        0,
                        true,
                        empty_weather_model(&self.locale),
                        empty_moon_model(&self.locale),
                        empty_jyotish_model(&self.locale),
                        empty_clock_model(&self.locale),
                        empty_system_model(&self.locale),
                        empty_processes_model(&self.locale),
                        empty_calculator_model(&self.locale),
                        empty_notes_model(&self.locale),
                        empty_calendar_model(&self.locale),
                        empty_rss_model(&self.locale),
                        empty_search_model(&self.locale),
                        empty_media_model(&self.locale),
                        empty_password_model(&self.locale),
                        empty_viewer_model(&self.locale),
                        empty_recent_files_model(&self.locale),
                        build_file_manager_model(
                            fm,
                            overlays,
                            pl.instance_id,
                            &self.locale,
                            false,
                            &self.fm_viewport.lock(),
                        ),
                    )
                }
                _ => (
                    tstr,
                    80,
                    24,
                    blank_terminal(80, 24),
                    Image::default(),
                    0,
                    0,
                    true,
                    empty_weather_model(&self.locale),
                    empty_moon_model(&self.locale),
                    empty_jyotish_model(&self.locale),
                    empty_clock_model(&self.locale),
                    empty_system_model(&self.locale),
                    empty_processes_model(&self.locale),
                    empty_calculator_model(&self.locale),
                    empty_notes_model(&self.locale),
                    empty_calendar_model(&self.locale),
                    empty_rss_model(&self.locale),
                    empty_search_model(&self.locale),
                    empty_media_model(&self.locale),
                    empty_password_model(&self.locale),
                    empty_viewer_model(&self.locale),
                    empty_recent_files_model(&self.locale),
                    empty_file_manager_model(&self.locale),
                ),
            }
        } else {
            default_frame_data_extended(&self.locale, iref.type_id.as_str())
        };
        let (terminal_tabs, terminal_active_tab) = if iref.type_id == "terminal" {
            if let Some(ws) = cached.as_deref() {
                if let WidgetPayload::Terminal(t) = &ws.payload {
                    build_terminal_tab_models(t)
                } else {
                    default_terminal_tab_models()
                }
            } else {
                default_terminal_tab_models()
            }
        } else {
            default_terminal_tab_models()
        };
        let terminal_panes = if iref.type_id == "terminal" {
            if let Some(ws) = cached.as_deref() {
                if let WidgetPayload::Terminal(t) = &ws.payload {
                    self.build_terminal_pane_models(t)
                } else {
                    default_terminal_pane_models()
                }
            } else {
                default_terminal_pane_models()
            }
        } else {
            default_terminal_pane_models()
        };
        let terminal_dividers = if iref.type_id == "terminal" {
            if let Some(ws) = cached.as_deref() {
                if let WidgetPayload::Terminal(t) = &ws.payload {
                    build_terminal_divider_models(t)
                } else {
                    default_terminal_divider_models()
                }
            } else {
                default_terminal_divider_models()
            }
        } else {
            default_terminal_divider_models()
        };
        let (cw, ch) = (
            self.font_metrics.cell_width_px,
            self.font_metrics.cell_height_px,
        );
        let close_confirm = self
            .close_confirm_overlays
            .read()
            .get(&pl.instance_id)
            .cloned()
            .unwrap_or_else(super::empty_close_confirm_dialog);
        let settings_dialog = self
            .settings_dialog_overlays
            .read()
            .get(&pl.instance_id)
            .cloned()
            .unwrap_or_else(super::empty_widget_settings_dialog);
        let has_settings = widget_has_settings(iref.type_id.as_str());
        let (group_id, group_tabs) = self.build_group_tab_models(pl.instance_id);
        WidgetFrameModel {
            instance_id: pl.instance_id.to_string().into(),
            type_id: type_s,
            title,
            has_settings,
            x: bounds.x,
            y: bounds.y,
            width: bounds.width,
            height: bounds.height,
            z_order,
            is_floating: false,
            window_state: 0,
            placement_valid: true,
            snap_visible: false,
            snap_x: 0.0,
            snap_y: 0.0,
            snap_width: 0.0,
            snap_height: 0.0,
            group_id,
            group_tabs,
            terminal_cols: tcols,
            terminal_rows: trows,
            terminal_cells: tcells,
            terminal_cursor_col: tcc,
            terminal_cursor_row: tcr,
            terminal_cursor_visible: tcvis,
            terminal_cell_width: cw,
            terminal_cell_height: ch,
            terminal_pixels: tpix,
            terminal_tabs,
            terminal_active_tab,
            terminal_panes,
            terminal_dividers,
            weather: weather_model,
            moon: moon_model,
            jyotish: jyotish_model,
            clock: clock_model,
            system: system_model,
            processes: processes_model,
            calculator: calculator_model,
            notes: notes_model,
            calendar: calendar_model,
            rss: rss_model,
            search: search_model,
            media: media_model,
            password: password_model,
            viewer: viewer_model,
            recent_files: recent_files_model,
            file_manager: file_manager_model,
            close_confirm,
            settings_dialog,
        }
    }

    pub(super) fn build_group_tab_models(
        &self,
        instance_id: Uuid,
    ) -> (SharedString, ModelRc<crate::slint_generated::GroupTabModel>) {
        let Some(group) = self.group_manager.find_for_instance(instance_id) else {
            return (SharedString::default(), ModelRc::new(VecModel::default()));
        };
        if group.members.len() < 2 {
            return (SharedString::default(), ModelRc::new(VecModel::default()));
        }
        let active = group.active_instance();
        let cache = self.widget_manager.snapshot_cache();
        let tabs: Vec<crate::slint_generated::GroupTabModel> = group
            .members
            .iter()
            .map(|mid| {
                let title = cache
                    .get(*mid)
                    .map(|ws| ws.title.clone())
                    .or_else(|| {
                        self.widget_manager
                            .get_instance(*mid)
                            .ok()
                            .map(|i| i.type_id.clone())
                    })
                    .unwrap_or_else(|| mid.to_string());
                crate::slint_generated::GroupTabModel {
                    instance_id: mid.to_string().into(),
                    title: title.into(),
                    is_active: active == Some(*mid),
                }
            })
            .collect();
        (
            group.id.to_string().into(),
            ModelRc::new(VecModel::from(tabs)),
        )
    }

    /// Rebuild the Slint [`WorkspaceModel`].
    pub fn rebuild_workspace_model(self: &Arc<Self>) -> Result<()> {
        let t0 = Instant::now();
        let w = self
            .workspace_manager
            .active()
            .map_err(|e| UiError::Slint(format!("{e}")))?;
        let (vw, vh) = *self.canvas_size.lock();
        let all_instances = self.widget_manager.instances_for_workspace(w.id);
        self.sync_floating_z_stack(&all_instances);
        let instances = Self::docked_instances(&all_instances);
        self.layout_engine
            .grow_grid_to_fit_instances(w.id, &instances);
        let n_inst = all_instances.len();
        let view = ViewportSize {
            width_px: vw,
            height_px: vh,
        };
        let snap = self.layout_engine.snapshot(w.id, &instances, view);
        let app_g = self.window.global::<AppState>();
        let off = self.drag_offset.lock().clone();
        let ro = self.resize_override.lock().clone();
        let mut frames: Vec<WidgetFrameModel> = Vec::new();
        let mut canvas_content_w = snap.content_width_px.max(vw);
        let mut canvas_content_h = snap.content_height_px.max(vh);
        let mut pty_changed_needs_rebuild = false;
        let workspace_groups = self.group_manager.list_for_workspace(w.id);
        for (idx, pl) in snap.cells.iter().enumerate() {
            // Hide non-active group members — only the active tab occupies the slot.
            if let Some(gid) = pl.group_id.or_else(|| {
                workspace_groups
                    .iter()
                    .find(|g| g.members.contains(&pl.instance_id))
                    .map(|g| g.id)
            }) {
                if let Ok(group) = self.group_manager.get(gid) {
                    if group.members.len() >= 2 && group.active_instance() != Some(pl.instance_id) {
                        continue;
                    }
                }
            }
            let mut bounds = pl.bounds;
            // Group slot uses the group's shared position/size when available.
            if let Some(group) = workspace_groups
                .iter()
                .find(|g| g.members.contains(&pl.instance_id) && g.members.len() >= 2)
            {
                let view = ViewportSize {
                    width_px: vw,
                    height_px: vh,
                };
                bounds = self
                    .layout_engine
                    .pixel_bounds_for(group.position, group.size, view);
            }
            if let Some(o) = off.get(&pl.instance_id) {
                bounds.x += o.0;
                bounds.y += o.1;
            }
            if let Some(ov) = ro.get(&pl.instance_id) {
                bounds = *ov;
            }
            canvas_content_w = canvas_content_w.max(bounds.x + bounds.width);
            canvas_content_h = canvas_content_h.max(bounds.y + bounds.height);
            let Ok(iref) = self.widget_manager.get_instance(pl.instance_id) else {
                continue;
            };
            let group_bar = if self
                .group_manager
                .find_for_instance(pl.instance_id)
                .is_some_and(|g| g.members.len() >= 2)
            {
                Self::GROUP_TAB_BAR_PX
            } else {
                0.0
            };
            if iref.type_id == "terminal" && !ro.contains_key(&pl.instance_id) {
                let cw = bounds.width.max(1.0);
                let ch = (bounds.height
                    - Self::WIDGET_FRAME_HEADER_PX
                    - Self::TERMINAL_TAB_BAR_PX
                    - group_bar)
                    .max(1.0);
                if self.resize_terminal_pty_to_content(pl.instance_id, cw, ch) {
                    pty_changed_needs_rebuild = true;
                }
            }
            let mut frame = self.build_widget_frame_for_placed(pl, idx as i32, bounds, &iref);
            frame.is_floating = false;
            frames.push(frame);
        }

        let floating_frames = self.build_floating_frames(&all_instances, &off, &ro);

        let (scroll_x, scroll_y) = *self.canvas_scroll.lock();
        let scroll_gen = self.canvas_scroll_gen.load(Ordering::Relaxed) as i32;

        let wlist: Vec<WorkspaceSummary> = self
            .workspace_manager
            .list()
            .into_iter()
            .map(|x| {
                let active = self
                    .workspace_manager
                    .active()
                    .ok()
                    .is_some_and(|a| a.id == x.id);
                WorkspaceSummary {
                    id: x.id.to_string().into(),
                    name: x.name.into(),
                    ordinal: i32::from(x.ordinal),
                    is_active: active,
                }
            })
            .collect();
        let n_frames = frames.len();
        sync_vec_model(&self.workspace_workspaces, wlist);
        sync_vec_model(&self.workspace_widgets, frames);
        sync_vec_model(&self.workspace_floating_widgets, floating_frames);
        sync_vec_model(
            &self.workspace_window_taskbar,
            self.build_window_taskbar_items(&all_instances),
        );
        sync_vec_model(
            &self.workspace_dock_types,
            super::catalog::dock_types_vec(&self.locale),
        );
        let snap_zone = *self.snap_zone.lock();
        app_g.set_workspace(WorkspaceModel {
            workspaces: self.workspace_workspaces.clone(),
            active_workspace_id: w.id.to_string().into(),
            widgets: self.workspace_widgets.clone(),
            floating_widgets: self.workspace_floating_widgets.clone(),
            window_taskbar: self.workspace_window_taskbar.clone(),
            snap_zone_visible: snap_zone.is_some(),
            snap_zone_x: snap_zone.map(|b| b.x).unwrap_or(0.0),
            snap_zone_y: snap_zone.map(|b| b.y).unwrap_or(0.0),
            snap_zone_width: snap_zone.map(|b| b.width).unwrap_or(0.0),
            snap_zone_height: snap_zone.map(|b| b.height).unwrap_or(0.0),
            dock_types: self.workspace_dock_types.clone(),
            dock_add_label: self.locale.tr("dock-add-label").into(),
            grid_columns: i32::from(snap.grid_columns),
            grid_rows: i32::from(snap.grid_rows),
            canvas_content_width: canvas_content_w,
            canvas_content_height: canvas_content_h,
            canvas_scroll_x: scroll_x,
            canvas_scroll_y: scroll_y,
            canvas_scroll_gen: scroll_gen,
        });
        if pty_changed_needs_rebuild {
            self.schedule_rebuild();
        }
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        debug!(
            target: "orchid_ui::workspace",
            instances = n_inst,
            frames = n_frames,
            "rebuild_workspace_model in {ms:.2} ms"
        );
        *self.search_autofocus_pending.lock() = None;
        self.sync_fm_transfer_notifications();
        self.sync_jyotish_notifications();
        Ok(())
    }
}

/// Empty [`WorkspaceModel`] for startup mode or when no layout is available yet.
pub fn build_empty_workspace_model(locale: &LocaleManager) -> WorkspaceModel {
    WorkspaceModel {
        workspaces: ModelRc::new(VecModel::default()),
        active_workspace_id: "".into(),
        widgets: ModelRc::new(VecModel::default()),
        floating_widgets: ModelRc::new(VecModel::default()),
        window_taskbar: ModelRc::new(VecModel::default()),
        snap_zone_visible: false,
        snap_zone_x: 0.0,
        snap_zone_y: 0.0,
        snap_zone_width: 0.0,
        snap_zone_height: 0.0,
        dock_types: ModelRc::new(VecModel::from(super::catalog::dock_types_vec(locale))),
        dock_add_label: locale.tr("dock-add-label").into(),
        grid_columns: 1,
        grid_rows: 1,
        canvas_content_width: 320.0,
        canvas_content_height: 240.0,
        canvas_scroll_x: 0.0,
        canvas_scroll_y: 0.0,
        canvas_scroll_gen: 0,
    }
}

pub(crate) fn fallback_widget_title(locale: &LocaleManager, type_id: &str) -> SharedString {
    match type_id {
        "weather" => locale.tr("dock-widget-weather").into(),
        "moon" => locale.tr("dock-widget-moon").into(),
        "jyotish" => locale.tr("dock-widget-jyotish").into(),
        "clock" => locale.tr("dock-widget-clock").into(),
        "system" => locale.tr("dock-widget-system").into(),
        "processes" => locale.tr("dock-widget-processes").into(),
        "calculator" => locale.tr("dock-widget-calculator").into(),
        "notes" => locale.tr("dock-widget-notes").into(),
        "calendar" => locale.tr("dock-widget-calendar").into(),
        "rss" => locale.tr("dock-widget-rss").into(),
        "recent-files" => locale.tr("dock-widget-recent-files").into(),
        "universal-search" | "search" => locale.tr("dock-widget-search").into(),
        "media-player" | "media" => locale.tr("dock-widget-media").into(),
        "password-manager" | "password" => locale.tr("dock-widget-password").into(),
        "viewer" => locale.tr("dock-widget-viewer").into(),
        "document-editor" => locale.tr("dock-widget-document-editor").into(),
        "file-manager" => locale.tr("dock-widget-fm").into(),
        _ => locale.tr("widget-title-terminal").into(),
    }
}

pub(crate) fn next_untitled_docx_path(dir: &std::path::Path) -> std::path::PathBuf {
    let first = dir.join("Untitled.docx");
    if !first.exists() {
        return first;
    }
    for n in 2..10_000 {
        let candidate = dir.join(format!("Untitled-{n}.docx"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!(
        "Untitled-{}.docx",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ))
}

#[allow(clippy::type_complexity)]
fn default_frame_data_extended(
    locale: &LocaleManager,
    type_id: &str,
) -> (
    SharedString,
    i32,
    i32,
    ModelRc<ModelRc<TerminalCellModel>>,
    Image,
    i32,
    i32,
    bool,
    WeatherModel,
    MoonModel,
    JyotishModel,
    ClockModel,
    SystemModel,
    ProcessesModel,
    CalculatorModel,
    NotesModel,
    CalendarModel,
    RssModel,
    SearchModel,
    MediaModel,
    PasswordModel,
    ViewerModel,
    RecentFilesModel,
    FileManagerModel,
) {
    (
        fallback_widget_title(locale, type_id),
        80,
        24,
        blank_terminal(80, 24),
        Image::default(),
        0,
        0,
        true,
        empty_weather_model(locale),
        empty_moon_model(locale),
        empty_jyotish_model(locale),
        empty_clock_model(locale),
        empty_system_model(locale),
        empty_processes_model(locale),
        empty_calculator_model(locale),
        empty_notes_model(locale),
        empty_calendar_model(locale),
        empty_rss_model(locale),
        empty_search_model(locale),
        empty_media_model(locale),
        empty_password_model(locale),
        empty_viewer_model(locale),
        empty_recent_files_model(locale),
        empty_file_manager_model(locale),
    )
}
