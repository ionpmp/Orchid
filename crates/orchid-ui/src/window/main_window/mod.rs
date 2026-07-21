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
use std::time::Instant;

use parking_lot::Mutex;
use slint::winit_030::WinitWindowAccessor;
use slint::ComponentHandle;
use slint::Image;
use slint::Model;
use slint::ModelRc;
use slint::SharedString;
use slint::VecModel;
use tracing::{debug, trace, warn};
use uuid::Uuid;

use orchid_core::{
    ActionContext, ActionDispatcher, CommandPalette, CommandRegistry, ConfigUpdated, Event,
    EventBus, EventFilter, GestureConfig, GestureRecognizer, HandlerPriority, HistoryRecorder,
    InputMapper, ParsedCommand, ScreenBounds, SubscriptionHandle,
};
use orchid_i18n::{LocaleId, LocaleManager};
use orchid_storage::{OrchidConfig, StateStore, WidgetSize};
use orchid_terminal::FontMetrics;
use orchid_terminal::SessionManager;
use orchid_widgets::layout::PixelBounds;
use orchid_widgets::layout::ViewportSize;
use orchid_widgets::SharedInstance;
use orchid_widgets::WidgetPayload;
use orchid_widgets::{
    visible_instance_ids, CreateWidgetRequest, GroupManager, LayoutEngine, PlacedWidget,
    RecentFilesStore, WidgetManager, WorkspaceManager,
};
use parking_lot::RwLock;

use super::models::{
    blank_terminal, build_clock_model, build_file_manager_model, build_jyotish_model, build_media_model, build_moon_model,
    build_calculator_model, build_notes_model, build_password_model, build_processes_model, build_recent_files_model,
    build_rss_model, patch_processes_model,
    build_search_model, build_system_model, build_terminal_divider_models, build_terminal_model,
    build_terminal_tab_models, build_viewer_model, build_weather_model,
    default_terminal_divider_models, default_terminal_pane_models, default_terminal_tab_models,
    empty_confirm_dialog, empty_context_menu, empty_file_manager_model, empty_managed_policy_state,
    empty_clock_model, empty_jyotish_model, empty_media_model, empty_moon_model, empty_passphrase_state, empty_password_model,
    empty_calculator_model, empty_notes_model, empty_processes_confirm, empty_processes_model, empty_recent_files_model, empty_rename_state, empty_rss_model,
    empty_search_model, empty_system_model, empty_tag_state, empty_viewer_model,
    empty_weather_model,
    locale_display_name, theme_display_name, widget_has_settings, FileManagerOverlays,
    PasswordAddDialogOverlay,
};
use super::spawn;
use crate::error::{Result, UiError};
use crate::slint_generated::{
    AppState, ClockModel, DockWidgetType, FileManagerModel, GroupTabModel, MainWindow, MediaModel,
    CalculatorModel, JyotishModel, MoonModel, NotesModel, NotificationItem, PasswordModel, ProcessesConfirmDialog, ProcessesModel,
    RecentFilesModel, RssModel,
    SearchCandidateEntry, SearchModel, SettingsFieldRow, SettingsSectionEntry, Strings, SystemModel,
    TerminalCellModel, Theme, ViewerModel, WeatherModel, WidgetCatalog, WidgetCloseConfirmDialog,
    WidgetFrameModel, WidgetSettingsDialog, WorkspaceModel, WorkspaceSummary,
};
use crate::terminal_font_metrics;
use crate::terminal_raster;
use crate::theme::ThemeManager;
use crate::widgets::terminal::TerminalWidgetDeps;

mod canvas;
mod fm;
mod input;
mod media_search;
mod password;
mod calculator;
mod jyotish;
mod notes;
mod processes;
mod shell_ui;
mod terminal;
mod weather;
mod clock;
mod widget_settings;
mod wire;

use canvas::ResizeInteraction;

/// Canvas fills the window; the workspace orb overlays the bottom corner.
const WORKSPACE_CHROME_H: f32 = 0.0;

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
    /// Debounces concurrent [`Self::sync_widget_visibility`] spawns from [`Self::schedule_rebuild`].
    visibility_sync_pending: Arc<AtomicBool>,
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
    /// Floating (undocked) viewer frames, viewport-relative.
    workspace_floating_widgets: ModelRc<WidgetFrameModel>,
    workspace_dock_types: ModelRc<DockWidgetType>,
    /// Paint order for floating viewers (last = top).
    floating_z_stack: Arc<Mutex<Vec<Uuid>>>,
    /// Bumped when requesting a programmatic canvas scroll.
    canvas_scroll_gen: AtomicU32,
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
    /// Per processes widget: context menu visibility and position.
    processes_context: Arc<RwLock<HashMap<Uuid, (bool, f32, f32)>>>,
    /// Per processes widget: End task / End tree / Sign out confirm dialog.
    processes_confirm: Arc<RwLock<HashMap<Uuid, ProcessesConfirmDialog>>>,
    /// UI-only overlays for file-manager widgets (context menu, confirm dialog, rename).
    fm_overlays: Arc<RwLock<HashMap<Uuid, FileManagerOverlays>>>,
    /// Unsaved text close-confirm overlays for viewer widgets.
    close_confirm_overlays: Arc<RwLock<HashMap<Uuid, WidgetCloseConfirmDialog>>>,
    /// Per-widget settings dialog overlays.
    settings_dialog_overlays: Arc<RwLock<HashMap<Uuid, WidgetSettingsDialog>>>,
    /// Last text-viewer instance that received an edit (for Ctrl+S when focus left the input).
    last_text_edit_instance: Arc<Mutex<Option<Uuid>>>,
    /// Last interacted file-manager instance and pane (for drop targeting).
    fm_focus: Arc<Mutex<Option<(Uuid, u8)>>>,
    /// Recent entry click for synthesizing double-open across workspace rebuilds.
    fm_last_click: Arc<Mutex<Option<(Uuid, u8, String, Instant)>>>,
    /// Debounce duplicate open from Slint double-click + Rust double-click.
    fm_last_open: Arc<Mutex<Option<(Uuid, String, Instant)>>>,
    /// Monotonic sequence per (instance, pane) for quick-filter debounce.
    fm_filter_seq: Arc<Mutex<HashMap<(Uuid, u8), u64>>>,
    /// Per-pane scroll/viewport size for entry-list virtualization.
    fm_viewport: Arc<Mutex<HashMap<(Uuid, u8), crate::window::models::FmViewport>>>,
    /// Last virtualized window start index; skip rebuild when unchanged.
    fm_viewport_window: Arc<Mutex<HashMap<(Uuid, u8), usize>>>,
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
    workspace_orb_open: bool,
    notification_center_visible: bool,
}

impl Default for NavigationUiState {
    fn default() -> Self {
        Self {
            workspace_orb_open: false,
            notification_center_visible: false,
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
        let workspace_floating_widgets: ModelRc<WidgetFrameModel> =
            ModelRc::new(VecModel::<WidgetFrameModel>::default());
        let workspace_dock_types: ModelRc<DockWidgetType> =
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
                    .take(shell_ui::NOTIFICATION_LIST_CAP)
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
            visibility_sync_pending: Arc::new(AtomicBool::new(false)),
            config_reload_pending,
            last_window_scale: parking_lot::Mutex::new(0.0),
            last_terminal_viewport_pty: Arc::new(Mutex::new(HashMap::new())),
            workspace_workspaces,
            workspace_widgets,
            workspace_floating_widgets,
            workspace_dock_types,
            floating_z_stack: Arc::new(Mutex::new(Vec::new())),
            canvas_scroll_gen: AtomicU32::new(0),
            search_selection: Arc::new(RwLock::new(HashMap::new())),
            search_autofocus_pending: Arc::new(Mutex::new(None)),
            password_toasts: Arc::new(RwLock::new(HashMap::new())),
            password_autofocus_pending: Arc::new(RwLock::new(HashMap::new())),
            password_add_dialogs: Arc::new(RwLock::new(HashMap::new())),
            processes_context: Arc::new(RwLock::new(HashMap::new())),
            processes_confirm: Arc::new(RwLock::new(HashMap::new())),
            fm_overlays: Arc::new(RwLock::new(HashMap::new())),
            close_confirm_overlays: Arc::new(RwLock::new(HashMap::new())),
            settings_dialog_overlays: Arc::new(RwLock::new(HashMap::new())),
            last_text_edit_instance: Arc::new(Mutex::new(None)),
            fm_focus: Arc::new(Mutex::new(None)),
            fm_last_click: Arc::new(Mutex::new(None)),
            fm_last_open: Arc::new(Mutex::new(None)),
            fm_filter_seq: Arc::new(Mutex::new(HashMap::new())),
            fm_viewport: Arc::new(Mutex::new(HashMap::new())),
            fm_viewport_window: Arc::new(Mutex::new(HashMap::new())),
            last_canvas_pointer: Arc::new(Mutex::new(None)),
            canvas_scroll: Arc::new(Mutex::new((0.0, 0.0))),
            keyboard_modifiers: Arc::new(Mutex::new(
                slint::winit_030::winit::keyboard::ModifiersState::empty(),
            )),
            leader_pending_until: Arc::new(Mutex::new(None)),
            os_drop_batch: Arc::new(Mutex::new(OsDropBatch::default())),
            catalog: Arc::new(RwLock::new(CatalogUiState::default())),
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
        g.set_dock_add_label(mgr.tr("dock-add-label").into());
        g.set_catalog_search_placeholder(mgr.tr("catalog-search-placeholder").into());
        g.set_catalog_no_results(mgr.tr("catalog-no-results").into());
        g.set_catalog_sort_default(mgr.tr("catalog-sort-default").into());
        g.set_catalog_sort_name_asc(mgr.tr("catalog-sort-name-asc").into());
        g.set_catalog_sort_name_desc(mgr.tr("catalog-sort-name-desc").into());
        g.set_dock_widget_terminal(mgr.tr("dock-widget-terminal").into());
        g.set_dock_widget_weather(mgr.tr("dock-widget-weather").into());
        g.set_dock_widget_moon(mgr.tr("dock-widget-moon").into());
        g.set_dock_widget_jyotish(mgr.tr("dock-widget-jyotish").into());
        g.set_dock_widget_clock(mgr.tr("dock-widget-clock").into());
        g.set_dock_widget_system(mgr.tr("dock-widget-system").into());
        g.set_dock_widget_processes(mgr.tr("dock-widget-processes").into());
        g.set_dock_widget_calculator(mgr.tr("dock-widget-calculator").into());
        g.set_dock_widget_notes(mgr.tr("dock-widget-notes").into());
        g.set_dock_widget_rss(mgr.tr("dock-widget-rss").into());
        g.set_dock_widget_recent_files(mgr.tr("dock-widget-recent-files").into());
        g.set_dock_widget_search(mgr.tr("dock-widget-search").into());
        g.set_dock_widget_media(mgr.tr("dock-widget-media").into());
        g.set_dock_widget_password(mgr.tr("dock-widget-password").into());
        g.set_dock_widget_viewer(mgr.tr("dock-widget-viewer").into());
        g.set_dock_widget_fm(mgr.tr("dock-widget-fm").into());
        g.set_widget_terminal_desc(mgr.tr("widget-terminal-desc").into());
        g.set_widget_weather_desc(mgr.tr("widget-weather-desc").into());
        g.set_widget_moon_desc(mgr.tr("widget-moon-desc").into());
        g.set_widget_jyotish_desc(mgr.tr("widget-jyotish-desc").into());
        g.set_widget_clock_desc(mgr.tr("widget-clock-desc").into());
        g.set_widget_system_desc(mgr.tr("widget-system-desc").into());
        g.set_widget_processes_desc(mgr.tr("widget-processes-desc").into());
        g.set_widget_calculator_desc(mgr.tr("widget-calculator-desc").into());
        g.set_widget_notes_desc(mgr.tr("widget-notes-desc").into());
        g.set_widget_rss_desc(mgr.tr("widget-rss-desc").into());
        g.set_widget_recent_files_desc(mgr.tr("widget-recent-files-desc").into());
        g.set_widget_search_desc(mgr.tr("widget-search-desc").into());
        g.set_widget_media_desc(mgr.tr("widget-media-desc").into());
        g.set_widget_password_desc(mgr.tr("widget-password-desc").into());
        g.set_widget_viewer_desc(mgr.tr("widget-viewer-desc").into());
        g.set_widget_fm_desc(mgr.tr("widget-fm-desc").into());
        g.set_widget_close_tooltip(mgr.tr("widget-close-tooltip").into());
        g.set_widget_settings_tooltip(mgr.tr("widget-settings-tooltip").into());
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
        g.set_current_density(self.locale.tr(shell_ui::density_i18n_key(density)).into());
        // Slint 1.16 has no Window `layout-direction`; drive RTL via `is-rtl`.
        let is_rtl = language.as_str().to_ascii_lowercase().starts_with("ar");
        g.set_is_rtl(is_rtl);
        let cfg = self.config.read();
        let swap_edges = matches!(cfg.input.primary_hand, orchid_storage::Hand::Left)
            || cfg.input.mirror_edge_swipes;
        // Panels must dock on the same edges as the swipe targets that open them.
        g.set_edge_panels_mirrored(orchid_core::input::edge_panels_mirrored(is_rtl, swap_edges));
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
        let retention_changed =
            retention_days != self.last_history_retention_days.load(Ordering::Acquire);
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
        let cutoff = chrono::Utc::now() - chrono::Duration::days(i64::from(days));
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
    /// Also schedules a visibility ↔ lifecycle sync (debounced).
    fn schedule_rebuild(self: &Arc<Self>) {
        self.rebuild_pending.store(true, Ordering::Release);
        self.schedule_visibility_sync();
        trace!(target: "orchid_ui::workspace", "rebuild requested");
    }

    /// Align widget lifecycle with canvas visibility (active workspace + group tab).
    fn schedule_visibility_sync(self: &Arc<Self>) {
        if self.visibility_sync_pending.swap(true, Ordering::AcqRel) {
            return;
        }
        let t = Arc::downgrade(self);
        spawn::spawn_local(async move {
            if let Some(c) = t.upgrade() {
                c.visibility_sync_pending.store(false, Ordering::Release);
                c.sync_widget_visibility().await;
            }
        });
    }

    async fn sync_widget_visibility(&self) {
        let Ok(ws) = self.workspace_manager.active() else {
            self.widget_manager.apply_visibility(&[]).await;
            return;
        };
        let mut ids = visible_instance_ids(
            &self.widget_manager,
            &self.workspace_manager,
            &self.group_manager,
        );
        let (sx, sy) = *self.canvas_scroll.lock();
        let (vw, vh) = *self.canvas_size.lock();
        // Sleep widgets scrolled fully outside the canvas viewport (e.g. Processes
        // stops its expensive refresh while off-screen).
        ids.retain(|id| {
            self.is_floating_viewer(*id)
                || self.widget_intersects_canvas_viewport(ws.id, *id, sx, sy, vw, vh)
        });
        self.widget_manager.apply_visibility(&ids).await;
    }

    /// `true` when the widget's layout bounds intersect the scrolled canvas viewport.
    fn widget_intersects_canvas_viewport(
        &self,
        workspace_id: Uuid,
        instance_id: Uuid,
        scroll_x: f32,
        scroll_y: f32,
        viewport_w: f32,
        viewport_h: f32,
    ) -> bool {
        let all = self.widget_manager.instances_for_workspace(workspace_id);
        let docked = Self::docked_instances(&all);
        let view = ViewportSize {
            width_px: viewport_w.max(1.0),
            height_px: viewport_h.max(1.0),
        };
        let snap = self.layout_engine.snapshot(workspace_id, &docked, view);
        let Some(pl) = snap.cells.iter().find(|c| c.instance_id == instance_id) else {
            // Unknown placement — keep active rather than sleep incorrectly.
            return true;
        };
        let mut bounds = pl.bounds;
        if let Some(group) = self
            .group_manager
            .find_for_instance(instance_id)
            .filter(|g| g.members.len() >= 2)
        {
            bounds = self
                .layout_engine
                .pixel_bounds_for(group.position, group.size, view);
        }
        let vx0 = scroll_x;
        let vy0 = scroll_y;
        let vx1 = scroll_x + viewport_w;
        let vy1 = scroll_y + viewport_h;
        bounds.x < vx1
            && bounds.x + bounds.width > vx0
            && bounds.y < vy1
            && bounds.y + bounds.height > vy0
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
        WORKSPACE_CHROME_H
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

    fn on_canvas_scrolled(self: &Arc<Self>, viewport_x: f32, viewport_y: f32) {
        *self.canvas_scroll.lock() = (viewport_x, viewport_y);
        self.schedule_visibility_sync();
    }

    fn on_catalog_dismiss(self: &Arc<Self>) {
        if !self.catalog.read().visible {
            return;
        }
        {
            let mut cat = self.catalog.write();
            cat.visible = false;
            cat.search_query.clear();
        }
        self.sync_widget_catalog_global();
    }

    fn on_catalog_search_changed(self: &Arc<Self>, query: &SharedString) {
        self.catalog.write().search_query = query.to_string();
        self.sync_widget_catalog_items_only();
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
                AddWidgetPlacement::CanvasPoint {
                    content_x,
                    content_y,
                } => {
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
        let empty = items.is_empty();
        let visible_ids: std::collections::HashSet<&str> =
            items.iter().map(|d| d.type_id.as_str()).collect();
        tracing::info!(
            count = items.len(),
            query = %cat.search_query,
            visible = cat.visible,
            "widget catalog sync"
        );
        let g = self.window.global::<WidgetCatalog>();
        apply_catalog_row_visibility(&g, &visible_ids);
        g.set_is_empty(empty);
        g.set_search_query(cat.search_query.clone().into());
        g.set_screen_x(cat.screen_x);
        g.set_screen_y(cat.screen_y);
        g.set_visible(cat.visible);
    }

    /// Update filtered card visibility while typing without resetting the search field.
    fn sync_widget_catalog_items_only(self: &Arc<Self>) {
        let cat = self.catalog.read().clone();
        let items = filter_catalog_items(&self.locale, &cat.search_query);
        let empty = items.is_empty();
        let visible_ids: std::collections::HashSet<&str> =
            items.iter().map(|d| d.type_id.as_str()).collect();
        let g = self.window.global::<WidgetCatalog>();
        apply_catalog_row_visibility(&g, &visible_ids);
        g.set_is_empty(empty);
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
            self.toggle_workspace_orb();
            return;
        }
        if cmd_id == "notification.show_center" {
            self.toggle_notification_center();
            return;
        }
        if cmd_id == "dock.show" {
            self.show_widget_catalog_center();
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
        let ctx = ActionContext::new(self.bus.clone(), self.storage.clone(), self.config.clone());
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
            // Floating viewers live in a separate model; patch content in place
            // when possible, otherwise rebuild the floating overlay.
            if self.is_floating_viewer(*id) {
                if !self.try_patch_viewer_row(
                    &self.workspace_floating_widgets,
                    *id,
                    None,
                    None,
                    true,
                ) {
                    need_floating_sync = true;
                }
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
            // Viewer fast path: update geometry + viewer model only; keep the
            // rest of the frame (empty sibling models) intact.
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
            // Processes: avoid rebuilding every sibling model + all Slint rows
            // from scratch on each sample.
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

    /// Patch an existing processes frame row without rebuilding sibling models.
    fn try_patch_processes_row(
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
        let orchid_widgets::WidgetPayload::Processes(p) = &ws.payload else {
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
            // Keep the same ModelRc list handles — list cells notify Slint directly.
            // Avoid set_row_data on the frame unless scalars/geometry actually changed;
            // rewriting the whole ProcessesModel forced a heavy view pass every sample.
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
                if row.x != b.x
                    || row.y != b.y
                    || row.width != b.width
                    || row.height != b.height
                {
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
            let title: slint::SharedString = ws.title.clone().into();
            if row.title != title {
                row.title = title;
                need_frame = true;
            }
            let (group_id, group_tabs) = self.build_group_tab_models(id);
            // Group tabs are ModelRcs; always refresh handles when writing the frame.
            if need_frame {
                row.group_id = group_id;
                row.group_tabs = group_tabs;
                v.set_row_data(r, row);
            }
            return true;
        }
        false
    }

    /// Patch an existing viewer frame row without rebuilding empty sibling models.
    ///
    /// Returns `false` when the row is missing or the cached payload is not a viewer.
    fn try_patch_viewer_row(
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
        let orchid_widgets::WidgetPayload::Viewer(vp) = &ws.payload else {
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
            jyotish_model,
            clock_model,
            system_model,
            processes_model,
            calculator_model,
            notes_model,
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
                            t, f, glyph_fb, size_md, cw, ch, scale, ccol,
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
                        empty_jyotish_model(&self.locale),
                        empty_clock_model(&self.locale),
                        empty_system_model(&self.locale),

                        empty_processes_model(&self.locale),
                        empty_calculator_model(&self.locale),
                        empty_notes_model(&self.locale),
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
                    build_jyotish_model(j, &self.locale),
                    empty_clock_model(&self.locale),
                    empty_system_model(&self.locale),
                    empty_processes_model(&self.locale),
                    empty_calculator_model(&self.locale),
                    empty_notes_model(&self.locale),
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
                        empty_jyotish_model(&self.locale),
                        empty_clock_model(&self.locale),
                        empty_system_model(&self.locale),

                        empty_processes_model(&self.locale),
                        empty_calculator_model(&self.locale),
                        empty_notes_model(&self.locale),
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
            .unwrap_or_else(empty_close_confirm_dialog);
        let settings_dialog = self
            .settings_dialog_overlays
            .read()
            .get(&pl.instance_id)
            .cloned()
            .unwrap_or_else(empty_widget_settings_dialog);
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
        sync_vec_model(&self.workspace_dock_types, dock_types_vec(&self.locale));
        app_g.set_workspace(WorkspaceModel {
            workspaces: self.workspace_workspaces.clone(),
            active_workspace_id: w.id.to_string().into(),
            widgets: self.workspace_widgets.clone(),
            floating_widgets: self.workspace_floating_widgets.clone(),
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
        Ok(())
    }

    /// Show the window and run the Slint event loop.
    pub fn run(self: Arc<Self>) -> Result<()> {
        tracing::info!("Opening main window");
        let tw = Arc::downgrade(&self);
        self.window
            .window()
            .on_winit_window_event(move |_winit_window, event| {
                use slint::winit_030::{winit::event::WindowEvent, EventResult};
                match event {
                    WindowEvent::CursorMoved { position, .. } => {
                        if let Some(c) = tw.upgrade() {
                            let win = c.window.window();
                            let scale = win.scale_factor();
                            let logical: slint::winit_030::winit::dpi::LogicalPosition<f64> =
                                position.to_logical(f64::from(scale));
                            let canvas_y = logical.y;
                            if canvas_y >= 0.0 {
                                let (scroll_x, scroll_y) = *c.canvas_scroll.lock();
                                *c.last_canvas_pointer.lock() =
                                    Some((logical.x as f32 + scroll_x, canvas_y as f32 + scroll_y));
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
                                } else if c.navigation.read().workspace_orb_open
                                    && matches!(event.logical_key, Key::Named(NamedKey::Escape))
                                {
                                    c.on_workspace_orb_dismiss();
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
                                    if let Some(cmd_id) =
                                        c.try_leader_chord(mods, &event.logical_key)
                                    {
                                        c.dispatch_registry_shortcut(cmd_id);
                                    } else if c.try_activate_leader(mods, &event.logical_key) {
                                        // leader armed; consume without dispatching
                                    } else {
                                        let palette_sc = c.command_palette_shortcut();
                                        if input::winit_modifiers_match(palette_sc.modifiers, mods)
                                            && input::winit_key_matches(
                                                palette_sc.key,
                                                &event.logical_key,
                                            )
                                        {
                                            c.toggle_command_palette();
                                        } else if c.try_viewer_text_ctrl_s(mods, &event.logical_key)
                                        {
                                            // Saved focused/last text editor; consume.
                                        } else if let Some(shortcut) =
                                            input::winit_to_shortcut(mods, &event.logical_key)
                                        {
                                            if let Some(cmd_id) = input::resolve_registry_shortcut(
                                                &c.command_registry,
                                                &shortcut,
                                            ) {
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
                            if let Some(ev) =
                                input::winit_touch_to_orchid(&touch, c.window.window())
                            {
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

    /// Instances that participate in the canvas grid (excludes floating viewers).
    fn docked_instances(instances: &[SharedInstance]) -> Vec<SharedInstance> {
        instances
            .iter()
            .filter(|i| {
                if i.type_id == orchid_widgets::builtin::viewer::TYPE_ID {
                    orchid_widgets::builtin::viewer::floating_bounds(i.id).is_none()
                } else {
                    true
                }
            })
            .cloned()
            .collect()
    }

    fn is_floating_viewer(&self, id: Uuid) -> bool {
        orchid_widgets::builtin::viewer::floating_bounds(id).is_some()
    }

    fn sync_floating_z_stack(&self, instances: &[SharedInstance]) {
        let live: HashSet<Uuid> = instances
            .iter()
            .filter(|i| i.type_id == orchid_widgets::builtin::viewer::TYPE_ID)
            .filter(|i| orchid_widgets::builtin::viewer::floating_bounds(i.id).is_some())
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

    fn raise_floating(&self, id: Uuid) {
        let mut stack = self.floating_z_stack.lock();
        stack.retain(|x| *x != id);
        stack.push(id);
    }

    /// Raise a floating viewer and refresh the overlay model so paint / hit-test
    /// order matches the stack (Slint `for` order; later = on top).
    pub(crate) fn bring_floating_to_front(self: &Arc<Self>, id: Uuid) {
        if !self.is_floating_viewer(id) {
            return;
        }
        let already_top = self.floating_z_stack.lock().last().copied() == Some(id);
        self.raise_floating(id);
        if already_top {
            return;
        }
        self.sync_floating_widgets_model();
    }

    /// Rebuild only the floating overlay rows (no full workspace rebuild).
    fn sync_floating_widgets_model(&self) {
        let Ok(w) = self.workspace_manager.active() else {
            return;
        };
        let all_instances = self.widget_manager.instances_for_workspace(w.id);
        self.sync_floating_z_stack(&all_instances);
        let off = self.drag_offset.lock().clone();
        let ro = self.resize_override.lock().clone();
        let floating_frames = self.build_floating_frames(&all_instances, &off, &ro);
        sync_vec_model(&self.workspace_floating_widgets, floating_frames);
        // Keep AppState.workspace.floating-widgets pointing at the same ModelRc;
        // sync_vec_model mutates it in place.
    }

    fn default_floating_bounds(&self) -> PixelBounds {
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

    fn build_floating_frames(
        &self,
        _instances: &[SharedInstance],
        off: &HashMap<Uuid, (f32, f32)>,
        ro: &HashMap<Uuid, PixelBounds>,
    ) -> Vec<WidgetFrameModel> {
        let stack = self.floating_z_stack.lock().clone();
        let mut frames = Vec::new();
        for (zi, id) in stack.iter().enumerate() {
            let Some(bounds0) = orchid_widgets::builtin::viewer::floating_bounds(*id) else {
                continue;
            };
            let Ok(iref) = self.widget_manager.get_instance(*id) else {
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
            frames.push(frame);
        }
        frames
    }

    fn request_canvas_scroll_to(&self, bounds: PixelBounds) {
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
        self.canvas_scroll_gen.fetch_add(1, Ordering::Relaxed);
    }

    fn focus_viewer(self: &Arc<Self>, id: Uuid) {
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

    fn focus_viewer_ui(&self, id: Uuid) {
        if self.is_floating_viewer(id) {
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

    async fn open_in_viewer_for_controller(
        ctrl: std::sync::Weak<MainWindowController>,
        path: orchid_fs::FsPath,
        schedule_rebuild: bool,
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

        let bounds = c.default_floating_bounds();
        for _ in 0..50 {
            if c.widget_manager.get_instance(id).is_ok()
                && orchid_widgets::builtin::viewer::set_floating_bounds(id, Some(bounds)).is_ok()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        c.raise_floating(id);

        orchid_widgets::builtin::viewer::open_path(id, path.clone())
            .await
            .map_err(|e| UiError::Slint(format!("viewer open: {e}")))?;
        c.recent_files.touch(&path, Some(&c.bus));
        if schedule_rebuild {
            if let Some(c2) = ctrl.upgrade() {
                c2.schedule_rebuild();
            }
        }
        Ok((id, true))
    }
}

/// Replace all rows in a `VecModel` wrapped by `ModelRc` without creating a new `ModelRc`, so
/// `for` loops in Slint keep the same item instances and retain focus/scroll state.
pub(crate) fn sync_vec_model<T: Clone + 'static>(model: &ModelRc<T>, new_rows: Vec<T>) {
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
        floating_widgets: ModelRc::new(VecModel::default()),
        dock_types: ModelRc::new(VecModel::from(dock_types_vec(locale))),
        dock_add_label: locale.tr("dock-add-label").into(),
        grid_columns: 16,
        grid_rows: 10,
        canvas_content_width: 1f32,
        canvas_content_height: 1f32,
        canvas_scroll_x: 0f32,
        canvas_scroll_y: 0f32,
        canvas_scroll_gen: 0,
    }
}

fn is_known_widget_type(type_id: &str) -> bool {
    matches!(
        orchid_widgets::WidgetRegistry::canonical_type_id(type_id),
        "terminal"
            | "weather"
            | "moon"
            | "jyotish"
            | "clock"
            | "system"
            | "processes"
            | "calculator"
            | "notes"
            | "rss"
            | "recent-files"
            | "universal-search"
            | "media-player"
            | "password-manager"
            | "viewer"
            | "file-manager"
    )
}


fn apply_catalog_row_visibility(
    g: &crate::slint_generated::WidgetCatalog,
    visible_ids: &std::collections::HashSet<&str>,
) {
    g.set_show_terminal(visible_ids.contains("terminal"));
    g.set_show_weather(visible_ids.contains("weather"));
    g.set_show_moon(visible_ids.contains("moon"));
    g.set_show_jyotish(visible_ids.contains("jyotish"));
    g.set_show_clock(visible_ids.contains("clock"));
    g.set_show_system(visible_ids.contains("system"));
    g.set_show_processes(visible_ids.contains("processes"));
    g.set_show_calculator(visible_ids.contains("calculator"));
    g.set_show_notes(visible_ids.contains("notes"));
    g.set_show_rss(visible_ids.contains("rss"));
    g.set_show_recent_files(visible_ids.contains("recent-files"));
    g.set_show_search(visible_ids.contains("search"));
    g.set_show_media(visible_ids.contains("media"));
    g.set_show_password(visible_ids.contains("password"));
    g.set_show_viewer(visible_ids.contains("viewer"));
    g.set_show_file_manager(visible_ids.contains("file-manager"));
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
        "jyotish" => "widget-jyotish-desc",
        "clock" => "widget-clock-desc",
        "system" => "widget-system-desc",
        "processes" => "widget-processes-desc",
        "calculator" => "widget-calculator-desc",
        "notes" => "widget-notes-desc",
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
            type_id: "jyotish".into(),
            label: locale.tr("dock-widget-jyotish").into(),
            description: dock_widget_description(locale, "jyotish"),
            icon: "jyotish".into(),
        },
        DockWidgetType {
            type_id: "clock".into(),
            label: locale.tr("dock-widget-clock").into(),
            description: dock_widget_description(locale, "clock"),
            icon: "clock".into(),
        },
        DockWidgetType {
            type_id: "system".into(),
            label: locale.tr("dock-widget-system").into(),
            description: dock_widget_description(locale, "system"),
            icon: "system".into(),
        },
        DockWidgetType {
            type_id: "processes".into(),
            label: locale.tr("dock-widget-processes").into(),
            description: dock_widget_description(locale, "processes"),
            icon: "processes".into(),
        },
        DockWidgetType {
            type_id: "calculator".into(),
            label: locale.tr("dock-widget-calculator").into(),
            description: dock_widget_description(locale, "calculator"),
            icon: "calculator".into(),
        },
        DockWidgetType {
            type_id: "notes".into(),
            label: locale.tr("dock-widget-notes").into(),
            description: dock_widget_description(locale, "notes"),
            icon: "notes".into(),
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
        "jyotish" => locale.tr("dock-widget-jyotish").into(),
        "clock" => locale.tr("dock-widget-clock").into(),
        "system" => locale.tr("dock-widget-system").into(),
        "processes" => locale.tr("dock-widget-processes").into(),
        "calculator" => locale.tr("dock-widget-calculator").into(),
        "notes" => locale.tr("dock-widget-notes").into(),
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
    JyotishModel,
    ClockModel,
    SystemModel,
    ProcessesModel,
    CalculatorModel,
    NotesModel,
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

pub(super) fn empty_widget_settings_dialog() -> WidgetSettingsDialog {
    WidgetSettingsDialog {
        visible: false,
        title: SharedString::new(),
        close_label: SharedString::new(),
        fields: ModelRc::new(VecModel::<SettingsFieldRow>::default()),
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
