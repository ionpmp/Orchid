//! Main window controller for workspace mode (task 11B).
//!
//! # Invariant
//!
//! The Slint main thread must not block on async widget locks (e.g. by waiting
//! on the terminal [`tokio::sync::Mutex`]). Grid data comes from the lock-free
//! [`orchid_widgets::WidgetSnapshotCache`], which a background task in
//! `WidgetManager` fills. Blocking the UI thread to await snapshots reintroduces
//! the jank fixed in task 11B-Fix.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_compat::Compat;
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

use orchid_core::{ActionContext, ActionDispatcher, CommandRegistry, EventBus, ParsedCommand};
use orchid_i18n::LocaleManager;
use orchid_storage::{OrchidConfig, StateStore, WidgetSize};
use orchid_terminal::SessionManager;
use orchid_terminal::{FontMetrics, PtySize};
use orchid_widgets::layout::PixelBounds;
use orchid_widgets::layout::ViewportSize;
use orchid_widgets::TerminalPayload;
use orchid_widgets::builtin::search::{self as search_widget, ActionTarget};
use orchid_widgets::{ViewerPayload, WidgetPayload};
use orchid_widgets::{
    CreateWidgetRequest,
    LayoutEngine, PlacedWidget, WidgetManager, WorkspaceManager,
};
use orchid_widgets::SharedInstance;
use parking_lot::RwLock;

use crate::error::{Result, UiError};
use crate::terminal_font_metrics;
use crate::terminal_raster;
use crate::slint_generated::{
    AppState, DockWidgetType, MainWindow, MediaModel, MoonModel, MoonValueEntry, PasswordDetail,
    PasswordEntryItem, PasswordModel, PasswordTagChip, RssItemEntry, RssModel, SearchCandidateEntry,
    SearchModel, Strings, SystemIndicatorEntry, SystemModel, TerminalCellModel, Theme,
    FileManagerModel, FmBreadcrumb, FmConfirmDialog, FmContextAction, FmContextMenu, FmEntry, FmPane,
    FmRenameState, FmSidebarItem, FmTab, FmTagChip, FmTagState, FmPassphraseState, FmContextSubitem,
    ViewerArchiveEntry, ViewerArchiveModel, ViewerEmptyModel, ViewerImageModel, ViewerModel,
    ViewerPdfModel, ViewerStatusModel, ViewerSyntaxLine, ViewerSyntaxSegment, ViewerTextModel,
    WeatherForecastEntry, WeatherModel, WidgetCatalog, WidgetFrameModel, WorkspaceModel,
    WorkspaceSummary,
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
    storage: Arc<StateStore>,
    command_registry: Arc<CommandRegistry>,
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
    /// Local (header) grab point at `pointer down`: frame origin is `pointer_canvas - grab`.
    drag_grab: Arc<Mutex<HashMap<Uuid, (f32, f32)>>>,
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
    /// Per universal-search instance: selected candidate row (clamped on rebuild).
    search_selection: Arc<RwLock<HashMap<Uuid, i32>>>,
    /// Set when a search widget is created from the dock; cleared after the next workspace rebuild.
    search_autofocus_pending: Arc<Mutex<Option<Uuid>>>,
    /// Per password-manager instance: (message, visible) toast state.
    password_toasts: Arc<RwLock<HashMap<Uuid, (String, bool)>>>,
    /// One-shot autofocus request for password search input after dock creation.
    password_autofocus_pending: Arc<RwLock<HashMap<Uuid, bool>>>,
    /// UI-only overlays for file-manager widgets (context menu, confirm dialog, rename).
    fm_overlays: Arc<RwLock<HashMap<Uuid, FileManagerOverlays>>>,
    /// Long-press widget catalog (search + pick).
    catalog: Arc<RwLock<CatalogUiState>>,
    catalog_items: ModelRc<DockWidgetType>,
}

#[derive(Debug, Clone, Default)]
struct CatalogUiState {
    visible: bool,
    content_x: f32,
    content_y: f32,
    screen_x: f32,
    screen_y: f32,
    search_query: String,
}

#[derive(Debug, Clone, Copy)]
enum AddWidgetPlacement {
    AutoSlot,
    CanvasPoint { content_x: f32, content_y: f32 },
}

struct ResizeInteraction {
    instance_id: Uuid,
    corner: String,
    start: PixelBounds,
    /// First pointer report in canvas space.
    press_canvas: (f32, f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PasswordCopyKind {
    Password,
    Username,
    Totp,
}

impl MainWindowController {
    /// Build the window, apply globals, and wire Slint callbacks.
    #[allow(clippy::too_many_arguments, clippy::arc_with_non_send_sync)]
    pub fn new(
        theme: Arc<ThemeManager>,
        locale: Arc<LocaleManager>,
        config: Arc<RwLock<OrchidConfig>>,
        storage: Arc<StateStore>,
        bus: Arc<EventBus>,
        command_registry: Arc<CommandRegistry>,
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
        let catalog_items: ModelRc<DockWidgetType> =
            ModelRc::new(VecModel::from(dock_types_vec(&locale)));
        let this = Arc::new(Self {
            window,
            theme,
            locale,
            config,
            storage,
            command_registry,
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
            drag_grab: Arc::new(Mutex::new(HashMap::new())),
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
            search_selection: Arc::new(RwLock::new(HashMap::new())),
            search_autofocus_pending: Arc::new(Mutex::new(None)),
            password_toasts: Arc::new(RwLock::new(HashMap::new())),
            password_autofocus_pending: Arc::new(RwLock::new(HashMap::new())),
            fm_overlays: Arc::new(RwLock::new(HashMap::new())),
            catalog: Arc::new(RwLock::new(CatalogUiState::default())),
            catalog_items,
        });
        this.apply_theme()?;
        this.apply_strings()?;
        this.sync_widget_catalog_global();
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
        g.set_catalog_title(mgr.tr("catalog-title").into());
        g.set_catalog_search_placeholder(mgr.tr("catalog-search-placeholder").into());
        g.set_widget_close_tooltip(mgr.tr("widget-close-tooltip").into());

        g.set_media_no_session(mgr.tr("media-no-session").into());

        g.set_password_locked(mgr.tr("password-locked").into());
        g.set_password_no_entries(mgr.tr("password-no-entries").into());
        g.set_password_search_placeholder(mgr.tr("password-search-placeholder").into());
        g.set_password_select_entry(mgr.tr("password-select-entry").into());
        g.set_password_label_username(mgr.tr("password-label-username").into());
        g.set_password_label_password(mgr.tr("password-label-password").into());
        g.set_password_label_url(mgr.tr("password-label-url").into());
        g.set_password_label_notes(mgr.tr("password-label-notes").into());
        g.set_password_label_totp(mgr.tr("password-label-totp").into());
        g.set_password_action_copy(mgr.tr("password-action-copy").into());
        g.set_password_action_open(mgr.tr("password-action-open").into());
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

    /// `create` stores new instances at a placeholder cell; place them on a free grid cell.
    async fn move_new_widget_to_free_slot(
        layout: &LayoutEngine,
        widgets: &WidgetManager,
        workspace_id: Uuid,
        new_id: Uuid,
    ) {
        let inst = match widgets.get_instance(new_id) {
            Ok(i) => i,
            Err(e) => {
                warn!(?e, "auto-place: new instance");
                return;
            }
        };
        let size = *inst.size.read();
        let all = widgets.instances_for_workspace(workspace_id);
        let pos = match layout.auto_place_excluding_with_growth(workspace_id, size, &all, new_id) {
            Ok(p) => p,
            Err(e) => {
                warn!(?e, "auto-place: no free cell after expanding grid");
                return;
            }
        };
        if let Err(e) = widgets.move_to(new_id, pos).await {
            warn!(?e, "auto-place: move_to");
        }
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
                    let rebuild_flag = c.rebuild_pending.swap(false, Ordering::AcqRel);
                    let from_layout = rebuild_flag || canvas_size_mismatch;
                    let need_full = from_layout || scale_changed;
                    // While the user drags or resizes, full rebuild + terminal patch are far too
                    // heavy to run on every ~60Hz tick; defer until the gesture ends.
                    // Do not require `!canvas_size_mismatch`: winit can report sub-pixel / jittery
                    // size every frame; that would set `from_layout` and force a full rebuild
                    // during drag, undoing the preview path. `sync_canvas_size_from_winit` still
                    // runs so `canvas_size` stays current; a pending rebuild flushes when the
                    // gesture ends. We only bypass defer for scale (DPR) changes, which are rare
                    // mid-gesture but need a full pass immediately.
                    let live_gesture = {
                        let d = c.drag_offset.lock();
                        let r = c.resize_override.lock();
                        !d.is_empty() || !r.is_empty()
                    };
                    let defer_heavy = live_gesture && !scale_changed;
                    if need_full {
                        if defer_heavy {
                            c.rebuild_pending.store(true, Ordering::Release);
                        } else {
                            c.widget_manager.drain_frame_dirty_ids();
                            let _ = c.rebuild_workspace_model();
                        }
                    } else if !defer_heavy {
                        let dirty = c.widget_manager.drain_frame_dirty_ids();
                        if !dirty.is_empty() {
                            let _ = c.patch_workspace_frames(&dirty);
                        }
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
        self.window.on_canvas_long_pressed({
            let t = t.clone();
            move |cx, cy, vx, vy| {
                if let Some(c) = t.upgrade() {
                    c.on_canvas_long_pressed(cx, cy, vx, vy);
                }
            }
        });
        self.window.on_catalog_pick({
            let t = t.clone();
            move |type_id| {
                if let Some(c) = t.upgrade() {
                    c.on_catalog_pick(&type_id);
                }
            }
        });
        self.window.on_catalog_dismiss({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_catalog_dismiss();
                }
            }
        });
        self.window.on_catalog_search_changed({
            let t = t.clone();
            move |q| {
                if let Some(c) = t.upgrade() {
                    c.on_catalog_search_changed(&q);
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
            move |id, lx, ly| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_drag_started(&id, lx, ly);
                }
            }
        });
        self.window.on_widget_drag_moved({
            let t = t.clone();
            move |id, canvas_x, canvas_y| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_drag_moved(&id, canvas_x, canvas_y);
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
            move |id, corner, press_x, press_y| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_resize_started(&id, &corner, press_x, press_y);
                }
            }
        });
        self.window.on_widget_resize_moved({
            let t = t.clone();
            move |id, canvas_x, canvas_y| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_resize_moved(&id, canvas_x, canvas_y);
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
        self.window.on_rss_item_clicked({
            let t = t.clone();
            move |link| {
                if let Some(c) = t.upgrade() {
                    c.on_rss_item_clicked(&link);
                }
            }
        });
        self.window.on_search_query_changed({
            let t = t.clone();
            move |inst, q| {
                if let Some(c) = t.upgrade() {
                    c.on_search_query_changed(&inst, &q);
                }
            }
        });
        self.window.on_search_candidate_activated({
            let t = t.clone();
            move |inst, id| {
                if let Some(c) = t.upgrade() {
                    c.on_search_candidate_activated(&inst, &id);
                }
            }
        });
        self.window.on_search_selection_changed({
            let t = t.clone();
            move |inst, idx| {
                if let Some(c) = t.upgrade() {
                    c.on_search_selection_changed(&inst, idx);
                }
            }
        });

        self.window.on_media_play_pause({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_media_play_pause();
                }
            }
        });
        self.window.on_media_next({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_media_command("next");
                }
            }
        });
        self.window.on_media_previous({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_media_command("previous");
                }
            }
        });

        self.window.on_password_search_changed({
            let t = t.clone();
            move |q| {
                if let Some(c) = t.upgrade() {
                    c.on_password_search_changed(&q);
                }
            }
        });
        self.window.on_password_entry_clicked({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_password_entry_clicked(&id);
                }
            }
        });
        self.window.on_password_copy_password({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_password_copy(&id, PasswordCopyKind::Password);
                }
            }
        });
        self.window.on_password_copy_username({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_password_copy(&id, PasswordCopyKind::Username);
                }
            }
        });
        self.window.on_password_copy_totp({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_password_copy(&id, PasswordCopyKind::Totp);
                }
            }
        });
        self.window.on_password_open_url({
            let t = t.clone();
            move |url| {
                if let Some(c) = t.upgrade() {
                    c.on_password_open_url(&url);
                }
            }
        });

        macro_rules! viewer_spawn {
            ($weak:expr, $fut:expr) => {{
                let tw = $weak.clone();
                let _ = slint::spawn_local(Compat::new(async move {
                    if let Err(e) = $fut.await {
                        warn!(?e, "viewer action");
                    }
                    if let Some(c) = tw.upgrade() {
                        c.schedule_rebuild();
                    }
                }));
            }};
        }

        self.window.on_viewer_image_zoom_in({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::image_zoom_in(inst));
                    }
                }
            }
        });
        self.window.on_viewer_image_zoom_out({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::image_zoom_out(inst));
                    }
                }
            }
        });
        self.window.on_viewer_image_fit({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::image_fit(inst));
                    }
                }
            }
        });
        self.window.on_viewer_image_actual_size({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::image_actual_size(inst));
                    }
                }
            }
        });
        self.window.on_viewer_image_rotate_cw({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::image_rotate_cw(inst));
                    }
                }
            }
        });
        self.window.on_viewer_image_rotate_ccw({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::image_rotate_ccw(inst));
                    }
                }
            }
        });
        self.window.on_viewer_image_pan({
            let t = t.clone();
            move |id, dx, dy| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let _ = slint::spawn_local(Compat::new(async move {
                            if let Err(e) =
                                orchid_widgets::builtin::viewer::image_pan(inst, dx, dy).await
                            {
                                warn!(?e, "viewer pan");
                            }
                            if let Some(ctrl) = tw.upgrade() {
                                ctrl.schedule_rebuild();
                            }
                        }));
                    }
                }
            }
        });
        self.window.on_viewer_viewport_changed({
            let t = t.clone();
            move |id, w, h| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let _ = slint::spawn_local(Compat::new(async move {
                            let _ = orchid_widgets::builtin::viewer::set_viewport(inst, w, h).await;
                            if let Some(ctrl) = tw.upgrade() {
                                ctrl.schedule_rebuild();
                            }
                        }));
                    }
                }
            }
        });
        self.window.on_viewer_pdf_prev_page({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::pdf_prev_page(inst));
                    }
                }
            }
        });
        self.window.on_viewer_pdf_next_page({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::pdf_next_page(inst));
                    }
                }
            }
        });
        self.window.on_viewer_pdf_fit_width({
            let t = t.clone();
            move |id, vw| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let _ = slint::spawn_local(Compat::new(async move {
                            let _ = orchid_widgets::builtin::viewer::pdf_fit_width(inst, vw).await;
                            if let Some(ctrl) = tw.upgrade() {
                                ctrl.schedule_rebuild();
                            }
                        }));
                    }
                }
            }
        });
        self.window.on_viewer_pdf_fit_page({
            let t = t.clone();
            move |id, vw, vh| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let _ = slint::spawn_local(Compat::new(async move {
                            let _ = orchid_widgets::builtin::viewer::pdf_fit_page(inst, vw, vh).await;
                            if let Some(ctrl) = tw.upgrade() {
                                ctrl.schedule_rebuild();
                            }
                        }));
                    }
                }
            }
        });
        self.window.on_viewer_pdf_zoom_in({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::pdf_zoom_in(inst));
                    }
                }
            }
        });
        self.window.on_viewer_pdf_zoom_out({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::pdf_zoom_out(inst));
                    }
                }
            }
        });
        self.window.on_viewer_pdf_go_to_page({
            let t = t.clone();
            move |id, page| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::pdf_go_to_page(inst, page));
                    }
                }
            }
        });
        self.window.on_viewer_archive_navigate_into({
            let t = t.clone();
            move |id, path| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let p = path.to_string();
                        let tw = Arc::downgrade(&c);
                        let _ = slint::spawn_local(Compat::new(async move {
                            let _ = orchid_widgets::builtin::viewer::archive_navigate_into(inst, p).await;
                            if let Some(ctrl) = tw.upgrade() {
                                ctrl.schedule_rebuild();
                            }
                        }));
                    }
                }
            }
        });
        self.window.on_viewer_archive_navigate_up({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::archive_navigate_up(inst));
                    }
                }
            }
        });
        self.window.on_viewer_archive_select({
            let t = t.clone();
            move |id, path| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let p = path.to_string();
                        let tw = Arc::downgrade(&c);
                        let _ = slint::spawn_local(Compat::new(async move {
                            let _ = orchid_widgets::builtin::viewer::archive_select(inst, p).await;
                            if let Some(ctrl) = tw.upgrade() {
                                ctrl.schedule_rebuild();
                            }
                        }));
                    }
                }
            }
        });

        self.window.on_fm_sidebar_clicked({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_sidebar_clicked(&id);
                }
            }
        });
        self.window.on_fm_toggle_dual_pane({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_fm_toggle_dual_pane();
                }
            }
        });
        self.window.on_fm_toggle_show_hidden({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_fm_toggle_show_hidden();
                }
            }
        });
        self.window.on_fm_toggle_click_behavior({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_fm_toggle_click_behavior();
                }
            }
        });
        self.window.on_fm_pane_clicked({
            let t = t.clone();
            move |pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_pane_clicked(pane);
                }
            }
        });
        self.window.on_fm_tab_clicked({
            let t = t.clone();
            move |pane, tab_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_tab_clicked(pane, &tab_id);
                }
            }
        });
        self.window.on_fm_tab_closed({
            let t = t.clone();
            move |pane, tab_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_tab_closed(pane, &tab_id);
                }
            }
        });
        self.window.on_fm_tab_new({
            let t = t.clone();
            move |pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_tab_new(pane);
                }
            }
        });
        self.window.on_fm_new_folder({
            let t = t.clone();
            move |pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_new_folder(pane);
                }
            }
        });
        self.window.on_fm_nav_back({
            let t = t.clone();
            move |pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_nav_back(pane);
                }
            }
        });
        self.window.on_fm_nav_forward({
            let t = t.clone();
            move |pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_nav_forward(pane);
                }
            }
        });
        self.window.on_fm_nav_up({
            let t = t.clone();
            move |pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_nav_up(pane);
                }
            }
        });
        self.window.on_fm_breadcrumb_clicked({
            let t = t.clone();
            move |pane, path| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_breadcrumb_clicked(pane, &path);
                }
            }
        });
        self.window.on_fm_view_mode_cycle({
            let t = t.clone();
            move |pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_view_mode_cycle(pane);
                }
            }
        });
        self.window.on_fm_sort_cycle({
            let t = t.clone();
            move |pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_sort_cycle(pane);
                }
            }
        });
        self.window.on_fm_sort_column_clicked({
            let t = t.clone();
            move |pane, col| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_sort_column_clicked(pane, col);
                }
            }
        });
        self.window.on_fm_quick_filter_changed({
            let t = t.clone();
            move |pane, q| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_quick_filter_changed(pane, &q);
                }
            }
        });
        self.window.on_fm_entry_clicked({
            let t = t.clone();
            move |pane, path, ctrl| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_clicked(pane, &path, ctrl);
                }
            }
        });
        self.window.on_fm_entry_shift_clicked({
            let t = t.clone();
            move |pane, path| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_shift_clicked(pane, &path);
                }
            }
        });
        self.window.on_fm_entry_double_clicked({
            let t = t.clone();
            move |pane, path, is_dir| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_double_clicked(pane, &path, is_dir);
                }
            }
        });
        self.window.on_fm_entry_context({
            let t = t.clone();
            move |pane, path, x, y| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_context(pane, &path, x, y);
                }
            }
        });
        self.window.on_fm_context_action({
            let t = t.clone();
            move |action_id, paths| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_context_action(&action_id, &paths);
                }
            }
        });
        self.window.on_fm_context_dismiss({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_fm_context_dismiss();
                }
            }
        });
        self.window.on_fm_confirm_yes({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_fm_confirm_yes();
                }
            }
        });
        self.window.on_fm_confirm_no({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_fm_confirm_no();
                }
            }
        });
        self.window.on_fm_rename_commit({
            let t = t.clone();
            move |old_path, new_name| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_rename_commit(&old_path, &new_name);
                }
            }
        });
        self.window.on_fm_rename_cancel({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_fm_rename_cancel();
                }
            }
        });
        self.window.on_fm_tag_commit({
            let t = t.clone();
            move |tag| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_tag_commit(&tag);
                }
            }
        });
        self.window.on_fm_tag_cancel({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_fm_tag_cancel();
                }
            }
        });
        self.window.on_fm_passphrase_commit({
            let t = t.clone();
            move |pw| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_passphrase_commit(&pw);
                }
            }
        });
        self.window.on_fm_passphrase_cancel({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_fm_passphrase_cancel();
                }
            }
        });
        self.window.on_fm_select_all({
            let t = t.clone();
            move |pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_select_all(pane);
                }
            }
        });
        self.window.on_fm_delete_selected({
            let t = t.clone();
            move |pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_delete_selected(pane);
                }
            }
        });
        self.window.on_fm_copy_selected({
            let t = t.clone();
            move |pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_copy_selected(pane);
                }
            }
        });
        self.window.on_fm_paste_clipboard({
            let t = t.clone();
            move |pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_paste_clipboard(pane);
                }
            }
        });
        self.window.on_fm_rename_selected({
            let t = t.clone();
            move |pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_rename_selected(pane);
                }
            }
        });
        self.window.on_fm_deselect_all({
            let t = t.clone();
            move |pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_deselect_all(pane);
                }
            }
        });
        self.window.on_fm_open_selected({
            let t = t.clone();
            move |pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_open_selected(pane);
                }
            }
        });
        self.window.on_fm_entry_drag_start({
            let t = t.clone();
            move |pane, path| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_drag_start(pane, &path);
                }
            }
        });
        self.window.on_fm_entry_drag_hover({
            let t = t.clone();
            move |pane, path| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_drag_hover(pane, &path);
                }
            }
        });
        self.window.on_fm_entry_drag_drop({
            let t = t.clone();
            move |pane, path| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_drag_drop(pane, &path);
                }
            }
        });
        self.window.on_fm_entry_drag_cancel({
            let t = t.clone();
            move |pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_drag_cancel(pane);
                }
            }
        });
        self.window.on_fm_pane_drag_hover({
            let t = t.clone();
            move |pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_pane_drag_hover(pane);
                }
            }
        });
        self.window.on_fm_drop_on_current_dir({
            let t = t.clone();
            move |pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_drop_on_current_dir(pane);
                }
            }
        });
        Ok(())
    }

    fn on_get_started(self: &Arc<Self>) {
        let le = self.layout_engine.clone();
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
            let new_id = match wm
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
                Ok(id) => id,
                Err(e) => {
                    warn!(?e, "terminal");
                    return;
                }
            };
            Self::move_new_widget_to_free_slot(&le, &wm, ws, new_id).await;
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

    fn on_canvas_long_pressed(
        self: &Arc<Self>,
        content_x: f32,
        content_y: f32,
        viewport_x: f32,
        viewport_y: f32,
    ) {
        {
            let mut cat = self.catalog.write();
            cat.visible = true;
            cat.content_x = content_x;
            cat.content_y = content_y;
            cat.screen_x = content_x - viewport_x;
            cat.screen_y = content_y - viewport_y;
            cat.search_query.clear();
        }
        self.sync_widget_catalog_global();
    }

    fn on_catalog_dismiss(self: &Arc<Self>) {
        if !self.catalog.read().visible {
            return;
        }
        self.catalog.write().visible = false;
        self.sync_widget_catalog_global();
    }

    fn on_catalog_search_changed(self: &Arc<Self>, query: &SharedString) {
        self.catalog.write().search_query = query.to_string();
        self.sync_widget_catalog_global();
    }

    fn on_catalog_pick(self: &Arc<Self>, type_id: &SharedString) {
        let placement = {
            let cat = self.catalog.read();
            AddWidgetPlacement::CanvasPoint {
                content_x: cat.content_x,
                content_y: cat.content_y,
            }
        };
        self.on_catalog_dismiss();
        self.spawn_add_widget(type_id.as_str(), placement);
    }

    fn on_dock_add(self: &Arc<Self>, type_id: &SharedString) {
        self.spawn_add_widget(type_id.as_str(), AddWidgetPlacement::AutoSlot);
    }

    fn spawn_add_widget(self: &Arc<Self>, type_id: &str, placement: AddWidgetPlacement) {
        if !is_known_widget_type(type_id) {
            warn!(type_id, "unknown widget type");
            return;
        }
        let le = self.layout_engine.clone();
        let wm = self.widget_manager.clone();
        let wsm = self.workspace_manager.clone();
        let t = Arc::downgrade(self);
        let type_id_owned = type_id.to_string();
        let focus_search_input = type_id_owned == "search";
        let focus_password_input = type_id_owned == "password";
        let _ = slint::spawn_local(async move {
            let wid = match wsm.active() {
                Ok(w) => w.id,
                Err(_) => return,
            };
            let size = Self::minimal_widget_size(&wm, &type_id_owned);
            let new_id = match wm
                .create(CreateWidgetRequest {
                    type_id: type_id_owned,
                    workspace_id: wid,
                    position: None,
                    size: Some(size),
                    initial_lifecycle: None,
                    config_bytes: None,
                })
                .await
            {
                Ok(id) => id,
                Err(e) => {
                    warn!(?e, "add widget");
                    return;
                }
            };
            match placement {
                AddWidgetPlacement::AutoSlot => {
                    Self::move_new_widget_to_free_slot(&le, &wm, wid, new_id).await;
                }
                AddWidgetPlacement::CanvasPoint { content_x, content_y } => {
                    if let Some(c) = t.upgrade() {
                        c.place_widget_at_canvas_point(wid, new_id, size, content_x, content_y)
                            .await;
                    }
                }
            }
            if let Err(e) = wm.refresh_snapshot_cache(new_id).await {
                warn!(?e, widget_id = %new_id, "prime snapshot cache after add");
            }
            if let Some(c) = t.upgrade() {
                if focus_search_input {
                    *c.search_autofocus_pending.lock() = Some(new_id);
                }
                if focus_password_input {
                    c.password_autofocus_pending.write().insert(new_id, true);
                }
                c.schedule_rebuild();
            }
        });
    }

    async fn place_widget_at_canvas_point(
        self: &Arc<Self>,
        workspace_id: Uuid,
        instance_id: Uuid,
        size: WidgetSize,
        content_x: f32,
        content_y: f32,
    ) {
        let (vw, vh) = *self.canvas_size.lock();
        let viewport = ViewportSize {
            width_px: vw,
            height_px: vh,
        };
        let preferred = self
            .layout_engine
            .placement_from_content_top_left(viewport, content_x, content_y, size);
        let instances = self.widget_manager.instances_for_workspace(workspace_id);
        let place = if self
            .layout_engine
            .can_place(workspace_id, instance_id, preferred, size, &instances)
            .is_ok()
        {
            preferred
        } else {
            match self.layout_engine.auto_place_excluding_with_growth(
                workspace_id,
                size,
                &instances,
                instance_id,
            ) {
                Ok(p) => p,
                Err(e) => {
                    warn!(?e, "catalog place: no free cell");
                    return;
                }
            }
        };
        if let Err(e) = self.widget_manager.move_to(instance_id, place).await {
            warn!(?e, "catalog place: move_to");
        }
    }

    fn sync_widget_catalog_global(self: &Arc<Self>) {
        let cat = self.catalog.read().clone();
        let items = filter_catalog_items(&self.locale, &cat.search_query);
        sync_vec_model(&self.catalog_items, items);
        let g = self.window.global::<WidgetCatalog>();
        g.set_visible(cat.visible);
        g.set_screen_x(cat.screen_x);
        g.set_screen_y(cat.screen_y);
        g.set_search_query(cat.search_query.into());
        g.set_items(self.catalog_items.clone());
    }

    fn minimal_widget_size(wm: &WidgetManager, type_id: &str) -> WidgetSize {
        wm.registry()
            .get(type_id)
            .map(|d| d.min_size.unwrap_or(d.default_size))
            .unwrap_or(WidgetSize::Medium)
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
                c.drag_start_bounds.lock().remove(&u);
                c.drag_grab.lock().remove(&u);
                c.resize_override.lock().remove(&u);
                c.search_selection.write().remove(&u);
                c.password_toasts.write().remove(&u);
                c.password_autofocus_pending.write().remove(&u);
                c.schedule_rebuild();
            }
        });
    }

    fn on_widget_drag_started(self: &Arc<Self>, id: &SharedString, grab_lx: f32, grab_ly: f32) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        self.drag_grab.lock().insert(u, (grab_lx, grab_ly));
        if let (Ok(w), Ok(_)) = (self.workspace_manager.active(), self.widget_manager.get_instance(u)) {
            let inst = self.widget_manager.instances_for_workspace(w.id);
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

    fn on_widget_drag_moved(self: &Arc<Self>, id: &SharedString, canvas_x: f32, canvas_y: f32) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        self.apply_drag_frame_preview(u, canvas_x, canvas_y);
    }

    /// O(1) update of the dragged widget's `x`/`y` in the Slint model (no full rebuild).
    fn apply_drag_frame_preview(self: &Arc<Self>, instance: Uuid, canvas_x: f32, canvas_y: f32) {
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
        let v = match self
            .workspace_widgets
            .as_any()
            .downcast_ref::<VecModel<WidgetFrameModel>>()
        {
            Some(m) => m,
            None => {
                self.schedule_rebuild();
                return;
            }
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
            v.set_row_data(r, row);
            self.sync_canvas_scroll_extent();
            return;
        }
        self.schedule_rebuild();
    }

    fn on_widget_drag_ended(self: &Arc<Self>, id: &SharedString) {
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
        let (off, start) = match (off, start) {
            (Some(o), Some(s)) => (o, s),
            _ => return,
        };
        let wm = self.widget_manager.clone();
        let le = self.layout_engine.clone();
        let t = Arc::downgrade(self);
        let _ = slint::spawn_local(async move {
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
            let all = c.widget_manager.instances_for_workspace(w.id);
            if le.can_place(w.id, u, pos, size, &all).is_err() {
                if let Some(c) = t.upgrade() {
                    end_drag(&c);
                    c.schedule_rebuild();
                }
                return;
            }
            let (pos, _) = le.snap(pos, size);
            if let Err(e) = wm.move_to(u, pos).await {
                warn!(?e, "move");
            }
            if let Some(c) = t.upgrade() {
                end_drag(&c);
                c.schedule_rebuild();
            }
        });
    }

    fn on_widget_resize_started(
        self: &Arc<Self>,
        id: &SharedString,
        corner: &SharedString,
        press_x: f32,
        press_y: f32,
    ) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        if let (Ok(w), Ok(_)) = (self.workspace_manager.active(), self.widget_manager.get_instance(u)) {
            let (vw, vh) = *self.canvas_size.lock();
            let inst = self.widget_manager.instances_for_workspace(w.id);
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

    fn on_widget_resize_moved(self: &Arc<Self>, id: &SharedString, canvas_x: f32, canvas_y: f32) {
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
    fn apply_resize_frame_preview(self: &Arc<Self>, instance: Uuid, b: PixelBounds) {
        let v = match self
            .workspace_widgets
            .as_any()
            .downcast_ref::<VecModel<WidgetFrameModel>>()
        {
            Some(m) => m,
            None => {
                self.schedule_rebuild();
                return;
            }
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
            v.set_row_data(r, row);
            self.sync_canvas_scroll_extent();
            return;
        }
        self.schedule_rebuild();
    }

    /// Keep the Flickable scroll extent in sync while drag/resize previews move frames
    /// beyond the last committed layout bounds.
    fn sync_canvas_scroll_extent(self: &Arc<Self>) {
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
            let (vw, vh) = *c.canvas_size.lock();
            let viewport = ViewportSize {
                width_px: vw,
                height_px: vh,
            };
            let (new_pos, new_size) = le.placement_from_free_bounds(&pb, viewport);
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

    fn on_rss_item_clicked(self: &Arc<Self>, link: &SharedString) {
        let s = link.as_str();
        if s.is_empty() {
            return;
        }
        tracing::debug!(target: "orchid_ui::rss", link = %s, "opening rss item");
        if let Err(e) = orchid_widgets::builtin::rss::open_link(s) {
            warn!(?e, "failed to open RSS link");
        }
    }

    fn on_media_play_pause(self: &Arc<Self>) {
        let Some((inst_id, is_playing)) = self.find_active_media_widget() else {
            return;
        };
        let _ = slint::spawn_local(async move {
            let cmd = if is_playing { "pause" } else { "play" };
            if let Err(e) = orchid_widgets::builtin::media::execute_command(inst_id, cmd).await {
                warn!(?e, "media play/pause");
            }
        });
    }

    fn on_media_command(self: &Arc<Self>, cmd: &'static str) {
        let Some((inst_id, _)) = self.find_active_media_widget() else {
            return;
        };
        let _ = slint::spawn_local(async move {
            if let Err(e) = orchid_widgets::builtin::media::execute_command(inst_id, cmd).await {
                warn!(?e, "media command");
            }
        });
    }

    fn find_active_media_widget(&self) -> Option<(Uuid, bool)> {
        let w = self.workspace_manager.active().ok()?;
        let cache = self.widget_manager.snapshot_cache();
        for inst in self.widget_manager.instances_for_workspace(w.id) {
            if inst.type_id == "media-player" {
                let is_playing = cache
                    .get(inst.id)
                    .and_then(|s| match &s.payload {
                        orchid_widgets::WidgetPayload::MediaPlayer(p) => Some(p.is_playing),
                        _ => None,
                    })
                    .unwrap_or(false);
                return Some((inst.id, is_playing));
            }
        }
        None
    }

    fn on_password_search_changed(self: &Arc<Self>, q: &SharedString) {
        let query = q.to_string();
        let Some(inst_id) = self.find_active_password_widget() else {
            return;
        };
        orchid_widgets::builtin::password::update_search(inst_id, query);
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let _ = wm.refresh_snapshot_cache(inst_id).await;
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_password_entry_clicked(self: &Arc<Self>, id: &SharedString) {
        let entry_id = id.to_string();
        let Some(inst_id) = self.find_active_password_widget() else {
            return;
        };
        orchid_widgets::builtin::password::select_entry(inst_id, entry_id);
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let _ = wm.refresh_snapshot_cache(inst_id).await;
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_password_copy(self: &Arc<Self>, id: &SharedString, kind: PasswordCopyKind) {
        let entry_id = id.to_string();
        let Some(inst_id) = self.find_active_password_widget() else {
            return;
        };
        let t = Arc::downgrade(self);
        let locale = self.locale.clone();
        let _ = slint::spawn_local(Compat::new(async move {
            let toast_key = match kind {
                PasswordCopyKind::Password => {
                    match orchid_widgets::builtin::password::copy_password(inst_id, &entry_id).await
                    {
                        Ok(()) => "password-password-copied",
                        Err(e) => {
                            warn!(?e, "copy password");
                            return;
                        }
                    }
                }
                PasswordCopyKind::Username => {
                    match orchid_widgets::builtin::password::copy_username(inst_id, &entry_id).await
                    {
                        Ok(()) => "password-username-copied",
                        Err(e) => {
                            warn!(?e, "copy username");
                            return;
                        }
                    }
                }
                PasswordCopyKind::Totp => {
                    match orchid_widgets::builtin::password::copy_totp(inst_id, &entry_id).await {
                        Ok(()) => "password-totp-copied",
                        Err(e) => {
                            warn!(?e, "copy totp");
                            return;
                        }
                    }
                }
            };

            let Some(c) = t.upgrade() else {
                return;
            };
            let msg = locale.tr(toast_key).to_string();
            c.password_toasts.write().insert(inst_id, (msg, true));
            c.schedule_rebuild();

            let t2 = Arc::downgrade(&c);
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            if let Some(cc) = t2.upgrade() {
                cc.password_toasts.write().remove(&inst_id);
                cc.schedule_rebuild();
            }
        }));
    }

    fn on_password_open_url(self: &Arc<Self>, url: &SharedString) {
        let url_str = url.to_string();
        if url_str.is_empty() {
            return;
        }
        if let Err(e) = opener::open(&url_str) {
            warn!(?e, "failed to open URL");
        }
    }

    fn find_active_password_widget(&self) -> Option<Uuid> {
        let w = self.workspace_manager.active().ok()?;
        for inst in self.widget_manager.instances_for_workspace(w.id) {
            if inst.type_id == "password-manager" {
                return Some(inst.id);
            }
        }
        None
    }

    fn on_search_query_changed(self: &Arc<Self>, inst: &SharedString, q: &SharedString) {
        let Ok(instance_id) = Uuid::parse_str(inst.as_str()) else {
            return;
        };
        search_widget::universal_search_push_query(instance_id, q.to_string());
        if q.as_str().trim().is_empty() {
            self.search_selection.write().insert(instance_id, -1);
        } else {
            self.search_selection.write().insert(instance_id, 0);
        }
        // `refresh_snapshot_cache` awaits `tokio::sync::Mutex`. Slint's `spawn_local` cannot
        // drive raw Tokio futures; `Compat` runs them on a Tokio pool and resumes on Slint.
        //
        // TODO(search): If candidates are still empty, check logs for
        // `universal_search_push_query: instance not in SEARCH_LIVE`. That indicates the
        // instance isn't active/live (Sleeping/Unloaded) or UI is sending the wrong id.
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            if wm.refresh_snapshot_cache(instance_id).await.is_err() {
                return;
            }
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_search_candidate_activated(self: &Arc<Self>, inst: &SharedString, cand: &SharedString) {
        let Ok(instance_id) = Uuid::parse_str(inst.as_str()) else {
            return;
        };
        let candidate_id = cand.to_string();
        let this = Arc::clone(self);
        let _ = slint::spawn_local(Compat::new(async move {
            this.dispatch_search_action_target(instance_id, candidate_id).await;
        }));
    }

    async fn dispatch_search_action_target(self: &Arc<Self>, instance_id: Uuid, candidate_id: String) {
        let Some(target) =
            search_widget::universal_search_action_target(instance_id, candidate_id.as_str())
        else {
            warn!(%candidate_id, "unknown search candidate");
            return;
        };
        match target {
            ActionTarget::OpenFile(path) => {
                if let Err(e) = opener::open(&path) {
                    warn!(?e, path = %path, "open file from search");
                }
            }
            ActionTarget::RunCommand(cmd_id) => {
                let action = match self
                    .command_registry
                    .build_action(&cmd_id, ParsedCommand::default())
                {
                    Ok(a) => a,
                    Err(e) => {
                        warn!(?e, cmd_id = %cmd_id, "build command action");
                        return;
                    }
                };
                let ctx = ActionContext::new(
                    self._bus.clone(),
                    self.storage.clone(),
                    self.config.clone(),
                );
                let dispatcher = ActionDispatcher::new();
                if let Err(e) = dispatcher.dispatch(action, &ctx).await {
                    warn!(?e, "dispatch command from search");
                }
            }
            ActionTarget::OpenSettings(section) => {
                debug!(section = %section, "open settings from search (not wired to UI yet)");
            }
        }
    }

    fn on_search_selection_changed(self: &Arc<Self>, inst: &SharedString, new_idx: i32) {
        let Ok(instance_id) = Uuid::parse_str(inst.as_str()) else {
            return;
        };
        let count = self
            .widget_manager
            .snapshot_cache()
            .get(instance_id)
            .and_then(|s| match &s.payload {
                WidgetPayload::UniversalSearch(p) => Some(p.candidates.len() as i32),
                _ => None,
            })
            .unwrap_or(0);
        let clamped = if count == 0 {
            -1
        } else {
            new_idx.clamp(0, count - 1)
        };
        self.search_selection.write().insert(instance_id, clamped);
        self.schedule_rebuild();
    }

    fn on_terminal_viewport(self: &Arc<Self>, id: &SharedString, w: f32, h: f32) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        // `content` width/height `changed` fires on every live resize step; do not
        // resize the PTY here — that thrashes the shell and triggers extra rebuilds.
        // `TerminalView` uses `image-fit: fill` until the PTY is committed in
        // [`on_widget_resize_ended`] and the next non-preview rebuild.
        if self.drag_offset.lock().contains_key(&inst) {
            return;
        }
        if self.resize_override.lock().contains_key(&inst) {
            return;
        }
        if self.resize_terminal_pty_to_content(inst, w, h) {
            self.schedule_rebuild();
        }
    }

    /// Patch Slint `WidgetFrameModel` rows for instances whose [`WidgetSnapshotCache`] data changed
    /// without a layout canvas / scale / workspace event (e.g. terminal text at ~30Hz).
    fn patch_workspace_frames(self: &Arc<Self>, ids: &[Uuid]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
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
        let snap = self
            .layout_engine
            .snapshot(w.id, &instances, view);
        let off = self.drag_offset.lock().clone();
        let ro = self.resize_override.lock().clone();
        let v = self
            .workspace_widgets
            .as_any()
            .downcast_ref::<VecModel<WidgetFrameModel>>()
            .expect("workspace widgets must be VecModel-backed");
        for id in &unique {
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
                let ch = (bounds.height - Self::WIDGET_FRAME_HEADER_PX).max(1.0);
                let _ = self.resize_terminal_pty_to_content(*id, cw, ch);
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
        Ok(())
    }

    fn build_widget_frame_for_placed(
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
            system_model,
            rss_model,
            search_model,
            media_model,
            password_model,
            viewer_model,
            file_manager_model,
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
                        empty_rss_model(&self.locale),
                        empty_search_model(&self.locale),
                        empty_media_model(),
                        empty_password_model(),
                        empty_viewer_model(&self.locale),
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
                    empty_moon_model(),
                    empty_system_model(),
                    empty_rss_model(&self.locale),
                    empty_search_model(&self.locale),
                    empty_media_model(),
                    empty_password_model(),
                    empty_viewer_model(&self.locale),
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
                    empty_weather_model(),
                    build_moon_model(m, &self.locale),
                    empty_system_model(),
                    empty_rss_model(&self.locale),
                    empty_search_model(&self.locale),
                    empty_media_model(),
                    empty_password_model(),
                    empty_viewer_model(&self.locale),
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
                    empty_weather_model(),
                    empty_moon_model(),
                    build_system_model(s),
                    empty_rss_model(&self.locale),
                    empty_search_model(&self.locale),
                    empty_media_model(),
                    empty_password_model(),
                    empty_viewer_model(&self.locale),
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
                    empty_weather_model(),
                    empty_moon_model(),
                    empty_system_model(),
                    build_rss_model(r, &self.locale),
                    empty_search_model(&self.locale),
                    empty_media_model(),
                    empty_password_model(),
                    empty_viewer_model(&self.locale),
                    empty_file_manager_model(&self.locale),
                ),
                WidgetPayload::UniversalSearch(s) => {
                    let selected = self
                        .search_selection
                        .read()
                        .get(&pl.instance_id)
                        .copied()
                        .unwrap_or(if s.candidates.is_empty() {
                            -1
                        } else {
                            0
                        });
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
                        empty_weather_model(),
                        empty_moon_model(),
                        empty_system_model(),
                        empty_rss_model(&self.locale),
                        build_search_model(s, &self.locale, selected, request_autofocus),
                        empty_media_model(),
                        empty_password_model(),
                        empty_viewer_model(&self.locale),
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
                    empty_weather_model(),
                    empty_moon_model(),
                    empty_system_model(),
                    empty_rss_model(&self.locale),
                    empty_search_model(&self.locale),
                    build_media_model(m),
                    empty_password_model(),
                    empty_viewer_model(&self.locale),
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
                        self.password_autofocus_pending.write().remove(&pl.instance_id);
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
                        empty_weather_model(),
                        empty_moon_model(),
                        empty_system_model(),
                        empty_rss_model(&self.locale),
                        empty_search_model(&self.locale),
                        empty_media_model(),
                        build_password_model(p, toast, autofocus),
                        empty_viewer_model(&self.locale),
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
                    empty_weather_model(),
                    empty_moon_model(),
                    empty_system_model(),
                    empty_rss_model(&self.locale),
                    empty_search_model(&self.locale),
                    empty_media_model(),
                    empty_password_model(),
                    build_viewer_model(v, &self.locale),
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
                            passphrase_paths: Vec::new(),
                            passphrase_purpose: None,
                            create_folder_parent: None,
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
                        empty_weather_model(),
                        empty_moon_model(),
                        empty_system_model(),
                        empty_rss_model(&self.locale),
                        empty_search_model(&self.locale),
                        empty_media_model(),
                        empty_password_model(),
                        empty_viewer_model(&self.locale),
                        build_file_manager_model(
                            fm,
                            overlays,
                            pl.instance_id,
                            &self.locale,
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
                    empty_weather_model(),
                    empty_moon_model(),
                    empty_system_model(),
                    empty_rss_model(&self.locale),
                    empty_search_model(&self.locale),
                    empty_media_model(),
                    empty_password_model(),
                    empty_viewer_model(&self.locale),
                    empty_file_manager_model(&self.locale),
                ),
            }
        } else {
            default_frame_data_extended(&self.locale, iref.type_id.as_str())
        };
        let (cw, ch) = (self.font_metrics.cell_width_px, self.font_metrics.cell_height_px);
        WidgetFrameModel {
            instance_id: pl.instance_id.to_string().into(),
            type_id: type_s,
            title,
            x: bounds.x,
            y: bounds.y,
            width: bounds.width,
            height: bounds.height,
            z_order,
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
            rss: rss_model,
            search: search_model,
            media: media_model,
            password: password_model,
            viewer: viewer_model,
            file_manager: file_manager_model,
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
        self.layout_engine
            .grow_grid_to_fit_instances(w.id, &instances);
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
        let mut frames: Vec<WidgetFrameModel> = Vec::new();
        let mut canvas_content_w = snap.content_width_px.max(vw);
        let mut canvas_content_h = snap.content_height_px.max(vh);
        let mut pty_changed_needs_rebuild = false;
        for (idx, pl) in snap.cells.iter().enumerate() {
            let mut bounds = pl.bounds;
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
            if iref.type_id == "terminal" && !ro.contains_key(&pl.instance_id) {
                let cw = bounds.width.max(1.0);
                let ch = (bounds.height - Self::WIDGET_FRAME_HEADER_PX).max(1.0);
                if self.resize_terminal_pty_to_content(pl.instance_id, cw, ch) {
                    pty_changed_needs_rebuild = true;
                }
            }
            frames.push(self.build_widget_frame_for_placed(
                pl,
                idx as i32,
                bounds,
                &iref,
            ));
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
            canvas_content_width: canvas_content_w,
            canvas_content_height: canvas_content_h,
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
        Ok(())
    }

    /// Show the window and run the Slint event loop.
    pub fn run(self: Arc<Self>) -> Result<()> {
        tracing::info!("Opening main window");
        self.window
            .show()
            .map_err(|e| UiError::Slint(format!("show: {e}")))?;
        // Converge layout: sync canvas, then `schedule_rebuild` (do not call
        // `rebuild_workspace_model` synchronously here). A sync rebuild after
        // `show()` can re-enter Slint/winit while a borrow is still held, causing
        // `RefCell already borrowed` panics on Windows.
        if self.sync_canvas_size_from_winit() && self.workspace_manager.active().is_ok() {
            self.schedule_rebuild();
        }
        slint::run_event_loop().map_err(|e| UiError::Slint(format!("loop: {e}")))?;
        tracing::info!("Main window closed");
        Ok(())
    }

    fn find_active_fm(&self) -> Option<Uuid> {
        let w = self.workspace_manager.active().ok()?;
        for inst in self.widget_manager.instances_for_workspace(w.id) {
            if inst.type_id == "file-manager" {
                return Some(inst.id);
            }
        }
        None
    }

    fn ensure_fm_overlays(&self, inst: Uuid) -> FileManagerOverlays {
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
                passphrase_paths: Vec::new(),
                passphrase_purpose: None,
                create_folder_parent: None,
                drag_active: false,
                drag_paths: Vec::new(),
                drag_drop_target: String::new(),
                drag_target_pane: -1,
            })
    }

    fn clear_fm_drag(&self, inst: Uuid) {
        let mut over = self.fm_overlays.write();
        if let Some(entry) = over.get_mut(&inst) {
            entry.drag_active = false;
            entry.drag_paths.clear();
            entry.drag_drop_target.clear();
            entry.drag_target_pane = -1;
        }
    }

    fn fm_active_tab_path(&self, inst: Uuid, pane: u8) -> Option<String> {
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

    fn fm_selected_paths(&self, inst: Uuid, pane: u8) -> Vec<String> {
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
            .map(|e| e.path.clone())
            .collect()
    }

    async fn open_in_viewer_for_controller(
        ctrl: std::sync::Weak<MainWindowController>,
        path: orchid_fs::FsPath,
    ) -> Result<Uuid> {
        let Some(c) = ctrl.upgrade() else {
            return Err(UiError::Slint("controller gone".into()));
        };
        let ws_id = c
            .workspace_manager
            .active()
            .map_err(|e| UiError::Slint(format!("no active workspace: {e}")))?
            .id;

        for inst in c.widget_manager.instances_for_workspace(ws_id) {
            if inst.type_id == orchid_widgets::builtin::viewer::TYPE_ID {
                orchid_widgets::builtin::viewer::open_path(inst.id, path.clone())
                    .await
                    .map_err(|e| UiError::Slint(format!("viewer open: {e}")))?;
                for fm in c.widget_manager.instances_for_workspace(ws_id) {
                    if fm.type_id == orchid_widgets::builtin::file_manager::TYPE_ID {
                        let path_str = path.as_str().to_string();
                        let _ =
                            orchid_widgets::builtin::file_manager::touch_recent(fm.id, &path_str)
                                .await;
                        break;
                    }
                }
                if let Some(c2) = ctrl.upgrade() {
                    c2.schedule_rebuild();
                }
                return Ok(inst.id);
            }
        }

        let id = c
            .widget_manager
            .create(orchid_widgets::CreateWidgetRequest {
                type_id: orchid_widgets::builtin::viewer::TYPE_ID.into(),
                workspace_id: ws_id,
                position: None,
                size: None,
                initial_lifecycle: None,
                config_bytes: None,
            })
            .await
            .map_err(|e| UiError::Slint(format!("viewer create: {e}")))?;

        for _ in 0..50 {
            if c.widget_manager.get_instance(id).is_ok() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }

        orchid_widgets::builtin::viewer::open_path(id, path.clone())
            .await
            .map_err(|e| UiError::Slint(format!("viewer open: {e}")))?;
        for inst in c.widget_manager.instances_for_workspace(ws_id) {
            if inst.type_id == orchid_widgets::builtin::file_manager::TYPE_ID {
                let path_str = path.as_str().to_string();
                let _ =
                    orchid_widgets::builtin::file_manager::touch_recent(inst.id, &path_str).await;
                break;
            }
        }
        if let Some(c2) = ctrl.upgrade() {
            c2.schedule_rebuild();
        }
        Ok(id)
    }

    fn on_fm_sidebar_clicked(self: &Arc<Self>, id: &SharedString) {
        let item_id = id.to_string();
        if item_id.starts_with("section:") {
            return;
        }
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let pane = 0u8;
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            if let Err(e) = orchid_widgets::builtin::file_manager::navigate_virtual(inst, pane, &item_id).await {
                warn!(?e, "fm sidebar navigation");
            }
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_toggle_dual_pane(self: &Arc<Self>) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let _ = orchid_widgets::builtin::file_manager::toggle_dual_pane(inst).await;
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_toggle_show_hidden(self: &Arc<Self>) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let _ = orchid_widgets::builtin::file_manager::toggle_show_hidden(inst).await;
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_toggle_click_behavior(self: &Arc<Self>) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let _ = orchid_widgets::builtin::file_manager::toggle_click_behavior(inst).await;
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_open_selected(self: &Arc<Self>, pane: i32) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
        let paths = self.fm_selected_paths(inst, p);
        let Some(path) = paths.first() else {
            return;
        };
        self.fm_dispatch_open(inst, p, path.clone());
    }

    fn on_fm_entry_drag_start(self: &Arc<Self>, pane: i32, _path: &SharedString) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
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

    fn on_fm_entry_drag_hover(self: &Arc<Self>, pane: i32, folder: &SharedString) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
        if !entry.drag_active {
            return;
        }
        entry.drag_drop_target = folder.to_string();
        entry.drag_target_pane = pane;
        drop(over);
        self.schedule_rebuild();
    }

    fn fm_dispatch_drag_move(self: &Arc<Self>, inst: Uuid, paths: Vec<String>, dest: String) {
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            if let Err(e) =
                orchid_widgets::builtin::file_manager::move_paths_to_directory(inst, paths, &dest)
                    .await
            {
                warn!(?e, dest = %dest, "fm drag drop");
            }
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_entry_drag_drop(self: &Arc<Self>, _pane: i32, folder: &SharedString) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let folder_path = folder.to_string();
        let paths = {
            let over = self.fm_overlays.read();
            over.get(&inst)
                .filter(|e| e.drag_active)
                .map(|e| e.drag_paths.clone())
                .unwrap_or_default()
        };
        if paths.is_empty() {
            self.clear_fm_drag(inst);
            self.schedule_rebuild();
            return;
        }
        self.clear_fm_drag(inst);
        self.schedule_rebuild();
        self.fm_dispatch_drag_move(inst, paths, folder_path);
    }

    fn on_fm_pane_drag_hover(self: &Arc<Self>, pane: i32) {
        let Some(inst) = self.find_active_fm() else {
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

    fn on_fm_drop_on_current_dir(self: &Arc<Self>, pane: i32) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
        let paths = {
            let over = self.fm_overlays.read();
            over.get(&inst)
                .filter(|e| e.drag_active)
                .map(|e| e.drag_paths.clone())
                .unwrap_or_default()
        };
        if paths.is_empty() {
            self.clear_fm_drag(inst);
            self.schedule_rebuild();
            return;
        }
        let dest = self.fm_active_tab_path(inst, p);
        self.clear_fm_drag(inst);
        self.schedule_rebuild();
        if let Some(dest) = dest.filter(|d| !d.is_empty()) {
            self.fm_dispatch_drag_move(inst, paths, dest);
        }
    }

    fn on_fm_entry_drag_cancel(self: &Arc<Self>, _pane: i32) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        self.clear_fm_drag(inst);
        self.schedule_rebuild();
    }

    fn on_fm_pane_clicked(self: &Arc<Self>, pane: i32) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let _ = orchid_widgets::builtin::file_manager::switch_active_pane(inst, p).await;
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_tab_clicked(self: &Arc<Self>, pane: i32, tab_id: &SharedString) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
        let tab = tab_id.to_string();
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let _ = orchid_widgets::builtin::file_manager::switch_to_tab(inst, p, &tab).await;
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_tab_closed(self: &Arc<Self>, pane: i32, tab_id: &SharedString) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
        let tab = tab_id.to_string();
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let _ = orchid_widgets::builtin::file_manager::close_tab(inst, p, &tab).await;
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_tab_new(self: &Arc<Self>, pane: i32) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let _ = orchid_widgets::builtin::file_manager::new_tab(inst, p).await;
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_new_folder(self: &Arc<Self>, pane: i32) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let outcome = match orchid_widgets::builtin::file_manager::request_new_folder(inst, p).await {
                Ok(o) => o,
                Err(e) => {
                    warn!(?e, "fm new folder");
                    return;
                }
            };
            if let Some(c) = tw.upgrade() {
                c.apply_fm_action_outcome(inst, outcome);
            }
        }));
    }

    fn on_fm_nav_back(self: &Arc<Self>, pane: i32) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let _ = orchid_widgets::builtin::file_manager::navigate_back(inst, p).await;
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_nav_forward(self: &Arc<Self>, pane: i32) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let _ = orchid_widgets::builtin::file_manager::navigate_forward(inst, p).await;
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_nav_up(self: &Arc<Self>, pane: i32) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let _ = orchid_widgets::builtin::file_manager::navigate_up(inst, p).await;
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_breadcrumb_clicked(self: &Arc<Self>, pane: i32, path: &SharedString) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
        let raw = path.to_string();
        let Ok(fs_path) = orchid_fs::FsPath::new(raw) else {
            return;
        };
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let _ = orchid_widgets::builtin::file_manager::navigate(inst, p, fs_path).await;
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_view_mode_cycle(self: &Arc<Self>, pane: i32) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let _ = orchid_widgets::builtin::file_manager::cycle_view_mode(inst, p).await;
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_sort_cycle(self: &Arc<Self>, pane: i32) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let _ = orchid_widgets::builtin::file_manager::cycle_sort(inst, p).await;
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_sort_column_clicked(self: &Arc<Self>, pane: i32, column: i32) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
        let col = column.max(0).min(3) as u8;
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let _ = orchid_widgets::builtin::file_manager::set_sort_column(inst, p, col).await;
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_quick_filter_changed(self: &Arc<Self>, pane: i32, q: &SharedString) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
        let query = q.to_string();
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let _ = orchid_widgets::builtin::file_manager::set_quick_filter(inst, p, query).await;
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_entry_clicked(self: &Arc<Self>, pane: i32, path: &SharedString, ctrl: bool) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
        let ps = path.to_string();
        let ps_for_select = ps.clone();
        let mode = if ctrl {
            orchid_widgets::builtin::file_manager::SelectionMode::Toggle
        } else {
            orchid_widgets::builtin::file_manager::SelectionMode::Single
        };
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let _ =
                orchid_widgets::builtin::file_manager::select_entry(inst, p, &ps_for_select, mode)
                    .await;
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));

        let behavior = orchid_widgets::builtin::file_manager::click_behavior(inst)
            .unwrap_or(orchid_widgets::builtin::file_manager::ClickBehavior::DoubleToOpen);
        if behavior != orchid_widgets::builtin::file_manager::ClickBehavior::SingleToOpen {
            return;
        }
        self.fm_dispatch_open(inst, p, ps);
    }

    fn on_fm_entry_shift_clicked(self: &Arc<Self>, pane: i32, path: &SharedString) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
        let ps = path.to_string();
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let _ = orchid_widgets::builtin::file_manager::select_entry(
                inst,
                p,
                &ps,
                orchid_widgets::builtin::file_manager::SelectionMode::Range,
            )
            .await;
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_entry_double_clicked(self: &Arc<Self>, pane: i32, path: &SharedString, is_dir: bool) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
        let raw = path.to_string();
        let behavior = orchid_widgets::builtin::file_manager::click_behavior(inst)
            .unwrap_or(orchid_widgets::builtin::file_manager::ClickBehavior::DoubleToOpen);
        if is_dir {
            self.fm_dispatch_open(inst, p, raw);
            return;
        }
        if behavior == orchid_widgets::builtin::file_manager::ClickBehavior::DoubleToOpen {
            self.fm_dispatch_open(inst, p, raw);
        }
    }

    fn fm_dispatch_open(self: &Arc<Self>, inst: Uuid, pane: u8, path: String) {
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let outcome = match orchid_widgets::builtin::file_manager::open_path(inst, pane, &path)
                .await
            {
                Ok(o) => o,
                Err(e) => {
                    warn!(?e, path = %path, "fm open path");
                    return;
                }
            };
            if let Some(c) = tw.upgrade() {
                c.apply_fm_action_outcome(inst, outcome);
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_entry_context(self: &Arc<Self>, pane: i32, path: &SharedString, x: f32, y: f32) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
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
        let _ = slint::spawn_local(Compat::new(async move {
            if let Err(e) =
                orchid_widgets::builtin::file_manager::focus_context_target(inst, p, &target).await
            {
                warn!(?e, "fm context focus");
            }
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_context_action(self: &Arc<Self>, action_id: &SharedString, paths: &ModelRc<SharedString>) {
        let id = action_id.to_string();
        let path_vec: Vec<String> = (0..paths.row_count())
            .filter_map(|i| paths.row_data(i))
            .map(|s| s.to_string())
            .collect();
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
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
                    return;
                }
            };

            if let Some(c) = tw.upgrade() {
                c.apply_fm_action_outcome(inst, outcome);
            }
        }));
    }

    fn apply_fm_action_outcome(
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
                let dlg = FmConfirmDialog {
                    visible: true,
                    title: self.locale.tr("fm-confirm-title").into(),
                    message: message.into(),
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
                    proposed_name: "New folder".into(),
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
                let title = match purpose {
                    orchid_widgets::builtin::file_manager::PassphrasePurpose::Encrypt => {
                        self.locale.tr("fm-encrypt-title")
                    }
                    orchid_widgets::builtin::file_manager::PassphrasePurpose::Decrypt => {
                        self.locale.tr("fm-decrypt-title")
                    }
                    orchid_widgets::builtin::file_manager::PassphrasePurpose::Reveal => {
                        self.locale.tr("fm-reveal-title")
                    }
                    orchid_widgets::builtin::file_manager::PassphrasePurpose::RevealInViewer => {
                        self.locale.tr("fm-reveal-title")
                    }
                };
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
                entry.passphrase_paths = paths;
                entry.passphrase_purpose = Some(purpose);
                entry.passphrase = FmPassphraseState {
                    active: true,
                    proposed_passphrase: SharedString::new(),
                    title: title.into(),
                    ok_label: self.locale.tr("fm-rename-ok").into(),
                    cancel_label: self.locale.tr("fm-rename-cancel").into(),
                };
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
                let _ = slint::spawn_local(Compat::new(async move {
                    let _ =
                        MainWindowController::open_in_viewer_for_controller(tw2, fs_path).await;
                }));
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::OpenInViewerMany { paths } => {
                let tw2 = Arc::downgrade(self);
                let _ = slint::spawn_local(Compat::new(async move {
                    for path in paths {
                        let Ok(fs_path) = orchid_fs::FsPath::new(&path) else {
                            continue;
                        };
                        let _ = MainWindowController::open_in_viewer_for_controller(
                            tw2.clone(),
                            fs_path,
                        )
                        .await;
                    }
                }));
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

    fn on_fm_context_dismiss(self: &Arc<Self>) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
        entry.context_menu = empty_context_menu();
        drop(over);
        self.schedule_rebuild();
    }

    fn on_fm_confirm_yes(self: &Arc<Self>) {
        let Some(inst) = self.find_active_fm() else {
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
        let _ = slint::spawn_local(Compat::new(async move {
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
        }));
    }

    fn on_fm_confirm_no(self: &Arc<Self>) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
        entry.confirm_dialog = empty_confirm_dialog();
        drop(over);
        self.schedule_rebuild();
    }

    fn on_fm_rename_commit(self: &Arc<Self>, old_path: &SharedString, new_name: &SharedString) {
        let Some(inst) = self.find_active_fm() else {
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
            let _ = slint::spawn_local(Compat::new(async move {
                let _ =
                    orchid_widgets::builtin::file_manager::create_folder(inst, &parent, &newn).await;
                if let Some(c) = tw.upgrade() {
                    let mut over = c.fm_overlays.write();
                    let entry = over.entry(inst).or_insert_with(|| c.ensure_fm_overlays(inst));
                    entry.rename = empty_rename_state();
                    entry.create_folder_parent = None;
                    drop(over);
                    c.schedule_rebuild();
                }
            }));
            return;
        }
        let old = old_path.to_string();
        let newn = new_name.to_string();
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            let _ = orchid_widgets::builtin::file_manager::rename(inst, &old, &newn).await;
            if let Some(c) = tw.upgrade() {
                let mut over = c.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(|| c.ensure_fm_overlays(inst));
                entry.rename = empty_rename_state();
                drop(over);
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_rename_cancel(self: &Arc<Self>) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
        entry.rename = empty_rename_state();
        entry.create_folder_parent = None;
        drop(over);
        self.schedule_rebuild();
    }

    fn on_fm_tag_commit(self: &Arc<Self>, tag: &SharedString) {
        let Some(inst) = self.find_active_fm() else {
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
        let _ = slint::spawn_local(Compat::new(async move {
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
        }));
    }

    fn on_fm_tag_cancel(self: &Arc<Self>) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
        entry.tag = empty_tag_state();
        entry.tag_paths.clear();
        drop(over);
        self.schedule_rebuild();
    }

    fn on_fm_passphrase_commit(self: &Arc<Self>, passphrase: &SharedString) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let overlay = self.fm_overlays.read().get(&inst).cloned();
        let Some(over) = overlay else {
            return;
        };
        let purpose = over
            .passphrase_purpose
            .unwrap_or(orchid_widgets::builtin::file_manager::PassphrasePurpose::Encrypt);
        let paths = over.passphrase_paths.clone();
        let pw = passphrase.to_string();
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
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
                    warn!(?e, "fm passphrase");
                    return;
                }
            };
            if let Some(c) = tw.upgrade() {
                c.clear_fm_passphrase_overlay(inst);
                c.apply_fm_action_outcome(inst, outcome);
            }
        }));
    }

    fn on_fm_passphrase_cancel(self: &Arc<Self>) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        self.clear_fm_passphrase_overlay(inst);
        self.schedule_rebuild();
    }

    fn clear_fm_passphrase_overlay(self: &Arc<Self>, inst: Uuid) {
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
        entry.passphrase = empty_passphrase_state();
        entry.passphrase_paths.clear();
        entry.passphrase_purpose = None;
    }

    fn on_fm_select_all(self: &Arc<Self>, pane: i32) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            if let Err(e) =
                orchid_widgets::builtin::file_manager::select_all_in_pane(inst, p).await
            {
                warn!(?e, "fm select all");
                return;
            }
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_deselect_all(self: &Arc<Self>, pane: i32) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        let _ = slint::spawn_local(Compat::new(async move {
            if let Err(e) =
                orchid_widgets::builtin::file_manager::deselect_all_in_pane(inst, p).await
            {
                warn!(?e, "fm deselect all");
                return;
            }
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        }));
    }

    fn on_fm_delete_selected(self: &Arc<Self>, pane: i32) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let paths = self.fm_selected_paths(inst, pane.max(0) as u8);
        if paths.is_empty() {
            return;
        }
        self.spawn_fm_action(inst, "fs.delete", paths);
    }

    fn on_fm_copy_selected(self: &Arc<Self>, pane: i32) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let paths = self.fm_selected_paths(inst, pane.max(0) as u8);
        if paths.is_empty() {
            return;
        }
        self.spawn_fm_action(inst, "fs.copy", paths);
    }

    fn on_fm_paste_clipboard(self: &Arc<Self>, _pane: i32) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        self.spawn_fm_action(inst, "fs.paste", Vec::new());
    }

    fn on_fm_rename_selected(self: &Arc<Self>, pane: i32) {
        let Some(inst) = self.find_active_fm() else {
            return;
        };
        let paths = self.fm_selected_paths(inst, pane.max(0) as u8);
        if paths.len() != 1 {
            return;
        }
        self.spawn_fm_action(inst, "fs.rename", paths);
    }

    fn spawn_fm_action(self: &Arc<Self>, inst: Uuid, action_id: &str, paths: Vec<String>) {
        let tw = Arc::downgrade(self);
        let action_id = action_id.to_string();
        let _ = slint::spawn_local(Compat::new(async move {
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
                    return;
                }
            };
            if let Some(c) = tw.upgrade() {
                c.apply_fm_action_outcome(inst, outcome);
            }
        }));
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
        canvas_content_width: 1f32,
        canvas_content_height: 1f32,
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

fn fm_rgba_to_image(rgba: &[u8], width: u32, height: u32) -> Image {
    if width == 0 || height == 0 || rgba.is_empty() {
        return Image::default();
    }
    let buf =
        slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(rgba, width, height);
    Image::from_rgba8(buf)
}

fn base64_decode(input: &str) -> std::result::Result<Vec<u8>, ()> {
    fn val(b: u8) -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let bytes: Vec<u8> = input
        .bytes()
        .filter(|b| !matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
        .collect();

    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    if bytes.len() % 4 != 0 {
        return Err(());
    }

    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks_exact(4) {
        let a = val(chunk[0]).ok_or(())?;
        let b = val(chunk[1]).ok_or(())?;
        let c = if chunk[2] == b'=' {
            0
        } else {
            val(chunk[2]).ok_or(())?
        };
        let d = if chunk[3] == b'=' {
            0
        } else {
            val(chunk[3]).ok_or(())?
        };

        let n = ((a as u32) << 18) | ((b as u32) << 12) | ((c as u32) << 6) | (d as u32);
        out.push((n >> 16) as u8);
        if chunk[2] != b'=' {
            out.push((n >> 8) as u8);
        }
        if chunk[3] != b'=' {
            out.push(n as u8);
        }
    }

    Ok(out)
}

fn is_known_widget_type(type_id: &str) -> bool {
    matches!(
        type_id,
        "terminal"
            | "weather"
            | "moon"
            | "system"
            | "rss"
            | "search"
            | "media"
            | "password"
            | "viewer"
            | "file-manager"
    )
}

fn filter_catalog_items(locale: &LocaleManager, query: &str) -> Vec<DockWidgetType> {
    let q = query.trim().to_lowercase();
    dock_types_vec(locale)
        .into_iter()
        .filter(|d| {
            q.is_empty() || d.label.as_str().to_lowercase().contains(&q)
        })
        .collect()
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
        DockWidgetType {
            type_id: "rss".into(),
            label: locale.tr("dock-widget-rss").into(),
            icon: "rss".into(),
        },
        DockWidgetType {
            type_id: "search".into(),
            label: locale.tr("dock-widget-search").into(),
            icon: "search".into(),
        },
        DockWidgetType {
            type_id: "media".into(),
            label: locale.tr("dock-widget-media").into(),
            icon: "media".into(),
        },
        DockWidgetType {
            type_id: "password".into(),
            label: locale.tr("dock-widget-password").into(),
            icon: "password".into(),
        },
        DockWidgetType {
            type_id: "viewer".into(),
            label: locale.tr("dock-widget-viewer").into(),
            icon: "viewer".into(),
        },
        DockWidgetType {
            type_id: "file-manager".into(),
            label: locale.tr("dock-widget-fm").into(),
            icon: "fm".into(),
        },
    ]
}

fn fallback_widget_title(locale: &LocaleManager, type_id: &str) -> SharedString {
    match type_id {
        "weather" => locale.tr("dock-widget-weather").into(),
        "moon" => locale.tr("dock-widget-moon").into(),
        "system" => locale.tr("dock-widget-system").into(),
        "rss" => locale.tr("dock-widget-rss").into(),
        "universal-search" | "search" => locale.tr("dock-widget-search").into(),
        "media-player" | "media" => locale.tr("dock-widget-media").into(),
        "password-manager" | "password" => locale.tr("dock-widget-password").into(),
        "viewer" => locale.tr("dock-widget-viewer").into(),
        "file-manager" => locale.tr("dock-widget-fm").into(),
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
    RssModel,
    SearchModel,
    MediaModel,
    PasswordModel,
    ViewerModel,
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
        empty_weather_model(),
        empty_moon_model(),
        empty_system_model(),
        empty_rss_model(locale),
        empty_search_model(locale),
        empty_media_model(),
        empty_password_model(),
        empty_viewer_model(locale),
        empty_file_manager_model(locale),
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

fn empty_rss_model(locale: &LocaleManager) -> RssModel {
    RssModel {
        items: ModelRc::new(VecModel::default()),
        last_updated: SharedString::new(),
        error_summary: SharedString::new(),
        has_items: false,
        empty_state_text: locale.tr("rss-no-feeds").into(),
    }
}

fn build_rss_model(p: &orchid_widgets::RssPayload, locale: &LocaleManager) -> RssModel {
    let items: Vec<RssItemEntry> = p
        .items
        .iter()
        .map(|it| RssItemEntry {
            id: it.id.clone().into(),
            title: it.title.clone().into(),
            source: it.source_name.clone().into(),
            published: it.published_text.clone().into(),
            summary: it.summary_text.clone().unwrap_or_default().into(),
            link: it.link.clone().unwrap_or_default().into(),
        })
        .collect();
    let has_items = !items.is_empty();
    RssModel {
        items: ModelRc::new(VecModel::from(items)),
        last_updated: p.last_updated_text.clone().into(),
        error_summary: p.error_summary.clone().unwrap_or_default().into(),
        has_items,
        empty_state_text: locale.tr("rss-no-feeds").into(),
    }
}

fn empty_search_model(locale: &LocaleManager) -> SearchModel {
    SearchModel {
        query: SharedString::new(),
        candidates: ModelRc::new(VecModel::default()),
        is_searching: false,
        error: SharedString::new(),
        selected_index: -1,
        placeholder_text: locale.tr("search-placeholder").into(),
        empty_state_text: locale.tr("search-empty-state").into(),
        no_results_text: locale.tr("search-no-results-short").into(),
        searching_text: locale.tr("search-searching").into(),
        request_autofocus: false,
    }
}

fn empty_media_model() -> MediaModel {
    MediaModel {
        has_session: false,
        title: SharedString::new(),
        artist: SharedString::new(),
        album: SharedString::new(),
        source_app: SharedString::new(),
        position: SharedString::new(),
        duration: SharedString::new(),
        progress: 0.0,
        is_playing: false,
        has_thumbnail: false,
        thumbnail: Image::default(),
    }
}

fn empty_password_detail() -> PasswordDetail {
    PasswordDetail {
        has_selection: false,
        id: SharedString::new(),
        title: SharedString::new(),
        username: SharedString::new(),
        url: SharedString::new(),
        notes: SharedString::new(),
        totp_code: SharedString::new(),
        totp_remaining: 0,
        tags: ModelRc::new(VecModel::default()),
    }
}

fn empty_password_model() -> PasswordModel {
    PasswordModel {
        is_unlocked: false,
        lock_reason: SharedString::new(),
        entries: ModelRc::new(VecModel::default()),
        selected: empty_password_detail(),
        search_query: SharedString::new(),
        toast_message: SharedString::new(),
        toast_visible: false,
        request_autofocus: false,
    }
}

fn empty_viewer_model(locale: &LocaleManager) -> ViewerModel {
    ViewerModel {
        kind: 0,
        status: ViewerStatusModel {
            path_display: SharedString::new(),
            message: SharedString::new(),
            icon: SharedString::new(),
        },
        empty: ViewerEmptyModel {
            placeholder_text: locale.tr("viewer-no-file").into(),
        },
        image: empty_viewer_image_model(),
        pdf: empty_viewer_pdf_model(locale),
        text: empty_viewer_text_model(),
        archive: empty_viewer_archive_model(locale),
    }
}

#[derive(Clone)]
struct FileManagerOverlays {
    context_menu: FmContextMenu,
    confirm_dialog: FmConfirmDialog,
    rename: FmRenameState,
    tag: FmTagState,
    tag_paths: Vec<String>,
    passphrase: FmPassphraseState,
    passphrase_paths: Vec<String>,
    passphrase_purpose: Option<orchid_widgets::builtin::file_manager::PassphrasePurpose>,
    create_folder_parent: Option<String>,
    drag_active: bool,
    drag_paths: Vec<String>,
    drag_drop_target: String,
    drag_target_pane: i32,
}

fn empty_file_manager_model(locale: &LocaleManager) -> FileManagerModel {
    FileManagerModel {
        panes: ModelRc::new(VecModel::default()),
        active_pane: 0,
        dual_pane: false,
        dual_pane_label: locale.tr("fm-dual-pane-on").into(),
        clipboard_indicator: SharedString::new(),
        sidebar_items: build_sidebar_items(locale, ""),
        context_menu: empty_context_menu(),
        confirm_dialog: empty_confirm_dialog(),
        rename: empty_rename_state(),
        tag: empty_tag_state(),
        passphrase: empty_passphrase_state(),
        show_hidden: false,
        show_hidden_label: locale.tr("fm-show-hidden-off").into(),
        single_click_open: false,
        single_click_open_label: locale.tr("fm-click-single-off").into(),
        drag_active: false,
        drag_drop_target: SharedString::new(),
        drag_target_pane: -1,
    }
}

fn empty_passphrase_state() -> FmPassphraseState {
    FmPassphraseState {
        active: false,
        proposed_passphrase: SharedString::new(),
        title: SharedString::new(),
        ok_label: SharedString::new(),
        cancel_label: SharedString::new(),
    }
}

fn empty_tag_state() -> FmTagState {
    FmTagState {
        active: false,
        proposed_tag: SharedString::new(),
        title: SharedString::new(),
        ok_label: SharedString::new(),
        cancel_label: SharedString::new(),
    }
}

fn empty_context_menu() -> FmContextMenu {
    FmContextMenu {
        visible: false,
        x: 0.0,
        y: 0.0,
        actions: ModelRc::new(VecModel::default()),
        target_paths: ModelRc::new(VecModel::default()),
    }
}

fn empty_confirm_dialog() -> FmConfirmDialog {
    FmConfirmDialog {
        visible: false,
        title: SharedString::new(),
        message: SharedString::new(),
        confirm_label: SharedString::new(),
        cancel_label: SharedString::new(),
        pending_action: SharedString::new(),
        pending_paths: ModelRc::new(VecModel::default()),
    }
}

fn empty_rename_state() -> FmRenameState {
    FmRenameState {
        active: false,
        path: SharedString::new(),
        proposed_name: SharedString::new(),
        title: SharedString::new(),
        ok_label: SharedString::new(),
        cancel_label: SharedString::new(),
    }
}

fn fm_sidebar_id_for_path(path: &str) -> Option<&'static str> {
    match path {
        "virtual:recent" => Some("fav:recent"),
        "virtual:starred" => Some("fav:starred"),
        "virtual:tags" => Some("fav:tags"),
        "virtual:categories/images" => Some("cat:images"),
        "virtual:categories/documents" => Some("cat:documents"),
        "virtual:categories/video" => Some("cat:video"),
        "virtual:categories/audio" => Some("cat:audio"),
        "virtual:categories/archives" => Some("cat:archives"),
        _ => None,
    }
}

fn build_sidebar_items(locale: &LocaleManager, active_path: &str) -> ModelRc<FmSidebarItem> {
    let active_id = fm_sidebar_id_for_path(active_path);
    let items = vec![
        FmSidebarItem {
            id: "section:favorites".into(),
            label: locale.tr("fm-sidebar-favorites").into(),
            icon: "★".into(),
            indent: 0,
            is_section_header: true,
            is_active: false,
        },
        FmSidebarItem {
            id: "fav:starred".into(),
            label: locale.tr("fm-virtual-starred").into(),
            icon: "★".into(),
            indent: 1,
            is_section_header: false,
            is_active: active_id == Some("fav:starred"),
        },
        FmSidebarItem {
            id: "fav:tags".into(),
            label: locale.tr("fm-virtual-tags").into(),
            icon: "🏷".into(),
            indent: 1,
            is_section_header: false,
            is_active: active_id == Some("fav:tags"),
        },
        FmSidebarItem {
            id: "fav:recent".into(),
            label: locale.tr("fm-virtual-recent").into(),
            icon: "🕐".into(),
            indent: 1,
            is_section_header: false,
            is_active: active_id == Some("fav:recent"),
        },
        FmSidebarItem {
            id: "section:categories".into(),
            label: locale.tr("fm-sidebar-categories").into(),
            icon: "▾".into(),
            indent: 0,
            is_section_header: true,
            is_active: false,
        },
        FmSidebarItem {
            id: "cat:images".into(),
            label: locale.tr("fm-category-images").into(),
            icon: "🖼".into(),
            indent: 1,
            is_section_header: false,
            is_active: active_id == Some("cat:images"),
        },
        FmSidebarItem {
            id: "cat:documents".into(),
            label: locale.tr("fm-category-documents").into(),
            icon: "📄".into(),
            indent: 1,
            is_section_header: false,
            is_active: active_id == Some("cat:documents"),
        },
        FmSidebarItem {
            id: "cat:video".into(),
            label: locale.tr("fm-category-video").into(),
            icon: "🎬".into(),
            indent: 1,
            is_section_header: false,
            is_active: active_id == Some("cat:video"),
        },
        FmSidebarItem {
            id: "cat:audio".into(),
            label: locale.tr("fm-category-audio").into(),
            icon: "🎵".into(),
            indent: 1,
            is_section_header: false,
            is_active: active_id == Some("cat:audio"),
        },
        FmSidebarItem {
            id: "cat:archives".into(),
            label: locale.tr("fm-category-archives").into(),
            icon: "📦".into(),
            indent: 1,
            is_section_header: false,
            is_active: active_id == Some("cat:archives"),
        },
    ];
    ModelRc::new(VecModel::from(items))
}

fn build_file_manager_model(
    p: &orchid_widgets::FileManagerPayload,
    overlays: FileManagerOverlays,
    instance_id: Uuid,
    locale: &LocaleManager,
) -> FileManagerModel {
    let active_path = p
        .panes
        .get(p.active_pane as usize)
        .and_then(|pp| pp.tabs.get(pp.active_tab as usize))
        .map(|t| t.path_display.clone())
        .unwrap_or_default();
    let sidebar_items = build_sidebar_items(locale, &active_path);
    let sort_name_label = locale.tr("fm-sort-name");
    let sort_size_label = locale.tr("fm-sort-size");
    let sort_modified_label = locale.tr("fm-sort-modified");
    let sort_type_label = locale.tr("fm-sort-type");
    let panes: Vec<FmPane> = p
        .panes
        .iter()
        .map(|pp| {
            let tabs: Vec<FmTab> = pp
                .tabs
                .iter()
                .map(|t| {
                    let entries: Vec<FmEntry> = t
                        .entries
                        .iter()
                        .map(|e| {
                            let tags: Vec<FmTagChip> = e
                                .tags
                                .iter()
                                .map(|tag| FmTagChip {
                                    label: tag.clone().into(),
                                    color: slint::Color::from_argb_u8(255, 0x4d, 0x82, 0xff),
                                })
                                .collect();
                            let thumb_img = if e.has_thumbnail {
                                e.thumbnail_rgba
                                    .as_ref()
                                    .map(|rgba| fm_rgba_to_image(rgba, e.thumbnail_width, e.thumbnail_height))
                                    .unwrap_or_default()
                            } else {
                                Image::default()
                            };
                            FmEntry {
                                path: e.path.clone().into(),
                                name: e.name.clone().into(),
                                is_dir: e.is_dir,
                                size_text: e.size_text.clone().into(),
                                modified_text: e.modified_text.clone().into(),
                                type_text: e.type_text.clone().into(),
                                icon: e.icon.clone().into(),
                                has_thumbnail: e.has_thumbnail,
                                thumbnail_key: e.thumbnail_key.clone().unwrap_or_default().into(),
                                thumbnail: thumb_img,
                                is_selected: e.is_selected,
                                is_hidden: e.is_hidden,
                                is_encrypted: e.is_encrypted,
                                is_managed: e.is_managed,
                                is_starred: e.is_starred,
                                color_label: e.color_label.clone().unwrap_or_default().into(),
                                tags: ModelRc::new(VecModel::from(tags)),
                            }
                        })
                        .collect();

                    let breadcrumbs: Vec<FmBreadcrumb> = t
                        .breadcrumbs
                        .iter()
                        .map(|(bp, bl)| FmBreadcrumb {
                            path: bp.clone().into(),
                            label: bl.clone().into(),
                        })
                        .collect();

                    FmTab {
                        id: t.tab_id.clone().into(),
                        path_display: t.path_display.clone().into(),
                        breadcrumbs: ModelRc::new(VecModel::from(breadcrumbs)),
                        can_back: t.can_go_back,
                        can_forward: t.can_go_forward,
                        view_mode: view_mode_to_int(t.view_mode),
                        entries: ModelRc::new(VecModel::from(entries)),
                        selection_count: t.selection_count as i32,
                        status_text: t.status_text.clone().into(),
                        quick_filter: t.quick_filter.clone().into(),
                        is_loading: t.is_loading,
                        error: t.error.clone().unwrap_or_default().into(),
                        sort_by: t.sort_by as i32,
                        sort_descending: t.sort_descending,
                        sort_name_label: sort_name_label.clone().into(),
                        sort_size_label: sort_size_label.clone().into(),
                        sort_modified_label: sort_modified_label.clone().into(),
                        sort_type_label: sort_type_label.clone().into(),
                    }
                })
                .collect();
            FmPane {
                tabs: ModelRc::new(VecModel::from(tabs)),
                active_tab: pp.active_tab as i32,
            }
        })
        .collect();

    let show_hidden = orchid_widgets::builtin::file_manager::show_hidden(instance_id)
        .unwrap_or(false);
    let single_click_open = orchid_widgets::builtin::file_manager::click_behavior(instance_id)
        .map(|b| b == orchid_widgets::builtin::file_manager::ClickBehavior::SingleToOpen)
        .unwrap_or(false);

    FileManagerModel {
        panes: ModelRc::new(VecModel::from(panes)),
        active_pane: i32::from(p.active_pane),
        dual_pane: p.dual_pane,
        dual_pane_label: if p.dual_pane {
            locale.tr("fm-dual-pane-off").into()
        } else {
            locale.tr("fm-dual-pane-on").into()
        },
        clipboard_indicator: p.clipboard_indicator.clone().unwrap_or_default().into(),
        show_hidden,
        show_hidden_label: if show_hidden {
            locale.tr("fm-show-hidden-on").into()
        } else {
            locale.tr("fm-show-hidden-off").into()
        },
        single_click_open,
        single_click_open_label: if single_click_open {
            locale.tr("fm-click-single-on").into()
        } else {
            locale.tr("fm-click-single-off").into()
        },
        drag_active: overlays.drag_active,
        drag_drop_target: overlays.drag_drop_target.clone().into(),
        drag_target_pane: overlays.drag_target_pane,
        sidebar_items,
        context_menu: overlays.context_menu,
        confirm_dialog: overlays.confirm_dialog,
        rename: overlays.rename,
        tag: overlays.tag,
        passphrase: overlays.passphrase,
    }
}

fn view_mode_to_int(vm: orchid_widgets::FmViewMode) -> i32 {
    use orchid_widgets::FmViewMode::*;
    match vm {
        Icons => 0,
        List => 1,
        Details => 2,
        Gallery => 3,
    }
}

fn open_with_application_picker(path: &str) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("rundll32.exe")
            .args(["shell32.dll,OpenAs_RunDLL", path])
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        let escaped = path.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(
            r#"set appPath to POSIX path of (choose file with prompt "Open with" of type {{"com.apple.application-bundle"}})
do shell script "open -a " & quoted form of appPath & " " & quoted form of "{escaped}""#
        );
        let output = std::process::Command::new("osascript")
            .args(["-e", &script])
            .output()?;
        if !output.status.success() {
            return Ok(());
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        opener::open(path).map_err(|e| std::io::Error::other(e.to_string()))?;
    }
    Ok(())
}

fn fm_action_shortcut(id: &str) -> &'static str {
    match id {
        "fs.select-all" => "Ctrl+A",
        "fs.deselect-all" => "Esc",
        "fs.copy" => "Ctrl+C",
        "fs.paste" => "Ctrl+V",
        "fs.rename" => "F2",
        "fs.delete" => "Del",
        "fs.new-folder" => "Ctrl+Shift+N",
        _ => "",
    }
}

fn context_menu_item_label(
    a: &orchid_widgets::builtin::file_manager::ContextMenuItem,
    locale: &LocaleManager,
) -> SharedString {
    if a.id.starts_with("fs.tag:") || a.id.starts_with("fs.tag-remove:") || a.id.starts_with("fs.color-label:") {
        if a.id.starts_with("fs.tag-remove:") {
            return format!("− {}", a.label_key).into();
        }
        if a.id.starts_with("fs.color-label:") {
            return locale.tr(&a.label_key).into();
        }
        return a.label_key.clone().into();
    }
    locale.tr(&a.label_key).into()
}

fn context_menu_item_enabled(
    a: &orchid_widgets::builtin::file_manager::ContextMenuItem,
) -> bool {
    if a.id == "fs.tag-add" || a.id == "fs.tag-remove" || a.id == "fs.color-label" {
        return false;
    }
    a.enabled
}

fn build_context_subitems(
    actions: &[orchid_widgets::builtin::file_manager::ContextMenuItem],
    locale: &LocaleManager,
) -> Vec<FmContextSubitem> {
    let mut out = Vec::new();
    for a in actions {
        out.push(FmContextSubitem {
            id: a.id.clone().into(),
            label: context_menu_item_label(a, locale),
            icon: a.icon.into(),
            swatch_color: a.swatch_color.unwrap_or("").into(),
            enabled: a.enabled,
            is_separator: false,
        });
        if a.separator_after {
            out.push(FmContextSubitem {
                id: SharedString::new(),
                label: SharedString::new(),
                icon: SharedString::new(),
                swatch_color: SharedString::new(),
                enabled: false,
                is_separator: true,
            });
        }
    }
    out
}

fn build_context_menu_actions(
    actions: &[orchid_widgets::builtin::file_manager::ContextMenuItem],
    locale: &LocaleManager,
) -> Vec<FmContextAction> {
    let mut out = Vec::new();
    for a in actions {
        let children = build_context_subitems(&a.submenu, locale);
        out.push(FmContextAction {
            id: a.id.clone().into(),
            label: context_menu_item_label(a, locale),
            shortcut: fm_action_shortcut(&a.id).into(),
            icon: a.icon.into(),
            enabled: context_menu_item_enabled(a),
            is_separator: false,
            has_submenu: !a.submenu.is_empty(),
            children: ModelRc::new(VecModel::from(children)),
        });
        if a.separator_after {
            out.push(FmContextAction {
                id: SharedString::new(),
                label: SharedString::new(),
                shortcut: SharedString::new(),
                icon: SharedString::new(),
                enabled: false,
                is_separator: true,
                has_submenu: false,
                children: ModelRc::new(VecModel::default()),
            });
        }
    }
    out
}

fn build_context_menu(
    actions: &[orchid_widgets::builtin::file_manager::ContextMenuItem],
    target_paths: &[String],
    x: f32,
    y: f32,
    locale: &LocaleManager,
) -> FmContextMenu {
    let actions_vec = build_context_menu_actions(actions, locale);
    let paths_vec: Vec<SharedString> = target_paths.iter().cloned().map(Into::into).collect();
    FmContextMenu {
        visible: true,
        x,
        y,
        actions: ModelRc::new(VecModel::from(actions_vec)),
        target_paths: ModelRc::new(VecModel::from(paths_vec)),
    }
}

fn empty_viewer_image_model() -> ViewerImageModel {
    ViewerImageModel {
        width_px: 0,
        height_px: 0,
        rgba_image: Image::default(),
        zoom: 1.0,
        pan_x: 0.0,
        pan_y: 0.0,
        rotation_deg: 0,
        flipped_h: false,
        flipped_v: false,
        info_text: SharedString::new(),
        path_display: SharedString::new(),
    }
}

fn empty_viewer_pdf_model(locale: &LocaleManager) -> ViewerPdfModel {
    ViewerPdfModel {
        page_count: 0,
        current_page: 0,
        page_width_px: 0,
        page_height_px: 0,
        page_image: Image::default(),
        zoom: 1.0,
        info_text: SharedString::new(),
        path_display: SharedString::new(),
        available: true,
        unavailable_reason: locale.tr("viewer-pdf-unavailable").into(),
    }
}

fn empty_viewer_text_model() -> ViewerTextModel {
    ViewerTextModel {
        language: "plaintext".into(),
        encoding: "UTF-8".into(),
        line_ending: "LF".into(),
        dirty: false,
        read_only: true,
        total_lines: 0,
        first_visible_line: 0,
        cursor_line: 0,
        cursor_col: 0,
        visible_lines: ModelRc::new(VecModel::default()),
        info_text: SharedString::new(),
        path_display: SharedString::new(),
    }
}

fn empty_viewer_archive_model(locale: &LocaleManager) -> ViewerArchiveModel {
    ViewerArchiveModel {
        format: SharedString::new(),
        total_entries: 0,
        current_inner_path: SharedString::new(),
        breadcrumbs: ModelRc::new(VecModel::default()),
        entries: ModelRc::new(VecModel::default()),
        selected_path: SharedString::new(),
        preview_kind: 0,
        preview_text: locale.tr("viewer-archive-select-preview").into(),
        preview_binary_size: SharedString::new(),
        info_text: SharedString::new(),
        path_display: SharedString::new(),
    }
}

fn build_viewer_model(p: &ViewerPayload, locale: &LocaleManager) -> ViewerModel {
    use orchid_viewers::ViewerError;
    use orchid_viewers::ViewerSnapshot as Vs;

    let mut model = empty_viewer_model(locale);

    match &p.snapshot {
        Vs::Loading { path_display } if path_display.is_empty() => {
            model.kind = 0;
        }
        Vs::Loading { path_display } => {
            model.kind = 1;
            model.status.path_display = path_display.clone().into();
            model.status.icon = "loading".into();
            let args = orchid_i18n::FluentArgs::new().with("path", path_display.as_str());
            model.status.message = locale.tr_args("viewer-loading-path", &args).into();
        }
        Vs::Error {
            path_display,
            message,
        } if *message == ViewerError::PdfUnavailable.to_string() => {
            model.kind = 4;
            model.pdf.path_display = path_display.clone().into();
            model.pdf.available = false;
            model.pdf.unavailable_reason = locale.tr("viewer-pdf-unavailable").into();
        }
        Vs::Error {
            path_display,
            message,
        } => {
            model.kind = 2;
            model.status.path_display = path_display.clone().into();
            model.status.icon = "error".into();
            let args = orchid_i18n::FluentArgs::new().with("reason", message.as_str());
            model.status.message = locale.tr_args("viewer-error-with-reason", &args).into();
        }
        Vs::Image(s) => {
            model.kind = 3;
            model.image = build_image_snapshot(s);
        }
        Vs::Pdf(s) => {
            model.kind = 4;
            model.pdf = build_pdf_snapshot(s, locale);
        }
        Vs::Text(s) => {
            model.kind = 5;
            model.text = build_text_snapshot(s);
        }
        Vs::Archive(s) => {
            model.kind = 6;
            model.archive = build_archive_snapshot(s, locale);
        }
    }

    model
}

fn build_image_snapshot(s: &orchid_viewers::ImageSnapshot) -> ViewerImageModel {
    let image = if s.width_px > 0 && s.height_px > 0 && !s.rgba_bytes.is_empty() {
        let img = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
            s.rgba_bytes.as_slice(),
            s.width_px,
            s.height_px,
        );
        Image::from_rgba8(img)
    } else {
        Image::default()
    };

    ViewerImageModel {
        width_px: s.width_px as i32,
        height_px: s.height_px as i32,
        rgba_image: image,
        zoom: s.zoom,
        pan_x: s.pan_x,
        pan_y: s.pan_y,
        rotation_deg: i32::from(s.rotation_degrees),
        flipped_h: s.flipped_horizontal,
        flipped_v: s.flipped_vertical,
        info_text: s.info_text.clone().into(),
        path_display: s.path_display.clone().into(),
    }
}

fn build_pdf_snapshot(s: &orchid_viewers::PdfSnapshot, locale: &LocaleManager) -> ViewerPdfModel {
    let available = !s.page_rgba_bytes.is_empty() && s.page_count > 0;
    let image = if available {
        let img = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
            s.page_rgba_bytes.as_slice(),
            s.page_width_px,
            s.page_height_px,
        );
        Image::from_rgba8(img)
    } else {
        Image::default()
    };

    ViewerPdfModel {
        page_count: s.page_count as i32,
        current_page: s.current_page as i32,
        page_width_px: s.page_width_px as i32,
        page_height_px: s.page_height_px as i32,
        page_image: image,
        zoom: s.zoom,
        info_text: s.info_text.clone().into(),
        path_display: s.path_display.clone().into(),
        available,
        unavailable_reason: if available {
            SharedString::new()
        } else {
            locale.tr("viewer-pdf-unavailable").into()
        },
    }
}

fn build_text_snapshot(s: &orchid_viewers::TextSnapshot) -> ViewerTextModel {
    let lines: Vec<ViewerSyntaxLine> = s
        .visible_lines
        .iter()
        .map(|line| {
            let segments: Vec<ViewerSyntaxSegment> = line
                .segments
                .iter()
                .map(|seg| ViewerSyntaxSegment {
                    text: seg.text.clone().into(),
                    scope: syntax_scope_to_int(&seg.scope),
                })
                .collect();
            ViewerSyntaxLine {
                line_number: line.line_number as i32,
                segments: ModelRc::new(VecModel::from(segments)),
            }
        })
        .collect();

    ViewerTextModel {
        language: s.language.clone().into(),
        encoding: s.encoding.clone().into(),
        line_ending: s.line_ending.clone().into(),
        dirty: s.dirty,
        read_only: s.read_only,
        total_lines: s.total_lines as i32,
        first_visible_line: s.first_visible_line as i32,
        cursor_line: s.cursor_line as i32,
        cursor_col: s.cursor_column as i32,
        visible_lines: ModelRc::new(VecModel::from(lines)),
        info_text: s.info_text.clone().into(),
        path_display: s.path_display.clone().into(),
    }
}

fn syntax_scope_to_int(scope: &orchid_viewers::SyntaxScope) -> i32 {
    use orchid_viewers::SyntaxScope::*;
    match scope {
        Plain => 0,
        Keyword => 1,
        String => 2,
        Number => 3,
        Comment => 4,
        Function => 5,
        Type => 6,
        Variable => 7,
        Constant => 8,
        Operator => 9,
        Punctuation => 10,
        Attribute => 11,
        Preprocessor => 12,
        Tag => 13,
        Property => 14,
        Error => 15,
    }
}

fn format_byte_size(n: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let f = n as f64;
    if f >= MB {
        format!("{:.1} MB", f / MB)
    } else if f >= KB {
        format!("{:.0} KB", f / KB)
    } else {
        format!("{n} B")
    }
}

fn build_archive_snapshot(s: &orchid_viewers::ArchiveSnapshot, locale: &LocaleManager) -> ViewerArchiveModel {
    let mut entries: Vec<ViewerArchiveEntry> = Vec::with_capacity(s.entries.len() + 1);

    if !s.current_inner_path.is_empty() {
        entries.push(ViewerArchiveEntry {
            path_in_archive: SharedString::new(),
            name: "..".into(),
            is_dir: true,
            size_text: SharedString::new(),
            modified_text: SharedString::new(),
            icon: "up".into(),
            is_up: true,
        });
    }

    for e in &s.entries {
        entries.push(ViewerArchiveEntry {
            path_in_archive: e.path_in_archive.clone().into(),
            name: e.name.clone().into(),
            is_dir: e.is_dir,
            size_text: format_byte_size(e.size).into(),
            modified_text: e.modified_text.clone().into(),
            icon: e.icon.into(),
            is_up: false,
        });
    }

    let breadcrumbs: Vec<SharedString> = s
        .current_inner_path
        .split('/')
        .filter(|seg| !seg.is_empty())
        .map(|p| p.into())
        .collect();

    let (preview_kind, preview_text, preview_binary) = match &s.preview {
        Some(orchid_viewers::ArchivePreview::Text(t)) => (1, t.clone().into(), SharedString::new()),
        Some(orchid_viewers::ArchivePreview::Binary { size }) => {
            let args = orchid_i18n::FluentArgs::new().with("size", format_byte_size(*size));
            (
                2,
                SharedString::new(),
                locale.tr_args("viewer-archive-binary-preview", &args).into(),
            )
        }
        None => (
            0,
            locale.tr("viewer-archive-select-preview").into(),
            SharedString::new(),
        ),
    };

    ViewerArchiveModel {
        format: s.format.clone().into(),
        total_entries: s.total_entries as i32,
        current_inner_path: s.current_inner_path.clone().into(),
        breadcrumbs: ModelRc::new(VecModel::from(breadcrumbs)),
        entries: ModelRc::new(VecModel::from(entries)),
        selected_path: s.selected_path.clone().into(),
        preview_kind,
        preview_text,
        preview_binary_size: preview_binary,
        info_text: s.info_text.clone().into(),
        path_display: s.path_display.clone().into(),
    }
}

fn build_media_model(p: &orchid_widgets::MediaPlayerPayload) -> MediaModel {
    let (has_thumb, thumb_img) = p
        .thumbnail_base64
        .as_ref()
        .and_then(|b64| {
            let bytes = base64_decode(b64).ok()?;
            let dyn_img = image::load_from_memory(&bytes).ok()?;
            let rgba = dyn_img.to_rgba8();
            let (w, h) = rgba.dimensions();
            if w == 0 || h == 0 {
                return None;
            }
            let buf =
                slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(rgba.as_raw(), w, h);
            Some((true, Image::from_rgba8(buf)))
        })
        .unwrap_or((false, Image::default()));
    MediaModel {
        has_session: p.has_session,
        title: p.title.clone().into(),
        artist: p.artist.clone().into(),
        album: p.album.clone().into(),
        source_app: p.source_app.clone().into(),
        position: p.position_text.clone().into(),
        duration: p.duration_text.clone().into(),
        progress: p.progress_fraction.clamp(0.0, 1.0),
        is_playing: p.is_playing,
        has_thumbnail: has_thumb,
        thumbnail: thumb_img,
    }
}

fn build_password_model(
    p: &orchid_widgets::PasswordManagerPayload,
    toast: Option<(String, bool)>,
    autofocus: bool,
) -> PasswordModel {
    let entries: Vec<PasswordEntryItem> = p
        .entries
        .iter()
        .map(|e| {
            let tags: Vec<SharedString> = e.tags.iter().map(|t| t.clone().into()).collect();
            PasswordEntryItem {
                id: e.id.clone().into(),
                title: e.title.clone().into(),
                username: e.username.clone().into(),
                url_host: e.url_host.clone().unwrap_or_default().into(),
                has_totp: e.has_totp,
                tags: ModelRc::new(VecModel::from(tags)),
                color_label: e.color_label.clone().unwrap_or_default().into(),
                modified: e.modified_text.clone().into(),
            }
        })
        .collect();

    let selected = match &p.selected {
        Some(d) => {
            let tags: Vec<PasswordTagChip> = d
                .tags
                .iter()
                .map(|t| PasswordTagChip {
                    label: t.clone().into(),
                })
                .collect();
            PasswordDetail {
                has_selection: true,
                id: d.id.clone().into(),
                title: d.title.clone().into(),
                username: d.username.clone().into(),
                url: d.url.clone().unwrap_or_default().into(),
                notes: d.notes.clone().unwrap_or_default().into(),
                totp_code: d.totp_code.clone().unwrap_or_default().into(),
                totp_remaining: d.totp_remaining_seconds as i32,
                tags: ModelRc::new(VecModel::from(tags)),
            }
        }
        None => empty_password_detail(),
    };

    let (toast_msg, toast_vis) = toast.unwrap_or((String::new(), false));

    PasswordModel {
        is_unlocked: p.is_unlocked,
        lock_reason: p.lock_reason.clone().unwrap_or_default().into(),
        entries: ModelRc::new(VecModel::from(entries)),
        selected,
        search_query: p.search_query.clone().into(),
        toast_message: toast_msg.into(),
        toast_visible: toast_vis,
        request_autofocus: autofocus,
    }
}

fn build_search_model(
    p: &orchid_widgets::UniversalSearchPayload,
    locale: &LocaleManager,
    selected: i32,
    request_autofocus: bool,
) -> SearchModel {
    let candidates: Vec<SearchCandidateEntry> = p
        .candidates
        .iter()
        .map(|c| {
            let title: SharedString = if c.source_name == "commands" {
                locale.tr(c.title.as_str()).into()
            } else {
                c.title.clone().into()
            };
            let source_label = match c.source_name.as_str() {
                "files" => locale.tr("search-source-files"),
                "commands" => locale.tr("search-source-commands"),
                "settings" => locale.tr("search-source-settings"),
                _ => c.source_name.clone(),
            };
            let subtitle: SharedString = match &c.subtitle {
                Some(s) => s.clone().into(),
                None => source_label.clone().into(),
            };
            SearchCandidateEntry {
                id: c.id.clone().into(),
                source_name: source_label.into(),
                source_icon: c.source_name.as_str().into(),
                title,
                subtitle,
                shortcut: c.shortcut_hint.clone().unwrap_or_default().into(),
            }
        })
        .collect();
    let max = candidates.len() as i32;
    let clamped = if candidates.is_empty() {
        -1
    } else {
        selected.clamp(0, max - 1)
    };
    SearchModel {
        query: p.query.clone().into(),
        candidates: ModelRc::new(VecModel::from(candidates)),
        is_searching: p.is_searching,
        error: p.error.clone().unwrap_or_default().into(),
        selected_index: clamped,
        placeholder_text: locale.tr("search-placeholder").into(),
        empty_state_text: locale.tr("search-empty-state").into(),
        no_results_text: locale.tr("search-no-results-short").into(),
        searching_text: locale.tr("search-searching").into(),
        request_autofocus,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_media_payload() -> orchid_widgets::MediaPlayerPayload {
        orchid_widgets::MediaPlayerPayload {
            has_session: true,
            title: "t".into(),
            artist: "a".into(),
            album: "al".into(),
            source_app: "app".into(),
            position_text: "0:00".into(),
            duration_text: "1:00".into(),
            progress_fraction: 0.5,
            is_playing: true,
            thumbnail_base64: None,
        }
    }

    #[test]
    fn media_progress_clamps() {
        let mut p = sample_media_payload();
        p.progress_fraction = 1.5;
        let m = build_media_model(&p);
        assert!(m.progress <= 1.0);

        p.progress_fraction = -0.3;
        let m = build_media_model(&p);
        assert!(m.progress >= 0.0);
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
