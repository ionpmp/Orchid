//! Processes widget — Task Manager–style process / service / startup / users view.

pub mod classify;
pub mod config;
pub mod provider;
pub mod services;
pub mod startup;
pub mod types;
pub mod users;
pub mod windows;

use std::path::Path;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use uuid::Uuid;

use crate::error::{Result as WidgetResult, WidgetError};
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::{
    ProcessGroup, ProcessRowView, ProcessSortColumn, ProcessesPayload, ProcessesTab,
    ServiceRowView, StartupRowView, UserRowView,
};
use crate::widget::refresh::PeriodicRefresh;
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};
use orchid_storage::{LifecycleState, WidgetSize};

pub use config::ProcessesConfig;
pub use provider::ProcessesProvider;
pub use types::{ProcessSample, ProcessesSnapshot};

/// Stable type id.
pub const TYPE_ID: &str = "processes";

static PROCESSES_LIVE: LazyLock<DashMap<Uuid, Arc<ProcessesHandle>>> = LazyLock::new(DashMap::new);

struct UiState {
    tab: ProcessesTab,
    search_query: String,
    sort_column: ProcessSortColumn,
    sort_descending: bool,
    selected_pid: u32,
    selected_service: String,
    selected_startup: String,
    selected_session: u32,
    status_message: String,
    services: Vec<ServiceRowView>,
    startups: Vec<StartupRowView>,
    users: Vec<UserRowView>,
}

struct ProcessesHandle {
    instance_id: Uuid,
    config: Arc<RwLock<ProcessesConfig>>,
    snapshot: Arc<RwLock<Option<ProcessesSnapshot>>>,
    ui: Arc<RwLock<UiState>>,
    provider: Arc<ProcessesProvider>,
    refresh: Mutex<PeriodicRefresh>,
    bus: Arc<orchid_core::EventBus>,
    locale: Arc<orchid_i18n::LocaleManager>,
    /// Built UI snapshot; invalidated on every [`Self::publish`] so the ~30 Hz
    /// snapshot pump does not rebuild process rows between samples.
    cached_ui_snapshot: RwLock<Option<WidgetSnapshot>>,
}

impl ProcessesHandle {
    fn publish(&self) {
        *self.cached_ui_snapshot.write() = None;
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    fn refresh_interval(&self) -> Duration {
        Duration::from_secs(self.config.read().refresh_interval_seconds.max(1) as u64)
    }

    fn schedule_refresh(self: &Arc<Self>) {
        let interval = self.refresh_interval();
        let mut refresh = self.refresh.lock();
        refresh.set_interval(interval);
        let provider = self.provider.clone();
        let snap_slot = self.snapshot.clone();
        let ui = self.ui.clone();
        let locale = self.locale.clone();
        let handle = Arc::clone(self);
        refresh.start(move || {
            let provider = provider.clone();
            let snap_slot = snap_slot.clone();
            let ui = ui.clone();
            let locale = locale.clone();
            let handle = Arc::clone(&handle);
            async move {
                let provider2 = provider.clone();
                let snap = match tokio::task::spawn_blocking(move || provider2.refresh()).await {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(error = %e, "processes periodic refresh join failed");
                        return;
                    }
                };
                let tab = ui.read().tab;
                refresh_side_tabs(&ui, &locale, &snap, tab);
                *snap_slot.write() = Some(snap);
                handle.publish();
            }
        });
    }

    fn stop_refresh(&self) {
        self.refresh.lock().stop();
    }

    fn set_status(&self, msg: impl Into<String>) {
        self.ui.write().status_message = msg.into();
        self.publish();
    }
}

fn refresh_side_tabs(
    ui: &RwLock<UiState>,
    locale: &orchid_i18n::LocaleManager,
    snap: &ProcessesSnapshot,
    tab: ProcessesTab,
) {
    match tab {
        ProcessesTab::Services => {
            let services = services::list_services().unwrap_or_else(|e| {
                tracing::warn!(error = %e, "list services failed");
                Vec::new()
            });
            ui.write().services = services;
        }
        ProcessesTab::Startup => {
            let startups = startup::list_startup().unwrap_or_else(|e| {
                tracing::warn!(error = %e, "list startup failed");
                Vec::new()
            });
            ui.write().startups = startups;
        }
        ProcessesTab::Users => {
            let mut users = users::list_sessions(&snap.processes).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "list sessions failed");
                Vec::new()
            });
            for u in &mut users {
                u.memory_text = locale.format_byte_size(u.memory_bytes);
            }
            ui.write().users = users;
        }
        ProcessesTab::Processes => {}
    }
}

/// Snapshot the live config for the settings dialog.
#[must_use]
pub fn current_config(instance_id: Uuid) -> Option<ProcessesConfig> {
    PROCESSES_LIVE
        .get(&instance_id)
        .map(|h| h.config.read().clone())
}

/// Apply a settings-dialog mutation to the live config.
pub fn update_config(instance_id: Uuid, mutate: impl FnOnce(&mut ProcessesConfig)) {
    let Some(h) = PROCESSES_LIVE.get(&instance_id) else {
        return;
    };
    let before_interval = h.config.read().refresh_interval_seconds;
    {
        let mut cfg = h.config.write();
        mutate(&mut cfg);
        if cfg.refresh_interval_seconds == 0 {
            cfg.refresh_interval_seconds = 1;
        }
    }
    let after_interval = h.config.read().refresh_interval_seconds;
    h.publish();
    if before_interval != after_interval && h.refresh.lock().is_running() {
        h.schedule_refresh();
    }
}

/// Update search filter.
pub fn update_search(instance_id: Uuid, query: String) {
    let Some(h) = PROCESSES_LIVE.get(&instance_id) else {
        return;
    };
    h.ui.write().search_query = query.clone();
    h.config.write().search_query = query;
    h.publish();
}

/// Switch tab.
pub fn set_tab(instance_id: Uuid, tab: i32) {
    let Some(h) = PROCESSES_LIVE.get(&instance_id) else {
        return;
    };
    let tab = ProcessesTab::from_index(tab);
    h.ui.write().tab = tab;
    h.config.write().default_tab = tab as u8;
    // Refresh side data immediately for the new tab.
    if let Some(snap) = h.snapshot.read().clone() {
        refresh_side_tabs(&h.ui, &h.locale, &snap, tab);
    } else {
        let empty = ProcessesSnapshot {
            processes: Vec::new(),
            captured_at: chrono::Utc::now(),
        };
        refresh_side_tabs(&h.ui, &h.locale, &empty, tab);
    }
    h.publish();
}

/// Change sort column (toggle direction if same column).
pub fn set_sort(instance_id: Uuid, column: i32) {
    let Some(h) = PROCESSES_LIVE.get(&instance_id) else {
        return;
    };
    let col = ProcessSortColumn::from_index(column);
    {
        let mut ui = h.ui.write();
        if ui.sort_column == col {
            ui.sort_descending = !ui.sort_descending;
        } else {
            ui.sort_column = col;
            ui.sort_descending = matches!(
                col,
                ProcessSortColumn::Cpu | ProcessSortColumn::Memory | ProcessSortColumn::Io
            );
        }
        h.config.write().sort_column = ui.sort_column as u8;
        h.config.write().sort_descending = ui.sort_descending;
    }
    h.publish();
}

/// Select a process by PID.
pub fn select_process(instance_id: Uuid, pid: u32) {
    if let Some(h) = PROCESSES_LIVE.get(&instance_id) {
        h.ui.write().selected_pid = pid;
        h.publish();
    }
}

/// Select a service by name.
pub fn select_service(instance_id: Uuid, name: String) {
    if let Some(h) = PROCESSES_LIVE.get(&instance_id) {
        h.ui.write().selected_service = name;
        h.publish();
    }
}

/// Select a startup entry.
pub fn select_startup(instance_id: Uuid, id: String) {
    if let Some(h) = PROCESSES_LIVE.get(&instance_id) {
        h.ui.write().selected_startup = id;
        h.publish();
    }
}

/// Select a user session.
pub fn select_session(instance_id: Uuid, session_id: u32) {
    if let Some(h) = PROCESSES_LIVE.get(&instance_id) {
        h.ui.write().selected_session = session_id;
        h.publish();
    }
}

/// Kill a single process.
pub fn kill_process(instance_id: Uuid, pid: u32) -> Result<(), String> {
    let h = PROCESSES_LIVE
        .get(&instance_id)
        .map(|r| Arc::clone(r.value()))
        .ok_or_else(|| "processes widget not live".to_string())?;
    if h.provider.kill(pid) {
        h.set_status(h.locale.tr("processes-kill-ok"));
        Ok(())
    } else {
        let msg = h.locale.tr("processes-access-denied");
        h.set_status(msg.clone());
        Err(msg)
    }
}

/// Kill a process tree.
pub fn kill_process_tree(instance_id: Uuid, pid: u32) -> Result<(), String> {
    let h = PROCESSES_LIVE
        .get(&instance_id)
        .map(|r| Arc::clone(r.value()))
        .ok_or_else(|| "processes widget not live".to_string())?;
    let (ok, total) = h.provider.kill_tree(pid);
    if ok == 0 {
        let msg = h.locale.tr("processes-access-denied");
        h.set_status(msg.clone());
        Err(msg)
    } else {
        let msg = h.locale.tr_args(
            "processes-kill-tree-ok",
            &orchid_i18n::FluentArgs::new()
                .with("ok", ok.to_string())
                .with("total", total.to_string()),
        );
        h.set_status(msg);
        Ok(())
    }
}

/// Open the folder containing the process executable.
pub fn open_file_location(instance_id: Uuid, pid: u32) -> Result<(), String> {
    let h = PROCESSES_LIVE
        .get(&instance_id)
        .ok_or_else(|| "processes widget not live".to_string())?;
    let path = h
        .snapshot
        .read()
        .as_ref()
        .and_then(|s| s.processes.iter().find(|p| p.pid == pid).map(|p| p.path.clone()))
        .unwrap_or_default();
    if path.is_empty() {
        return Err(h.locale.tr("processes-no-path"));
    }
    let parent = Path::new(&path).parent().unwrap_or(Path::new(&path));
    opener::open(parent).map_err(|e| e.to_string())
}

/// Resolve process path for clipboard copy.
#[must_use]
pub fn process_path(instance_id: Uuid, pid: u32) -> Option<String> {
    PROCESSES_LIVE.get(&instance_id).and_then(|h| {
        h.snapshot.read().as_ref().and_then(|s| {
            s.processes
                .iter()
                .find(|p| p.pid == pid)
                .map(|p| p.path.clone())
        })
    })
}

/// Start a Windows service.
pub fn service_start(instance_id: Uuid, name: &str) -> Result<(), String> {
    let h = PROCESSES_LIVE
        .get(&instance_id)
        .map(|r| Arc::clone(r.value()))
        .ok_or_else(|| "processes widget not live".to_string())?;
    services::start_service(name)?;
    h.set_status(h.locale.tr("processes-service-started"));
    Ok(())
}

/// Stop a Windows service.
pub fn service_stop(instance_id: Uuid, name: &str) -> Result<(), String> {
    let h = PROCESSES_LIVE
        .get(&instance_id)
        .map(|r| Arc::clone(r.value()))
        .ok_or_else(|| "processes widget not live".to_string())?;
    services::stop_service(name)?;
    h.set_status(h.locale.tr("processes-service-stopped"));
    Ok(())
}

/// Restart a Windows service.
pub fn service_restart(instance_id: Uuid, name: &str) -> Result<(), String> {
    let h = PROCESSES_LIVE
        .get(&instance_id)
        .map(|r| Arc::clone(r.value()))
        .ok_or_else(|| "processes widget not live".to_string())?;
    services::restart_service(name)?;
    h.set_status(h.locale.tr("processes-service-restarted"));
    Ok(())
}

/// Toggle a startup entry.
pub fn startup_set_enabled(instance_id: Uuid, id: &str, enabled: bool) -> Result<(), String> {
    let h = PROCESSES_LIVE
        .get(&instance_id)
        .map(|r| Arc::clone(r.value()))
        .ok_or_else(|| "processes widget not live".to_string())?;
    startup::set_startup_enabled(id, enabled)?;
    h.set_status(h.locale.tr("processes-startup-updated"));
    Ok(())
}

/// Open startup entry location.
pub fn startup_open_location(instance_id: Uuid, id: &str) -> Result<(), String> {
    let _ = PROCESSES_LIVE
        .get(&instance_id)
        .ok_or_else(|| "processes widget not live".to_string())?;
    startup::open_startup_location(id)
}

/// Disconnect a user session.
pub fn user_disconnect(instance_id: Uuid, session_id: u32) -> Result<(), String> {
    let h = PROCESSES_LIVE
        .get(&instance_id)
        .map(|r| Arc::clone(r.value()))
        .ok_or_else(|| "processes widget not live".to_string())?;
    users::disconnect_session(session_id)?;
    h.set_status(h.locale.tr("processes-user-disconnected"));
    Ok(())
}

/// Sign out a user session.
pub fn user_sign_out(instance_id: Uuid, session_id: u32) -> Result<(), String> {
    let h = PROCESSES_LIVE
        .get(&instance_id)
        .map(|r| Arc::clone(r.value()))
        .ok_or_else(|| "processes widget not live".to_string())?;
    users::sign_out_session(session_id)?;
    h.set_status(h.locale.tr("processes-user-signed-out"));
    Ok(())
}

/// Clear the status message line.
pub fn clear_status(instance_id: Uuid) {
    if let Some(h) = PROCESSES_LIVE.get(&instance_id) {
        h.ui.write().status_message.clear();
        h.publish();
    }
}

/// Processes widget.
pub struct ProcessesWidget {
    instance_id: Uuid,
    handle: Arc<ProcessesHandle>,
}

impl std::fmt::Debug for ProcessesWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessesWidget")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl ProcessesWidget {
    /// Construct a processes widget.
    pub fn new(
        instance_id: Uuid,
        cfg: ProcessesConfig,
        bus: Arc<orchid_core::EventBus>,
        locale: Arc<orchid_i18n::LocaleManager>,
    ) -> Self {
        let interval = Duration::from_secs(cfg.refresh_interval_seconds.max(1) as u64);
        let ui = UiState {
            tab: cfg.tab(),
            search_query: cfg.search_query.clone(),
            sort_column: cfg.sort(),
            sort_descending: cfg.sort_descending,
            selected_pid: 0,
            selected_service: String::new(),
            selected_startup: String::new(),
            selected_session: u32::MAX,
            status_message: String::new(),
            services: Vec::new(),
            startups: Vec::new(),
            users: Vec::new(),
        };
        let handle = Arc::new(ProcessesHandle {
            instance_id,
            config: Arc::new(RwLock::new(cfg)),
            snapshot: Arc::new(RwLock::new(None)),
            ui: Arc::new(RwLock::new(ui)),
            provider: Arc::new(ProcessesProvider::new()),
            refresh: Mutex::new(PeriodicRefresh::new(interval)),
            bus,
            locale,
            cached_ui_snapshot: RwLock::new(None),
        });
        PROCESSES_LIVE.insert(instance_id, Arc::clone(&handle));
        Self {
            instance_id,
            handle,
        }
    }
}

#[async_trait]
impl Widget for ProcessesWidget {
    fn type_id(&self) -> &'static str {
        TYPE_ID
    }
    fn instance_id(&self) -> Uuid {
        self.instance_id
    }
    async fn on_create(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        let provider = self.handle.provider.clone();
        let snap = tokio::task::spawn_blocking(move || provider.refresh())
            .await
            .map_err(|e| {
                WidgetError::CreationFailed(format!("processes initial refresh: {e}"))
            })?;
        let tab = self.handle.ui.read().tab;
        refresh_side_tabs(&self.handle.ui, &self.handle.locale, &snap, tab);
        *self.handle.snapshot.write() = Some(snap);
        Ok(())
    }
    async fn on_activate(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.handle.schedule_refresh();
        Ok(())
    }
    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.handle.stop_refresh();
        Ok(())
    }
    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.handle.stop_refresh();
        Ok(())
    }
    async fn on_close(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.handle.stop_refresh();
        PROCESSES_LIVE.remove(&self.instance_id);
        Ok(())
    }
    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }
    fn snapshot(&self) -> Option<WidgetSnapshot> {
        if let Some(cached) = self.handle.cached_ui_snapshot.read().clone() {
            return Some(cached);
        }
        let cfg = self.handle.config.read().clone();
        let ui = self.handle.ui.read().clone_view();
        let is_loading = self.handle.snapshot.read().is_none();
        let processes = match self.handle.snapshot.read().as_ref() {
            Some(snap) => build_process_rows(
                snap,
                &ui.search_query,
                ui.sort_column,
                ui.sort_descending,
                cfg.show_grouping,
                ui.selected_pid,
                &self.handle.locale,
            ),
            None => Vec::new(),
        };
        let built = WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title: self.handle.locale.tr("widget-processes-name").into(),
            status: if is_loading {
                WidgetStatus::Loading
            } else {
                WidgetStatus::Ready
            },
            payload: WidgetPayload::Processes(ProcessesPayload {
                tab: ui.tab,
                search_query: ui.search_query.clone(),
                sort_column: ui.sort_column,
                sort_descending: ui.sort_descending,
                selected_pid: ui.selected_pid,
                selected_service: ui.selected_service,
                selected_startup: ui.selected_startup,
                selected_session: ui.selected_session,
                processes,
                services: filter_services(&ui.services, &ui.search_query),
                startups: filter_startups(&ui.startups, &ui.search_query),
                users: filter_users(&ui.users, &ui.search_query),
                is_loading,
                status_message: ui.status_message,
                show_grouping: cfg.show_grouping,
            }),
        };
        *self.handle.cached_ui_snapshot.write() = Some(built.clone());
        Some(built)
    }
    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        let mut cfg = self.handle.config.read().clone();
        let ui = self.handle.ui.read();
        cfg.search_query = ui.search_query.clone();
        cfg.default_tab = ui.tab as u8;
        cfg.sort_column = ui.sort_column as u8;
        cfg.sort_descending = ui.sort_descending;
        state_codec::save_state(&cfg)
    }
    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let cfg: ProcessesConfig = state_codec::restore_state(bytes)?;
        let was_running = self.handle.refresh.lock().is_running();
        let before = self.handle.config.read().refresh_interval_seconds;
        {
            let mut ui = self.handle.ui.write();
            ui.tab = cfg.tab();
            ui.search_query = cfg.search_query.clone();
            ui.sort_column = cfg.sort();
            ui.sort_descending = cfg.sort_descending;
        }
        *self.handle.cached_ui_snapshot.write() = None;
        *self.handle.config.write() = cfg;
        let after = self.handle.config.read().refresh_interval_seconds;
        if was_running && before != after {
            self.handle.schedule_refresh();
        }
        Ok(())
    }
    fn capabilities(&self) -> WidgetCapabilities {
        WidgetCapabilities {
            supports_resize: true,
            min_size: Some(WidgetSize::Medium),
            max_size: None,
            preferred_size: Some(WidgetSize::Large),
            allows_grouping: true,
            keeps_state_when_unloaded: true,
            has_settings_panel: true,
        }
    }
}

/// Lightweight clone of UI selection / lists for snapshot building.
struct UiView {
    tab: ProcessesTab,
    search_query: String,
    sort_column: ProcessSortColumn,
    sort_descending: bool,
    selected_pid: u32,
    selected_service: String,
    selected_startup: String,
    selected_session: u32,
    status_message: String,
    services: Vec<ServiceRowView>,
    startups: Vec<StartupRowView>,
    users: Vec<UserRowView>,
}

impl UiState {
    fn clone_view(&self) -> UiView {
        UiView {
            tab: self.tab,
            search_query: self.search_query.clone(),
            sort_column: self.sort_column,
            sort_descending: self.sort_descending,
            selected_pid: self.selected_pid,
            selected_service: self.selected_service.clone(),
            selected_startup: self.selected_startup.clone(),
            selected_session: self.selected_session,
            status_message: self.status_message.clone(),
            services: self.services.clone(),
            startups: self.startups.clone(),
            users: self.users.clone(),
        }
    }
}

fn matches_query(hay: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    hay.to_ascii_lowercase()
        .contains(&query.to_ascii_lowercase())
}

/// Cap rows sent to Slint. ListView virtualizes paint, but building/sorting the
/// full system process table is still bounded for the sample worker.
const MAX_PROCESS_ROWS: usize = 80;

fn build_process_rows(
    snap: &ProcessesSnapshot,
    query: &str,
    sort: ProcessSortColumn,
    descending: bool,
    show_grouping: bool,
    selected_pid: u32,
    locale: &orchid_i18n::LocaleManager,
) -> Vec<ProcessRowView> {
    let mut rows: Vec<ProcessRowView> = snap
        .processes
        .iter()
        .filter(|p| {
            matches_query(&p.name, query)
                || matches_query(&p.path, query)
                || matches_query(&p.user, query)
                || matches_query(&p.pid.to_string(), query)
        })
        .map(|p| ProcessRowView {
            pid: p.pid,
            name: p.name.clone(),
            status: p.status.clone(),
            cpu_percent: p.cpu_percent,
            memory_bytes: p.memory_bytes,
            memory_text: locale.format_byte_size(p.memory_bytes),
            io_read_bps: p.io_read_bps,
            io_write_bps: p.io_write_bps,
            io_text: format_io(locale, p.io_read_bps, p.io_write_bps),
            user: p.user.clone(),
            path: p.path.clone(),
            group: p.group,
            parent_pid: p.parent_pid,
            session_id: p.session_id,
            is_group_header: false,
            group_label: String::new(),
        })
        .collect();

    sort_processes(&mut rows, sort, descending);
    truncate_process_rows(&mut rows, selected_pid);

    if !show_grouping || !query.is_empty() {
        return rows;
    }

    let mut grouped = Vec::new();
    for group in [
        ProcessGroup::Apps,
        ProcessGroup::Background,
        ProcessGroup::Windows,
    ] {
        let label = match group {
            ProcessGroup::Apps => locale.tr("processes-group-apps"),
            ProcessGroup::Background => locale.tr("processes-group-background"),
            ProcessGroup::Windows => locale.tr("processes-group-windows"),
        };
        let members: Vec<_> = rows.iter().filter(|r| r.group == group).cloned().collect();
        if members.is_empty() {
            continue;
        }
        grouped.push(ProcessRowView {
            pid: 0,
            name: label.clone(),
            status: String::new(),
            cpu_percent: 0.0,
            memory_bytes: 0,
            memory_text: String::new(),
            io_read_bps: 0,
            io_write_bps: 0,
            io_text: String::new(),
            user: String::new(),
            path: String::new(),
            group,
            parent_pid: None,
            session_id: None,
            is_group_header: true,
            group_label: label,
        });
        grouped.extend(members);
    }
    grouped
}

fn truncate_process_rows(rows: &mut Vec<ProcessRowView>, selected_pid: u32) {
    if rows.len() <= MAX_PROCESS_ROWS {
        return;
    }
    let selected = (selected_pid != 0)
        .then(|| rows.iter().find(|r| r.pid == selected_pid).cloned())
        .flatten();
    rows.truncate(MAX_PROCESS_ROWS);
    if let Some(sel) = selected {
        if !rows.iter().any(|r| r.pid == sel.pid) {
            if let Some(last) = rows.last_mut() {
                *last = sel;
            }
        }
    }
}

fn format_io(locale: &orchid_i18n::LocaleManager, read_bps: u64, write_bps: u64) -> String {
    if read_bps == 0 && write_bps == 0 {
        return "0".into();
    }
    format!(
        "R {}/s · W {}/s",
        locale.format_byte_size(read_bps),
        locale.format_byte_size(write_bps)
    )
}

fn sort_processes(rows: &mut [ProcessRowView], sort: ProcessSortColumn, descending: bool) {
    rows.sort_by(|a, b| {
        let ord = match sort {
            ProcessSortColumn::Name => a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()),
            ProcessSortColumn::Pid => a.pid.cmp(&b.pid),
            ProcessSortColumn::Cpu => a
                .cpu_percent
                .partial_cmp(&b.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal),
            ProcessSortColumn::Memory => a.memory_bytes.cmp(&b.memory_bytes),
            ProcessSortColumn::Io => {
                let ai = a.io_read_bps.saturating_add(a.io_write_bps);
                let bi = b.io_read_bps.saturating_add(b.io_write_bps);
                ai.cmp(&bi)
            }
            ProcessSortColumn::User => a.user.to_ascii_lowercase().cmp(&b.user.to_ascii_lowercase()),
            ProcessSortColumn::Path => a.path.to_ascii_lowercase().cmp(&b.path.to_ascii_lowercase()),
            ProcessSortColumn::Status => a.status.cmp(&b.status),
        };
        if descending {
            ord.reverse()
        } else {
            ord
        }
    });
}

fn filter_services(rows: &[ServiceRowView], query: &str) -> Vec<ServiceRowView> {
    rows.iter()
        .filter(|s| {
            matches_query(&s.name, query)
                || matches_query(&s.display_name, query)
                || matches_query(&s.status, query)
        })
        .cloned()
        .collect()
}

fn filter_startups(rows: &[StartupRowView], query: &str) -> Vec<StartupRowView> {
    rows.iter()
        .filter(|s| {
            matches_query(&s.name, query)
                || matches_query(&s.command, query)
                || matches_query(&s.location, query)
        })
        .cloned()
        .collect()
}

fn filter_users(rows: &[UserRowView], query: &str) -> Vec<UserRowView> {
    rows.iter()
        .filter(|u| {
            matches_query(&u.user_name, query)
                || matches_query(&u.state, query)
                || matches_query(&u.session_id.to_string(), query)
        })
        .cloned()
        .collect()
}

/// Descriptor ready to register on a widget registry.
#[must_use]
pub fn descriptor() -> WidgetDescriptor {
    let factory: WidgetFactory = Arc::new(|ctx: WidgetContext, state_bytes| {
        let cfg = match state_bytes {
            Some(bytes) => state_codec::restore_state::<ProcessesConfig>(bytes).unwrap_or_default(),
            None => ProcessesConfig::default(),
        };
        Ok(Box::new(ProcessesWidget::new(
            ctx.instance_id,
            cfg,
            ctx.bus.clone(),
            ctx.locale.clone(),
        )) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-processes-name",
        description_key: "widget-processes-desc",
        icon_name: "processes",
        category: WidgetCategory::System,
        default_size: WidgetSize::Large,
        min_size: Some(WidgetSize::Medium),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: true,
        factory,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::payloads::ProcessGroup;

    #[test]
    fn sort_by_cpu_descending() {
        let mut rows = vec![
            ProcessRowView {
                pid: 1,
                name: "a".into(),
                status: String::new(),
                cpu_percent: 1.0,
                memory_bytes: 0,
                memory_text: String::new(),
                io_read_bps: 0,
                io_write_bps: 0,
                io_text: String::new(),
                user: String::new(),
                path: String::new(),
                group: ProcessGroup::Apps,
                parent_pid: None,
                session_id: None,
                is_group_header: false,
                group_label: String::new(),
            },
            ProcessRowView {
                pid: 2,
                name: "b".into(),
                status: String::new(),
                cpu_percent: 50.0,
                memory_bytes: 0,
                memory_text: String::new(),
                io_read_bps: 0,
                io_write_bps: 0,
                io_text: String::new(),
                user: String::new(),
                path: String::new(),
                group: ProcessGroup::Apps,
                parent_pid: None,
                session_id: None,
                is_group_header: false,
                group_label: String::new(),
            },
        ];
        sort_processes(&mut rows, ProcessSortColumn::Cpu, true);
        assert_eq!(rows[0].pid, 2);
    }

    #[test]
    fn filter_by_pid_query() {
        let locale =
            orchid_i18n::LocaleManager::new(orchid_i18n::default_language(), None).expect("locale");
        let snap = ProcessesSnapshot {
            processes: vec![ProcessSample {
                pid: 4242,
                name: "foo.exe".into(),
                status: "Run".into(),
                cpu_percent: 0.0,
                memory_bytes: 100,
                io_read_bytes: 0,
                io_write_bytes: 0,
                io_read_bps: 0,
                io_write_bps: 0,
                user: "me".into(),
                path: r"C:\foo.exe".into(),
                parent_pid: None,
                session_id: Some(1),
                group: ProcessGroup::Apps,
            }],
            captured_at: chrono::Utc::now(),
        };
        let rows = build_process_rows(
            &snap,
            "4242",
            ProcessSortColumn::Name,
            false,
            false,
            0,
            &locale,
        );
        assert_eq!(rows.len(), 1);
    }
}
