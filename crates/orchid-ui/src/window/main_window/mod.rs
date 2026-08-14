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
use orchid_terminal::events::TerminalOutput;
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
    empty_terminal_cells, empty_viewer_model, empty_weather_model, locale_display_name,
    patch_calculator_model, patch_clock_model, patch_media_model, patch_password_model,
    patch_processes_model, patch_recent_files_model, patch_search_model, patch_system_model,
    theme_display_name, widget_has_settings, FileManagerOverlays, PasswordAddDialogOverlay,
};
use super::spawn;
use crate::error::{Result, UiError};
use crate::slint_generated::{
    AppState, CalculatorModel, CalendarModel, ClockModel, DockWidgetType, FileManagerModel,
    GroupTabModel, JyotishModel, MainWindow, MediaModel, MoonModel, NotesModel, NotificationItem,
    PasswordModel, ProcessesConfirmDialog, ProcessesModel, RecentFilesModel, RssModel,
    SearchCandidateEntry, SearchModel, SettingsFieldRow, SettingsSectionEntry, Strings,
    SystemModel, TerminalCellModel, Theme, ViewerModel, WeatherModel, WidgetCatalog,
    WidgetCloseConfirmDialog, WidgetFrameModel, WidgetSettingsDialog, WindowTaskbarItem,
    WorkspaceModel, WorkspaceSummary,
};
use crate::terminal_font_metrics;
use crate::terminal_raster;
use crate::theme::ThemeManager;
use crate::widgets::terminal::TerminalWidgetDeps;

mod calculator;
mod calendar;
mod canvas;
mod catalog;
mod clock;
mod floating;
mod fm;
mod input;
mod jyotish;
mod media_search;
mod notes;
mod password;
mod processes;
mod shell_ui;
mod system_file_clipboard;
mod terminal;
mod weather;
mod widget_settings;
mod wire;
mod workspace;

pub use workspace::build_empty_workspace_model;
pub(super) use workspace::fallback_widget_title;
pub(super) use workspace::next_untitled_docx_path;

use canvas::ResizeInteraction;

/// Canvas fills the window; the workspace orb overlays the bottom corner.
const WORKSPACE_CHROME_H: f32 = 0.0;

type ProcessesContextMap = HashMap<Uuid, (bool, f32, f32)>;
type FmLastClick = Option<(Uuid, u8, String, Instant)>;

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
    _terminal_output_sub: SubscriptionHandle,
    /// Managed ingest failure file name; drained on the next UI tick.
    fm_ingest_failure_pending: Arc<Mutex<Option<String>>>,
    /// Last file-manager transfer error mirrored to the notification center.
    last_fm_transfer_error: Arc<Mutex<Option<String>>>,
    /// Per-instance Jyotish notification state (Rahu Kalam edge, day color).
    jyotish_notify_state: Arc<Mutex<HashMap<Uuid, jyotish::JyotishNotifyState>>>,
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
    /// Retained terminal bitmaps keyed by pane session id (or `"root"`), for dirty-line patches.
    terminal_raster_cache: Mutex<HashMap<String, Option<terminal_raster::RetainedRaster>>>,
    /// Stable Slint `ModelRc`s for the workspace and widget lists. Replacing the whole
    /// `ModelRc` on every tick re-instantiated every `for` item and dropped keyboard focus
    /// on the terminal's `TextInput` (see `terminal-view.slint`); we only mutate these via
    /// [`sync_vec_model`].
    workspace_workspaces: ModelRc<WorkspaceSummary>,
    workspace_widgets: ModelRc<WidgetFrameModel>,
    /// Floating (undocked) window frames, viewport-relative.
    workspace_floating_widgets: ModelRc<WidgetFrameModel>,
    /// Taskbar entries for floating + minimized windows.
    workspace_window_taskbar: ModelRc<WindowTaskbarItem>,
    workspace_dock_types: ModelRc<DockWidgetType>,
    /// Paint order for floating windows (last = top).
    floating_z_stack: Arc<Mutex<Vec<Uuid>>>,
    /// Active snap-zone ghost while dragging a floating window.
    snap_zone: Arc<Mutex<Option<PixelBounds>>>,
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
    processes_context: Arc<RwLock<ProcessesContextMap>>,
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
    fm_last_click: Arc<Mutex<FmLastClick>>,
    /// Debounce duplicate open from Slint double-click + Rust double-click.
    fm_last_open: Arc<Mutex<Option<(Uuid, String, Instant)>>>,
    /// Monotonic sequence per (instance, pane) for quick-filter debounce.
    fm_filter_seq: Arc<Mutex<HashMap<(Uuid, u8), u64>>>,
    /// Monotonic sequence per (instance, pane) for address-bar path complete.
    fm_complete_seq: Arc<Mutex<HashMap<(Uuid, u8), u64>>>,
    /// Per-pane scroll/viewport size for entry-list virtualization.
    fm_viewport: Arc<Mutex<HashMap<(Uuid, u8), crate::window::models::FmViewport>>>,
    /// Last virtualized window start index; skip rebuild when unchanged.
    fm_viewport_window: Arc<Mutex<HashMap<(Uuid, u8), usize>>>,
    /// After navigation, ignore leftover Flickable scroll until it returns to top.
    fm_viewport_pin_top: Arc<Mutex<HashSet<(Uuid, u8)>>>,
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
    /// Directory for Document Editor untitled `.docx` files (`data/documents`).
    documents_dir: PathBuf,
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

#[derive(Debug, Clone, Default)]
struct NavigationUiState {
    workspace_orb_open: bool,
    notification_center_visible: bool,
}

#[derive(Debug, Clone, Default)]
struct OnboardingUiState {
    overlay_visible: bool,
    current_step: i32,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum AddWidgetPlacement {
    AutoSlot,
    CanvasPoint { content_x: f32, content_y: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PasswordCopyKind {
    Password,
    Username,
    Totp,
}

#[derive(Default)]
struct OsDropBatch {
    paths: Vec<String>,
    generation: u64,
}

impl MainWindowController {
    /// Build the window, apply globals, and wire Slint callbacks.
    #[allow(clippy::too_many_arguments, clippy::arc_with_non_send_sync)]
    pub fn new(
        theme: Arc<ThemeManager>,
        locale: Arc<LocaleManager>,
        config: Arc<RwLock<OrchidConfig>>,
        config_file_path: PathBuf,
        documents_dir: PathBuf,
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
        let workspace_window_taskbar: ModelRc<WindowTaskbarItem> =
            ModelRc::new(VecModel::<WindowTaskbarItem>::default());
        let workspace_dock_types: ModelRc<DockWidgetType> =
            ModelRc::new(VecModel::from(catalog::dock_types_vec(&locale)));
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
        let terminal_output_wm = widget_manager.clone();
        let terminal_output_layouts = terminal_deps.layouts.clone();
        let terminal_output_routing = session_routing.clone();
        let terminal_output_sub = bus
            .subscribe_async(
                EventFilter::of_type(TerminalOutput::event_type()),
                HandlerPriority::Normal,
                move |env| {
                    let session_id = env.downcast_arc::<TerminalOutput>().map(|e| e.session_id);
                    let wm = terminal_output_wm.clone();
                    let layouts = terminal_output_layouts.clone();
                    let routing = terminal_output_routing.clone();
                    async move {
                        let Some(session_id) = session_id else {
                            return;
                        };
                        // Prefer the focused-session routing table (hot path).
                        for entry in routing.lock().iter() {
                            if *entry.1 == session_id {
                                wm.mark_snapshot_dirty(*entry.0);
                                return;
                            }
                        }
                        // Multi-pane: any leaf in the layout may have produced output.
                        for (instance_id, layout) in layouts.lock().iter() {
                            let mut leaves = Vec::new();
                            for tab in &layout.tabs.tabs {
                                tab.root.leaves(&mut leaves);
                            }
                            if leaves.contains(&session_id) {
                                wm.mark_snapshot_dirty(*instance_id);
                            }
                        }
                    }
                },
            )
            .map_err(|e| UiError::Slint(format!("terminal output sub: {e}")))?;
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
            _terminal_output_sub: terminal_output_sub,
            fm_ingest_failure_pending,
            last_fm_transfer_error: Arc::new(Mutex::new(None)),
            jyotish_notify_state: Arc::new(Mutex::new(HashMap::new())),
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
            terminal_raster_cache: Mutex::new(HashMap::new()),
            workspace_workspaces,
            workspace_widgets,
            workspace_floating_widgets,
            workspace_window_taskbar,
            workspace_dock_types,
            floating_z_stack: Arc::new(Mutex::new(Vec::new())),
            snap_zone: Arc::new(Mutex::new(None)),
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
            fm_complete_seq: Arc::new(Mutex::new(HashMap::new())),
            fm_viewport: Arc::new(Mutex::new(HashMap::new())),
            fm_viewport_window: Arc::new(Mutex::new(HashMap::new())),
            fm_viewport_pin_top: Arc::new(Mutex::new(HashSet::new())),
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
            documents_dir,
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
        g.set_dock_widget_calendar(mgr.tr("dock-widget-calendar").into());
        g.set_dock_widget_rss(mgr.tr("dock-widget-rss").into());
        g.set_dock_widget_recent_files(mgr.tr("dock-widget-recent-files").into());
        g.set_dock_widget_search(mgr.tr("dock-widget-search").into());
        g.set_dock_widget_media(mgr.tr("dock-widget-media").into());
        g.set_dock_widget_password(mgr.tr("dock-widget-password").into());
        g.set_dock_widget_viewer(mgr.tr("dock-widget-viewer").into());
        g.set_dock_widget_document_editor(mgr.tr("dock-widget-document-editor").into());
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
        g.set_widget_calendar_desc(mgr.tr("widget-calendar-desc").into());
        g.set_widget_rss_desc(mgr.tr("widget-rss-desc").into());
        g.set_widget_recent_files_desc(mgr.tr("widget-recent-files-desc").into());
        g.set_widget_search_desc(mgr.tr("widget-search-desc").into());
        g.set_widget_media_desc(mgr.tr("widget-media-desc").into());
        g.set_widget_password_desc(mgr.tr("widget-password-desc").into());
        g.set_widget_viewer_desc(mgr.tr("widget-viewer-desc").into());
        g.set_widget_document_editor_desc(mgr.tr("widget-document-editor-desc").into());
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
        g.set_fm_nav_history(mgr.tr("fm-nav-history").into());
        g.set_fm_nav_history_frequent(mgr.tr("fm-nav-history-frequent").into());
        g.set_fm_nav_history_recent(mgr.tr("fm-nav-history-recent").into());
        g.set_fm_nav_history_empty(mgr.tr("fm-nav-history-empty").into());
        g.set_fm_nav_drives(mgr.tr("fm-nav-drives").into());
        g.set_fm_nav_drives_empty(mgr.tr("fm-nav-drives-empty").into());
        g.set_fm_action_branch_view(mgr.tr("fm-action-branch-view").into());
        g.set_fm_loading(mgr.tr("fm-loading").into());
        g.set_fm_empty_folder(mgr.tr("fm-empty-folder").into());
        g.set_fm_action_new_folder(mgr.tr("fm-action-new-folder").into());
        g.set_fm_action_new_tab(mgr.tr("fm-action-new-tab").into());
        g.set_fm_action_close_tab(mgr.tr("fm-action-close-tab").into());
        g.set_fm_action_copy(mgr.tr("fm-action-copy").into());
        g.set_fm_action_cut(mgr.tr("fm-action-cut").into());
        g.set_fm_action_paste(mgr.tr("fm-action-paste").into());
        g.set_fm_action_delete(mgr.tr("fm-action-delete").into());
        g.set_fm_action_rename(mgr.tr("fm-action-rename").into());
        g.set_fm_transfer_pause(mgr.tr("fm-transfer-pause").into());
        g.set_fm_transfer_resume(mgr.tr("fm-transfer-resume").into());
        g.set_fm_transfer_cancel(mgr.tr("fm-transfer-cancel").into());
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
            self.is_windowed(*id)
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
    pub(super) async fn move_new_widget_to_free_slot(
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
        *self.canvas_scroll.lock() = (-viewport_x, -viewport_y);
        self.schedule_visibility_sync();
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

    /// Content area of [`widget-frame.slint`] below the title bar (`height - 32px`); must match
    /// what `terminal-viewport-changed` would report as `w`/`h`.
    const WIDGET_FRAME_HEADER_PX: f32 = 32.0;
    /// Height of [`terminal-tabs.slint`] inside the terminal widget content area.
    const TERMINAL_TAB_BAR_PX: f32 = 29.0;
    /// Height of [`group-tabs.slint`] when a frame is part of a multi-widget group.
    const GROUP_TAB_BAR_PX: f32 = 28.0;

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
                                        } else if mods.control_key()
                                            && matches!(
                                                event.logical_key,
                                                Key::Named(NamedKey::Tab)
                                            )
                                        {
                                            c.cycle_floating_windows(mods.shift_key());
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
                            if let Some(ev) = input::winit_touch_to_orchid(touch, c.window.window())
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

    /// Soft cap on simultaneously open floating windows (taskbar still lists minimized).
    pub(super) const FLOATING_WINDOW_CAP: usize = 8;
    /// Alias used by multi-open viewer batching from the file manager.
    pub(super) const VIEWER_MULTI_OPEN_CAP: usize = Self::FLOATING_WINDOW_CAP;

    /// Height reserved for the in-app window taskbar (logical px).
    pub(super) const WINDOW_TASKBAR_HEIGHT_PX: f32 = 36.0;

    /// Edge threshold for snap zones (logical px).
    pub(super) const SNAP_EDGE_THRESHOLD_PX: f32 = 16.0;
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

pub(super) fn open_with_application_picker(path: &str) -> std::io::Result<()> {
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

pub(super) fn open_file_associations(path: &str) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        let _ = path;
        std::process::Command::new("explorer.exe")
            .arg("ms-settings:defaultapps")
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        let _ = path;
        std::process::Command::new("open")
            .args(["x-apple.systempreferences:com.apple.preference.defaultapp"])
            .spawn()?;
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        opener::open(path).map_err(|e| std::io::Error::other(e.to_string()))?;
    }
    Ok(())
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
