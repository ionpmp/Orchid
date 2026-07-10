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
use secrecy::ExposeSecret;
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
use orchid_storage::{ConfigLoader, LifecycleState, OrchidConfig, StateStore, WidgetSize};
use orchid_terminal::SessionManager;
use orchid_terminal::SplitDirection;
use orchid_terminal::{FontMetrics};
use orchid_widgets::layout::PixelBounds;
use orchid_widgets::layout::ViewportSize;
use orchid_widgets::TerminalPayload;
use orchid_widgets::builtin::search::{self as search_widget, ActionTarget};
use orchid_widgets::WidgetPayload;
use orchid_widgets::{CreateWidgetRequest,
    GroupManager, LayoutEngine, PlacedWidget, RecentFilesStore, WidgetManager, WorkspaceManager,
};
use orchid_widgets::SharedInstance;
use parking_lot::RwLock;

use super::errors::{
    fm_localized_error, is_passphrase_retryable, media_localized_error, password_localized_error,
    storage_localized_error, ui_localized_error, viewer_localized_error,
};
use super::spawn;
use super::models::{
    blank_terminal, build_context_menu, build_file_manager_model, build_managed_policy_state,
    build_media_model, build_moon_model, build_palette_candidates, build_password_model,
    build_recent_files_model, build_rss_model, build_search_model, build_settings_fields,
    build_settings_sections, build_system_model, build_terminal_divider_models,
    build_terminal_model, build_terminal_tab_models, build_viewer_model, build_weather_model,
    default_terminal_divider_models, default_terminal_pane_models, default_terminal_tab_models,
    empty_confirm_dialog, empty_context_menu, empty_file_manager_model, empty_managed_policy_state,
    empty_media_model, empty_moon_model, empty_passphrase_state, empty_password_model,
    empty_recent_files_model, empty_rename_state, empty_rss_model, empty_search_model,
    empty_system_model, empty_tag_state, empty_viewer_model, empty_weather_model,
    fm_passphrase_dialog_labels, locale_display_name, pane_payload_to_terminal, settings_section_id,
    settings_section_index, theme_display_name, FileManagerOverlays, PasswordAddDialogOverlay,
    SETTINGS_SECTION_IDS,
};
use crate::error::{Result, UiError};
use crate::terminal_font_metrics;
use crate::widgets::terminal::{
    add_tab, close_focused_pane_or_tab, close_pane, close_tab, focus_next_pane, focus_pane,
    focus_previous_pane, set_split_ratio, split_horizontal, split_vertical, switch_tab,
    switch_tab_relative, TerminalWidgetDeps,
};
use crate::terminal_raster;
use crate::slint_generated::{
    AppState, DockWidgetType, MainWindow, MediaModel, MoonModel, PasswordModel, RecentFilesModel,
    RssModel, SearchCandidateEntry,
    SearchModel, Strings, SystemModel, TerminalCellModel,
    TerminalPaneModel,
    Theme,
    FileManagerModel, FmConfirmDialog,
    FmRenameState, FmTagState, FmPassphraseState,
    ViewerModel,
    WeatherModel, WidgetCatalog, WidgetCloseConfirmDialog, WidgetFrameModel, WorkspaceModel,
    WorkspaceSummary, CommandPaletteGlobal, NavigationGlobal, NotificationGlobal,
    NotificationItem, OnboardingGlobal, SettingsFieldRow,
    SettingsGlobal,
    SettingsSectionEntry, GroupTabModel,
};
use crate::theme::ThemeManager;

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

    fn wire_callbacks(self: &Arc<Self>) -> Result<()> {
        let t = Arc::downgrade(self);
        self.window.on_ui_tick({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.drain_fm_ingest_failure_notification();
                    if c.config_reload_pending.swap(false, Ordering::AcqRel) {
                        if let Err(e) = c.apply_hot_config() {
                            warn!(?e, "config hot-reload");
                            let reason = ui_localized_error(&c.locale, &e);
                            let body = c.locale.tr_args(
                                "settings-config-reload-failed",
                                &orchid_i18n::FluentArgs::new().with("reason", reason),
                            );
                            c.push_notification(
                                &c.locale.tr("settings-panel-title"),
                                &body,
                                2,
                            );
                        }
                    }
                    let canvas_size_mismatch = c.sync_canvas_size_from_winit();
                    if canvas_size_mismatch {
                        c.update_gesture_bounds();
                        if c.config.read().appearance.density == orchid_storage::Density::Hybrid {
                            let _ = c.apply_theme();
                        }
                    }
                    let gestures = {
                        let mut rec = c.gesture_recognizer.lock();
                        rec.tick(Instant::now())
                    };
                    c.handle_recognized_gestures(gestures);
                    c.check_vault_auto_lock();
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
        self.window.on_canvas_scrolled({
            let t = t.clone();
            move |vx, vy| {
                if let Some(c) = t.upgrade() {
                    c.on_canvas_scrolled(vx, vy);
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
        self.window.on_command_palette_query_changed({
            let t = t.clone();
            move |q| {
                if let Some(c) = t.upgrade() {
                    c.on_command_palette_query_changed(&q);
                }
            }
        });
        self.window.on_command_palette_candidate_activated({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_command_palette_candidate_activated(&id);
                }
            }
        });
        self.window.on_command_palette_selection_changed({
            let t = t.clone();
            move |idx| {
                if let Some(c) = t.upgrade() {
                    c.on_command_palette_selection_changed(idx);
                }
            }
        });
        self.window.on_command_palette_dismiss({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_command_palette_dismiss();
                }
            }
        });
        self.window.on_settings_dismiss({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_settings_dismiss();
                }
            }
        });
        self.window.on_settings_section_selected({
            let t = t.clone();
            move |idx| {
                if let Some(c) = t.upgrade() {
                    c.on_settings_section_selected(idx);
                }
            }
        });
        self.window.on_settings_field_changed({
            let t = t.clone();
            move |section, key, value| {
                if let Some(c) = t.upgrade() {
                    c.on_settings_field_changed(&section, &key, &value);
                }
            }
        });
        self.window.on_settings_open_config({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.open_config_file();
                }
            }
        });
        self.window.on_navigation_workspace_panel_dismiss({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_navigation_workspace_panel_dismiss();
                }
            }
        });
        self.window.on_notification_center_dismiss({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_notification_center_dismiss();
                }
            }
        });
        self.window.global::<NotificationGlobal>().on_clear_all({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.clear_notifications();
                }
            }
        });
        self.window.global::<NotificationGlobal>().on_dismiss_item({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.dismiss_notification(id.as_str());
                }
            }
        });
        self.window.on_onboarding_next_clicked({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_onboarding_next();
                }
            }
        });
        self.window.on_onboarding_back_clicked({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_onboarding_back();
                }
            }
        });
        self.window.on_onboarding_skip_clicked({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_onboarding_skip();
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
        self.window.on_widget_close_confirm_save({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_close_confirm_save(&id);
                }
            }
        });
        self.window.on_widget_close_confirm_discard({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_close_confirm_discard(&id);
                }
            }
        });
        self.window.on_widget_close_confirm_cancel({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_close_confirm_cancel(&id);
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
        self.window.on_group_tab_clicked({
            let t = t.clone();
            move |group_id, member_id| {
                if let Some(c) = t.upgrade() {
                    c.on_group_tab_clicked(&group_id, &member_id);
                }
            }
        });
        self.window.on_group_tab_closed({
            let t = t.clone();
            move |group_id, member_id| {
                if let Some(c) = t.upgrade() {
                    c.on_group_tab_closed(&group_id, &member_id);
                }
            }
        });
        self.window.on_group_tab_move_left({
            let t = t.clone();
            move |group_id, member_id| {
                if let Some(c) = t.upgrade() {
                    c.on_group_tab_move(&group_id, &member_id, -1);
                }
            }
        });
        self.window.on_group_tab_move_right({
            let t = t.clone();
            move |group_id, member_id| {
                if let Some(c) = t.upgrade() {
                    c.on_group_tab_move(&group_id, &member_id, 1);
                }
            }
        });
        self.window.on_group_dissolve_clicked({
            let t = t.clone();
            move |group_id| {
                if let Some(c) = t.upgrade() {
                    c.on_group_dissolve_clicked(&group_id);
                }
            }
        });
        self.window.on_terminal_key_pressed({
            let t = t.clone();
            move |id, text, ctrl, shift, alt| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_key(&id, &text, ctrl, shift, alt);
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
        self.window.on_terminal_tab_clicked({
            let t = t.clone();
            move |id, idx| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_tab_clicked(&id, idx);
                }
            }
        });
        self.window.on_terminal_tab_closed({
            let t = t.clone();
            move |id, idx| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_tab_closed(&id, idx);
                }
            }
        });
        self.window.on_terminal_tab_new({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_tab_new(&id);
                }
            }
        });
        self.window.on_terminal_split_horizontal({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_split_horizontal(&id);
                }
            }
        });
        self.window.on_terminal_split_vertical({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_split_vertical(&id);
                }
            }
        });
        self.window.on_terminal_pane_clicked({
            let t = t.clone();
            move |id, sid| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_pane_clicked(&id, &sid);
                }
            }
        });
        self.window.on_terminal_pane_closed({
            let t = t.clone();
            move |id, sid| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_pane_closed(&id, &sid);
                }
            }
        });
        self.window.on_terminal_split_drag_moved({
            let t = t.clone();
            move |id, first, second, fx, fy| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_split_drag_moved(&id, &first, &second, fx, fy);
                }
            }
        });
        self.window.on_terminal_shortcut({
            let t = t.clone();
            move |id, action| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_shortcut(&id, &action);
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
        self.window.on_recent_files_item_clicked({
            let t = t.clone();
            move |path| {
                if let Some(c) = t.upgrade() {
                    c.on_recent_files_item_clicked(&path);
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
        self.window.on_password_unlock_submit({
            let t = t.clone();
            move |passphrase| {
                if let Some(c) = t.upgrade() {
                    c.on_password_unlock_submit(&passphrase);
                }
            }
        });
        self.window.on_password_unlock_biometric({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_password_unlock_biometric();
                }
            }
        });
        self.window.on_password_lock_vault({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_password_lock_vault();
                }
            }
        });
        self.window.on_password_add_entry_request({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_password_add_entry_request();
                }
            }
        });
        self.window.on_password_add_entry_commit({
            let t = t.clone();
            move |title, username, password, url| {
                if let Some(c) = t.upgrade() {
                    c.on_password_add_entry_commit(&title, &username, &password, &url);
                }
            }
        });
        self.window.on_password_add_entry_cancel({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_password_add_entry_cancel();
                }
            }
        });
        self.window.on_password_add_entry_generate_password({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_password_add_entry_generate_password();
                }
            }
        });

        macro_rules! viewer_spawn {
            ($weak:expr, $fut:expr) => {{
                let tw = $weak.clone();
                spawn::spawn_local_compat(async move {
                    if let Err(e) = $fut.await {
                        warn!(?e, "viewer action");
                        if let Some(c) = tw.upgrade() {
                            let title = c.locale.tr("widget-viewer-name");
                            let reason = viewer_localized_error(&c.locale, &e.to_string());
                            let body = c.locale.tr_args(
                                "viewer-action-failed",
                                &orchid_i18n::FluentArgs::new().with("reason", reason),
                            );
                            c.push_notification(&title, &body, 3);
                        }
                    }
                    if let Some(c) = tw.upgrade() {
                        c.schedule_rebuild();
                    }
                });
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
        self.window.on_viewer_image_flip_h({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::image_flip_h(inst));
                    }
                }
            }
        });
        self.window.on_viewer_image_flip_v({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::image_flip_v(inst));
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
                        spawn::spawn_local_compat(async move {
                            if let Err(e) =
                                orchid_widgets::builtin::viewer::image_pan(inst, dx, dy).await
                            {
                                warn!(?e, "viewer pan");
                            }
                            if let Some(ctrl) = tw.upgrade() {
                                ctrl.schedule_rebuild();
                            }
                        });
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
                        viewer_spawn!(
                            tw,
                            orchid_widgets::builtin::viewer::set_viewport(inst, w, h)
                        );
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
                        viewer_spawn!(
                            tw,
                            orchid_widgets::builtin::viewer::pdf_fit_width(inst, vw)
                        );
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
                        viewer_spawn!(
                            tw,
                            orchid_widgets::builtin::viewer::pdf_fit_page(inst, vw, vh)
                        );
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
                        viewer_spawn!(
                            tw,
                            orchid_widgets::builtin::viewer::archive_navigate_into(inst, p)
                        );
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
                        viewer_spawn!(
                            tw,
                            orchid_widgets::builtin::viewer::archive_select(inst, p)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_archive_extract_selected({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::archive_extract_selected(inst));
                    }
                }
            }
        });
        self.window.on_viewer_archive_extract_all({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::archive_extract_all(inst));
                    }
                }
            }
        });
        self.window.on_viewer_text_toggle_edit({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::text_toggle_edit(inst));
                    }
                }
            }
        });
        self.window.on_viewer_text_save({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::text_save(inst));
                    }
                }
            }
        });
        self.window.on_viewer_text_edited({
            let t = t.clone();
            move |id, text| {
                if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                    if let Some(c) = t.upgrade() {
                        *c.last_text_edit_instance.lock() = Some(inst);
                    }
                    let body = text.to_string();
                    // Push edits without schedule_rebuild so the multiline
                    // TextInput keeps caret position; dirty ● uses local state.
                    spawn::spawn_local_compat(async move {
                        if let Err(e) =
                            orchid_widgets::builtin::viewer::text_push_edit(inst, body).await
                        {
                            warn!(?e, "viewer text edit");
                        }
                    });
                }
            }
        });
        self.window.on_viewer_text_scroll({
            let t = t.clone();
            move |id, delta| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(
                            tw,
                            orchid_widgets::builtin::viewer::text_scroll(inst, delta)
                        );
                    }
                }
            }
        });

        self.window.on_fm_sidebar_clicked({
            let t = t.clone();
            move |fm_id, id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_sidebar_clicked(&fm_id, &id);
                }
            }
        });
        self.window.on_fm_toggle_dual_pane({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_toggle_dual_pane(&fm_id);
                }
            }
        });
        self.window.on_fm_toggle_show_hidden({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_toggle_show_hidden(&fm_id);
                }
            }
        });
        self.window.on_fm_toggle_click_behavior({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_toggle_click_behavior(&fm_id);
                }
            }
        });
        self.window.on_fm_pane_clicked({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_pane_clicked(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_tab_clicked({
            let t = t.clone();
            move |fm_id, pane, tab_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_tab_clicked(&fm_id, pane, &tab_id);
                }
            }
        });
        self.window.on_fm_tab_closed({
            let t = t.clone();
            move |fm_id, pane, tab_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_tab_closed(&fm_id, pane, &tab_id);
                }
            }
        });
        self.window.on_fm_tab_new({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_tab_new(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_new_folder({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_new_folder(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_nav_back({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_nav_back(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_nav_forward({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_nav_forward(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_nav_up({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_nav_up(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_nav_home({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_nav_home(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_breadcrumb_clicked({
            let t = t.clone();
            move |fm_id, pane, path| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_breadcrumb_clicked(&fm_id, pane, &path);
                }
            }
        });
        self.window.on_fm_view_mode_cycle({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_view_mode_cycle(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_sort_cycle({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_sort_cycle(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_sort_column_clicked({
            let t = t.clone();
            move |fm_id, pane, col| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_sort_column_clicked(&fm_id, pane, col);
                }
            }
        });
        self.window.on_fm_quick_filter_changed({
            let t = t.clone();
            move |fm_id, pane, q| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_quick_filter_changed(&fm_id, pane, &q);
                }
            }
        });
        self.window.on_fm_entry_clicked({
            let t = t.clone();
            move |fm_id, pane, path, ctrl| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_clicked(&fm_id, pane, &path, ctrl);
                }
            }
        });
        self.window.on_fm_entry_shift_clicked({
            let t = t.clone();
            move |fm_id, pane, path| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_shift_clicked(&fm_id, pane, &path);
                }
            }
        });
        self.window.on_fm_entry_double_clicked({
            let t = t.clone();
            move |fm_id, pane, path, is_dir| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_double_clicked(&fm_id, pane, &path, is_dir);
                }
            }
        });
        self.window.on_fm_entry_context({
            let t = t.clone();
            move |fm_id, pane, path, x, y| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_context(&fm_id, pane, &path, x, y);
                }
            }
        });
        self.window.on_fm_context_action({
            let t = t.clone();
            move |fm_id, action_id, paths| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_context_action(&fm_id, &action_id, &paths);
                }
            }
        });
        self.window.on_fm_context_dismiss({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_context_dismiss(&fm_id);
                }
            }
        });
        self.window.on_fm_confirm_yes({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_confirm_yes(&fm_id);
                }
            }
        });
        self.window.on_fm_confirm_no({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_confirm_no(&fm_id);
                }
            }
        });
        self.window.on_fm_rename_commit({
            let t = t.clone();
            move |fm_id, old_path, new_name| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_rename_commit(&fm_id, &old_path, &new_name);
                }
            }
        });
        self.window.on_fm_rename_cancel({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_rename_cancel(&fm_id);
                }
            }
        });
        self.window.on_fm_tag_commit({
            let t = t.clone();
            move |fm_id, tag| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_tag_commit(&fm_id, &tag);
                }
            }
        });
        self.window.on_fm_tag_cancel({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_tag_cancel(&fm_id);
                }
            }
        });
        self.window.on_fm_passphrase_commit({
            let t = t.clone();
            move |fm_id, pw| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_passphrase_commit(&fm_id, &pw);
                }
            }
        });
        self.window.on_fm_passphrase_cancel({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_passphrase_cancel(&fm_id);
                }
            }
        });
        self.window.on_fm_passphrase_biometric({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_passphrase_biometric(&fm_id);
                }
            }
        });
        self.window.on_fm_managed_policy_close({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_managed_policy_close(&fm_id);
                }
            }
        });
        self.window.on_fm_select_all({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_select_all(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_delete_selected({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_delete_selected(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_copy_selected({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_copy_selected(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_paste_clipboard({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_paste_clipboard(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_rename_selected({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_rename_selected(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_deselect_all({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_deselect_all(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_open_selected({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_open_selected(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_entry_drag_start({
            let t = t.clone();
            move |fm_id, pane, path| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_drag_start(&fm_id, pane, &path);
                }
            }
        });
        self.window.on_fm_entry_drag_hover({
            let t = t.clone();
            move |fm_id, pane, path| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_drag_hover(&fm_id, pane, &path);
                }
            }
        });
        self.window.on_fm_entry_drag_drop({
            let t = t.clone();
            move |fm_id, pane, path| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_drag_drop(&fm_id, pane, &path);
                }
            }
        });
        self.window.on_fm_entry_drag_cancel({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_drag_cancel(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_pane_drag_hover({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_pane_drag_hover(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_drop_on_current_dir({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_drop_on_current_dir(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_entry_drag_scroll({
            let t = t.clone();
            move |fm_id, pane, mouse_x, mouse_y, viewport_y, width| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_drag_scroll(&fm_id, pane, mouse_x, mouse_y, viewport_y, width);
                }
            }
        });
        self.window.on_fm_error_action({
            let t = t.clone();
            move |_fm_id, _pane| {
                if let Some(c) = t.upgrade() {
                    c.open_config_file();
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
        let focus_search_input =
            type_id_owned == "search" || type_id_owned == "universal-search";
        let focus_password_input = type_id_owned == "password";
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
        g.set_visible(cat.visible);
        g.set_screen_x(cat.screen_x);
        g.set_screen_y(cat.screen_y);
        g.set_search_query(cat.search_query.into());
        g.set_items(self.catalog_items.clone());
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

    fn notify_fm_action_failed(self: &Arc<Self>, err: &impl std::fmt::Display) {
        let title = self.locale.tr("widget-fm-name");
        let reason = fm_localized_error(&self.locale, &err.to_string());
        let body = self.locale.tr_args(
            "fm-action-failed",
            &orchid_i18n::FluentArgs::new().with("reason", reason),
        );
        self.push_notification(&title, &body, 3);
    }

    /// Append a notification (newest first). `severity`: 0 info, 1 tip, 2 warning, 3 error.
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
    fn sync_fm_transfer_notifications(self: &Arc<Self>) {
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

    fn drain_fm_ingest_failure_notification(self: &Arc<Self>) {
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
        let cfg = self.config.read().clone();
        if let Err(e) = cfg.validate() {
            warn!(?e, "config validation failed on save");
            return;
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
        self.spawn_add_widget("universal-search", AddWidgetPlacement::AutoSlot);
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

    fn on_widget_close(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        if self.viewer_text_unsaved(u) {
            self.show_viewer_unsaved_close_confirm(u);
            return;
        }
        self.finish_widget_close(u);
    }

    fn on_widget_close_confirm_save(self: &Arc<Self>, id: &SharedString) {
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

    fn on_widget_close_confirm_discard(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        self.clear_close_confirm_overlay(u);
        self.finish_widget_close(u);
    }

    fn on_widget_close_confirm_cancel(self: &Arc<Self>, id: &SharedString) {
        let Ok(u) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        self.clear_close_confirm_overlay(u);
    }

    fn viewer_text_unsaved(&self, id: Uuid) -> bool {
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
        )
    }

    fn show_viewer_unsaved_close_confirm(&self, id: Uuid) {
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

    fn clear_close_confirm_overlay(self: &Arc<Self>, id: Uuid) {
        self.close_confirm_overlays.write().remove(&id);
        self.patch_frame_close_confirm(id);
    }

    fn patch_frame_close_confirm(&self, id: Uuid) {
        let dlg = self
            .close_confirm_overlays
            .read()
            .get(&id)
            .cloned()
            .unwrap_or_else(empty_close_confirm_dialog);
        let v = match self
            .workspace_widgets
            .as_any()
            .downcast_ref::<VecModel<WidgetFrameModel>>()
        {
            Some(m) => m,
            None => return,
        };
        let needle = id.to_string();
        for r in 0..v.row_count() {
            let Some(mut row) = v.row_data(r) else {
                continue;
            };
            if row.instance_id.as_str() == needle.as_str() {
                row.close_confirm = dlg;
                v.set_row_data(r, row);
                return;
            }
        }
    }

    fn finish_widget_close(self: &Arc<Self>, u: Uuid) {
        self.close_confirm_overlays.write().remove(&u);
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
                if c
                    .fm_focus
                    .lock()
                    .is_some_and(|(fm_id, _)| fm_id == u)
                {
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

        let (snap_bounds, placement_valid) = self.drag_snap_preview(instance, fx, fy);

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
            self.sync_canvas_scroll_extent();
            return;
        }
        self.schedule_rebuild();
    }

    /// Snapped cell bounds + whether that placement is free of collisions.
    fn drag_snap_preview(
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
        let all = self.widget_manager.instances_for_workspace(w.id);
        let valid = le.can_place(w.id, instance, pos, size, &all).is_ok();
        let bounds = le.pixel_bounds_for(pos, size, viewport);
        (Some(bounds), valid)
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
            let all = c.widget_manager.instances_for_workspace(w.id);

            // Drop onto another widget's header → form / join a group.
            if let Some(target_id) = c.find_group_drop_target(u, new_x, new_y, start.width) {
                if let Err(e) = c
                    .form_or_join_group(w.id, u, target_id, pos, size)
                    .await
                {
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
                            let _ = wm.move_to(*mid, pos).await;
                            let _ = wm.resize(*mid, size).await;
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

    /// Header hit-test: pointer near another frame's title bar → group drop target.
    fn find_group_drop_target(
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
        let instances = self.widget_manager.instances_for_workspace(w.id);
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

    async fn form_or_join_group(
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
                let _ = self
                    .group_manager
                    .remove_from_group(prev.id, dragged)
                    .await;
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
            let _ = self.widget_manager.move_to(dragged, slot_pos).await;
            let _ = self.widget_manager.resize(dragged, slot_size).await;
            let _ = self
                .group_manager
                .switch_active(target_group.id, dragged)
                .await;
            return Ok(());
        }

        // Target is ungrouped — create a new group.
        if let Some(prev) = self.group_manager.find_for_instance(dragged) {
            let _ = self
                .group_manager
                .remove_from_group(prev.id, dragged)
                .await;
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
            .create_group(
                workspace_id,
                vec![target, dragged],
                slot_pos,
                slot_size,
            )
            .await
            .map_err(|e| UiError::Slint(format!("create group: {e}")))?;
        for mid in [target, dragged] {
            if let Ok(inst) = self.widget_manager.get_instance(mid) {
                *inst.group_id.write() = Some(gid);
            }
            let _ = self.widget_manager.move_to(mid, slot_pos).await;
            let _ = self.widget_manager.resize(mid, slot_size).await;
        }
        let _ = self.group_manager.switch_active(gid, dragged).await;
        Ok(())
    }

    async fn dissolve_group_internal(self: &Arc<Self>, group_id: Uuid) -> Result<()> {
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

    fn on_group_tab_clicked(self: &Arc<Self>, group_id: &SharedString, member_id: &SharedString) {
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

    fn on_group_tab_closed(self: &Arc<Self>, group_id: &SharedString, member_id: &SharedString) {
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

    fn on_group_tab_move(
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

    fn on_group_dissolve_clicked(self: &Arc<Self>, group_id: &SharedString) {
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
        let (snap_bounds, placement_valid) = self.resize_snap_preview(instance, &b);
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
            self.sync_canvas_scroll_extent();
            return;
        }
        self.schedule_rebuild();
    }

    fn resize_snap_preview(
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
        let all = self.widget_manager.instances_for_workspace(w.id);
        let valid = le.can_place(w.id, instance, pos, size, &all).is_ok();
        let snapped = le.pixel_bounds_for(pos, size, viewport);
        (Some(snapped), valid)
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
            let all = c.widget_manager.instances_for_workspace(ws_id);
            if le.can_place(ws_id, u, new_pos, new_size, &all).is_err() {
                c.push_notification(
                    &c.locale.tr("workspace-placement-blocked-title"),
                    &c.locale.tr("workspace-placement-blocked-body"),
                    2,
                );
                c.schedule_rebuild();
                return;
            }
            if let Err(e) = wm.move_to(u, new_pos).await {
                warn!(?e, "resize move");
            }
            if let Err(e) = wm.resize(u, new_size).await {
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
                            let _ = wm.move_to(*mid, new_pos).await;
                            let _ = wm.resize(*mid, new_size).await;
                        }
                    }
                }
            }
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }

    /// Content area of [`widget-frame.slint`] below the title bar (`height - 32px`); must match
    /// what `terminal-viewport-changed` would report as `w`/`h`.
    const WIDGET_FRAME_HEADER_PX: f32 = 32.0;
    /// Height of [`terminal-tabs.slint`] inside the terminal widget content area.
    const TERMINAL_TAB_BAR_PX: f32 = 29.0;
    /// Height of [`group-tabs.slint`] when a frame is part of a multi-widget group.
    const GROUP_TAB_BAR_PX: f32 = 28.0;

    /// Resize PTY grids for every pane in the active tab to match the terminal viewport.
    /// Returns `true` if any session was resized.
    fn resize_terminal_pty_to_content(
        self: &Arc<Self>,
        inst: Uuid,
        viewport_w: f32,
        viewport_h: f32,
    ) -> bool {
        let w = viewport_w.max(1.0);
        let h = viewport_h.max(1.0);
        let layout = self
            .terminal_deps
            .layouts
            .lock()
            .get(&inst)
            .cloned();
        let Some(layout) = layout else {
            return false;
        };
        let snap = layout.snapshot();
        let Some(tab) = snap.tabs.get(snap.active_tab) else {
            return false;
        };
        if tab.panes.is_empty() {
            return false;
        }
        let mut any = false;
        for pane in &tab.panes {
            let pw = w * (pane.bounds.right - pane.bounds.left);
            let ph = h * (pane.bounds.bottom - pane.bounds.top);
            let pty = self.font_metrics.fit(pw.max(1.0), ph.max(1.0));
            {
                let last = self.last_terminal_viewport_pty.lock();
                if last.get(&pane.session) == Some(&(pty.cols, pty.rows)) {
                    continue;
                }
            }
            let Ok(s) = self.session_manager.get(pane.session) else {
                continue;
            };
            if let Err(e) = s.resize(pty) {
                warn!(?e, "pty");
                continue;
            }
            self.last_terminal_viewport_pty
                .lock()
                .insert(pane.session, (pty.cols, pty.rows));
            any = true;
        }
        any
    }

    fn raster_terminal_payload(&self, t: &TerminalPayload) -> Image {
        if let Some(ref f) = self.mono_font {
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
        }
    }

    fn build_terminal_pane_models(&self, t: &TerminalPayload) -> ModelRc<TerminalPaneModel> {
        let panes: Vec<TerminalPaneModel> = if t.panes.is_empty() {
            let mini = TerminalPayload {
                cols: t.cols,
                rows: t.rows,
                cells: t.cells.clone(),
                cursor_col: t.cursor_col,
                cursor_row: t.cursor_row,
                cursor_visible: t.cursor_visible,
                tabs: Vec::new(),
                active_tab: 0,
                panes: Vec::new(),
                dividers: Vec::new(),
            };
            vec![TerminalPaneModel {
                session_id: SharedString::new(),
                left: 0.0,
                top: 0.0,
                right: 1.0,
                bottom: 1.0,
                is_focused: true,
                show_close: false,
                cols: i32::from(t.cols),
                rows: i32::from(t.rows),
                cells: build_terminal_model(&mini),
                pixels: self.raster_terminal_payload(&mini),
                cursor_col: i32::from(t.cursor_col),
                cursor_row: i32::from(t.cursor_row),
                cursor_visible: t.cursor_visible,
            }]
        } else {
            t.panes
                .iter()
                .map(|p| {
                    let mini = pane_payload_to_terminal(p);
                    TerminalPaneModel {
                        session_id: p.session_id.clone().into(),
                        left: p.left,
                        top: p.top,
                        right: p.right,
                        bottom: p.bottom,
                        is_focused: p.is_focused,
                        show_close: p.show_close,
                        cols: i32::from(p.cols),
                        rows: i32::from(p.rows),
                        cells: build_terminal_model(&mini),
                        pixels: self.raster_terminal_payload(&mini),
                        cursor_col: i32::from(p.cursor_col),
                        cursor_row: i32::from(p.cursor_row),
                        cursor_visible: p.cursor_visible,
                    }
                })
                .collect()
        };
        ModelRc::new(VecModel::from(panes))
    }

    fn on_terminal_key(
        self: &Arc<Self>,
        id: &SharedString,
        text: &SharedString,
        ctrl: bool,
        shift: bool,
        alt: bool,
    ) {
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
        let encoder = session.encoder.read();
        let encoded = encode_slint_key_event(text.as_str(), ctrl, shift, alt, &encoder);
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

    fn on_recent_files_item_clicked(self: &Arc<Self>, path: &SharedString) {
        let s = path.as_str();
        if s.is_empty() {
            return;
        }
        let Ok(fp) = orchid_fs::FsPath::new(s) else {
            return;
        };
        let path_label = s.to_string();
        let ctrl = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = Self::open_in_viewer_for_controller(ctrl, fp, true, true).await {
                warn!(?e, path = %path_label, "open recent file in viewer");
            }
        });
    }

    fn on_media_play_pause(self: &Arc<Self>) {
        let Some((inst_id, is_playing)) = self.find_active_media_widget() else {
            return;
        };
        let t = Arc::downgrade(self);
        spawn::spawn_local(async move {
            let cmd = if is_playing { "pause" } else { "play" };
            if let Err(e) = orchid_widgets::builtin::media::execute_command(inst_id, cmd).await {
                warn!(?e, "media play/pause");
                if let Some(c) = t.upgrade() {
                    c.notify_media_control_failed(&e);
                }
            }
        });
    }

    fn on_media_command(self: &Arc<Self>, cmd: &'static str) {
        let Some((inst_id, _)) = self.find_active_media_widget() else {
            return;
        };
        let t = Arc::downgrade(self);
        spawn::spawn_local(async move {
            if let Err(e) = orchid_widgets::builtin::media::execute_command(inst_id, cmd).await {
                warn!(?e, "media command");
                if let Some(c) = t.upgrade() {
                    c.notify_media_control_failed(&e);
                }
            }
        });
    }

    fn notify_media_control_failed(self: &Arc<Self>, err: &orchid_widgets::builtin::media::MediaError) {
        let body = media_localized_error(&self.locale, err);
        self.push_notification(&self.locale.tr("widget-media-name"), &body, 2);
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

    fn touch_vault_activity(self: &Arc<Self>) {
        *self.vault_last_activity.lock() = Some(Instant::now());
    }

    fn check_vault_auto_lock(self: &Arc<Self>) {
        let timeout_secs = self.config.read().privacy.vault_auto_lock_seconds;
        if timeout_secs == 0 {
            return;
        }
        if !self.password_vault.is_unlocked() {
            return;
        }
        let mut last = self.vault_last_activity.lock();
        let Some(at) = *last else {
            // Unlocked without a recorded touch (e.g. restored session) — start the timer now.
            *last = Some(Instant::now());
            return;
        };
        if at.elapsed() >= Duration::from_secs(u64::from(timeout_secs)) {
            drop(last);
            self.push_notification(
                &self.locale.tr("widget-password-name"),
                &self.locale.tr("password-locked"),
                0,
            );
            self.on_password_lock_vault();
        }
    }

    fn on_password_search_changed(self: &Arc<Self>, q: &SharedString) {
        let query = q.to_string();
        let Some(inst_id) = self.find_active_password_widget() else {
            return;
        };
        self.touch_vault_activity();
        orchid_widgets::builtin::password::update_search(inst_id, query);
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = wm.refresh_snapshot_cache(inst_id).await;
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }

    fn on_password_entry_clicked(self: &Arc<Self>, id: &SharedString) {
        let entry_id = id.to_string();
        let Some(inst_id) = self.find_active_password_widget() else {
            return;
        };
        self.touch_vault_activity();
        orchid_widgets::builtin::password::select_entry(inst_id, entry_id);
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = wm.refresh_snapshot_cache(inst_id).await;
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }

    fn on_password_copy(self: &Arc<Self>, id: &SharedString, kind: PasswordCopyKind) {
        let entry_id = id.to_string();
        let Some(inst_id) = self.find_active_password_widget() else {
            return;
        };
        self.touch_vault_activity();
        let clear_clipboard_secs = self.config.read().privacy.clear_clipboard_seconds;
        let t = Arc::downgrade(self);
        let locale = self.locale.clone();
        spawn::spawn_local_compat(async move {
            let toast_key = match kind {
                PasswordCopyKind::Password => {
                    match orchid_widgets::builtin::password::copy_password(
                        inst_id,
                        &entry_id,
                        clear_clipboard_secs,
                    )
                    .await
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
                    match orchid_widgets::builtin::password::copy_totp(
                        inst_id,
                        &entry_id,
                        clear_clipboard_secs,
                    )
                    .await
                    {
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
            let title = locale.tr("widget-password-name");
            c.password_toasts.write().insert(inst_id, (msg.clone(), true));
            c.push_notification(&title, &msg, 1);
            c.schedule_rebuild();

            let t2 = Arc::downgrade(&c);
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            if let Some(cc) = t2.upgrade() {
                cc.password_toasts.write().remove(&inst_id);
                cc.schedule_rebuild();
            }
        });
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

    fn on_password_unlock_submit(self: &Arc<Self>, passphrase: &SharedString) {
        let pass = passphrase.to_string();
        if pass.is_empty() {
            return;
        }
        let vault = self.password_vault.clone();
        let bus = self.bus.clone();
        match orchid_widgets::builtin::password::unlock_with_passphrase(vault, bus, &pass) {
            Ok(()) => self.touch_vault_activity(),
            Err(e) => orchid_widgets::builtin::password::record_unlock_error(e),
        }
        self.schedule_rebuild_after_password_unlock();
    }

    fn on_password_unlock_biometric(self: &Arc<Self>) {
        let prompt = self.locale.tr("password-unlock-biometric-prompt");
        let vault = self.password_vault.clone();
        let bus = self.bus.clone();
        match orchid_widgets::builtin::password::unlock_with_biometric(vault, bus, &prompt) {
            Ok(()) => self.touch_vault_activity(),
            Err(e) => orchid_widgets::builtin::password::record_unlock_error(e),
        }
        self.schedule_rebuild_after_password_unlock();
    }

    fn schedule_rebuild_after_password_unlock(self: &Arc<Self>) {
        let Some(inst_id) = self.find_active_password_widget() else {
            self.schedule_rebuild();
            return;
        };
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = wm.refresh_snapshot_cache(inst_id).await;
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }

    fn on_password_lock_vault(self: &Arc<Self>) {
        orchid_widgets::builtin::password::lock_vault(
            self.password_vault.clone(),
            self.bus.clone(),
        );
        *self.vault_last_activity.lock() = None;
        self.schedule_rebuild_after_password_unlock();
    }

    fn on_password_add_entry_request(self: &Arc<Self>) {
        let Some(inst_id) = self.find_active_password_widget() else {
            return;
        };
        self.touch_vault_activity();
        self.password_add_dialogs.write().insert(
            inst_id,
            PasswordAddDialogOverlay {
                visible: true,
                error: None,
                request_autofocus: true,
                ..Default::default()
            },
        );
        self.schedule_rebuild_after_password_unlock();
    }

    fn on_password_add_entry_commit(
        self: &Arc<Self>,
        title: &SharedString,
        username: &SharedString,
        password: &SharedString,
        url: &SharedString,
    ) {
        let Some(inst_id) = self.find_active_password_widget() else {
            return;
        };
        self.touch_vault_activity();
        let url_opt = if url.is_empty() {
            None
        } else {
            Some(url.to_string())
        };
        match orchid_widgets::builtin::password::create_entry(
            inst_id,
            self.password_vault.clone(),
            title.to_string(),
            username.to_string(),
            password.to_string(),
            url_opt,
        ) {
            Ok(_) => {
                self.password_add_dialogs.write().remove(&inst_id);
                let msg = self.locale.tr("password-entry-added");
                self.password_toasts.write().insert(inst_id, (msg, true));
                self.schedule_rebuild_after_password_unlock();
                let t = Arc::downgrade(self);
                spawn::spawn_local_compat(async move {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    if let Some(c) = t.upgrade() {
                        c.password_toasts.write().remove(&inst_id);
                        c.schedule_rebuild_after_password_unlock();
                    }
                });
            }
            Err(e) => {
                let error = password_localized_error(&self.locale, &e);
                self.password_add_dialogs.write().insert(
                    inst_id,
                    PasswordAddDialogOverlay {
                        visible: true,
                        error: Some(error),
                        request_autofocus: false,
                        ..self
                            .password_add_dialogs
                            .read()
                            .get(&inst_id)
                            .cloned()
                            .unwrap_or_default()
                    },
                );
                self.schedule_rebuild_after_password_unlock();
            }
        }
    }

    fn on_password_add_entry_cancel(self: &Arc<Self>) {
        let Some(inst_id) = self.find_active_password_widget() else {
            return;
        };
        self.password_add_dialogs.write().remove(&inst_id);
        self.schedule_rebuild_after_password_unlock();
    }

    fn on_password_add_entry_generate_password(self: &Arc<Self>) {
        let Some(inst_id) = self.find_active_password_widget() else {
            return;
        };
        self.touch_vault_activity();
        let password = orchid_crypto::generate_password(orchid_crypto::DEFAULT_PASSWORD_LENGTH)
            .unwrap_or_default();
        let mut overlay = self
            .password_add_dialogs
            .read()
            .get(&inst_id)
            .cloned()
            .unwrap_or_default();
        overlay.visible = true;
        overlay.generation_seq = overlay.generation_seq.saturating_add(1);
        overlay.generated_password = Some(password);
        self.password_add_dialogs.write().insert(inst_id, overlay);
        self.schedule_rebuild_after_password_unlock();
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
        // Do not rebuild the whole workspace on every keystroke — that recreates
        // SearchView, steals focus, and races the debouncer. Snapshot updates
        // arrive via `WidgetSnapshotUpdated` and are patched through
        // `patch_workspace_frames` on the next frame.
        let wm = self.widget_manager.clone();
        spawn::spawn_local_compat(async move {
            wm.touch(instance_id);
            if let Ok(inst_ref) = wm.get_instance(instance_id) {
                if *inst_ref.lifecycle.read() == LifecycleState::Sleeping {
                    let _ = wm
                        .change_lifecycle(instance_id, LifecycleState::Active)
                        .await;
                }
            }
        });
    }

    fn on_search_candidate_activated(self: &Arc<Self>, inst: &SharedString, cand: &SharedString) {
        let Ok(instance_id) = Uuid::parse_str(inst.as_str()) else {
            return;
        };
        let candidate_id = cand.to_string();
        let this = Arc::clone(self);
        spawn::spawn_local_compat(async move {
            this.dispatch_search_action_target(instance_id, candidate_id).await;
        });
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
                self.dispatch_command(&cmd_id).await;
            }
            ActionTarget::OpenSettings(section) => {
                self.open_settings(&section);
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
            new_idx.clamp(0, (count - 1) as i32)
        };
        self.search_selection.write().insert(instance_id, clamped);
        let _ = self.patch_workspace_frames(&[instance_id]);
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

    fn on_terminal_tab_clicked(self: &Arc<Self>, id: &SharedString, tab_idx: i32) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        if tab_idx < 0 {
            return;
        }
        let deps = self.terminal_deps.clone();
        let tw = Arc::downgrade(self);
        let idx = tab_idx as usize;
        spawn::spawn_local_compat(async move {
            if let Err(e) = switch_tab(&deps, inst, idx) {
                warn!(?e, %inst, tab_idx = idx, "terminal tab switch");
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    fn on_terminal_tab_new(self: &Arc<Self>, id: &SharedString) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let deps = self.terminal_deps.clone();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = add_tab(&deps, inst).await {
                warn!(?e, %inst, "terminal tab add");
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    fn on_terminal_tab_closed(self: &Arc<Self>, id: &SharedString, tab_idx: i32) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        if tab_idx < 0 {
            return;
        };
        let deps = self.terminal_deps.clone();
        let tw = Arc::downgrade(self);
        let idx = tab_idx as usize;
        spawn::spawn_local_compat(async move {
            if let Err(e) = close_tab(&deps, inst, idx).await {
                warn!(?e, %inst, tab_idx = idx, "terminal tab close");
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    fn on_terminal_split_horizontal(self: &Arc<Self>, id: &SharedString) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let deps = self.terminal_deps.clone();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = split_horizontal(&deps, inst).await {
                warn!(?e, %inst, "terminal split horizontal");
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    fn on_terminal_split_vertical(self: &Arc<Self>, id: &SharedString) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let deps = self.terminal_deps.clone();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = split_vertical(&deps, inst).await {
                warn!(?e, %inst, "terminal split vertical");
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    fn on_terminal_pane_clicked(self: &Arc<Self>, id: &SharedString, session_id: &SharedString) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let Ok(sid) = Uuid::parse_str(session_id.as_str()) else {
            return;
        };
        let deps = self.terminal_deps.clone();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = focus_pane(&deps, inst, sid) {
                warn!(?e, %inst, %sid, "terminal pane focus");
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    fn on_terminal_pane_closed(self: &Arc<Self>, id: &SharedString, session_id: &SharedString) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let Ok(sid) = Uuid::parse_str(session_id.as_str()) else {
            return;
        };
        let deps = self.terminal_deps.clone();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = close_pane(&deps, inst, sid).await {
                warn!(?e, %inst, %sid, "terminal pane close");
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    fn on_terminal_split_drag_moved(
        self: &Arc<Self>,
        id: &SharedString,
        first: &SharedString,
        second: &SharedString,
        fx: f32,
        fy: f32,
    ) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let Ok(first_uuid) = Uuid::parse_str(first.as_str()) else {
            return;
        };
        let Ok(second_uuid) = Uuid::parse_str(second.as_str()) else {
            return;
        };
        let ratio = {
            let layouts = self.terminal_deps.layouts.lock();
            let Some(layout) = layouts.get(&inst) else {
                return;
            };
            let snap = layout.snapshot();
            let Some(tab) = snap.tabs.get(snap.active_tab) else {
                return;
            };
            let Some(div) = tab
                .dividers
                .iter()
                .find(|d| d.first_session == first_uuid && d.second_session == second_uuid)
            else {
                return;
            };
            match div.direction {
                SplitDirection::Horizontal => {
                    let pw = div.parent_bounds.right - div.parent_bounds.left;
                    if pw <= 0.0 {
                        return;
                    }
                    ((fx - div.parent_bounds.left) / pw).clamp(0.05, 0.95)
                }
                SplitDirection::Vertical => {
                    let ph = div.parent_bounds.bottom - div.parent_bounds.top;
                    if ph <= 0.0 {
                        return;
                    }
                    ((fy - div.parent_bounds.top) / ph).clamp(0.05, 0.95)
                }
            }
        };
        let deps = self.terminal_deps.clone();
        if let Err(e) = set_split_ratio(&deps, inst, first_uuid, second_uuid, ratio) {
            warn!(?e, %inst, %first_uuid, %second_uuid, "terminal split drag");
        }
        self.schedule_rebuild();
    }

    fn on_terminal_shortcut(self: &Arc<Self>, id: &SharedString, action: &SharedString) {
        let Ok(inst) = Uuid::parse_str(id.as_str()) else {
            return;
        };
        let deps = self.terminal_deps.clone();
        let tw = Arc::downgrade(self);
        let act = action.to_string();
        spawn::spawn_local_compat(async move {
            let result = match act.as_str() {
                "split-h" => split_horizontal(&deps, inst).await,
                "split-v" => split_vertical(&deps, inst).await,
                "tab-new" => add_tab(&deps, inst).await,
                "close" => close_focused_pane_or_tab(&deps, inst).await,
                "focus-next" => focus_next_pane(&deps, inst),
                "focus-prev" => focus_previous_pane(&deps, inst),
                "tab-next" => switch_tab_relative(&deps, inst, 1),
                "tab-prev" => switch_tab_relative(&deps, inst, -1),
                _ => Ok(()),
            };
            if let Err(e) = result {
                warn!(?e, %inst, action = %act, "terminal shortcut");
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

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

    fn set_fm_focus(&self, inst: Uuid, pane: u8) {
        *self.fm_focus.lock() = Some((inst, pane));
    }

    fn fm_instances_on_active_workspace(&self) -> Vec<Uuid> {
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

    fn find_active_fm(&self) -> Option<Uuid> {
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

    fn fm_prepare_instance(
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

    fn fm_wake_instance(self: &Arc<Self>, inst: Uuid) {
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

    async fn fm_refresh_ui(self: &Arc<Self>, inst: Uuid) {
        let _ = self.widget_manager.refresh_snapshot_cache(inst).await;
        self.schedule_rebuild();
    }

    fn widget_bounds_at_canvas_point(
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

    fn fm_pane_at_point(&self, inst: Uuid, content_x: f32, bounds: PixelBounds) -> u8 {
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

    fn fm_drop_target(&self) -> Option<(Uuid, u8)> {
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

    fn pointer_over_viewer_content(&self) -> bool {
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

    fn fm_open_paths_in_viewer(self: &Arc<Self>, paths: Vec<String>) {
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

    fn fm_dispatch_drag_transfer(
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

    fn fm_resolve_move_dest(
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

    fn fm_complete_drag_drop(self: &Arc<Self>, source_inst: Uuid, hinted_dest: Option<String>) {
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

    fn fm_active_pane(&self, inst: Uuid) -> u8 {
        let cache = self.widget_manager.snapshot_cache();
        cache
            .get(inst)
            .and_then(|s| match &s.payload {
                WidgetPayload::FileManager(fm) => Some(fm.active_pane),
                _ => None,
            })
            .unwrap_or(0)
    }

    fn queue_os_file_drop(self: &Arc<Self>, path: String) {
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

    fn on_os_files_dropped(self: &Arc<Self>, paths: Vec<String>) {
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

    /// Soft cap for multi-file "open in viewer" to avoid creating dozens of widgets
    /// and flooding the UI rebuild loop (hang / OOM).
    const VIEWER_MULTI_OPEN_CAP: usize = 8;

    fn fm_selected_paths(&self, inst: Uuid, pane: u8) -> Vec<String> {
        self.fm_selected_entries(inst, pane)
            .into_iter()
            .map(|(path, _)| path)
            .collect()
    }

    /// Selected FM entries as `(path, is_dir)` from the cached snapshot.
    fn fm_selected_entries(&self, inst: Uuid, pane: u8) -> Vec<(String, bool)> {
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

    /// Look up `is_dir` for a path in the FM snapshot (fallback `false`).
    fn fm_entry_is_dir(&self, inst: Uuid, pane: u8, path: &str) -> bool {
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

    /// Open `path` in a viewer on the active workspace.
    ///
    /// When `reuse_existing` is true, replace content in the first viewer widget
    /// (single-file open / recent files). When false, always create a new viewer
    /// so multi-select open and multi-file drag keep every file.
    ///
    /// When `schedule_rebuild` is false, the caller must rebuild once after a batch
    /// (avoids N full workspace rebuilds that freeze the UI).
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

    fn on_fm_sidebar_clicked(self: &Arc<Self>, fm_id: &SharedString, id: &SharedString) {
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

    fn on_fm_toggle_dual_pane(self: &Arc<Self>, fm_id: &SharedString) {
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

    fn on_fm_toggle_show_hidden(self: &Arc<Self>, fm_id: &SharedString) {
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

    fn on_fm_toggle_click_behavior(self: &Arc<Self>, fm_id: &SharedString) {
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

    fn on_fm_open_selected(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
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

    fn on_fm_entry_drag_start(self: &Arc<Self>, fm_id: &SharedString, pane: i32, _path: &SharedString) {
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

    fn on_fm_entry_drag_hover(self: &Arc<Self>, fm_id: &SharedString, pane: i32, folder: &SharedString) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        self.set_fm_drag_hover(inst, pane, folder.to_string());
    }

    fn set_fm_drag_hover(self: &Arc<Self>, inst: Uuid, pane: i32, folder: String) {
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

    fn clear_fm_drag_hover_to_pane(self: &Arc<Self>, inst: Uuid, pane: i32) {
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

    fn on_fm_entry_drag_scroll(
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

    fn fm_drag_hover_path_at_pointer(
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

    fn on_fm_entry_drag_drop(self: &Arc<Self>, fm_id: &SharedString, pane: i32, folder: &SharedString) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let folder_path = folder.to_string();
        self.fm_complete_drag_drop(inst, Some(folder_path));
    }

    fn on_fm_pane_drag_hover(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
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

    fn on_fm_drop_on_current_dir(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(source) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        self.set_fm_focus(source, p);
        self.fm_complete_drag_drop(source, None);
    }

    fn on_fm_entry_drag_cancel(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        self.clear_fm_drag(inst);
        self.schedule_rebuild();
    }

    fn on_fm_pane_clicked(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
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

    fn on_fm_tab_clicked(self: &Arc<Self>, fm_id: &SharedString, pane: i32, tab_id: &SharedString) {
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

    fn on_fm_tab_closed(self: &Arc<Self>, fm_id: &SharedString, pane: i32, tab_id: &SharedString) {
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

    fn on_fm_tab_new(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
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

    fn on_fm_new_folder(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
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

    fn on_fm_nav_back(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
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

    fn on_fm_nav_forward(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
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

    fn on_fm_nav_up(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
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

    fn on_fm_nav_home(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
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

    fn on_fm_breadcrumb_clicked(self: &Arc<Self>, fm_id: &SharedString, pane: i32, path: &SharedString) {
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

    fn on_fm_view_mode_cycle(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
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

    fn on_fm_sort_cycle(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
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

    fn on_fm_sort_column_clicked(self: &Arc<Self>, fm_id: &SharedString, pane: i32, column: i32) {
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

    fn on_fm_quick_filter_changed(self: &Arc<Self>, fm_id: &SharedString, pane: i32, q: &SharedString) {
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

    fn on_fm_entry_clicked(self: &Arc<Self>, fm_id: &SharedString, pane: i32, path: &SharedString, ctrl: bool) {
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

    fn on_fm_entry_shift_clicked(self: &Arc<Self>, fm_id: &SharedString, pane: i32, path: &SharedString) {
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

    fn on_fm_entry_double_clicked(self: &Arc<Self>, fm_id: &SharedString, pane: i32, path: &SharedString, is_dir: bool) {
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

    fn fm_dispatch_open(self: &Arc<Self>, inst: Uuid, pane: u8, path: String, is_dir: bool) {
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

    fn on_fm_entry_context(self: &Arc<Self>, fm_id: &SharedString, pane: i32, path: &SharedString, x: f32, y: f32) {
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

    fn on_fm_context_action(self: &Arc<Self>, fm_id: &SharedString, action_id: &SharedString, paths: &ModelRc<SharedString>) {
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

    fn on_fm_context_dismiss(self: &Arc<Self>, fm_id: &SharedString) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
        entry.context_menu = empty_context_menu();
        drop(over);
        self.schedule_rebuild();
    }

    fn on_fm_confirm_yes(self: &Arc<Self>, fm_id: &SharedString) {
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

    fn on_fm_confirm_no(self: &Arc<Self>, fm_id: &SharedString) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(|| self.ensure_fm_overlays(inst));
        entry.confirm_dialog = empty_confirm_dialog();
        drop(over);
        self.schedule_rebuild();
    }

    fn on_fm_rename_commit(self: &Arc<Self>, fm_id: &SharedString, old_path: &SharedString, new_name: &SharedString) {
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

    fn on_fm_rename_cancel(self: &Arc<Self>, fm_id: &SharedString) {
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

    fn on_fm_tag_commit(self: &Arc<Self>, fm_id: &SharedString, tag: &SharedString) {
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

    fn on_fm_tag_cancel(self: &Arc<Self>, fm_id: &SharedString) {
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

    fn on_fm_passphrase_commit(self: &Arc<Self>, fm_id: &SharedString, passphrase: &SharedString) {
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

    fn on_fm_passphrase_cancel(self: &Arc<Self>, fm_id: &SharedString) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        self.clear_fm_passphrase_overlay(inst);
        self.schedule_rebuild();
    }

    fn on_fm_managed_policy_close(self: &Arc<Self>, fm_id: &SharedString) {
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

    fn on_fm_passphrase_biometric(self: &Arc<Self>, fm_id: &SharedString) {
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

    fn clear_fm_passphrase_overlay(self: &Arc<Self>, inst: Uuid) {
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

    fn on_fm_select_all(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
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

    fn on_fm_deselect_all(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
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

    fn on_fm_delete_selected(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
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

    fn on_fm_copy_selected(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
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

    fn on_fm_paste_clipboard(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        self.spawn_fm_action(inst, "fs.paste", Vec::new());
    }

    fn on_fm_rename_selected(self: &Arc<Self>, fm_id: &SharedString, pane: i32) {
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

    fn spawn_fm_action(self: &Arc<Self>, inst: Uuid, action_id: &str, paths: Vec<String>) {
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
        type_id,
        "terminal"
            | "weather"
            | "moon"
            | "system"
            | "rss"
            | "recent-files"
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
            q.is_empty()
                || d.label.as_str().to_lowercase().contains(&q)
                || d.description.as_str().to_lowercase().contains(&q)
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


fn empty_close_confirm_dialog() -> WidgetCloseConfirmDialog {
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










/// Slint `KeyboardModifiers` → [`orchid_core::Modifiers`].
fn slint_kb_modifiers(ctrl: bool, shift: bool, alt: bool) -> orchid_core::Modifiers {
    use orchid_core::Modifiers;
    let mut mods = Modifiers::empty();
    if ctrl {
        mods |= Modifiers::CTRL;
    }
    if shift {
        mods |= Modifiers::SHIFT;
    }
    if alt {
        mods |= Modifiers::ALT;
    }
    mods
}

/// Peel Slint embedded modifier key identities (U+0010..=U+0013) from `text`.
fn peel_slint_modifier_prefix(
    text: &str,
    mut mods: orchid_core::Modifiers,
) -> (&str, orchid_core::Modifiers) {
    use orchid_core::Modifiers;
    let mut t = text;
    loop {
        let Some(c) = t.chars().next() else {
            break;
        };
        let embedded = match c as u32 {
            0x10 => Some(Modifiers::SHIFT),
            0x11 => Some(Modifiers::CTRL),
            0x12 | 0x13 => Some(Modifiers::ALT),
            _ => None,
        };
        if let Some(m) = embedded {
            mods |= m;
            t = &t[c.len_utf8()..];
        } else {
            break;
        }
    }
    (t, mods)
}

/// Map a Slint `KeyEvent.text` code point to [`orchid_core::Key`] (see `i-slint-common` `key_codes`).
fn slint_codepoint_to_key(cp: char) -> Option<orchid_core::Key> {
    use orchid_core::Key;
    match cp as u32 {
        0x08 => Some(Key::Backspace),
        0x09 => Some(Key::Tab),
        0x0A | 0x0D => Some(Key::Enter),
        0x1B => Some(Key::Escape),
        0x7F => Some(Key::Delete),
        0x20 => Some(Key::Space),
        0xF700 => Some(Key::ArrowUp),
        0xF701 => Some(Key::ArrowDown),
        0xF702 => Some(Key::ArrowLeft),
        0xF703 => Some(Key::ArrowRight),
        0xF704..=0xF71B => Some(Key::F((cp as u32 - 0xF704 + 1) as u8)),
        0xF727 => Some(Key::Insert),
        0xF729 => Some(Key::Home),
        0xF72B => Some(Key::End),
        0xF72C => Some(Key::PageUp),
        0xF72D => Some(Key::PageDown),
        _ => match cp {
            ',' => Some(Key::Comma),
            '.' => Some(Key::Period),
            '/' => Some(Key::Slash),
            '`' => Some(Key::Backtick),
            '-' => Some(Key::Minus),
            '=' => Some(Key::Equals),
            '[' => Some(Key::LeftBracket),
            ']' => Some(Key::RightBracket),
            ';' => Some(Key::Semicolon),
            '\'' => Some(Key::Quote),
            '\\' => Some(Key::Backslash),
            c if c.is_ascii_alphabetic() => Some(Key::Char(c.to_ascii_lowercase())),
            c if c.is_ascii_graphic() => Some(Key::Char(c)),
            _ => None,
        },
    }
}

/// Maps Slint `KeyEvent` (`text` + modifiers) to PTY bytes via [`orchid_terminal::InputEncoder`].
/// Falls back to [`encode_slint_key_text`] for multi-code-unit printable payloads.
fn encode_slint_key_event(
    text: &str,
    ctrl: bool,
    shift: bool,
    alt: bool,
    encoder: &orchid_terminal::InputEncoder,
) -> Vec<u8> {
    use orchid_core::{Key, Modifiers};

    let mods = slint_kb_modifiers(ctrl, shift, alt);
    let (peeled, mods) = peel_slint_modifier_prefix(text, mods);

    if peeled.is_empty() {
        trace!(
            target: "orchid_ui::terminal_input",
            "empty Slint key text after modifier peel (modifier-only or platform gap)"
        );
        return Vec::new();
    }

    // Slint Backtab identity (U+0019).
    if peeled == "\u{19}" {
        return encoder.encode_key(Key::Tab, mods | Modifiers::SHIFT);
    }

    // Pre-formed CSI / SS3 sequences from older Slint paths or platform quirks.
    if is_leading_escape_to_preserve(peeled) {
        return peeled.as_bytes().to_vec();
    }

    let trimmed = trim_slint_key_artifacts(peeled);
    if trimmed.is_empty() {
        if peeled.chars().count() == 1 {
            if let Some(key) = slint_codepoint_to_key(peeled.chars().next().expect("one char")) {
                return encoder.encode_key(key, mods);
            }
        }
        trace!(
            target: "orchid_ui::terminal_input",
            "key text was only Slint key-identity (PUA or modifier id); not forwarding to PTY"
        );
        return Vec::new();
    }

    if trimmed.chars().count() == 1 {
        let c = trimmed.chars().next().expect("one char");
        let cp = c as u32;
        if (0x10..=0x19).contains(&cp) {
            trace!(
                target: "orchid_ui::terminal_input",
                "Slint key id U+{:04X} only; not forwarding to PTY",
                cp
            );
            return Vec::new();
        }
        if let Some(key) = slint_codepoint_to_key(c) {
            return encoder.encode_key(key, mods);
        }
    }

    encode_slint_key_text(peeled)
}

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

/// Maps Slint `KeyEvent.text` payloads to bytes for the PTY (printable / legacy paths).
fn encode_slint_key_text(text: &str) -> Vec<u8> {
    if text.is_empty() {
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
    use orchid_core::{Key, Modifiers};
    use orchid_terminal::InputEncoder;

    use super::{encode_slint_key_event, encode_slint_key_text};

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

    #[test]
    fn event_encoder_maps_lone_pua_arrow() {
        let encoder = InputEncoder::new();
        assert_eq!(
            encode_slint_key_event("\u{F700}", false, false, false, &encoder),
            vec![0x1B, b'[', b'A']
        );
        assert_eq!(
            encode_slint_key_event("\u{F703}", false, false, false, &encoder),
            vec![0x1B, b'[', b'C']
        );
    }

    #[test]
    fn event_encoder_ctrl_c() {
        let encoder = InputEncoder::new();
        assert_eq!(
            encode_slint_key_event("c", true, false, false, &encoder),
            vec![0x03]
        );
        assert_eq!(
            encode_slint_key_event("\u{11}c", false, false, false, &encoder),
            vec![0x03]
        );
    }

    #[test]
    fn event_encoder_f4_and_application_cursor() {
        let encoder = InputEncoder::new();
        assert_eq!(
            encode_slint_key_event("\u{F707}", false, false, false, &encoder),
            vec![0x1B, b'O', b'S']
        );
        let mut app = InputEncoder::new();
        app.application_cursor = true;
        assert_eq!(
            encode_slint_key_event("\u{F700}", false, false, false, &app),
            vec![0x1B, b'O', b'A']
        );
    }

    #[test]
    fn event_encoder_shift_tab_and_backtab() {
        let encoder = InputEncoder::new();
        assert_eq!(
            encode_slint_key_event("\t", false, true, false, &encoder),
            vec![0x1B, b'[', b'Z']
        );
        assert_eq!(
            encode_slint_key_event("\u{19}", false, false, false, &encoder),
            vec![0x1B, b'[', b'Z']
        );
    }

    #[test]
    fn event_encoder_preserves_csi_pass_through() {
        let encoder = InputEncoder::new();
        assert_eq!(
            encode_slint_key_event("\u{1b}[A", false, false, false, &encoder),
            b"\x1b[A"
        );
    }

    #[test]
    fn event_encoder_printable_matches_text_path() {
        let encoder = InputEncoder::new();
        assert_eq!(
            encode_slint_key_event("hello", false, false, false, &encoder),
            encode_slint_key_text("hello")
        );
        assert_eq!(
            encode_slint_key_event("a", false, false, false, &encoder),
            b"a"
        );
    }

    #[test]
    fn event_encoder_named_keys_via_input_encoder() {
        let encoder = InputEncoder::new();
        assert_eq!(
            encoder.encode_key(Key::Enter, Modifiers::empty()),
            encode_slint_key_event("\n", false, false, false, &encoder)
        );
        assert_eq!(
            encoder.encode_key(Key::Backspace, Modifiers::empty()),
            encode_slint_key_event("\u{8}", false, false, false, &encoder)
        );
    }
}
