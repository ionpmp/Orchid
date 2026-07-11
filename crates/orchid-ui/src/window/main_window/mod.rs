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
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use slint::ComponentHandle;
use slint::Image;
use slint::Model;
use slint::ModelRc;
use slint::SharedString;
use slint::VecModel;
use slint::winit_030::WinitWindowAccessor;
use tracing::{debug, trace, warn};
use uuid::Uuid;

use orchid_core::{
    default_bindings_mirrored, ActionContext, ActionDispatcher, CommandPalette,
    CommandRegistry, ConfigUpdated, Event, EventBus, EventFilter, GestureConfig,
    GestureRecognizer, HandlerPriority, HistoryRecorder, InputEvent, InputMapper, ParsedCommand,
    Point, RecognizedGesture, ScreenBounds, Shortcut, SubscriptionHandle, TouchEvent, TouchPhase,
};
use orchid_i18n::{LocaleId, LocaleManager};
use orchid_storage::{ConfigLoader, OrchidConfig, StateStore, WidgetSize};
use orchid_terminal::SessionManager;
use orchid_terminal::{FontMetrics};
use orchid_widgets::layout::PixelBounds;
use orchid_widgets::layout::ViewportSize;
use orchid_widgets::WidgetPayload;
use orchid_widgets::{CreateWidgetRequest,
    GroupManager, LayoutEngine, PlacedWidget, RecentFilesStore, WidgetManager, WorkspaceManager,
};
use orchid_widgets::SharedInstance;
use parking_lot::RwLock;

use super::errors::{
    storage_localized_error, ui_localized_error, viewer_localized_error,
};
use super::spawn;
use super::models::{
    blank_terminal, build_file_manager_model,
    build_media_model, build_moon_model, build_palette_candidates, build_password_model,
    build_recent_files_model, build_rss_model, build_search_model, build_settings_fields,
    build_settings_sections, build_system_model, build_terminal_divider_models,
    build_terminal_model, build_terminal_tab_models, build_viewer_model, build_weather_model,
    default_terminal_divider_models, default_terminal_pane_models, default_terminal_tab_models,
    empty_confirm_dialog, empty_context_menu, empty_file_manager_model, empty_managed_policy_state,
    empty_media_model, empty_moon_model, empty_passphrase_state, empty_password_model,
    empty_recent_files_model, empty_rename_state, empty_rss_model, empty_search_model,
    empty_system_model, empty_tag_state, empty_viewer_model, empty_weather_model, locale_display_name, settings_section_id,
    settings_section_index, theme_display_name, FileManagerOverlays, PasswordAddDialogOverlay,
    SETTINGS_SECTION_IDS,
};
use crate::error::{Result, UiError};
use crate::terminal_font_metrics;
use crate::widgets::terminal::TerminalWidgetDeps;
use crate::terminal_raster;
use crate::slint_generated::{
    AppState, DockWidgetType, MainWindow, MediaModel, MoonModel, PasswordModel, RecentFilesModel,
    RssModel, SearchCandidateEntry,
    SearchModel, Strings, SystemModel, TerminalCellModel,
    Theme,
    FileManagerModel,
    ViewerModel,
    WeatherModel, WidgetCatalog, WidgetCloseConfirmDialog, WidgetFrameModel, WorkspaceModel,
    WorkspaceSummary, CommandPaletteGlobal, NavigationGlobal, NotificationGlobal,
    NotificationItem, OnboardingGlobal, SettingsFieldRow,
    SettingsGlobal,
    SettingsSectionEntry, GroupTabModel,
};
use crate::theme::ThemeManager;

mod canvas;
mod fm;
mod media_search;
mod password;
mod terminal;
mod wire;

use canvas::ResizeInteraction;


/// Max command palette hits (fuzzy search or browse).
const COMMAND_PALETTE_LIMIT: usize = 50;

/// Top switcher (40) + bottom dock (64 when visible) = canvas height inset in [`workspace.slint`].
const WORKSPACE_SWITCHER_H: f32 = 40.0;
const DOCK_H: f32 = 64.0;

const ONBOARDING_STEP_COUNT: i32 = 4;
const ONBOARDING_STEP_KEYS: [(&str, &str); 4] = [
    ("onboarding-step-welcome-title", "onboarding-step-welcome-body"),
    ("onboarding-step-workspace-title", "onboarding-step-workspace-body"),
    ("onboarding-step-palette-title", "onboarding-step-palette-body"),
    ("onboarding-step-gestures-title", "onboarding-step-gestures-body"),
];


/// Drives the main window: workspace model, terminal I/O, drag/resize previews.
pub struct MainWindowController {
    window: MainWindow,
    theme: Arc<ThemeManager>,
    locale: Arc<LocaleManager>,
    config: Arc<RwLock<OrchidConfig>>,
    storage: Arc<StateStore>,
    command_registry: Arc<CommandRegistry>,
    bus: Arc<EventBus>,
    _config_reload_sub: SubscriptionHandle,
    _fm_ingest_failed_sub: SubscriptionHandle,
    /// Managed ingest failure file name; drained on the next UI tick.
    fm_ingest_failure_pending: Arc<Mutex<Option<String>>>,
    /// Last file-manager transfer error mirrored to the notification center.
    last_fm_transfer_error: Arc<Mutex<Option<String>>>,
    widget_manager: Arc<WidgetManager>,
    workspace_manager: Arc<WorkspaceManager>,
    layout_engine: Arc<LayoutEngine>,
    group_manager: Arc<GroupManager>,
    session_manager: Arc<SessionManager>,
    session_routing: Arc<Mutex<HashMap<Uuid, Uuid>>>,
    terminal_deps: TerminalWidgetDeps,
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
    /// Set when `config.toml` hot-reload completes; applied on the next UI tick.
    config_reload_pending: Arc<AtomicBool>,
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
    /// Per password-manager instance: add-entry dialog overlay state.
    password_add_dialogs: Arc<RwLock<HashMap<Uuid, PasswordAddDialogOverlay>>>,
    /// UI-only overlays for file-manager widgets (context menu, confirm dialog, rename).
    fm_overlays: Arc<RwLock<HashMap<Uuid, FileManagerOverlays>>>,
    /// Unsaved text close-confirm overlays for viewer widgets.
    close_confirm_overlays: Arc<RwLock<HashMap<Uuid, WidgetCloseConfirmDialog>>>,
    /// Last text-viewer instance that received an edit (for Ctrl+S when focus left the input).
    last_text_edit_instance: Arc<Mutex<Option<Uuid>>>,
    /// Last interacted file-manager instance and pane (for drop targeting).
    fm_focus: Arc<Mutex<Option<(Uuid, u8)>>>,
    /// Last pointer position in workspace canvas coordinates (content space).
    last_canvas_pointer: Arc<Mutex<Option<(f32, f32)>>>,
    /// Canvas flickable scroll offset (content coordinates).
    canvas_scroll: Arc<Mutex<(f32, f32)>>,
    /// Last winit keyboard modifier state (Ctrl+drop → copy).
    keyboard_modifiers: Arc<Mutex<slint::winit_030::winit::keyboard::ModifiersState>>,
    /// Leader-key chord window deadline; `None` when not armed.
    leader_pending_until: Arc<Mutex<Option<Instant>>>,
    /// Pending OS file-drop paths batched across rapid `DroppedFile` events.
    os_drop_batch: Arc<Mutex<OsDropBatch>>,
    /// Long-press widget catalog (search + pick).
    catalog: Arc<RwLock<CatalogUiState>>,
    catalog_items: ModelRc<DockWidgetType>,
    command_palette: Arc<CommandPalette>,
    palette: Arc<RwLock<PaletteUiState>>,
    palette_candidates: ModelRc<SearchCandidateEntry>,
    settings: Arc<RwLock<SettingsUiState>>,
    config_file_path: PathBuf,
    settings_sections: ModelRc<SettingsSectionEntry>,
    settings_fields: ModelRc<SettingsFieldRow>,
    navigation: Arc<RwLock<NavigationUiState>>,
    /// UI-owned notification-center items (newest first).
    notifications: ModelRc<NotificationItem>,
    /// True after the first startup tip has been pushed (once per window).
    notification_tip_pushed: AtomicBool,
    onboarding: Arc<RwLock<OnboardingUiState>>,
    gesture_recognizer: Arc<Mutex<GestureRecognizer>>,
    input_mapper: Arc<InputMapper>,
    recent_files: Arc<RecentFilesStore>,
    password_vault: Arc<orchid_crypto::PasswordVault>,
    /// Last unlock or vault interaction; used for privacy.vault_auto_lock_seconds.
    vault_last_activity: Arc<Mutex<Option<Instant>>>,
    fm_passphrase_vault: Arc<orchid_crypto::FmPassphraseVault>,
    /// Persists dispatched actions when [`OrchidConfig::privacy::record_action_history`] is on.
    history_recorder: Arc<HistoryRecorder>,
    /// Last applied [`OrchidConfig::privacy::history_retention_days`]; used to detect hot-config changes.
    last_history_retention_days: AtomicU32,
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

#[derive(Debug, Clone, Default)]
struct PaletteUiState {
    visible: bool,
    query: String,
    selected_index: i32,
    request_autofocus: bool,
}

#[derive(Debug, Clone, Default)]
struct SettingsUiState {
    visible: bool,
    section: String,
}

#[derive(Debug, Clone)]
struct NavigationUiState {
    workspace_panel_visible: bool,
    notification_center_visible: bool,
    dock_visible: bool,
}

impl Default for NavigationUiState {
    fn default() -> Self {
        Self {
            workspace_panel_visible: false,
            notification_center_visible: false,
            dock_visible: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct OnboardingUiState {
    overlay_visible: bool,
    current_step: i32,
}

#[derive(Debug, Clone, Copy)]
enum AddWidgetPlacement {
    AutoSlot,
    CanvasPoint { content_x: f32, content_y: f32 },
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PasswordCopyKind {
    Password,
    Username,
    Totp,
}

struct OsDropBatch {
    paths: Vec<String>,
    generation: u64,
}

impl Default for OsDropBatch {
    fn default() -> Self {
        Self {
            paths: Vec::new(),
            generation: 0,
        }
    }
}

impl MainWindowController {
    /// Build the window, apply globals, and wire Slint callbacks.
    #[allow(clippy::too_many_arguments, clippy::arc_with_non_send_sync)]
    pub fn new(
        theme: Arc<ThemeManager>,
        locale: Arc<LocaleManager>,
        config: Arc<RwLock<OrchidConfig>>,
        config_file_path: PathBuf,
        recent_files: Arc<RecentFilesStore>,
        password_vault: Arc<orchid_crypto::PasswordVault>,
        fm_passphrase_vault: Arc<orchid_crypto::FmPassphraseVault>,
        storage: Arc<StateStore>,
        bus: Arc<EventBus>,
        command_registry: Arc<CommandRegistry>,
        command_palette: Arc<CommandPalette>,
        widget_manager: Arc<WidgetManager>,
        workspace_manager: Arc<WorkspaceManager>,
        layout_engine: Arc<LayoutEngine>,
        group_manager: Arc<GroupManager>,
        session_manager: Arc<SessionManager>,
        session_routing: Arc<Mutex<HashMap<Uuid, Uuid>>>,
        terminal_deps: TerminalWidgetDeps,
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
        let palette_candidates: ModelRc<SearchCandidateEntry> =
            ModelRc::new(VecModel::<SearchCandidateEntry>::default());
        let settings_sections: ModelRc<SettingsSectionEntry> =
            ModelRc::new(VecModel::<SettingsSectionEntry>::default());
        let settings_fields: ModelRc<SettingsFieldRow> =
            ModelRc::new(VecModel::<SettingsFieldRow>::default());
        let notifications: ModelRc<NotificationItem> =
            ModelRc::new(VecModel::<NotificationItem>::default());
        if let Ok(Ok(Some(saved))) = storage.read().map(|r| r.get_notification_center()) {
            if let Some(model) = notifications
                .as_any()
                .downcast_ref::<VecModel<NotificationItem>>()
            {
                let rows: Vec<NotificationItem> = saved
                    .items
                    .into_iter()
                    .take(Self::NOTIFICATION_LIST_CAP)
                    .map(|n| NotificationItem {
                        id: n.id.into(),
                        title: n.title.into(),
                        body: n.body.into(),
                        time_label: n.time_label.into(),
                        severity: n.severity,
                    })
                    .collect();
                model.set_vec(rows);
            }
        }
        let config_reload_pending = Arc::new(AtomicBool::new(false));
        let config_reload_flag = config_reload_pending.clone();
        let config_reload_sub = bus
            .subscribe_async(
                EventFilter::of_type(ConfigUpdated::event_type()),
                HandlerPriority::Normal,
                move |_env| {
                    let flag = config_reload_flag.clone();
                    async move {
                        flag.store(true, Ordering::Release);
                    }
                },
            )
            .map_err(|e| UiError::Slint(format!("config reload sub: {e}")))?;
        let fm_ingest_failure_pending = Arc::new(Mutex::new(None));
        let fm_ingest_flag = fm_ingest_failure_pending.clone();
        let fm_ingest_failed_sub = bus
            .subscribe_async(
                EventFilter::of_type(orchid_fs::ManagedFileIngestFailedEvent::event_type()),
                HandlerPriority::Normal,
                move |env| {
                    let name = env
                        .downcast_arc::<orchid_fs::ManagedFileIngestFailedEvent>()
                        .map(|e| {
                            e.path
                                .file_name()
                                .map(String::from)
                                .unwrap_or_else(|| e.path.as_str().to_string())
                        });
                    let flag = fm_ingest_flag.clone();
                    async move {
                        if let Some(name) = name {
                            *flag.lock() = Some(name);
                        }
                    }
                },
            )
            .map_err(|e| UiError::Slint(format!("fm ingest failed sub: {e}")))?;
        let input_mapper = Arc::new(InputMapper::new());
        let gesture_recognizer = Arc::new(Mutex::new(GestureRecognizer::new(
            GestureConfig::default(),
            ScreenBounds::new(800.0, 600.0),
        )));
        let history_recorder = Arc::new(HistoryRecorder::new(
            storage.clone(),
            config.read().privacy.record_action_history,
        ));
        let last_history_retention_days = config.read().privacy.history_retention_days;
        let this = Arc::new(Self {
            window,
            theme,
            locale,
            config,
            storage,
            command_registry,
            command_palette: command_palette.clone(),
            bus,
            _config_reload_sub: config_reload_sub,
            _fm_ingest_failed_sub: fm_ingest_failed_sub,
            fm_ingest_failure_pending,
            last_fm_transfer_error: Arc::new(Mutex::new(None)),
            widget_manager: widget_manager.clone(),
            workspace_manager: workspace_manager.clone(),
            layout_engine: layout_engine.clone(),
            group_manager,
            session_manager: session_manager.clone(),
            session_routing,
            terminal_deps,
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
            config_reload_pending,
            last_window_scale: parking_lot::Mutex::new(0.0),
            last_terminal_viewport_pty: Arc::new(Mutex::new(HashMap::new())),
            workspace_workspaces,
            workspace_widgets,
            workspace_dock_types,
            search_selection: Arc::new(RwLock::new(HashMap::new())),
            search_autofocus_pending: Arc::new(Mutex::new(None)),
            password_toasts: Arc::new(RwLock::new(HashMap::new())),
            password_autofocus_pending: Arc::new(RwLock::new(HashMap::new())),
            password_add_dialogs: Arc::new(RwLock::new(HashMap::new())),
            fm_overlays: Arc::new(RwLock::new(HashMap::new())),
            close_confirm_overlays: Arc::new(RwLock::new(HashMap::new())),
            last_text_edit_instance: Arc::new(Mutex::new(None)),
            fm_focus: Arc::new(Mutex::new(None)),
            last_canvas_pointer: Arc::new(Mutex::new(None)),
            canvas_scroll: Arc::new(Mutex::new((0.0, 0.0))),
            keyboard_modifiers: Arc::new(Mutex::new(
                slint::winit_030::winit::keyboard::ModifiersState::empty(),
            )),
            leader_pending_until: Arc::new(Mutex::new(None)),
            os_drop_batch: Arc::new(Mutex::new(OsDropBatch::default())),
            catalog: Arc::new(RwLock::new(CatalogUiState::default())),
            catalog_items,
            palette: Arc::new(RwLock::new(PaletteUiState::default())),
            palette_candidates,
            settings: Arc::new(RwLock::new(SettingsUiState::default())),
            config_file_path,
            settings_sections,
            settings_fields,
            navigation: Arc::new(RwLock::new(NavigationUiState::default())),
            notifications,
            notification_tip_pushed: AtomicBool::new(false),
            onboarding: Arc::new(RwLock::new(OnboardingUiState::default())),
            gesture_recognizer,
            input_mapper,
            recent_files,
            password_vault,
            vault_last_activity: Arc::new(Mutex::new(None)),
            fm_passphrase_vault,
            history_recorder,
            last_history_retention_days: AtomicU32::new(last_history_retention_days),
        });
        this.apply_input_gesture_bindings();
        this.apply_theme()?;
        this.apply_strings()?;
        this.sync_widget_catalog_global();
        this.sync_command_palette_global();
        this.sync_settings_global();
        this.sync_navigation_global();
        this.sync_notification_global();
        this.apply_initial_mode()?;
        if let Err(e) = this.evict_history_by_retention_policy() {
            warn!(?e, "action history retention prune on startup");
        }
        if !this.config.read().onboarding.completed {
            let mut ob = this.onboarding.write();
            ob.overlay_visible = true;
            ob.current_step = 0;
        }
        this.sync_onboarding_global();
        this.ensure_startup_notification_tip();
        this.wire_callbacks()?;
        Ok(this)
    }

    fn apply_theme(self: &Arc<Self>) -> Result<()> {
        let theme = self.theme.current();
        let cfg = self.config.read();
        let (canvas_w, _) = *self.canvas_size.lock();
        let scale = crate::window::effective_ui_scale(cfg.appearance.density, canvas_w)
            * cfg.appearance.font_scale.clamp(0.75, 2.0);
        let reduce_motion = cfg.appearance.reduce_motion;
        let font_sans = crate::system_theme::resolve_font_family_sans(
            &cfg.appearance,
            &theme.tokens.typography.font_family_sans,
        );
        drop(cfg);
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
        g.set_font_family_sans(font_sans.into());
        g.set_font_family_mono(t.typography.font_family_mono.clone().into());
        let sz = &t.typography;
        g.set_font_size_sm(sz.size_sm * scale);
        g.set_font_size_md(sz.size_md * scale);
        g.set_font_size_lg(sz.size_lg * scale);
        g.set_font_size_xl(sz.size_xl * scale);
        g.set_font_size_2xl(sz.size_2xl * scale);
        g.set_font_size_3xl(sz.size_3xl * scale);
        g.set_weight_regular(i32::from(sz.weight_regular));
        g.set_weight_medium(i32::from(sz.weight_medium));
        g.set_weight_semibold(i32::from(sz.weight_semibold));
        g.set_radius_md(t.radius.md * scale);
        g.set_spacing_unit(t.spacing.unit * scale);
        g.set_reduce_motion(reduce_motion);
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
        g.set_catalog_title(mgr.tr("catalog-title").into());
        g.set_catalog_search_placeholder(mgr.tr("catalog-search-placeholder").into());
        g.set_catalog_no_results(mgr.tr("catalog-no-results").into());
        g.set_widget_close_tooltip(mgr.tr("widget-close-tooltip").into());
        g.set_widget_resize_tooltip(mgr.tr("widget-resize-tooltip").into());
        g.set_viewer_text_dirty_indicator(mgr.tr("viewer-text-dirty-indicator").into());
        g.set_recent_files_open_hint(mgr.tr("recent-files-open-hint").into());
        g.set_rss_open_item_hint(mgr.tr("rss-open-item-hint").into());
        g.set_search_open_hint(mgr.tr("search-open-hint").into());
        g.set_terminal_tooltip_split_h(mgr.tr("terminal-tooltip-split-h").into());
        g.set_terminal_tooltip_split_v(mgr.tr("terminal-tooltip-split-v").into());
        g.set_terminal_tooltip_split_drag(mgr.tr("terminal-tooltip-split-drag").into());
        g.set_terminal_tooltip_tab_new(mgr.tr("terminal-tooltip-tab-new").into());
        g.set_terminal_tooltip_tab_close(mgr.tr("terminal-tooltip-tab-close").into());
        g.set_terminal_tooltip_pane_close(mgr.tr("terminal-tooltip-pane-close").into());
        g.set_group_tooltip_dissolve(mgr.tr("group-tooltip-dissolve").into());
        g.set_group_tooltip_move_left(mgr.tr("group-tooltip-move-left").into());
        g.set_group_tooltip_move_right(mgr.tr("group-tooltip-move-right").into());
        g.set_group_tooltip_close_tab(mgr.tr("group-tooltip-close-tab").into());
        g.set_group_hint_alt_detach(mgr.tr("group-hint-alt-detach").into());
        g.set_viewer_image_zoom_in(mgr.tr("viewer-image-zoom-in").into());
        g.set_viewer_image_zoom_out(mgr.tr("viewer-image-zoom-out").into());
        g.set_viewer_image_rotate_cw(mgr.tr("viewer-image-rotate-cw").into());
        g.set_viewer_image_rotate_ccw(mgr.tr("viewer-image-rotate-ccw").into());
        g.set_viewer_image_flip_h(mgr.tr("viewer-image-flip-h").into());
        g.set_viewer_image_flip_v(mgr.tr("viewer-image-flip-v").into());
        g.set_viewer_pdf_prev_page(mgr.tr("viewer-pdf-prev-page").into());
        g.set_viewer_pdf_next_page(mgr.tr("viewer-pdf-next-page").into());
        g.set_viewer_loading(mgr.tr("viewer-loading").into());
        g.set_viewer_error(mgr.tr("viewer-error").into());
        g.set_fm_nav_back(mgr.tr("fm-nav-back").into());
        g.set_fm_nav_back_disabled(mgr.tr("fm-nav-back-disabled").into());
        g.set_fm_nav_forward(mgr.tr("fm-nav-forward").into());
        g.set_fm_nav_forward_disabled(mgr.tr("fm-nav-forward-disabled").into());
        g.set_fm_nav_up(mgr.tr("fm-nav-up").into());
        g.set_fm_nav_home(mgr.tr("fm-nav-home").into());
        g.set_fm_loading(mgr.tr("fm-loading").into());
        g.set_fm_empty_folder(mgr.tr("fm-empty-folder").into());
        g.set_fm_action_new_folder(mgr.tr("fm-action-new-folder").into());
        g.set_fm_action_new_tab(mgr.tr("fm-action-new-tab").into());
        g.set_fm_action_close_tab(mgr.tr("fm-action-close-tab").into());
        g.set_fm_quick_filter_placeholder(mgr.tr("fm-quick-filter-placeholder").into());
        g.set_fm_view_icons(mgr.tr("fm-view-icons").into());
        g.set_fm_view_list(mgr.tr("fm-view-list").into());
        g.set_fm_view_details(mgr.tr("fm-view-details").into());
        g.set_fm_view_gallery(mgr.tr("fm-view-gallery").into());
        g.set_fm_entry_encrypted_hint(mgr.tr("fm-entry-encrypted-hint").into());
        g.set_fm_entry_managed_hint(mgr.tr("fm-entry-managed-hint").into());
        g.set_settings_panel_ok(mgr.tr("settings-panel-ok").into());
        g.set_settings_open_in_editor(mgr.tr("settings-open-in-editor").into());
        g.set_settings_open_config_file(mgr.tr("settings-open-config-file").into());

        g.set_media_play(mgr.tr("media-play").into());
        g.set_media_pause(mgr.tr("media-pause").into());
        g.set_media_next(mgr.tr("media-next").into());
        g.set_media_previous(mgr.tr("media-previous").into());

        g.set_password_locked(mgr.tr("password-locked").into());
        g.set_password_no_entries(mgr.tr("password-no-entries").into());
        g.set_password_search_placeholder(mgr.tr("password-search-placeholder").into());
        g.set_password_select_entry(mgr.tr("password-select-entry").into());
        g.set_password_label_username(mgr.tr("password-label-username").into());
        g.set_password_label_password(mgr.tr("password-label-password").into());
        g.set_password_label_url(mgr.tr("password-label-url").into());
        g.set_password_label_notes(mgr.tr("password-label-notes").into());
        g.set_password_label_totp(mgr.tr("password-label-totp").into());
        g.set_password_copy_username(mgr.tr("password-copy-username").into());
        g.set_password_copy_password(mgr.tr("password-copy-password").into());
        g.set_password_copy_totp(mgr.tr("password-copy-totp").into());
        g.set_password_open_url(mgr.tr("password-open-url").into());
        g.set_password_action_lock(mgr.tr("password-action-lock").into());
        g.set_password_unlock_label(mgr.tr("password-unlock-label").into());
        g.set_password_unlock_placeholder(mgr.tr("password-unlock-placeholder").into());
        g.set_password_unlock_submit(mgr.tr("password-unlock-submit").into());
        g.set_password_unlock_biometric(mgr.tr("password-unlock-biometric").into());
        g.set_password_action_add(mgr.tr("password-action-add").into());
        Ok(())
    }

    /// Status-bar labels for theme, language, and density.
    fn apply_app_state_status(self: &Arc<Self>) -> Result<()> {
        let g = self.window.global::<AppState>();
        let th = self.theme.current();
        let language = self.locale.current();
        let density = self.config.read().appearance.density;
        g.set_current_theme_id(
            theme_display_name(&self.locale, &th.meta.id, &th.meta.display_name).into(),
        );
        g.set_current_language(locale_display_name(&self.locale, &language).into());
        g.set_current_density(self.locale.tr(density_i18n_key(density)).into());
        // Slint 1.16 has no Window `layout-direction`; drive RTL via `is-rtl`.
        let is_rtl = language.as_str().to_ascii_lowercase().starts_with("ar");
        g.set_is_rtl(is_rtl);
        let cfg = self.config.read();
        let swap_edges = matches!(cfg.input.primary_hand, orchid_storage::Hand::Left)
            || cfg.input.mirror_edge_swipes;
        // Panels must dock on the same edges as the swipe targets that open them.
        g.set_edge_panels_mirrored(orchid_core::input::edge_panels_mirrored(
            is_rtl, swap_edges,
        ));
        Ok(())
    }

    /// Re-apply theme, locale, and density after a hot config reload.
    fn apply_hot_config(self: &Arc<Self>) -> Result<()> {
        let cfg = self.config.read();
        if self.history_recorder.is_enabled() != cfg.privacy.record_action_history {
            self.history_recorder
                .set_enabled(cfg.privacy.record_action_history);
        }
        let retention_days = cfg.privacy.history_retention_days;
        let retention_changed = retention_days
            != self
                .last_history_retention_days
                .load(Ordering::Acquire);
        if let Ok(lang) = LocaleId::parse(&cfg.locale.language) {
            self.locale.set_current(lang);
        }
        let theme_id = crate::system_theme::resolve_theme_id(&cfg.appearance);
        if let Err(e) = self.theme.set_current(&theme_id) {
            warn!(
                configured = %theme_id,
                error = %e,
                "unknown theme id after config reload"
            );
        }
        crate::autostart::sync_open_on_startup(&cfg.general);
        drop(cfg);
        if retention_changed {
            self.last_history_retention_days
                .store(retention_days, Ordering::Release);
            if let Err(e) = self.evict_history_by_retention_policy() {
                warn!(?e, "action history retention prune after config change");
            }
        }
        self.apply_command_shortcut_overrides();
        self.apply_input_gesture_bindings();
        self.apply_theme()?;
        self.apply_strings()?;
        self.apply_app_state_status()?;
        self.sync_widget_catalog_global();
        if self.config.read().onboarding.completed {
            self.onboarding.write().overlay_visible = false;
        }
        if self.settings.read().visible {
            self.sync_settings_global();
        }
        self.sync_navigation_global();
        self.sync_notification_global();
        self.sync_onboarding_global();
        self.schedule_rebuild();
        Ok(())
    }

    fn action_dispatcher(&self) -> ActionDispatcher {
        ActionDispatcher::new().with_middleware(self.history_recorder.clone() as _)
    }

    /// Delete persisted action-history rows older than
    /// [`OrchidConfig::privacy::history_retention_days`].
    fn evict_history_by_retention_policy(self: &Arc<Self>) -> Result<()> {
        let days = self.config.read().privacy.history_retention_days;
        let cutoff =
            chrono::Utc::now() - chrono::Duration::days(i64::from(days));
        let mut w = self.storage.write().map_err(UiError::Storage)?;
        let removed = w
            .evict_history_older_than(cutoff)
            .map_err(UiError::Storage)?;
        w.commit().map_err(UiError::Storage)?;
        if removed > 0 {
            debug!(
                removed,
                retention_days = days,
                "pruned action history by retention policy"
            );
        }
        Ok(())
    }

    fn apply_initial_mode(self: &Arc<Self>) -> Result<()> {
        let g = self.window.global::<AppState>();
        self.apply_app_state_status()?;
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

    fn canvas_inset_h(&self) -> f32 {
        WORKSPACE_SWITCHER_H
            + if self.navigation.read().dock_visible {
                DOCK_H
            } else {
                0.0
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
        let next = (log.width, (log.height - self.canvas_inset_h()).max(1.0));
        let mut cur = self.canvas_size.lock();
        if (cur.0 - next.0).abs() > 0.5 || (cur.1 - next.1).abs() > 0.5 {
            *cur = next;
            true
        } else {
            false
        }
    }


    fn on_get_started(self: &Arc<Self>) {
        let le = self.layout_engine.clone();
        let wm = self.widget_manager.clone();
        let wsm = self.workspace_manager.clone();
        let loc = self.locale.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local(async move {
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
        spawn::spawn_local(async move {
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
        spawn::spawn_local(async move {
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

    fn on_canvas_scrolled(&self, viewport_x: f32, viewport_y: f32) {
        *self.canvas_scroll.lock() = (viewport_x, viewport_y);
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
        let canonical = orchid_widgets::WidgetRegistry::canonical_type_id(&type_id_owned);
        let focus_search_input = canonical == "universal-search";
        let focus_password_input = canonical == "password-manager";
        spawn::spawn_local(async move {
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
        // Push model + query before `visible` so a freshly mounted panel sees rows.
        g.set_items(self.catalog_items.clone());
        g.set_search_query(cat.search_query.into());
        g.set_screen_x(cat.screen_x);
        g.set_screen_y(cat.screen_y);
        g.set_visible(cat.visible);
    }

    fn command_palette_shortcut(&self) -> Shortcut {
        self.config
            .read()
            .shortcuts
            .overrides
            .get("command-palette")
            .and_then(|s| Shortcut::parse(s).ok())
            .unwrap_or_else(|| Shortcut::parse("Ctrl+Shift+P").expect("valid default shortcut"))
    }

    fn leader_key_shortcut(&self) -> Option<Shortcut> {
        let cfg = self.config.read();
        let key = cfg.shortcuts.leader_key.as_ref()?;
        if key.is_empty() {
            return None;
        }
        Shortcut::parse(key).ok()
    }

    fn clear_leader_pending(&self) {
        *self.leader_pending_until.lock() = None;
    }

    /// Ctrl+S fallback for the last edited text viewer (when focus left the TextInput).
    fn try_viewer_text_ctrl_s(
        self: &Arc<Self>,
        mods: slint::winit_030::winit::keyboard::ModifiersState,
        logical: &slint::winit_030::winit::keyboard::Key,
    ) -> bool {
        use slint::winit_030::winit::keyboard::Key;
        if !mods.control_key() || mods.shift_key() || mods.alt_key() || mods.super_key() {
            return false;
        }
        let is_s = matches!(logical, Key::Character(s) if s.eq_ignore_ascii_case("s"));
        if !is_s {
            return false;
        }
        let Some(inst) = *self.last_text_edit_instance.lock() else {
            return false;
        };
        let cache = self.widget_manager.snapshot_cache();
        let Some(ws) = cache.get(inst) else {
            return false;
        };
        let WidgetPayload::Viewer(v) = &ws.payload else {
            return false;
        };
        let orchid_viewers::ViewerSnapshot::Text(t) = &v.snapshot else {
            return false;
        };
        if t.read_only {
            return false;
        }
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = orchid_widgets::builtin::viewer::text_save(inst).await {
                warn!(?e, "viewer text Ctrl+S");
                if let Some(c) = tw.upgrade() {
                    let title = c.locale.tr("widget-viewer-name");
                    let reason = viewer_localized_error(&c.locale, &e.to_string());
                    let body = c.locale.tr_args(
                        "viewer-text-save-failed",
                        &orchid_i18n::FluentArgs::new().with("reason", reason),
                    );
                    c.push_notification(&title, &body, 3);
                }
                return;
            }
            if let Some(c) = tw.upgrade() {
                c.schedule_rebuild();
            }
        });
        true
    }

    fn try_activate_leader(
        &self,
        mods: slint::winit_030::winit::keyboard::ModifiersState,
        logical: &slint::winit_030::winit::keyboard::Key,
    ) -> bool {
        let Some(sc) = self.leader_key_shortcut() else {
            return false;
        };
        if !winit_modifiers_match(sc.modifiers, mods) || !winit_key_matches(sc.key, logical) {
            return false;
        }
        let timeout_ms = self.config.read().shortcuts.leader_timeout_ms;
        *self.leader_pending_until.lock() =
            Some(Instant::now() + Duration::from_millis(timeout_ms));
        debug!(target: "orchid_ui::shortcuts", "leader-key armed");
        true
    }

    fn try_leader_chord(
        &self,
        mods: slint::winit_030::winit::keyboard::ModifiersState,
        logical: &slint::winit_030::winit::keyboard::Key,
    ) -> Option<String> {
        use slint::winit_030::winit::keyboard::{Key, NamedKey};
        {
            let guard = self.leader_pending_until.lock();
            let until = (*guard)?;
            if Instant::now() > until {
                drop(guard);
                self.clear_leader_pending();
                return None;
            }
        }

        if mods.control_key() || mods.alt_key() || mods.super_key() {
            self.clear_leader_pending();
            return None;
        }

        let key_str = match logical {
            Key::Character(s) => {
                let ch = s.chars().next()?;
                if ch.is_ascii_alphabetic() {
                    ch.to_ascii_lowercase().to_string()
                } else {
                    self.clear_leader_pending();
                    return None;
                }
            }
            Key::Named(NamedKey::Escape) => {
                self.clear_leader_pending();
                return None;
            }
            _ => {
                self.clear_leader_pending();
                return None;
            }
        };

        let cmd_id = self
            .config
            .read()
            .shortcuts
            .leader_bindings
            .get(&key_str)
            .cloned();
        self.clear_leader_pending();
        if let Some(ref id) = cmd_id {
            debug!(target: "orchid_ui::shortcuts", cmd_id = %id, key = %key_str, "leader chord");
        }
        cmd_id
    }

    fn apply_command_shortcut_overrides(self: &Arc<Self>) {
        let overrides = self.config.read().shortcuts.overrides.clone();
        if overrides.is_empty() {
            return;
        }
        for result in self.command_registry.apply_shortcut_overrides(&overrides) {
            if let Err(reason) = result.outcome {
                warn!(
                    command = %result.command_id,
                    reason = %reason,
                    "shortcut override rejected"
                );
            }
        }
    }

    fn apply_input_gesture_bindings(self: &Arc<Self>) {
        let cfg = self.config.read();
        let swap = matches!(cfg.input.primary_hand, orchid_storage::Hand::Left)
            || cfg.input.mirror_edge_swipes;
        self.input_mapper.set_bindings(default_bindings_mirrored(swap));
    }

    fn dispatch_registry_shortcut(self: &Arc<Self>, cmd_id: String) {
        let this = Arc::clone(self);
        spawn::spawn_local_compat(async move {
            this.dispatch_command(&cmd_id).await;
            this.schedule_rebuild();
        });
    }

    fn sync_settings_global(self: &Arc<Self>) {
        let st = self.settings.read().clone();
        let section = if st.section.is_empty() {
            SETTINGS_SECTION_IDS[0].to_string()
        } else {
            st.section.clone()
        };
        let title_key = format!("settings.section.{}", section);
        let title = self.locale.tr(&title_key).into();
        let hint = self.locale.tr("settings-panel-hint").into();
        // Shortcuts (and similar) are view-only in the panel — surface the
        // dedicated coming-soon copy so users know to edit config.toml.
        let coming_soon = if section == "shortcuts" {
            self.locale.tr("settings-panel-coming-soon").into()
        } else {
            SharedString::default()
        };
        let cfg = self.config.read();
        let fields = build_settings_fields(
            &section,
            &cfg,
            &self.locale,
            &self.theme,
            &self.command_registry,
        );
        drop(cfg);
        sync_vec_model(&self.settings_sections, build_settings_sections(&self.locale));
        sync_vec_model(&self.settings_fields, fields);
        let g = self.window.global::<SettingsGlobal>();
        g.set_visible(st.visible);
        g.set_panel_title(title);
        g.set_hint_text(hint);
        g.set_coming_soon_text(coming_soon);
        g.set_config_path(self.config_file_path.display().to_string().into());
        g.set_current_section_id(section.clone().into());
        g.set_selected_section_index(settings_section_index(&section));
        g.set_sections(self.settings_sections.clone());
        g.set_fields(self.settings_fields.clone());
    }

    fn on_settings_field_changed(self: &Arc<Self>, section: &str, key: &str, value: &str) {
        if !self.settings.read().visible {
            return;
        }
        let mut cfg = self.config.write();
        if let Err(reason) = apply_settings_field(&mut cfg, section, key, value, &self.locale) {
            warn!(
                section = %section,
                key = %key,
                value = %value,
                reason = %reason,
                "settings field rejected"
            );
            let body = self.locale.tr_args(
                "settings-field-rejected",
                &orchid_i18n::FluentArgs::new().with("reason", reason),
            );
            self.push_notification(&self.locale.tr("settings-panel-title"), &body, 2);
            return;
        }
        if let Err(e) = cfg.validate() {
            warn!(
                section = %section,
                key = %key,
                value = %value,
                error = %e,
                "settings field failed validation"
            );
            let body = self.locale.tr_args(
                "settings-validation-failed",
                &orchid_i18n::FluentArgs::new().with("reason", e.to_string()),
            );
            self.push_notification(&self.locale.tr("settings-panel-title"), &body, 2);
            return;
        }
        let snapshot = cfg.clone();
        drop(cfg);
        if let Err(e) = ConfigLoader::save(&snapshot, &self.config_file_path) {
            warn!(?e, "settings save failed");
            let reason = storage_localized_error(&self.locale, &e);
            let body = self.locale.tr_args(
                "settings-save-failed",
                &orchid_i18n::FluentArgs::new().with("reason", reason),
            );
            self.push_notification(&self.locale.tr("settings-panel-title"), &body, 3);
            return;
        }
        if let Err(e) = self.apply_hot_config() {
            warn!(?e, "settings apply after save");
            let reason = ui_localized_error(&self.locale, &e);
            let body = self.locale.tr_args(
                "settings-config-reload-failed",
                &orchid_i18n::FluentArgs::new().with("reason", reason),
            );
            self.push_notification(&self.locale.tr("settings-panel-title"), &body, 2);
        }
    }

    fn open_settings(self: &Arc<Self>, section: &str) {
        self.on_command_palette_dismiss();
        let section = if SETTINGS_SECTION_IDS.iter().any(|&id| id == section) {
            section.to_string()
        } else {
            SETTINGS_SECTION_IDS[0].to_string()
        };
        {
            let mut st = self.settings.write();
            st.visible = true;
            st.section = section;
        }
        self.sync_settings_global();
    }

    fn on_settings_dismiss(self: &Arc<Self>) {
        if !self.settings.read().visible {
            return;
        }
        self.settings.write().visible = false;
        self.sync_settings_global();
    }

    fn on_settings_section_selected(self: &Arc<Self>, idx: i32) {
        if !self.settings.read().visible {
            return;
        }
        self.settings.write().section = settings_section_id(idx).to_string();
        self.sync_settings_global();
    }

    fn open_config_file(self: &Arc<Self>) {
        let path = self.config_file_path.clone();
        if !path.exists() {
            warn!(?path, "config file missing");
            return;
        }
        if let Err(e) = opener::open(&path) {
            warn!(?e, path = %path.display(), "open config file");
        }
    }

    fn sync_navigation_global(self: &Arc<Self>) {
        let nav = self.navigation.read().clone();
        let hint_mode = self.config.read().onboarding.hint_mode_enabled;
        let g = self.window.global::<NavigationGlobal>();
        g.set_workspace_panel_visible(nav.workspace_panel_visible);
        g.set_notification_center_visible(nav.notification_center_visible);
        g.set_dock_visible(nav.dock_visible);
        g.set_hint_mode_enabled(hint_mode);
        g.set_workspace_panel_title(self.locale.tr("navigation-workspace-panel-title").into());
        g.set_notification_center_title(self.locale.tr("notification-center-title").into());
        g.set_notification_center_placeholder(
            self.locale.tr("notification-center-placeholder").into(),
        );
        g.set_panel_dismiss_label(self.locale.tr("notification-center-dismiss").into());
        g.set_hint_dock_label(self.locale.tr("onboarding-hint-dock").into());
        g.set_hint_workspace_label(self.locale.tr("onboarding-hint-workspace").into());
        g.set_hint_gestures_label(self.locale.tr("onboarding-hint-gestures").into());
    }

    fn sync_notification_global(self: &Arc<Self>) {
        let g = self.window.global::<NotificationGlobal>();
        g.set_notifications(self.notifications.clone());
        g.set_clear_all_label(self.locale.tr("notification-center-clear").into());
        g.set_dismiss_label(self.locale.tr("notification-center-dismiss").into());
        g.set_empty_placeholder(self.locale.tr("notification-center-placeholder").into());
    }

    /// Soft cap so bridges/toasts cannot grow the in-memory list without bound.
    const NOTIFICATION_LIST_CAP: usize = 50;

    fn push_notification(self: &Arc<Self>, title: &str, body: &str, severity: i32) {
        let item = NotificationItem {
            id: uuid::Uuid::new_v4().to_string().into(),
            title: title.into(),
            body: body.into(),
            time_label: self
                .config
                .read()
                .locale
                .format_time(chrono::Utc::now())
                .into(),
            severity,
        };
        if let Some(model) = self.notifications.as_any().downcast_ref::<VecModel<NotificationItem>>()
        {
            model.insert(0, item);
            while model.row_count() > Self::NOTIFICATION_LIST_CAP {
                model.remove(model.row_count() - 1);
            }
        }
        self.sync_notification_global();
        self.persist_notifications();
    }

    fn clear_notifications(self: &Arc<Self>) {
        if let Some(model) = self.notifications.as_any().downcast_ref::<VecModel<NotificationItem>>()
        {
            model.set_vec(Vec::new());
        }
        self.sync_notification_global();
        self.persist_notifications();
    }

    fn dismiss_notification(self: &Arc<Self>, id: &str) {
        if let Some(model) = self.notifications.as_any().downcast_ref::<VecModel<NotificationItem>>()
        {
            if let Some(idx) = (0..model.row_count()).find(|&i| {
                model.row_data(i).is_some_and(|item| item.id.as_str() == id)
            }) {
                model.remove(idx);
            }
        }
        self.sync_notification_global();
        self.persist_notifications();
    }

    fn snapshot_notifications(&self) -> orchid_storage::NotificationCenterState {
        let mut items = Vec::new();
        if let Some(model) = self.notifications.as_any().downcast_ref::<VecModel<NotificationItem>>()
        {
            for i in 0..model.row_count() {
                if let Some(row) = model.row_data(i) {
                    items.push(orchid_storage::NotificationCenterItem {
                        id: row.id.to_string(),
                        title: row.title.to_string(),
                        body: row.body.to_string(),
                        time_label: row.time_label.to_string(),
                        severity: row.severity,
                    });
                }
            }
        }
        orchid_storage::NotificationCenterState { items }
    }

    fn persist_notifications(self: &Arc<Self>) {
        let state = self.snapshot_notifications();
        if let Err(e) = (|| -> Result<()> {
            let mut w = self.storage.write().map_err(UiError::Storage)?;
            w.put_notification_center(&state)
                .map_err(UiError::Storage)?;
            w.commit().map_err(UiError::Storage)?;
            Ok(())
        })() {
            warn!(?e, "persist notification center");
        }
    }

    /// Mirror high-value file-manager transfer failures into the notification center (deduped).


    fn ensure_startup_notification_tip(self: &Arc<Self>) {
        if self
            .notification_tip_pushed
            .swap(true, Ordering::AcqRel)
        {
            return;
        }
        if self.notifications.row_count() > 0 {
            return;
        }
        self.push_notification(
            &self.locale.tr("notification-center-tip-title"),
            &self.locale.tr("notification-center-tip-body"),
            1,
        );
    }

    fn sync_onboarding_global(self: &Arc<Self>) {
        let ob = self.onboarding.read().clone();
        let step = ob.current_step.clamp(0, ONBOARDING_STEP_COUNT - 1) as usize;
        let (title_key, body_key) = ONBOARDING_STEP_KEYS[step];
        let g = self.window.global::<OnboardingGlobal>();
        g.set_overlay_visible(ob.overlay_visible);
        g.set_current_step(step as i32);
        g.set_step_count(ONBOARDING_STEP_COUNT);
        let progress = self.locale.tr_args(
            "onboarding-step-progress",
            &orchid_i18n::FluentArgs::new()
                .with("current", (step + 1).to_string())
                .with("total", ONBOARDING_STEP_COUNT.to_string()),
        );
        g.set_step_progress_label(progress.into());
        g.set_step_title(self.locale.tr(title_key).into());
        g.set_step_body(self.locale.tr(body_key).into());
        g.set_back_label(self.locale.tr("onboarding-back").into());
        g.set_next_label(self.locale.tr("onboarding-next").into());
        g.set_skip_label(self.locale.tr("onboarding-skip").into());
        g.set_finish_label(self.locale.tr("onboarding-finish").into());
    }

    fn save_config_to_disk(self: &Arc<Self>) {
        let mut cfg = self.config.read().clone();
        if let Err(e) = cfg.validate() {
            warn!(?e, "config validation failed on save");
            return;
        }
        match orchid_crypto::protect_network_mount_passwords(&mut cfg.file_manager.network_mounts)
        {
            Ok(true) => {
                self.config.write().file_manager.network_mounts =
                    cfg.file_manager.network_mounts.clone();
            }
            Ok(false) => {}
            Err(e) => warn!(?e, "could not DPAPI-protect mount passwords before save"),
        }
        if let Err(e) = ConfigLoader::save(&cfg, &self.config_file_path) {
            warn!(?e, "failed to save config.toml");
        }
    }

    fn complete_onboarding(self: &Arc<Self>) {
        {
            let mut cfg = self.config.write();
            cfg.onboarding.completed = true;
        }
        self.onboarding.write().overlay_visible = false;
        self.save_config_to_disk();
        self.sync_onboarding_global();
    }

    fn ensure_workspace_mode_for_onboarding(self: &Arc<Self>) {
        if self.window.global::<AppState>().get_mode() == 0 {
            self.on_get_started();
        }
    }

    fn on_onboarding_next(self: &Arc<Self>) {
        if !self.onboarding.read().overlay_visible {
            return;
        }
        let step = self.onboarding.read().current_step;
        if step + 1 >= ONBOARDING_STEP_COUNT {
            self.ensure_workspace_mode_for_onboarding();
            self.complete_onboarding();
            return;
        }
        if step == 0 {
            self.ensure_workspace_mode_for_onboarding();
        }
        {
            let mut ob = self.onboarding.write();
            ob.current_step = step + 1;
        }
        self.sync_onboarding_global();
    }

    fn on_onboarding_back(self: &Arc<Self>) {
        let mut ob = self.onboarding.write();
        if !ob.overlay_visible || ob.current_step <= 0 {
            return;
        }
        ob.current_step -= 1;
        drop(ob);
        self.sync_onboarding_global();
    }

    fn on_onboarding_skip(self: &Arc<Self>) {
        if !self.onboarding.read().overlay_visible {
            return;
        }
        self.ensure_workspace_mode_for_onboarding();
        self.complete_onboarding();
    }

    fn toggle_hint_mode(self: &Arc<Self>) {
        {
            let mut cfg = self.config.write();
            cfg.onboarding.hint_mode_enabled = !cfg.onboarding.hint_mode_enabled;
        }
        self.save_config_to_disk();
        self.sync_navigation_global();
    }

    fn toggle_workspace_panel(self: &Arc<Self>) {
        self.on_command_palette_dismiss();
        {
            let mut nav = self.navigation.write();
            nav.workspace_panel_visible = !nav.workspace_panel_visible;
            if nav.workspace_panel_visible {
                nav.notification_center_visible = false;
            }
        }
        self.sync_navigation_global();
    }

    fn toggle_notification_center(self: &Arc<Self>) {
        self.on_command_palette_dismiss();
        let opening = {
            let mut nav = self.navigation.write();
            nav.notification_center_visible = !nav.notification_center_visible;
            if nav.notification_center_visible {
                nav.workspace_panel_visible = false;
            }
            nav.notification_center_visible
        };
        if opening {
            self.ensure_startup_notification_tip();
        }
        self.sync_navigation_global();
    }

    fn toggle_dock(self: &Arc<Self>) {
        {
            let mut nav = self.navigation.write();
            nav.dock_visible = !nav.dock_visible;
        }
        self.sync_navigation_global();
        self.update_gesture_bounds();
        let _ = self.sync_canvas_size_from_winit();
        self.schedule_rebuild();
    }

    fn on_navigation_workspace_panel_dismiss(self: &Arc<Self>) {
        if !self.navigation.read().workspace_panel_visible {
            return;
        }
        self.navigation.write().workspace_panel_visible = false;
        self.sync_navigation_global();
    }

    fn on_notification_center_dismiss(self: &Arc<Self>) {
        if !self.navigation.read().notification_center_visible {
            return;
        }
        self.navigation.write().notification_center_visible = false;
        self.sync_navigation_global();
    }

    fn show_universal_search(self: &Arc<Self>) {
        self.on_command_palette_dismiss();
        if let Ok(w) = self.workspace_manager.active() {
            if let Some(inst) = self
                .widget_manager
                .instances_for_workspace(w.id)
                .into_iter()
                .find(|inst| inst.type_id == "universal-search" || inst.type_id == "search")
            {
                *self.search_autofocus_pending.lock() = Some(inst.id);
                self.schedule_rebuild();
                return;
            }
        }
        // UI allowlist + dock use the short id; registry maps it to `universal-search`.
        self.spawn_add_widget("search", AddWidgetPlacement::AutoSlot);
    }

    fn show_widget_catalog_center(self: &Arc<Self>) {
        let (vw, vh) = *self.canvas_size.lock();
        let (scroll_x, scroll_y) = *self.canvas_scroll.lock();
        {
            let mut cat = self.catalog.write();
            cat.visible = true;
            cat.content_x = vw / 2.0 + scroll_x;
            cat.content_y = vh / 2.0 + scroll_y;
            cat.screen_x = vw / 2.0;
            cat.screen_y = WORKSPACE_SWITCHER_H + vh / 2.0;
            cat.search_query.clear();
        }
        self.sync_widget_catalog_global();
    }

    fn update_gesture_bounds(self: &Arc<Self>) {
        let win = self.window.window();
        let p = win.size();
        if p.width < 2 || p.height < 2 {
            return;
        }
        let log = p.to_logical(win.scale_factor());
        self.gesture_recognizer.lock().set_bounds(ScreenBounds::new(
            log.width,
            log.height,
        ));
    }

    fn handle_recognized_gestures(
        self: &Arc<Self>,
        gestures: impl IntoIterator<Item = RecognizedGesture>,
    ) {
        let gestures: Vec<_> = gestures.into_iter().collect();
        if gestures.is_empty() {
            return;
        }
        let win = self.window.window();
        let p = win.size();
        if p.width < 2 || p.height < 2 {
            return;
        }
        let log = p.to_logical(win.scale_factor());
        let bounds = ScreenBounds::new(log.width, log.height);
        for gesture in gestures {
            if let Some(cmd_id) = self.input_mapper.resolve_gesture(&gesture, bounds) {
                debug!(target: "orchid_ui::gestures", cmd_id = %cmd_id, ?gesture, "gesture resolved");
                self.dispatch_registry_shortcut(cmd_id);
            }
        }
    }

    fn feed_touch_input(self: &Arc<Self>, touch: TouchEvent) {
        let gestures = self.gesture_recognizer.lock().feed(&InputEvent::Touch(touch));
        self.handle_recognized_gestures(gestures);
    }

    fn sync_command_palette_global(self: &Arc<Self>) {
        let st = self.palette.read().clone();
        let candidates = build_palette_candidates(
            &self.command_palette,
            &self.command_registry,
            &self.locale,
            &st.query,
            COMMAND_PALETTE_LIMIT,
        );
        sync_vec_model(&self.palette_candidates, candidates);
        let count = self.palette_candidates.row_count();
        let selected = if count == 0 {
            -1
        } else {
            st.selected_index.clamp(0, count as i32 - 1)
        };
        let no_results_text = if !st.query.trim().is_empty() {
            self.locale.tr_args(
                "search-no-results",
                &orchid_i18n::FluentArgs::new().with("query", st.query.clone()),
            )
        } else {
            self.locale.tr("search-no-results-short")
        };
        let g = self.window.global::<CommandPaletteGlobal>();
        g.set_visible(st.visible);
        g.set_model(SearchModel {
            query: st.query.clone().into(),
            candidates: self.palette_candidates.clone(),
            is_searching: false,
            error: SharedString::new(),
            selected_index: selected,
            placeholder_text: self.locale.tr("command-palette-placeholder").into(),
            empty_state_text: self.locale.tr("command-palette-empty").into(),
            no_results_text: no_results_text.into(),
            searching_text: self.locale.tr("search-searching").into(),
            request_autofocus: st.request_autofocus,
        });
        if st.request_autofocus {
            self.palette.write().request_autofocus = false;
        }
    }

    fn toggle_command_palette(self: &Arc<Self>) {
        if self.palette.read().visible {
            self.on_command_palette_dismiss();
        } else {
            self.open_command_palette();
        }
    }

    fn open_command_palette(self: &Arc<Self>) {
        {
            let mut st = self.palette.write();
            st.visible = true;
            st.query.clear();
            st.selected_index = 0;
            st.request_autofocus = true;
        }
        self.sync_command_palette_global();
    }

    fn on_command_palette_dismiss(self: &Arc<Self>) {
        if !self.palette.read().visible {
            return;
        }
        self.palette.write().visible = false;
        self.sync_command_palette_global();
    }

    fn on_command_palette_query_changed(self: &Arc<Self>, query: &SharedString) {
        {
            let mut st = self.palette.write();
            st.query = query.to_string();
            st.selected_index = 0;
        }
        self.sync_command_palette_global();
    }

    fn on_command_palette_selection_changed(self: &Arc<Self>, new_idx: i32) {
        let count = self.palette_candidates.row_count();
        let clamped = if count == 0 {
            -1
        } else {
            new_idx.clamp(0, count as i32 - 1)
        };
        self.palette.write().selected_index = clamped;
        self.sync_command_palette_global();
    }

    fn on_command_palette_candidate_activated(self: &Arc<Self>, cmd_id: &SharedString) {
        let id = cmd_id.to_string();
        if id.is_empty() {
            return;
        }
        self.on_command_palette_dismiss();
        let this = Arc::clone(self);
        spawn::spawn_local_compat(async move {
            this.dispatch_command(&id).await;
            this.schedule_rebuild();
        });
    }

    async fn dispatch_command(self: &Arc<Self>, cmd_id: &str) {
        if cmd_id == "command-palette" {
            self.toggle_command_palette();
            return;
        }
        if cmd_id == "settings.open" {
            self.open_settings("general");
            return;
        }
        if cmd_id == "settings.open_config_file" {
            self.open_config_file();
            return;
        }
        if cmd_id == "password.lock" {
            self.on_password_lock_vault();
            return;
        }
        if cmd_id == "navigation.show_workspace_panel" {
            self.toggle_workspace_panel();
            return;
        }
        if cmd_id == "notification.show_center" {
            self.toggle_notification_center();
            return;
        }
        if cmd_id == "dock.show" {
            self.toggle_dock();
            return;
        }
        if cmd_id == "search.show_universal" {
            self.show_universal_search();
            return;
        }
        if cmd_id == "onboarding.toggle_hint_mode" {
            self.toggle_hint_mode();
            return;
        }
        if cmd_id == "widget.show_all" {
            self.show_widget_catalog_center();
            return;
        }
        let action = match self
            .command_registry
            .build_action(cmd_id, ParsedCommand::default())
        {
            Ok(a) => a,
            Err(e) => {
                warn!(?e, cmd_id = %cmd_id, "build command action");
                return;
            }
        };
        let ctx = ActionContext::new(
            self.bus.clone(),
            self.storage.clone(),
            self.config.clone(),
        );
        let dispatcher = self.action_dispatcher();
        if let Err(e) = dispatcher.dispatch(action, &ctx).await {
            warn!(?e, cmd_id = %cmd_id, "dispatch command");
        }
    }

    fn minimal_widget_size(wm: &WidgetManager, type_id: &str) -> WidgetSize {
        wm.registry()
            .get(type_id)
            .map(|d| d.min_size.unwrap_or(d.default_size))
            .unwrap_or(WidgetSize::Medium)
    }


    /// Content area of [`widget-frame.slint`] below the title bar (`height - 32px`); must match
    /// what `terminal-viewport-changed` would report as `w`/`h`.
    const WIDGET_FRAME_HEADER_PX: f32 = 32.0;
    /// Height of [`terminal-tabs.slint`] inside the terminal widget content area.
    const TERMINAL_TAB_BAR_PX: f32 = 29.0;
    /// Height of [`group-tabs.slint`] when a frame is part of a multi-widget group.
    const GROUP_TAB_BAR_PX: f32 = 28.0;




    /// Patch Slint `WidgetFrameModel` rows for instances whose [`WidgetSnapshotCache`] data changed
    /// without a layout canvas / scale / workspace event (e.g. terminal text at ~30Hz).
    ///
    /// # Universal search contract
    ///
    /// `on_search_query_changed` must **not** call [`Self::rebuild_workspace_model`]: a full rebuild
    /// recreates `SearchView`, steals focus, and races the widget debouncer. Instead:
    ///
    /// 1. UI calls [`orchid_widgets::builtin::search::universal_search_push_query`].
    /// 2. The widget debouncer publishes [`orchid_widgets::WidgetSnapshotUpdated`].
    /// 3. [`WidgetManager::start`]'s snapshot subscriber refreshes [`WidgetSnapshotCache`] and marks
    ///    the instance frame-dirty.
    /// 4. Each UI tick drains dirty ids and calls this method (when no full rebuild is pending).
    ///
    /// See `docs/universal-search-issue.md` for regression notes.
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
                let ch = (bounds.height - Self::WIDGET_FRAME_HEADER_PX - Self::TERMINAL_TAB_BAR_PX)
                    .max(1.0);
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
            recent_files_model,
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
                        empty_weather_model(&self.locale),
                        empty_moon_model(&self.locale),
                        empty_system_model(&self.locale),
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
                    empty_system_model(&self.locale),
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
                    empty_system_model(&self.locale),
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
                    build_system_model(s, &self.locale),
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
                    empty_system_model(&self.locale),
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
                        empty_weather_model(&self.locale),
                        empty_moon_model(&self.locale),
                        empty_system_model(&self.locale),
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
                    empty_system_model(&self.locale),
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
                        self.password_autofocus_pending.write().remove(&pl.instance_id);
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
                        empty_system_model(&self.locale),
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
                    empty_system_model(&self.locale),
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
                    empty_system_model(&self.locale),
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
                        empty_system_model(&self.locale),
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
                    empty_system_model(&self.locale),
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
        let (cw, ch) = (self.font_metrics.cell_width_px, self.font_metrics.cell_height_px);
        let close_confirm = self
            .close_confirm_overlays
            .read()
            .get(&pl.instance_id)
            .cloned()
            .unwrap_or_else(empty_close_confirm_dialog);
        let (group_id, group_tabs) = self.build_group_tab_models(pl.instance_id);
        WidgetFrameModel {
            instance_id: pl.instance_id.to_string().into(),
            type_id: type_s,
            title,
            x: bounds.x,
            y: bounds.y,
            width: bounds.width,
            height: bounds.height,
            z_order,
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
            system: system_model,
            rss: rss_model,
            search: search_model,
            media: media_model,
            password: password_model,
            viewer: viewer_model,
            recent_files: recent_files_model,
            file_manager: file_manager_model,
            close_confirm,
        }
    }

    fn build_group_tab_models(&self, instance_id: Uuid) -> (SharedString, ModelRc<GroupTabModel>) {
        let Some(group) = self.group_manager.find_for_instance(instance_id) else {
            return (SharedString::default(), ModelRc::new(VecModel::default()));
        };
        if group.members.len() < 2 {
            return (SharedString::default(), ModelRc::new(VecModel::default()));
        }
        let active = group.active_instance();
        let cache = self.widget_manager.snapshot_cache();
        let tabs: Vec<GroupTabModel> = group
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
                GroupTabModel {
                    instance_id: mid.to_string().into(),
                    title: title.into(),
                    is_active: active == Some(*mid),
                }
            })
            .collect();
        (group.id.to_string().into(), ModelRc::new(VecModel::from(tabs)))
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
                    if group.members.len() >= 2
                        && group.active_instance() != Some(pl.instance_id)
                    {
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
                bounds = self.layout_engine.pixel_bounds_for(
                    group.position,
                    group.size,
                    view,
                );
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
        self.sync_fm_transfer_notifications();
        Ok(())
    }

    /// Show the window and run the Slint event loop.
    pub fn run(self: Arc<Self>) -> Result<()> {
        tracing::info!("Opening main window");
        let tw = Arc::downgrade(&self);
        self.window.window().on_winit_window_event(move |_winit_window, event| {
            use slint::winit_030::{EventResult, winit::event::WindowEvent};
            match event {
                WindowEvent::CursorMoved { position, .. } => {
                    if let Some(c) = tw.upgrade() {
                        let win = c.window.window();
                        let scale = win.scale_factor();
                        let logical: slint::winit_030::winit::dpi::LogicalPosition<f64> =
                            position.to_logical(f64::from(scale));
                        let canvas_y = logical.y - f64::from(WORKSPACE_SWITCHER_H);
                        if canvas_y >= 0.0 {
                            let (scroll_x, scroll_y) = *c.canvas_scroll.lock();
                            *c.last_canvas_pointer.lock() = Some((
                                logical.x as f32 + scroll_x,
                                canvas_y as f32 + scroll_y,
                            ));
                        }
                    }
                }
                WindowEvent::ModifiersChanged(modifiers) => {
                    if let Some(c) = tw.upgrade() {
                        *c.keyboard_modifiers.lock() = modifiers.state();
                    }
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    use slint::winit_030::winit::event::ElementState;
                    use slint::winit_030::winit::keyboard::{Key, NamedKey};
                    if event.state == ElementState::Pressed {
                        if let Some(c) = tw.upgrade() {
                            if c.palette.read().visible
                                && matches!(event.logical_key, Key::Named(NamedKey::Escape))
                            {
                                c.on_command_palette_dismiss();
                            } else if c.settings.read().visible
                                && matches!(event.logical_key, Key::Named(NamedKey::Escape))
                            {
                                c.on_settings_dismiss();
                            } else if c.navigation.read().workspace_panel_visible
                                && matches!(event.logical_key, Key::Named(NamedKey::Escape))
                            {
                                c.on_navigation_workspace_panel_dismiss();
                            } else if c.navigation.read().notification_center_visible
                                && matches!(event.logical_key, Key::Named(NamedKey::Escape))
                            {
                                c.on_notification_center_dismiss();
                            } else if c.leader_pending_until.lock().is_some()
                                && matches!(event.logical_key, Key::Named(NamedKey::Escape))
                            {
                                c.clear_leader_pending();
                            } else {
                                let mods = *c.keyboard_modifiers.lock();
                                if let Some(cmd_id) = c.try_leader_chord(mods, &event.logical_key)
                                {
                                    c.dispatch_registry_shortcut(cmd_id);
                                } else if c.try_activate_leader(mods, &event.logical_key) {
                                    // leader armed; consume without dispatching
                                } else {
                                let palette_sc = c.command_palette_shortcut();
                                if winit_modifiers_match(palette_sc.modifiers, mods)
                                    && winit_key_matches(palette_sc.key, &event.logical_key)
                                {
                                    c.toggle_command_palette();
                                } else if c.try_viewer_text_ctrl_s(mods, &event.logical_key) {
                                    // Saved focused/last text editor; consume.
                                } else if let Some(shortcut) =
                                    winit_to_shortcut(mods, &event.logical_key)
                                {
                                    if let Some(cmd_id) =
                                        resolve_registry_shortcut(&c.command_registry, &shortcut)
                                    {
                                        c.dispatch_registry_shortcut(cmd_id);
                                    }
                                }
                                }
                            }
                        }
                    }
                }
                WindowEvent::DroppedFile(path_buf) => {
                    let path = path_buf.to_string_lossy().into_owned();
                    if let Some(c) = tw.upgrade() {
                        c.queue_os_file_drop(path);
                    }
                }
                WindowEvent::Touch(touch) => {
                    if let Some(c) = tw.upgrade() {
                        if let Some(ev) = winit_touch_to_orchid(&touch, c.window.window()) {
                            c.feed_touch_input(ev);
                        }
                    }
                }
                _ => {}
            }
            EventResult::Propagate
        });
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
        self.update_gesture_bounds();
        slint::run_event_loop().map_err(|e| UiError::Slint(format!("loop: {e}")))?;
        self.persist_notifications();
        tracing::info!("Main window closed");
        Ok(())
    }




















    const VIEWER_MULTI_OPEN_CAP: usize = 8;

    async fn open_in_viewer_for_controller(
        ctrl: std::sync::Weak<MainWindowController>,
        path: orchid_fs::FsPath,
        reuse_existing: bool,
        schedule_rebuild: bool,
    ) -> Result<Uuid> {
        let Some(c) = ctrl.upgrade() else {
            return Err(UiError::Slint("controller gone".into()));
        };
        let ws_id = c
            .workspace_manager
            .active()
            .map_err(|e| UiError::Slint(format!("no active workspace: {e}")))?
            .id;

        if reuse_existing {
            for inst in c.widget_manager.instances_for_workspace(ws_id) {
                if inst.type_id == orchid_widgets::builtin::viewer::TYPE_ID {
                    orchid_widgets::builtin::viewer::open_path(inst.id, path.clone())
                        .await
                        .map_err(|e| UiError::Slint(format!("viewer open: {e}")))?;
                    c.recent_files.touch(&path, Some(&c.bus));
                    if schedule_rebuild {
                        if let Some(c2) = ctrl.upgrade() {
                            c2.schedule_rebuild();
                        }
                    }
                    return Ok(inst.id);
                }
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

        // Place off the (0,0) placeholder so stacked multi-open widgets stay usable.
        Self::move_new_widget_to_free_slot(&c.layout_engine, &c.widget_manager, ws_id, id).await;

        orchid_widgets::builtin::viewer::open_path(id, path.clone())
            .await
            .map_err(|e| UiError::Slint(format!("viewer open: {e}")))?;
        c.recent_files.touch(&path, Some(&c.bus));
        if schedule_rebuild {
            if let Some(c2) = ctrl.upgrade() {
                c2.schedule_rebuild();
            }
        }
        Ok(id)
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


fn resolve_registry_shortcut(
    registry: &CommandRegistry,
    shortcut: &Shortcut,
) -> Option<String> {
    registry.list_all().into_iter().find_map(|desc| {
        registry
            .effective_shortcut(&desc.id)
            .filter(|s| shortcuts_equivalent(s, shortcut))
            .map(|_| desc.id)
    })
}

/// Match shortcuts from winit, allowing an extra Shift for punctuation keys
/// (e.g. `Win+?` is typed as Win+Shift+? on US layouts).
fn shortcuts_equivalent(expected: &Shortcut, actual: &Shortcut) -> bool {
    use orchid_core::{Key, Modifiers};
    if expected == actual {
        return true;
    }
    if expected.key != actual.key {
        return false;
    }
    if matches!(expected.key, Key::Char(c) if !c.is_ascii_alphabetic())
        && !expected.modifiers.contains(Modifiers::SHIFT)
        && actual.modifiers == expected.modifiers | Modifiers::SHIFT
    {
        return true;
    }
    false
}

fn winit_to_shortcut(
    state: slint::winit_030::winit::keyboard::ModifiersState,
    logical: &slint::winit_030::winit::keyboard::Key,
) -> Option<Shortcut> {
    use orchid_core::{Key as Ok, Modifiers};
    use slint::winit_030::winit::keyboard::{Key, NamedKey};

    let mut modifiers = Modifiers::empty();
    if state.control_key() {
        modifiers |= Modifiers::CTRL;
    }
    if state.shift_key() {
        modifiers |= Modifiers::SHIFT;
    }
    if state.alt_key() {
        modifiers |= Modifiers::ALT;
    }
    if state.super_key() {
        modifiers |= Modifiers::WIN;
    }

    let key = match logical {
        Key::Character(s) => {
            let ch = s.chars().next()?;
            if ch.is_ascii_alphabetic() {
                Ok::Char(ch.to_ascii_lowercase())
            } else {
                Ok::Char(ch)
            }
        }
        Key::Named(NamedKey::Escape) => Ok::Escape,
        Key::Named(NamedKey::Enter) => Ok::Enter,
        Key::Named(NamedKey::Tab) => Ok::Tab,
        Key::Named(NamedKey::Backspace) => Ok::Backspace,
        Key::Named(NamedKey::Delete) => Ok::Delete,
        Key::Named(NamedKey::Insert) => Ok::Insert,
        Key::Named(NamedKey::Home) => Ok::Home,
        Key::Named(NamedKey::End) => Ok::End,
        Key::Named(NamedKey::PageUp) => Ok::PageUp,
        Key::Named(NamedKey::PageDown) => Ok::PageDown,
        Key::Named(NamedKey::ArrowUp) => Ok::ArrowUp,
        Key::Named(NamedKey::ArrowDown) => Ok::ArrowDown,
        Key::Named(NamedKey::ArrowLeft) => Ok::ArrowLeft,
        Key::Named(NamedKey::ArrowRight) => Ok::ArrowRight,
        Key::Named(NamedKey::Space) => Ok::Space,
        Key::Named(named) => Ok::F(winit_named_f_index(named)?),
        _ => return None,
    };
    Some(Shortcut::new(modifiers, key))
}

fn winit_touch_to_orchid(
    touch: &slint::winit_030::winit::event::Touch,
    window: &slint::Window,
) -> Option<TouchEvent> {
    use slint::winit_030::winit::event::TouchPhase as WinitTouchPhase;

    let scale = f64::from(window.scale_factor());
    let logical: slint::winit_030::winit::dpi::LogicalPosition<f64> =
        touch.location.to_logical(scale);
    let phase = match touch.phase {
        WinitTouchPhase::Started => TouchPhase::Began,
        WinitTouchPhase::Moved => TouchPhase::Moved,
        WinitTouchPhase::Ended => TouchPhase::Ended,
        WinitTouchPhase::Cancelled => TouchPhase::Cancelled,
    };
    Some(TouchEvent {
        pointer_id: touch.id as u32,
        phase,
        position: Point::new(logical.x as f32, logical.y as f32),
        pressure: 1.0,
        size: 10.0,
        timestamp: Instant::now(),
    })
}

fn winit_modifiers_match(
    shortcut_mods: orchid_core::Modifiers,
    state: slint::winit_030::winit::keyboard::ModifiersState,
) -> bool {
    use orchid_core::Modifiers;
    state.control_key() == shortcut_mods.contains(Modifiers::CTRL)
        && state.shift_key() == shortcut_mods.contains(Modifiers::SHIFT)
        && state.alt_key() == shortcut_mods.contains(Modifiers::ALT)
        && state.super_key() == shortcut_mods.contains(Modifiers::WIN)
}

fn winit_key_matches(
    shortcut_key: orchid_core::Key,
    logical: &slint::winit_030::winit::keyboard::Key,
) -> bool {
    use orchid_core::Key as Ok;
    use slint::winit_030::winit::keyboard::{Key, NamedKey};
    match (shortcut_key, logical) {
        (Ok::Char(c), Key::Character(s)) => s.as_str().eq_ignore_ascii_case(&c.to_string()),
        (Ok::Escape, Key::Named(NamedKey::Escape)) => true,
        (Ok::Enter, Key::Named(NamedKey::Enter)) => true,
        (Ok::Tab, Key::Named(NamedKey::Tab)) => true,
        (Ok::Backspace, Key::Named(NamedKey::Backspace)) => true,
        (Ok::Delete, Key::Named(NamedKey::Delete)) => true,
        (Ok::Insert, Key::Named(NamedKey::Insert)) => true,
        (Ok::Home, Key::Named(NamedKey::Home)) => true,
        (Ok::End, Key::Named(NamedKey::End)) => true,
        (Ok::PageUp, Key::Named(NamedKey::PageUp)) => true,
        (Ok::PageDown, Key::Named(NamedKey::PageDown)) => true,
        (Ok::ArrowUp, Key::Named(NamedKey::ArrowUp)) => true,
        (Ok::ArrowDown, Key::Named(NamedKey::ArrowDown)) => true,
        (Ok::ArrowLeft, Key::Named(NamedKey::ArrowLeft)) => true,
        (Ok::ArrowRight, Key::Named(NamedKey::ArrowRight)) => true,
        (Ok::Space, Key::Named(NamedKey::Space)) => true,
        (Ok::F(n), Key::Named(named)) => winit_named_f_index(named) == Some(n),
        _ => false,
    }
}

fn winit_named_f_index(key: &slint::winit_030::winit::keyboard::NamedKey) -> Option<u8> {
    use slint::winit_030::winit::keyboard::NamedKey;
    Some(match key {
        NamedKey::F1 => 1,
        NamedKey::F2 => 2,
        NamedKey::F3 => 3,
        NamedKey::F4 => 4,
        NamedKey::F5 => 5,
        NamedKey::F6 => 6,
        NamedKey::F7 => 7,
        NamedKey::F8 => 8,
        NamedKey::F9 => 9,
        NamedKey::F10 => 10,
        NamedKey::F11 => 11,
        NamedKey::F12 => 12,
        NamedKey::F13 => 13,
        NamedKey::F14 => 14,
        NamedKey::F15 => 15,
        NamedKey::F16 => 16,
        NamedKey::F17 => 17,
        NamedKey::F18 => 18,
        NamedKey::F19 => 19,
        NamedKey::F20 => 20,
        NamedKey::F21 => 21,
        NamedKey::F22 => 22,
        NamedKey::F23 => 23,
        NamedKey::F24 => 24,
        NamedKey::F25 => 25,
        NamedKey::F26 => 26,
        NamedKey::F27 => 27,
        NamedKey::F28 => 28,
        NamedKey::F29 => 29,
        NamedKey::F30 => 30,
        NamedKey::F31 => 31,
        NamedKey::F32 => 32,
        NamedKey::F33 => 33,
        NamedKey::F34 => 34,
        NamedKey::F35 => 35,
        _ => return None,
    })
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




fn is_known_widget_type(type_id: &str) -> bool {
    matches!(
        orchid_widgets::WidgetRegistry::canonical_type_id(type_id),
        "terminal"
            | "weather"
            | "moon"
            | "system"
            | "rss"
            | "recent-files"
            | "universal-search"
            | "media-player"
            | "password-manager"
            | "viewer"
            | "file-manager"
    )
}

fn filter_catalog_items(locale: &LocaleManager, query: &str) -> Vec<DockWidgetType> {
    let q = query.trim().to_lowercase();
    dock_types_vec(locale)
        .into_iter()
        .filter(|d| {
            q.is_empty()
                || d.label.as_str().to_lowercase().contains(&q)
                || d.description.as_str().to_lowercase().contains(&q)
                || d.type_id.as_str().to_lowercase().contains(&q)
                || d.icon.as_str().to_lowercase().contains(&q)
        })
        .collect()
}

fn dock_widget_description(locale: &LocaleManager, type_id: &str) -> SharedString {
    let key = match type_id {
        "terminal" => "widget-terminal-desc",
        "weather" => "widget-weather-desc",
        "moon" => "widget-moon-desc",
        "system" => "widget-system-desc",
        "rss" => "widget-rss-desc",
        "recent-files" => "widget-recent-files-desc",
        "search" | "universal-search" => "widget-search-desc",
        "media" => "widget-media-desc",
        "password" => "widget-password-desc",
        "viewer" => "widget-viewer-desc",
        "file-manager" => "widget-fm-desc",
        _ => return SharedString::new(),
    };
    locale.tr(key).into()
}

fn dock_types_vec(locale: &LocaleManager) -> Vec<DockWidgetType> {
    vec![
        DockWidgetType {
            type_id: "terminal".into(),
            label: locale.tr("dock-widget-terminal").into(),
            description: dock_widget_description(locale, "terminal"),
            icon: "terminal".into(),
        },
        DockWidgetType {
            type_id: "weather".into(),
            label: locale.tr("dock-widget-weather").into(),
            description: dock_widget_description(locale, "weather"),
            icon: "weather".into(),
        },
        DockWidgetType {
            type_id: "moon".into(),
            label: locale.tr("dock-widget-moon").into(),
            description: dock_widget_description(locale, "moon"),
            icon: "moon".into(),
        },
        DockWidgetType {
            type_id: "system".into(),
            label: locale.tr("dock-widget-system").into(),
            description: dock_widget_description(locale, "system"),
            icon: "system".into(),
        },
        DockWidgetType {
            type_id: "rss".into(),
            label: locale.tr("dock-widget-rss").into(),
            description: dock_widget_description(locale, "rss"),
            icon: "rss".into(),
        },
        DockWidgetType {
            type_id: "recent-files".into(),
            label: locale.tr("dock-widget-recent-files").into(),
            description: dock_widget_description(locale, "recent-files"),
            icon: "recent-files".into(),
        },
        DockWidgetType {
            type_id: "search".into(),
            label: locale.tr("dock-widget-search").into(),
            description: dock_widget_description(locale, "search"),
            icon: "search".into(),
        },
        DockWidgetType {
            type_id: "media".into(),
            label: locale.tr("dock-widget-media").into(),
            description: dock_widget_description(locale, "media"),
            icon: "media".into(),
        },
        DockWidgetType {
            type_id: "password".into(),
            label: locale.tr("dock-widget-password").into(),
            description: dock_widget_description(locale, "password"),
            icon: "password".into(),
        },
        DockWidgetType {
            type_id: "viewer".into(),
            label: locale.tr("dock-widget-viewer").into(),
            description: dock_widget_description(locale, "viewer"),
            icon: "viewer".into(),
        },
        DockWidgetType {
            type_id: "file-manager".into(),
            label: locale.tr("dock-widget-fm").into(),
            description: dock_widget_description(locale, "file-manager"),
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
        "recent-files" => locale.tr("dock-widget-recent-files").into(),
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
        empty_system_model(locale),
        empty_rss_model(locale),
        empty_search_model(locale),
        empty_media_model(locale),
        empty_password_model(locale),
        empty_viewer_model(locale),
        empty_recent_files_model(locale),
        empty_file_manager_model(locale),
    )
}


pub(super) fn empty_close_confirm_dialog() -> WidgetCloseConfirmDialog {
    WidgetCloseConfirmDialog {
        visible: false,
        title: SharedString::new(),
        message: SharedString::new(),
        save_label: SharedString::new(),
        discard_label: SharedString::new(),
        cancel_label: SharedString::new(),
    }
}

fn apply_settings_field(
    cfg: &mut OrchidConfig,
    section: &str,
    key: &str,
    value: &str,
    locale: &LocaleManager,
) -> Result<(), String> {
    match (section, key) {
        ("general", "open-on-startup") => {
            cfg.general.open_on_startup = parse_settings_bool(value)?;
        }
        ("appearance", "theme") => {
            if value.is_empty() {
                return Err("theme id must not be empty".into());
            }
            cfg.appearance.theme = value.to_string();
        }
        ("appearance", "density") => {
            cfg.appearance.density = match value {
                "touch" => orchid_storage::Density::Touch,
                "mouse" => orchid_storage::Density::Mouse,
                "hybrid" => orchid_storage::Density::Hybrid,
                other => return Err(format!("unknown density `{other}`")),
            };
        }
        ("appearance", "font-family") => {
            let trimmed = value.trim();
            let system_default = locale.tr("settings-value-system-default");
            cfg.appearance.font_family = if trimmed.is_empty() || trimmed == system_default {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        ("appearance", "font-scale") => {
            cfg.appearance.font_scale = value
                .parse::<f32>()
                .map_err(|_| format!("invalid font scale `{value}`"))?;
        }
        ("appearance", "reduce-motion") => {
            cfg.appearance.reduce_motion = parse_settings_bool(value)?;
        }
        ("appearance", "follow-system-theme") => {
            cfg.appearance.follow_system_theme = parse_settings_bool(value)?;
        }
        ("appearance", "dark-theme") => {
            if value.is_empty() {
                return Err("dark theme id must not be empty".into());
            }
            cfg.appearance.dark_theme = value.to_string();
        }
        ("appearance", "light-theme") => {
            if value.is_empty() {
                return Err("light theme id must not be empty".into());
            }
            cfg.appearance.light_theme = value.to_string();
        }
        ("input", "primary-hand") => {
            cfg.input.primary_hand = match value {
                "left" => orchid_storage::Hand::Left,
                "right" => orchid_storage::Hand::Right,
                other => return Err(format!("unknown hand `{other}`")),
            };
        }
        ("input", "mirror-edge-swipes") => {
            cfg.input.mirror_edge_swipes = parse_settings_bool(value)?;
        }
        ("shortcuts", "leader-key") => {
            cfg.shortcuts.leader_key = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        ("shortcuts", "leader-timeout") => {
            cfg.shortcuts.leader_timeout_ms = value
                .parse::<u64>()
                .map_err(|_| format!("invalid leader timeout `{value}`"))?;
        }
        ("locale", "language") => {
            if value.is_empty() {
                return Err("language tag must not be empty".into());
            }
            cfg.locale.language = value.to_string();
        }
        ("locale", "date-format") => {
            let trimmed = value.trim();
            let default_label = locale.tr("settings-value-default");
            cfg.locale.date_format = if trimmed.is_empty() || trimmed == default_label {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        ("locale", "time-format") => {
            let trimmed = value.trim();
            let default_label = locale.tr("settings-value-default");
            cfg.locale.time_format = if trimmed.is_empty() || trimmed == default_label {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
        ("locale", "first-day-of-week") => {
            cfg.locale.first_day_of_week = match value {
                "0" => 0,
                "1" => 1,
                other => return Err(format!("first day of week must be 0 or 1, got `{other}`")),
            };
        }
        ("privacy", "record-action-history") => {
            cfg.privacy.record_action_history = parse_settings_bool(value)?;
        }
        ("privacy", "history-retention-days") => {
            cfg.privacy.history_retention_days = value
                .parse::<u32>()
                .map_err(|_| format!("invalid history retention `{value}`"))?;
        }
        ("privacy", "clear-clipboard-seconds") => {
            cfg.privacy.clear_clipboard_seconds = value
                .parse::<u32>()
                .map_err(|_| format!("invalid clipboard timeout `{value}`"))?;
        }
        ("privacy", "vault-auto-lock-seconds") => {
            cfg.privacy.vault_auto_lock_seconds = value
                .parse::<u32>()
                .map_err(|_| format!("invalid vault auto-lock `{value}`"))?;
        }
        _ => return Err(format!("field `{section}.{key}` is not editable")),
    }
    Ok(())
}

fn parse_settings_bool(value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("expected true/false, got `{other}`")),
    }
}

fn density_i18n_key(density: orchid_storage::Density) -> &'static str {
    match density {
        orchid_storage::Density::Touch => "density-touch",
        orchid_storage::Density::Mouse => "density-mouse",
        orchid_storage::Density::Hybrid => "density-hybrid",
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










