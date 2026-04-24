//! Main window controller for workspace mode (task 11B).
//!
//! # Invariant
//!
//! The Slint main thread must not block on async widget locks (e.g. by waiting
//! on the terminal [`tokio::sync::Mutex`]). Grid data comes from the lock-free
//! [`orchid_widgets::WidgetSnapshotCache`], which a background task in
//! `WidgetManager` fills. Blocking the UI thread to await snapshots reintroduces
//! the jank fixed in task 11B-Fix.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;
use slint::Color;
use slint::ComponentHandle;
use slint::Image;
use slint::Model;
use slint::ModelRc;
use slint::SharedString;
use slint::VecModel;
use tracing::{debug, trace, warn};
use uuid::Uuid;

use orchid_core::EventBus;
use orchid_i18n::LocaleManager;
use orchid_storage::{GridPosition, OrchidConfig, WidgetSize};
use orchid_terminal::SessionManager;
use orchid_terminal::{FontMetrics, PtySize};
use orchid_widgets::layout::PixelBounds;
use orchid_widgets::layout::ViewportSize;
use orchid_widgets::TerminalPayload;
use orchid_widgets::WidgetPayload;
use orchid_widgets::{
    LayoutEngine, WidgetManager, WorkspaceManager,
};
use parking_lot::RwLock;

use crate::error::{Result, UiError};
use crate::terminal_font_metrics;
use crate::terminal_raster;
use crate::slint_generated::{
    AppState, DockWidgetType, MainWindow, MoonModel, MoonValueEntry, Strings, SystemIndicatorEntry,
    SystemModel, TerminalCellModel, Theme, WeatherForecastEntry, WeatherModel, WidgetFrameModel,
    WorkspaceModel, WorkspaceSummary,
};
use crate::theme::ThemeManager;

/// Top switcher (40) + bottom dock (64) = canvas height in [`workspace.slint`].
const CANVAS_INSET_H: f32 = 40.0 + 64.0;

/// Drives the main window: workspace model, terminal I/O, drag/resize previews.
pub struct MainWindowController {
    window: MainWindow,
    theme: Arc<ThemeManager>,
    locale: Arc<LocaleManager>,
    config: Arc<RwLock<OrchidConfig>>,
    _bus: Arc<EventBus>,
    widget_manager: Arc<WidgetManager>,
    workspace_manager: Arc<WorkspaceManager>,
    layout_engine: Arc<LayoutEngine>,
    session_manager: Arc<SessionManager>,
    session_routing: Arc<Mutex<HashMap<Uuid, Uuid>>>,
    font_metrics: FontMetrics,
    /// When [`Self::font_metrics`] is from system font resolution, the same `fontdue` face for
    /// [`crate::terminal_raster`]. Otherwise the terminal falls back to a blank `Image` layer.
    mono_font: Option<fontdue::Font>,
    /// Proportional / symbol font for drawing code points the monospace face does not contain.
    mono_font_glyph_fallback: Option<fontdue::Font>,
    drag_offset: Arc<Mutex<HashMap<Uuid, (f32, f32)>>>,
    resize_override: Arc<Mutex<HashMap<Uuid, PixelBounds>>>,
    drag_start_bounds: Arc<Mutex<HashMap<Uuid, PixelBounds>>>,
    resize_state: Arc<Mutex<Option<ResizeInteraction>>>,
    canvas_size: Arc<Mutex<(f32, f32)>>,
    /// When true, a later [`MainWindow::on_ui_tick`] flushes [`rebuild_workspace_model`].
    rebuild_pending: Arc<AtomicBool>,
    /// Last `Window::scale_factor` used to raster the terminal; when it changes, we re-raster.
    last_window_scale: parking_lot::Mutex<f32>,
    /// Last (cols, rows) applied to each terminal from [`Self::on_terminal_viewport`], to avoid
    /// resize+rebuild storms when `set_workspace` re-lays out the same pixel viewport.
    last_terminal_viewport_pty: Arc<Mutex<HashMap<Uuid, (u16, u16)>>>,
    /// Stable Slint `ModelRc`s for the workspace and widget lists. Replacing the whole
    /// `ModelRc` on every tick re-instantiated every `for` item and dropped keyboard focus
    /// on the terminal's `TextInput` (see `terminal-view.slint`); we only mutate these via
    /// [`sync_vec_model`].
    workspace_workspaces: ModelRc<WorkspaceSummary>,
    workspace_widgets: ModelRc<WidgetFrameModel>,
    workspace_dock_types: ModelRc<DockWidgetType>,
}

struct ResizeInteraction {
    instance_id: Uuid,
    corner: String,
    start: PixelBounds,
}

impl MainWindowController {
    /// Build the window, apply globals, and wire Slint callbacks.
    #[allow(clippy::too_many_arguments, clippy::arc_with_non_send_sync)]
    pub fn new(
        theme: Arc<ThemeManager>,
        locale: Arc<LocaleManager>,
        config: Arc<RwLock<OrchidConfig>>,
        bus: Arc<EventBus>,
        widget_manager: Arc<WidgetManager>,
        workspace_manager: Arc<WorkspaceManager>,
        layout_engine: Arc<LayoutEngine>,
        session_manager: Arc<SessionManager>,
        session_routing: Arc<Mutex<HashMap<Uuid, Uuid>>>,
    ) -> Result<Arc<Self>> {
        let window = MainWindow::new()
            .map_err(|e| UiError::Slint(format!("failed to create MainWindow: {e}")))?;
        let tokens = &theme.current().tokens.typography;
        // Cell size: prefer real `advance` + `line` metrics from the first matching system
        // monospace (fontdb + fontdue), so Slint/PTY share the same grid as the shaped font
        // (not hand-tuned 0.6×size heuristics).
        let (font_metrics, mono_font, mono_font_glyph_fallback) =
            terminal_font_metrics::font_and_metrics_from_typography(tokens);
        let workspace_workspaces: ModelRc<WorkspaceSummary> =
            ModelRc::new(VecModel::<WorkspaceSummary>::default());
        let workspace_widgets: ModelRc<WidgetFrameModel> =
            ModelRc::new(VecModel::<WidgetFrameModel>::default());
        let workspace_dock_types: ModelRc<DockWidgetType> =
            ModelRc::new(VecModel::from(dock_types_vec(&locale)));
        let this = Arc::new(Self {
            window,
            theme,
            locale,
            config,
            _bus: bus,
            widget_manager: widget_manager.clone(),
            workspace_manager: workspace_manager.clone(),
            layout_engine: layout_engine.clone(),
            session_manager: session_manager.clone(),
            session_routing,
            font_metrics,
            mono_font,
            mono_font_glyph_fallback,
            drag_offset: Arc::new(Mutex::new(HashMap::new())),
            resize_override: Arc::new(Mutex::new(HashMap::new())),
            drag_start_bounds: Arc::new(Mutex::new(HashMap::new())),
            resize_state: Arc::new(Mutex::new(None)),
            canvas_size: Arc::new(Mutex::new((800.0, 500.0))),
            rebuild_pending: Arc::new(AtomicBool::new(false)),
            last_window_scale: parking_lot::Mutex::new(0.0),
            last_terminal_viewport_pty: Arc::new(Mutex::new(HashMap::new())),
            workspace_workspaces,
            workspace_widgets,
            workspace_dock_types,
        });
        this.apply_theme()?;
        this.apply_strings()?;
        this.apply_initial_mode()?;
        this.wire_callbacks()?;
        Ok(this)
    }

    fn apply_theme(self: &Arc<Self>) -> Result<()> {
        let theme = self.theme.current();
        let g = self.window.global::<Theme>();
        let t = &theme.tokens;
        let c = &t.color;
        g.set_surface_base(c.surface_base.to_slint());
        g.set_surface_raised(c.surface_raised.to_slint());
        g.set_text_primary(c.text_primary.to_slint());
        g.set_text_secondary(c.text_secondary.to_slint());
        g.set_text_tertiary(c.text_tertiary.to_slint());
        g.set_accent_brand(c.accent_brand.to_slint());
        g.set_border_default(c.border_default.to_slint());
        g.set_font_family_sans(t.typography.font_family_sans.clone().into());
        g.set_font_family_mono(t.typography.font_family_mono.clone().into());
        let sz = &t.typography;
        g.set_font_size_sm(sz.size_sm);
        g.set_font_size_md(sz.size_md);
        g.set_font_size_lg(sz.size_lg);
        g.set_font_size_xl(sz.size_xl);
        g.set_font_size_2xl(sz.size_2xl);
        g.set_font_size_3xl(sz.size_3xl);
        g.set_weight_regular(i32::from(sz.weight_regular));
        g.set_weight_medium(i32::from(sz.weight_medium));
        g.set_weight_semibold(i32::from(sz.weight_semibold));
        g.set_radius_md(t.radius.md);
        g.set_spacing_unit(t.spacing.unit);
        Ok(())
    }

    fn apply_strings(self: &Arc<Self>) -> Result<()> {
        let g = self.window.global::<Strings>();
        let mgr = &self.locale;
        g.set_window_title(mgr.tr("window-title").into());
        g.set_welcome(mgr.tr("startup-welcome").into());
        g.set_subtitle(mgr.tr("startup-subtitle").into());
        let version = env!("CARGO_PKG_VERSION");
        let args = orchid_i18n::FluentArgs::new().with("version", version);
        g.set_version_label(mgr.tr_args("startup-version-label", &args).into());
        g.set_theme_label(mgr.tr("status-theme").into());
        g.set_language_label(mgr.tr("status-language").into());
        g.set_density_label(mgr.tr("status-density").into());
        g.set_get_started_label(mgr.tr("startup-get-started").into());
        g.set_workspace_new_label(mgr.tr("workspace-new").into());
        g.set_dock_add_label(mgr.tr("dock-add-label").into());
        g.set_widget_close_tooltip(mgr.tr("widget-close-tooltip").into());
        Ok(())
    }

    fn apply_initial_mode(self: &Arc<Self>) -> Result<()> {
        let g = self.window.global::<AppState>();
        let th = self.theme.current();
        let language = self.locale.current();
        let density = self.config.read().appearance.density;
        let key = match density {
            orchid_storage::Density::Touch => "density-touch",
            orchid_storage::Density::Mouse => "density-mouse",
            orchid_storage::Density::Hybrid => "density-hybrid",
        };
        g.set_current_theme_id(th.meta.id.clone().into());
        g.set_current_language(language.as_str().into());
        g.set_current_density(self.locale.tr(key).into());
        let wss = self.workspace_manager.list();
        // Any persisted workspace is enough to enter workspace shell (canvas may
        // be empty after the user closed every widget).
        let work = !wss.is_empty();
        g.set_mode(if work { 1 } else { 0 });
        if work {
            self.rebuild_workspace_model()?;
        } else {
            g.set_workspace(build_empty_workspace_model(&self.locale));
        }
        Ok(())
    }

    /// Batches a workspace model update onto the next [`on_ui_tick`] (≈60 Hz).
    fn schedule_rebuild(self: &Arc<Self>) {
        self.rebuild_pending.store(true, Ordering::Release);
        trace!(target: "orchid_ui::workspace", "rebuild requested");
    }

    /// Match [`Self::canvas_size`] to the window client in logical pixels (workspace canvas area).
    /// Slint `changed` on the canvas does not run for the *first* size, so we poll winit on every
    /// `on_ui_tick` and after `show` until the size converges. Returns `true` if the viewport changed.
    fn sync_canvas_size_from_winit(self: &Arc<Self>) -> bool {
        let win = self.window.window();
        let p = win.size();
        if p.width < 2 || p.height < 2 {
            return false;
        }
        let sc = win.scale_factor();
        let log = p.to_logical(sc);
        let next = (log.width, (log.height - CANVAS_INSET_H).max(1.0));
        let mut cur = self.canvas_size.lock();
        if (cur.0 - next.0).abs() > 0.5 || (cur.1 - next.1).abs() > 0.5 {
            *cur = next;
            true
        } else {
            false
        }
    }

    fn wire_callbacks(self: &Arc<Self>) -> Result<()> {
        let t = Arc::downgrade(self);
        self.window.on_ui_tick({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    let canvas_size_mismatch = c.sync_canvas_size_from_winit();
                    let scale = c.window.window().scale_factor();
                    let scale_changed = {
                        let mut last = c.last_window_scale.lock();
                        if (scale - *last).abs() > 0.001 {
                            *last = scale;
                            true
                        } else {
                            false
                        }
                    };
                    let from_layout = c
                        .rebuild_pending
                        .swap(false, Ordering::AcqRel)
                        || canvas_size_mismatch;
                    let from_cache = c.widget_manager.take_frame_pending();
                    if from_layout || from_cache || scale_changed {
                        let _ = c.rebuild_workspace_model();
                    }
                }
            }
        });
        self.window.on_get_started_clicked({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_get_started();
                }
            }
        });
        self.window.on_workspace_clicked({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_workspace_clicked(&id);
                }
            }
        });
        self.window.on_workspace_create_clicked({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_workspace_create();
                }
            }
        });
        self.window.on_dock_add_clicked({
            let t = t.clone();
            move |tid| {
                if let Some(c) = t.upgrade() {
                    c.on_dock_add(&tid);
                }
            }
        });
        self.window.on_widget_close_clicked({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_close(&id);
                }
            }
        });
        self.window.on_widget_drag_started({
            let t = t.clone();
            move |id, _x, _y| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_drag_started(&id);
                }
            }
        });
        self.window.on_widget_drag_moved({
            let t = t.clone();
            move |id, dx, dy| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_drag_moved(&id, dx, dy);
                }
            }
        });
        self.window.on_widget_drag_ended({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_drag_ended(&id);
                }
            }
        });
        self.window.on_widget_resize_started({
            let t = t.clone();
            move |id, corner| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_resize_started(&id, &corner);
                }
            }
        });
        self.window.on_widget_resize_moved({
            let t = t.clone();
            move |id, dx, dy| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_resize_moved(&id, dx, dy);
                }
            }
        });
        self.window.on_widget_resize_ended({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_resize_ended(&id);
                }
            }
        });
        self.window.on_terminal_key_pressed({
            let t = t.clone();
            move |id, text| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_key(&id, &text);
                }
            }
        });
        self.window.on_terminal_viewport_changed({
            let t = t.clone();
            move |id, w, h| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_viewport(&id, w, h);
                }
            }
        });
        Ok(())
    }

    fn on_get_started(self: &Arc<Self>) {
        let wm = self.widget_manager.clone();
        let wsm = self.workspace_manager.clone();
        let loc = self.locale.clone();
        let t = Arc::downgrade(self);
        let _ = slint::spawn_local(async move {
            let name = loc.tr("workspace-default-name");
            let ws = if wsm.list().is_empty() {
                match wsm.create(name).await {
                    Ok(i) => i,
                    Err(e) => {
                        warn!(?e, "workspace");
                        return;
                    }
                }
            } else {
                wsm.list()[0].id
            };
            if let Err(e) = wm
                .create(orchid_widgets::CreateWidgetRequest {
                    type_id: "terminal".into(),
                    workspace_id: ws,
                    position: None,
                    size: None,
                    initial_lifecycle: None,
                    config_bytes: None,
                })
                .await
            {
                warn!(?e, "terminal");
                return;
            }
            if let Some(c) = t.upgrade() {
                c.window.global::<AppState>().set_mode(1);
                c.schedule_rebuild();
            }
        });
    }

    fn on_workspace_clicked(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let wsm = self.workspace_manager.clone();
        let t = Arc::downgrade(self);
        let _ = slint::spawn_local(async move {
            if let Err(e) = wsm.switch_to(u).await {
                warn!(?e, "switch");
            }
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }

    fn on_workspace_create(self: &Arc<Self>) {
        let wsm = self.workspace_manager.clone();
        let loc = self.locale.clone();
        let t = Arc::downgrade(self);
        let _ = slint::spawn_local(async move {
            let n = wsm.list().len() as i64 + 1;
            let args = orchid_i18n::FluentArgs::new().with("n", n.to_string());
            let name = loc.tr_args("workspace-unnamed", &args);
            let id = match wsm.create(name).await {
                Ok(i) => i,
                Err(e) => {
                    warn!(?e, "create ws");
                    return;
                }
            };
            if let Err(e) = wsm.switch_to(id).await {
                warn!(?e, "switch new");
            }
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }

    fn on_dock_add(self: &Arc<Self>, type_id: &SharedString) {
        let type_id_str = type_id.as_str();
        if !matches!(type_id_str, "terminal" | "weather" | "moon" | "system") {
            warn!(type_id = type_id_str, "unknown widget type from dock");
            return;
        }
        let wm = self.widget_manager.clone();
        let wsm = self.workspace_manager.clone();
        let t = Arc::downgrade(self);
        let type_owned = type_id_str.to_string();
        let _ = slint::spawn_local(async move {
            let wid = match wsm.active() {
                Ok(w) => w.id,
                Err(_) => return,
            };
            if let Err(e) = wm
                .create(orchid_widgets::CreateWidgetRequest {
                    type_id: type_owned,
                    workspace_id: wid,
                    position: None,
                    size: None,
                    initial_lifecycle: None,
                    config_bytes: None,
                })
                .await
            {
                warn!(?e, "add widget");
                return;
            }
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }

    fn on_widget_close(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        let _ = slint::spawn_local(async move {
            if let Err(e) = wm.close(u).await {
                warn!(?e, "close");
            }
            if let Some(c) = t.upgrade() {
                c.drag_offset.lock().remove(&u);
                c.resize_override.lock().remove(&u);
                c.schedule_rebuild();
            }
        });
    }

    fn on_widget_drag_started(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        if let (Ok(w), Ok(_)) = (self.workspace_manager.active(), self.widget_manager.get_instance(u)) {
            let inst = self.widget_manager.instances_for_workspace(w.id);
            let (vw, vh) = *self.canvas_size.lock();
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

    fn on_widget_drag_moved(self: &Arc<Self>, id: &SharedString, dx: f32, dy: f32) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        *self.drag_offset.lock().entry(u).or_insert((0.0, 0.0)) = (dx, dy);
        self.schedule_rebuild();
    }

    fn on_widget_drag_ended(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let off = self.drag_offset.lock().remove(&u);
        let start = self.drag_start_bounds.lock().remove(&u);
        let (off, start) = match (off, start) {
            (Some(o), Some(s)) => (o, s),
            _ => return,
        };
        let wm = self.widget_manager.clone();
        let le = self.layout_engine.clone();
        let t = Arc::downgrade(self);
        let _ = slint::spawn_local(async move {
            let Some(c) = t.upgrade() else {
                return;
            };
            let w = match c.workspace_manager.active() {
                Ok(w) => w,
                Err(_) => return,
            };
            let (vw, vh) = *c.canvas_size.lock();
            let opts = le.options();
            let g = opts.gutter_px;
            let cell_w = vw / f32::from(opts.grid_columns);
            let cell_h = vh / f32::from(opts.grid_rows);
            let new_x = start.x + off.0;
            let new_y = start.y + off.1;
            let col = ((new_x - g * 0.5) / cell_w)
                .round()
                .clamp(0.0, f32::from(opts.grid_columns.saturating_sub(1))) as u16;
            let row = ((new_y - g * 0.5) / cell_h)
                .round()
                .clamp(0.0, f32::from(opts.grid_rows.saturating_sub(1))) as u16;
            let inst = match wm.get_instance(u) {
                Ok(i) => i,
                Err(_) => return,
            };
            let size = *inst.size.read();
            let all = c.widget_manager.instances_for_workspace(w.id);
            let pos = GridPosition { col, row };
            if le.can_place(w.id, u, pos, size, &all).is_err() {
                c.schedule_rebuild();
                return;
            }
            let (pos, _) = le.snap(pos, size);
            if let Err(e) = wm.move_to(u, pos).await {
                warn!(?e, "move");
            }
            if let Some(c) = t.upgrade() {
                c.drag_offset.lock().remove(&u);
                c.schedule_rebuild();
            }
        });
    }

    fn on_widget_resize_started(self: &Arc<Self>, id: &SharedString, corner: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        if let (Ok(w), Ok(_)) = (self.workspace_manager.active(), self.widget_manager.get_instance(u)) {
            let (vw, vh) = *self.canvas_size.lock();
            for pl in self
                .layout_engine
                .snapshot(
                    w.id,
                    &self.widget_manager.instances_for_workspace(w.id),
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
                    });
                    return;
                }
            }
        }
    }

    fn on_widget_resize_moved(self: &Arc<Self>, id: &SharedString, dx: f32, dy: f32) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let st = self.resize_state.lock();
        if let Some(s) = st.as_ref() {
            if s.instance_id != u {
                return;
            }
            let mut b = s.start;
            match s.corner.as_str() {
                "se" => {
                    b.width = (b.width + dx).max(40.0);
                    b.height = (b.height + dy).max(40.0);
                }
                "sw" => {
                    b.x += dx;
                    b.width = (b.width - dx).max(40.0);
                    b.height = (b.height + dy).max(40.0);
                }
                "ne" => {
                    b.y += dy;
                    b.width = (b.width + dx).max(40.0);
                    b.height = (b.height - dy).max(40.0);
                }
                "nw" => {
                    b.x += dx;
                    b.y += dy;
                    b.width = (b.width - dx).max(40.0);
                    b.height = (b.height - dy).max(40.0);
                }
                _ => {}
            }
            drop(st);
            self.resize_override.lock().insert(u, b);
            self.schedule_rebuild();
        }
    }

    fn on_widget_resize_ended(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let _ = self.resize_state.lock().take();
        let Some(pb) = self.resize_override.lock().remove(&u) else {
            return;
        };
        let wm = self.widget_manager.clone();
        let le = self.layout_engine.clone();
        let t = Arc::downgrade(self);
        let _ = slint::spawn_local(async move {
            let Some(c) = t.upgrade() else { return };
            if c.workspace_manager.active().is_err() {
                return;
            }
            let opts = le.options();
            let (vw, vh) = *c.canvas_size.lock();
            let g = opts.gutter_px;
            let cell_w = vw / f32::from(opts.grid_columns);
            let cell_h = vh / f32::from(opts.grid_rows);
            let col = ((pb.x - g * 0.5) / cell_w).round() as u16;
            let row = ((pb.y - g * 0.5) / cell_h).round() as u16;
            let wcells = (((pb.width + g) / cell_w).round() as u16).max(1);
            let hcells = (((pb.height + g) / cell_h).round() as u16).max(1);
            let new_pos = GridPosition { col, row };
            let new_size = WidgetSize::Free { w: wcells, h: hcells };
            if let Err(e) = wm.move_to(u, new_pos).await {
                warn!(?e, "resize move");
            }
            if let Err(e) = wm.resize(u, new_size).await {
                warn!(?e, "resize");
            }
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }

    /// Content area of [`widget-frame.slint`] below the title bar (`height - 32px`); must match
    /// what `terminal-viewport-changed` would report as `w`/`h`.
    const WIDGET_FRAME_HEADER_PX: f32 = 32.0;

    /// Resize the PTY grid to the terminal's content `Rectangle` size. Slint's `changed` on that
    /// area often does not run for the *first* layout, so this is also invoked from
    /// [`rebuild_workspace_model`]. Returns `true` if the TTY was actually resized.
    fn resize_terminal_pty_to_content(
        self: &Arc<Self>,
        inst: Uuid,
        content_w: f32,
        content_h: f32,
    ) -> bool {
        let w = content_w.max(1.0);
        let h = content_h.max(1.0);
        let pty: PtySize = self.font_metrics.fit(w, h);
        {
            let last = self.last_terminal_viewport_pty.lock();
            if last.get(&inst) == Some(&(pty.cols, pty.rows)) {
                return false;
            }
        }
        let Some(sid) = self.session_routing.lock().get(&inst).copied() else {
            return false;
        };
        let Ok(s) = self.session_manager.get(sid) else {
            return false;
        };
        if let Err(e) = s.resize(pty) {
            warn!(?e, "pty");
            return false;
        }
        self.last_terminal_viewport_pty
            .lock()
            .insert(inst, (pty.cols, pty.rows));
        true
    }

    fn on_terminal_key(self: &Arc<Self>, id: &SharedString, text: &SharedString) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let Some(sid) = self.session_routing.lock().get(&inst).copied() else {
            trace!(
                target: "orchid_ui::terminal_input",
                %inst,
                "key ignored: no session routing (PTY not ready for this instance)"
            );
            return;
        };
        let Ok(session) = self.session_manager.get(sid) else {
            return;
        };
        let encoded = encode_slint_key_text(text.as_str());
        if encoded.is_empty() {
            return;
        }
        trace!(
            target: "orchid_ui::terminal_input",
            ch_len = text.as_str().chars().count(),
            bytes = ?encoded,
            "encode key for PTY"
        );
        if let Err(e) = session.send_input(&encoded) {
            warn!(?e, "input");
            return;
        }
        debug!(
            target: "orchid_ui::terminal_input",
            %sid,
            sent = encoded.len(),
            "forwarding terminal key"
        );
    }

    fn on_terminal_viewport(self: &Arc<Self>, id: &SharedString, w: f32, h: f32) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        // `content` width/height `changed` fires on every live resize step; do not
        // resize the PTY here — that thrashes the shell and triggers extra rebuilds.
        // `TerminalView` uses `image-fit: fill` until the PTY is committed in
        // [`on_widget_resize_ended`] and the next non-preview rebuild.
        if self.resize_override.lock().contains_key(&inst) {
            return;
        }
        if self.resize_terminal_pty_to_content(inst, w, h) {
            self.schedule_rebuild();
        }
    }

    /// Rebuild the Slint [`WorkspaceModel`].
    pub fn rebuild_workspace_model(self: &Arc<Self>) -> Result<()> {
        let t0 = Instant::now();
        let w = self
            .workspace_manager
            .active()
            .map_err(|e| UiError::Slint(format!("{e}")))?;
        let (vw, vh) = *self.canvas_size.lock();
        let instances = self.widget_manager.instances_for_workspace(w.id);
        let n_inst = instances.len();
        let view = ViewportSize {
            width_px: vw,
            height_px: vh,
        };
        let snap = self
            .layout_engine
            .snapshot(w.id, &instances, view);
        let app_g = self.window.global::<AppState>();
        let off = self.drag_offset.lock().clone();
        let ro = self.resize_override.lock().clone();
        let cache = self.widget_manager.snapshot_cache();
        let mut frames: Vec<WidgetFrameModel> = Vec::new();
        for (idx, pl) in snap.cells.iter().enumerate() {
            let mut bounds = pl.bounds;
            if let Some(o) = off.get(&pl.instance_id) {
                bounds.x += o.0;
                bounds.y += o.1;
            }
            if let Some(ov) = ro.get(&pl.instance_id) {
                bounds = *ov;
            }
            let Ok(iref) = self.widget_manager.get_instance(pl.instance_id) else {
                continue;
            };
            if iref.type_id == "terminal" && !ro.contains_key(&pl.instance_id) {
                let cw = bounds.width.max(1.0);
                let ch = (bounds.height - Self::WIDGET_FRAME_HEADER_PX).max(1.0);
                if self.resize_terminal_pty_to_content(pl.instance_id, cw, ch) {
                    self.schedule_rebuild();
                }
            }
            let type_s: SharedString = iref.type_id.clone().into();
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
                system_model,
            ) = if let Some(ws) = cached.as_deref() {
                let tstr: SharedString = ws.title.clone().into();
                match &ws.payload {
                    WidgetPayload::Terminal(t) => {
                        let img = if let Some(ref f) = self.mono_font {
                            let size_md = self.theme.current().tokens.typography.size_md;
                            let acc = self.theme.current().tokens.color.accent_brand;
                            let ccol = [acc.r, acc.g, acc.b, acc.a];
                            let cw = self.font_metrics.cell_width_px as u32;
                            let ch = self.font_metrics.cell_height_px as u32;
                            let scale = self.window.window().scale_factor();
                            let glyph_fb = self.mono_font_glyph_fallback.as_ref();
                            terminal_raster::render_terminal(
                                t,
                                f,
                                glyph_fb,
                                size_md,
                                cw,
                                ch,
                                scale,
                                ccol,
                            )
                            .unwrap_or_default()
                        } else {
                            Image::default()
                        };
                        (
                            tstr,
                            i32::from(t.cols),
                            i32::from(t.rows),
                            build_terminal_model(t),
                            img,
                            i32::from(t.cursor_col),
                            i32::from(t.cursor_row),
                            t.cursor_visible,
                            empty_weather_model(),
                            empty_moon_model(),
                            empty_system_model(),
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
                        empty_moon_model(),
                        empty_system_model(),
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
                        empty_weather_model(),
                        build_moon_model(m, &self.locale),
                        empty_system_model(),
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
                        empty_weather_model(),
                        empty_moon_model(),
                        build_system_model(s),
                    ),
                    _ => (
                        tstr,
                        80,
                        24,
                        blank_terminal(80, 24),
                        Image::default(),
                        0,
                        0,
                        true,
                        empty_weather_model(),
                        empty_moon_model(),
                        empty_system_model(),
                    ),
                }
            } else {
                default_frame_data_extended(&self.locale, iref.type_id.as_str())
            };
            let (cw, ch) = (self.font_metrics.cell_width_px, self.font_metrics.cell_height_px);
            let fm = WidgetFrameModel {
                instance_id: pl.instance_id.to_string().into(),
                type_id: type_s,
                title,
                x: bounds.x,
                y: bounds.y,
                width: bounds.width,
                height: bounds.height,
                z_order: idx as i32,
                terminal_cols: tcols,
                terminal_rows: trows,
                terminal_cells: tcells,
                terminal_cursor_col: tcc,
                terminal_cursor_row: tcr,
                terminal_cursor_visible: tcvis,
                terminal_cell_width: cw,
                terminal_cell_height: ch,
                terminal_pixels: tpix,
                weather: weather_model,
                moon: moon_model,
                system: system_model,
            };
            frames.push(fm);
        }
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
        sync_vec_model(&self.workspace_dock_types, dock_types_vec(&self.locale));
        app_g.set_workspace(WorkspaceModel {
            workspaces: self.workspace_workspaces.clone(),
            active_workspace_id: w.id.to_string().into(),
            widgets: self.workspace_widgets.clone(),
            dock_types: self.workspace_dock_types.clone(),
            dock_add_label: self.locale.tr("dock-add-label").into(),
            grid_columns: i32::from(snap.grid_columns),
            grid_rows: i32::from(snap.grid_rows),
        });
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        debug!(
            target: "orchid_ui::workspace",
            instances = n_inst,
            frames = n_frames,
            "rebuild_workspace_model in {ms:.2} ms"
        );
        Ok(())
    }

    /// Show the window and run the Slint event loop.
    pub fn run(self: Arc<Self>) -> Result<()> {
        tracing::info!("Opening main window");
        self.window
            .show()
            .map_err(|e| UiError::Slint(format!("show: {e}")))?;
        // Converge layout viewport to the real client size; `on_ui_tick` also polls until stable.
        if self.sync_canvas_size_from_winit() && self.workspace_manager.active().is_ok() {
            let _ = self.rebuild_workspace_model();
        }
        slint::run_event_loop().map_err(|e| UiError::Slint(format!("loop: {e}")))?;
        tracing::info!("Main window closed");
        Ok(())
    }
}

/// Replace all rows in a `VecModel` wrapped by `ModelRc` without creating a new `ModelRc`, so
/// `for` loops in Slint keep the same item instances and retain focus/scroll state.
fn sync_vec_model<T: Clone + 'static>(model: &ModelRc<T>, new_rows: Vec<T>) {
    let v = model
        .as_any()
        .downcast_ref::<VecModel<T>>()
        .expect("sync_vec_model: model must be VecModel-backed");
    while v.row_count() > new_rows.len() {
        v.remove(v.row_count() - 1);
    }
    for (i, row) in new_rows.into_iter().enumerate() {
        if i < v.row_count() {
            v.set_row_data(i, row);
        } else {
            v.push(row);
        }
    }
}

/// Empty [`WorkspaceModel`] for startup mode or when no layout is available yet.
pub fn build_empty_workspace_model(locale: &LocaleManager) -> WorkspaceModel {
    WorkspaceModel {
        workspaces: ModelRc::new(VecModel::default()),
        active_workspace_id: SharedString::new(),
        widgets: ModelRc::new(VecModel::default()),
        dock_types: ModelRc::new(VecModel::from(dock_types_vec(locale))),
        dock_add_label: locale.tr("dock-add-label").into(),
        grid_columns: 16,
        grid_rows: 10,
    }
}

fn blank_terminal(cols: u16, rows: u16) -> ModelRc<ModelRc<TerminalCellModel>> {
    let c = char_to_cell(' ');
    let row: Vec<TerminalCellModel> = (0..cols).map(|_| c.clone()).collect();
    let rows_m: Vec<ModelRc<TerminalCellModel>> = (0..rows)
        .map(|_| ModelRc::new(VecModel::from(row.clone())))
        .collect();
    ModelRc::new(VecModel::from(rows_m))
}

fn char_to_cell(ch: char) -> TerminalCellModel {
    TerminalCellModel {
        ch: ch.to_string().into(),
        fg: Color::from_argb_u8(0xFF, 0xE6, 0xEB, 0xF0),
        bg: Color::from_argb_u8(0xFF, 0x12, 0x14, 0x18),
        bold: false,
    }
}

fn build_terminal_model(t: &TerminalPayload) -> ModelRc<ModelRc<TerminalCellModel>> {
    let mut rows = Vec::with_capacity(t.rows as usize);
    for r in 0..t.rows {
        let mut rowv = Vec::with_capacity(t.cols as usize);
        for c in 0..t.cols {
            let idx = (r as usize) * (t.cols as usize) + (c as usize);
            let cell = t.cells.get(idx).map_or_else(
                || char_to_cell(' '),
                |cell| TerminalCellModel {
                    ch: if cell.ch == '\0' {
                        " ".into()
                    } else {
                        cell.ch.to_string().into()
                    },
                    fg: Color::from_argb_u8(cell.fg_rgba[3], cell.fg_rgba[0], cell.fg_rgba[1], cell.fg_rgba[2]),
                    bg: Color::from_argb_u8(cell.bg_rgba[3], cell.bg_rgba[0], cell.bg_rgba[1], cell.bg_rgba[2]),
                    bold: cell.bold,
                },
            );
            rowv.push(cell);
        }
        rows.push(ModelRc::new(VecModel::from(rowv)));
    }
    ModelRc::new(VecModel::from(rows))
}

fn dock_types_vec(locale: &LocaleManager) -> Vec<DockWidgetType> {
    vec![
        DockWidgetType {
            type_id: "terminal".into(),
            label: locale.tr("dock-widget-terminal").into(),
            icon: "terminal".into(),
        },
        DockWidgetType {
            type_id: "weather".into(),
            label: locale.tr("dock-widget-weather").into(),
            icon: "weather".into(),
        },
        DockWidgetType {
            type_id: "moon".into(),
            label: locale.tr("dock-widget-moon").into(),
            icon: "moon".into(),
        },
        DockWidgetType {
            type_id: "system".into(),
            label: locale.tr("dock-widget-system").into(),
            icon: "system".into(),
        },
    ]
}

fn fallback_widget_title(locale: &LocaleManager, type_id: &str) -> SharedString {
    match type_id {
        "weather" => locale.tr("dock-widget-weather").into(),
        "moon" => locale.tr("dock-widget-moon").into(),
        "system" => locale.tr("dock-widget-system").into(),
        _ => locale.tr("widget-title-terminal").into(),
    }
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
    SystemModel,
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
        empty_weather_model(),
        empty_moon_model(),
        empty_system_model(),
    )
}

fn empty_weather_model() -> WeatherModel {
    WeatherModel {
        location: SharedString::new(),
        current_temp: SharedString::new(),
        condition_label: SharedString::new(),
        condition_icon: SharedString::new(),
        feels_like: SharedString::new(),
        humidity: SharedString::new(),
        wind: SharedString::new(),
        forecast: ModelRc::new(VecModel::default()),
        last_updated: SharedString::new(),
        status: 0,
    }
}

fn empty_moon_model() -> MoonModel {
    MoonModel {
        phase_label: SharedString::new(),
        phase_icon: SharedString::new(),
        illumination: SharedString::new(),
        values: ModelRc::new(VecModel::default()),
    }
}

fn empty_system_model() -> SystemModel {
    SystemModel {
        indicators: ModelRc::new(VecModel::default()),
    }
}

fn build_weather_model(p: &orchid_widgets::WeatherPayload, locale: &LocaleManager) -> WeatherModel {
    let forecast: Vec<WeatherForecastEntry> = p
        .forecast
        .iter()
        .map(|d| WeatherForecastEntry {
            day_label: d.day_label.clone().into(),
            high_text: d.high_text.clone().into(),
            low_text: d.low_text.clone().into(),
            icon: d.condition_icon.into(),
            precip_text: d
                .precipitation_probability_text
                .clone()
                .unwrap_or_default()
                .into(),
        })
        .collect();

    let _ = locale;

    WeatherModel {
        location: p.location_name.clone().into(),
        current_temp: p.current_temp_text.clone().into(),
        condition_label: p.condition_label.clone().into(),
        condition_icon: p.condition_icon.into(),
        feels_like: p.feels_like_text.clone().unwrap_or_default().into(),
        humidity: p.humidity_text.clone().unwrap_or_default().into(),
        wind: p.wind_text.clone().unwrap_or_default().into(),
        forecast: ModelRc::new(VecModel::from(forecast)),
        last_updated: p.last_updated_text.clone().into(),
        status: weather_status_to_int(&p.status),
    }
}

fn weather_status_to_int(s: &orchid_widgets::WeatherStatusTag) -> i32 {
    use orchid_widgets::WeatherStatusTag::*;
    match s {
        Fresh => 0,
        Stale => 1,
        Offline => 2,
        Error => 3,
    }
}

fn build_moon_model(p: &orchid_widgets::MoonPayload, locale: &LocaleManager) -> MoonModel {
    let mut values = vec![
        MoonValueEntry {
            label: locale.tr("moon-age-label").into(),
            value: p.age_text.clone().into(),
        },
        MoonValueEntry {
            label: locale.tr("moon-distance-label").into(),
            value: p.distance_text.clone().into(),
        },
        MoonValueEntry {
            label: locale.tr("moon-next-full-label").into(),
            value: p.next_full_text.clone().into(),
        },
        MoonValueEntry {
            label: locale.tr("moon-next-new-label").into(),
            value: p.next_new_text.clone().into(),
        },
    ];

    if let Some(t) = &p.moonrise_text {
        values.push(MoonValueEntry {
            label: locale.tr("moon-moonrise-label").into(),
            value: t.clone().into(),
        });
    }
    if let Some(t) = &p.moonset_text {
        values.push(MoonValueEntry {
            label: locale.tr("moon-moonset-label").into(),
            value: t.clone().into(),
        });
    }
    if let Some(t) = &p.sunrise_text {
        values.push(MoonValueEntry {
            label: locale.tr("moon-sunrise-label").into(),
            value: t.clone().into(),
        });
    }
    if let Some(t) = &p.sunset_text {
        values.push(MoonValueEntry {
            label: locale.tr("moon-sunset-label").into(),
            value: t.clone().into(),
        });
    }
    if let Some(t) = &p.libration_text {
        values.push(MoonValueEntry {
            label: locale.tr("moon-libration-label").into(),
            value: t.clone().into(),
        });
    }

    MoonModel {
        phase_label: p.phase_label.clone().into(),
        phase_icon: p.phase_icon.into(),
        illumination: p.illumination_text.clone().into(),
        values: ModelRc::new(VecModel::from(values)),
    }
}

fn build_system_model(p: &orchid_widgets::SystemPayload) -> SystemModel {
    let indicators: Vec<SystemIndicatorEntry> = p
        .indicators
        .iter()
        .map(|i| SystemIndicatorEntry {
            label: i.label.clone().into(),
            value_text: i.value_text.clone().into(),
            percent: i
                .percent
                .map(|pct| (pct / 100.0).clamp(0.0, 1.0))
                .unwrap_or(-1.0),
            icon: i.icon.into(),
            status: indicator_status_to_int(&i.status),
        })
        .collect();

    SystemModel {
        indicators: ModelRc::new(VecModel::from(indicators)),
    }
}

fn indicator_status_to_int(s: &orchid_widgets::IndicatorStatus) -> i32 {
    use orchid_widgets::IndicatorStatus::*;
    match s {
        Normal => 0,
        Warning => 1,
        Critical => 2,
    }
}

// TODO(11B-Fix follow-up): Expose `event.key` (semantic key) from Slint in addition
// to `event.text` and use `orchid_terminal::InputEncoder` for xterm-style arrow /
// F-key / Home / End sequences once the workspace `slint` version supports it
// in key handlers.
/// True when `t` should not have its leading U+001B removed by [`trim_slint_key_artifacts`].
fn is_leading_escape_to_preserve(t: &str) -> bool {
    if !t.starts_with('\u{1b}') {
        return false;
    }
    t.chars().nth(1).is_some_and(|c| matches!(c, '[' | 'O'))
}

/// Strips Slint / winit key identity that is not user text (see `slint` `key_codes`):
/// - U+FEFF (BOM)
/// - Private use U+E000..=U+F8FF
/// - Slint modifier-style C0 U+0010..=U+0019 (incl. Backtab id 0x19)
/// - When 2+ code points: other C0 (U+00..=U+1F) except U+001B (we keep real ESC and
///   CSI/SS3 via [`is_leading_escape_to_preserve`]).
fn trim_slint_key_artifacts(text: &str) -> &str {
    let mut t = text;
    loop {
        if t.is_empty() {
            break;
        }
        if is_leading_escape_to_preserve(t) {
            break;
        }
        let Some(c) = t.chars().next() else {
            break;
        };
        let n = c as u32;
        if n == 0xFEFF {
            t = &t[c.len_utf8()..];
            continue;
        }
        if (0xE000..=0xF8FF).contains(&n) {
            t = &t[c.len_utf8()..];
            continue;
        }
        if t.chars().count() > 1 && (0x10..=0x19).contains(&n) {
            t = &t[c.len_utf8()..];
            continue;
        }
        if t.chars().count() > 1 && n < 0x20 && n != 0x1B {
            t = &t[c.len_utf8()..];
            continue;
        }
        break;
    }
    t
}

/// Maps Slint `KeyEvent.text` payloads to bytes for the PTY. Empty input means
/// a non-textual key in current Slint builds; see TODO above.
fn encode_slint_key_text(text: &str) -> Vec<u8> {
    if text.is_empty() {
        trace!(
            target: "orchid_ui::terminal_input",
            "empty Slint key text (e.g. arrow or modifier-only key)"
        );
        return Vec::new();
    }
    if text == "\r\n" || text == "\n\r" {
        return vec![0x0D];
    }
    let t = trim_slint_key_artifacts(text);
    if t.is_empty() {
        trace!(
            target: "orchid_ui::terminal_input",
            "key text was only Slint key-identity (PUA or modifier id); not forwarding to PTY"
        );
        return Vec::new();
    }
    if t == "\r\n" || t == "\n\r" {
        return vec![0x0D];
    }
    let mut chars = t.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        let cp = c as u32;
        // Slint uses U+10..=U+19 (DC1..) as *modifier key identity* for Key.* wiring
        // when paired with a printable; alone they must not become raw C0 in the PTY
        // (DLE, DC1, ..), which would print as "extra" garbage before/after RU/EN.
        if (0x10..=0x19).contains(&cp) {
            trace!(
                target: "orchid_ui::terminal_input",
                "Slint key id U+{:04X} only; not forwarding to PTY",
                cp
            );
            return Vec::new();
        }
        match c {
            '\n' | '\r' => return vec![0x0D],
            '\u{8}' | '\u{7f}' => return vec![0x7F],
            '\t' => return vec![b'\t'],
            '\u{1b}' => return vec![0x1B],
            c if (c as u32) < 0x20 => return vec![c as u8],
            _ => {}
        }
    }
    t.as_bytes().to_vec()
}

#[cfg(test)]
mod key_encode_tests {
    use super::encode_slint_key_text;

    #[test]
    fn encodes_printable() {
        assert_eq!(&encode_slint_key_text("a"), b"a");
        assert_eq!(&encode_slint_key_text("hello"), b"hello");
    }

    #[test]
    fn strips_slint_pua_and_modifier_id_prefixes() {
        assert_eq!(&encode_slint_key_text("\u{F700}a"), b"a");
        assert_eq!(&encode_slint_key_text("\u{E000}Z"), b"Z");
        assert!(encode_slint_key_text("\u{F700}").is_empty());
        // Slint: Shift = U+0010; a stray prefix + '$' (Shift+4 on US layout) must
        // be a single 0x24, not 0x10, 0x24.
        assert_eq!(&encode_slint_key_text("\u{10}$"), b"$");
        assert_eq!(&encode_slint_key_text("\u{F700}\u{10}x"), b"x");
        // VT/FF/LF/CR and similar C0 + symbol (e.g. Shift+2/3 on some Winit paths)
        assert_eq!(&encode_slint_key_text("\u{0B}@"), b"@");
        assert_eq!(&encode_slint_key_text("\u{0A}#"), b"#");
        assert_eq!(&encode_slint_key_text("\u{FEFF}x"), b"x");
        // CSI/SS3 must stay intact
        assert_eq!(&encode_slint_key_text("\u{1b}[A"), b"\x1b[A");
        assert_eq!(&encode_slint_key_text("\u{1b}OP"), b"\x1bOP");
    }

    #[test]
    fn encodes_enter_as_cr() {
        assert_eq!(encode_slint_key_text("\n"), vec![0x0D]);
        assert_eq!(encode_slint_key_text("\r"), vec![0x0D]);
        assert_eq!(encode_slint_key_text("\r\n"), vec![0x0D]);
        assert_eq!(encode_slint_key_text("\n\r"), vec![0x0D]);
    }

    #[test]
    fn encodes_backspace_as_del() {
        assert_eq!(encode_slint_key_text("\u{8}"), vec![0x7F]);
        assert_eq!(encode_slint_key_text("\u{7f}"), vec![0x7F]);
    }

    #[test]
    fn encodes_tab() {
        assert_eq!(encode_slint_key_text("\t"), vec![b'\t']);
    }

    #[test]
    fn encodes_escape() {
        assert_eq!(encode_slint_key_text("\u{1b}"), vec![0x1B]);
    }

    #[test]
    fn empty_is_empty() {
        assert!(encode_slint_key_text("").is_empty());
    }

    #[test]
    fn slint_lone_modifier_id_sends_nothing() {
        // U+10..=U+19: Slint may emit these alone for modifier; never send as DLE/DC1/…
        for cp in 0x10u32..=0x19 {
            let c = char::from_u32(cp).expect("BMP C0");
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            assert!(
                encode_slint_key_text(s).is_empty(),
                "U+{cp:04X} should not be forwarded as raw C0"
            );
        }
    }

    #[test]
    fn utf8_passed_through() {
        assert_eq!(&encode_slint_key_text("ü"), "ü".as_bytes());
        assert_eq!(&encode_slint_key_text("日"), "日".as_bytes());
    }
}
