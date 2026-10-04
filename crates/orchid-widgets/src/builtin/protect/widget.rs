//! Protection widget lifecycle.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::thread;
use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;
use orchid_storage::{LifecycleState, WidgetSize};
use parking_lot::Mutex;
use sysinfo::{Disks, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::protect::{ProtectDrive, ProtectPayload, ProtectRow};
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};

use super::catalog::{CleanerId, ProtectTab};
use super::config::ProtectConfig;
use super::firewall::{block_refusal, merge_app_rows, rule_name, RunningApp};
use super::platform::{firewall_rules_json, run_netsh, PlatformError};
use super::resolve::{clean_cleaner, scan_cleaner, CleanReport, HostLayout};
use super::wipe::{clamp_passes, filler_directory, wipe_budget, wipe_free_space, RESERVE_BYTES};
use super::CleanStats;

/// Stable type id.
pub const TYPE_ID: &str = "protect";

struct DriveInfo {
    label: String,
    mount: String,
    free: u64,
}

struct AppView {
    path: String,
    name: String,
    blocked: bool,
}

struct ProtectUi {
    config: ProtectConfig,
    tab: i32,
    stats: Vec<(CleanerId, CleanStats)>,
    drives: Vec<DriveInfo>,
    apps: Vec<AppView>,
    status_key: String,
    status_files: u64,
    status_bytes: u64,
    status_detail: String,
    wipe_percent: i32,
    free_bytes: u64,
    scanned: bool,
}

struct ProtectHandle {
    instance_id: Uuid,
    ui: Mutex<ProtectUi>,
    busy: AtomicBool,
    cancel: AtomicBool,
    bus: Arc<orchid_core::EventBus>,
    locale: Arc<orchid_i18n::LocaleManager>,
}

static LIVE: LazyLock<DashMap<Uuid, Arc<ProtectHandle>>> = LazyLock::new(DashMap::new);

struct ProtectWidget {
    instance_id: Uuid,
    handle: Arc<ProtectHandle>,
}

impl ProtectWidget {
    fn new(
        instance_id: Uuid,
        config: ProtectConfig,
        bus: Arc<orchid_core::EventBus>,
        locale: Arc<orchid_i18n::LocaleManager>,
    ) -> Self {
        let handle = Arc::new(ProtectHandle {
            instance_id,
            ui: Mutex::new(ProtectUi {
                config,
                tab: 0,
                stats: Vec::new(),
                drives: Vec::new(),
                apps: Vec::new(),
                status_key: String::new(),
                status_files: 0,
                status_bytes: 0,
                status_detail: String::new(),
                wipe_percent: -1,
                free_bytes: 0,
                scanned: false,
            }),
            busy: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            bus,
            locale,
        });
        LIVE.insert(instance_id, Arc::clone(&handle));
        Self {
            instance_id,
            handle,
        }
    }
}

#[async_trait]
impl Widget for ProtectWidget {
    fn type_id(&self) -> &'static str {
        TYPE_ID
    }

    fn instance_id(&self) -> Uuid {
        self.instance_id
    }

    async fn on_create(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_activate(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        let scanned = self.handle.ui.lock().scanned;
        if scanned {
            self.handle.publish();
        } else {
            spawn_scan(Arc::clone(&self.handle));
        }
        Ok(())
    }

    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_close(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.handle.cancel.store(true, Ordering::Relaxed);
        LIVE.remove(&self.instance_id);
        Ok(())
    }

    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }

    fn snapshot(&self) -> Option<WidgetSnapshot> {
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title: self.handle.locale.tr("widget-protect-name"),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::Protect(self.handle.payload()),
        })
    }

    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        let config = self.handle.ui.lock().config.clone();
        state_codec::save_state(&config)
    }

    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let mut config: ProtectConfig = state_codec::restore_state(bytes).unwrap_or_default();
        config.normalize();
        self.handle.ui.lock().config = config;
        self.handle.publish();
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
            has_settings_panel: false,
        }
    }
}

/// Descriptor ready to register on a widget registry.
#[must_use]
pub fn descriptor() -> WidgetDescriptor {
    let factory: WidgetFactory = Arc::new(|ctx: WidgetContext, state_bytes| {
        let mut config = match state_bytes {
            Some(bytes) => state_codec::restore_state::<ProtectConfig>(bytes).unwrap_or_default(),
            None => ProtectConfig::default(),
        };
        config.normalize();
        Ok(Box::new(ProtectWidget::new(
            ctx.instance_id,
            config,
            ctx.bus.clone(),
            ctx.locale.clone(),
        )) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-protect-name",
        description_key: "widget-protect-desc",
        icon_name: "system",
        category: WidgetCategory::Security,
        default_size: WidgetSize::Large,
        min_size: Some(WidgetSize::Medium),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: false,
        factory,
    }
}

impl ProtectHandle {
    fn publish(&self) {
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    fn payload(&self) -> ProtectPayload {
        let ui = self.ui.lock();
        let rows = match ui.tab {
            0 | 1 => cleaner_rows(&ui),
            3 => ui
                .apps
                .iter()
                .map(|app| ProtectRow {
                    id: app.path.clone(),
                    title_key: String::new(),
                    title: app.name.clone(),
                    detail_key: String::new(),
                    detail: app.path.clone(),
                    size_key: if app.blocked {
                        "protect-blocked"
                    } else {
                        "protect-allowed"
                    }
                    .to_string(),
                    files: 0,
                    bytes: 0,
                    checked: app.blocked,
                })
                .collect(),
            _ => Vec::new(),
        };
        let selected = selected_drive(&ui);
        let drives = ui
            .drives
            .iter()
            .enumerate()
            .map(|(index, drive)| ProtectDrive {
                label: drive.label.clone(),
                selected: index as i32 == selected,
            })
            .collect();
        ProtectPayload {
            tab: ui.tab,
            rows,
            drives,
            passes: i32::from(ui.config.passes),
            wipe_percent: ui.wipe_percent,
            status_key: ui.status_key.clone(),
            status_files: ui.status_files,
            status_bytes: ui.status_bytes,
            status_detail: ui.status_detail.clone(),
            busy: self.busy.load(Ordering::Acquire),
            free_bytes: ui.free_bytes,
        }
    }
}

fn cleaner_rows(ui: &ProtectUi) -> Vec<ProtectRow> {
    CleanerId::ALL
        .iter()
        .copied()
        .filter(|id| tab_index(id.tab()) == ui.tab)
        .map(|id| {
            let stats = ui.stats.iter().find(|(row, _)| *row == id).map(|(_, s)| s);
            let (size_key, files, bytes) = match stats {
                None => ("protect-size-unknown", 0, 0),
                Some(stats) if stats.files == 0 && stats.bytes == 0 => {
                    ("protect-size-unknown", 0, 0)
                }
                Some(stats) if stats.bytes == 0 => ("protect-files", stats.files, 0),
                Some(stats) => ("", stats.files, stats.bytes),
            };
            ProtectRow {
                id: id.as_str().to_string(),
                title_key: id.title_key().to_string(),
                title: String::new(),
                detail_key: id.detail_key().to_string(),
                detail: String::new(),
                size_key: size_key.to_string(),
                files,
                bytes,
                checked: ui.config.is_enabled(id),
            }
        })
        .collect()
}

fn tab_index(tab: ProtectTab) -> i32 {
    match tab {
        ProtectTab::Traces => 0,
        ProtectTab::Histories => 1,
    }
}

fn selected_drive(ui: &ProtectUi) -> i32 {
    if ui.drives.is_empty() {
        return 0;
    }
    ui.drives
        .iter()
        .position(|drive| drive.mount.eq_ignore_ascii_case(&ui.config.drive))
        .unwrap_or(0) as i32
}

fn try_begin(handle: &ProtectHandle, status_key: &str) -> bool {
    if handle
        .busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return false;
    }
    {
        let mut ui = handle.ui.lock();
        ui.status_key = status_key.to_string();
        ui.status_files = 0;
        ui.status_bytes = 0;
        ui.status_detail.clear();
    }
    handle.publish();
    true
}

fn finish_idle(handle: &ProtectHandle, key: &str) {
    handle.busy.store(false, Ordering::Release);
    {
        let mut ui = handle.ui.lock();
        ui.status_key = key.to_string();
        if key == "protect-status-error" && ui.status_detail.is_empty() {
            ui.status_detail = "failed".to_string();
        }
    }
    handle.publish();
}

/// Switch the visible tab.
pub fn set_tab(id: Uuid, index: i32) {
    let Some(handle) = live(id) else {
        return;
    };
    let tab = index.clamp(0, 3);
    handle.ui.lock().tab = tab;
    handle.publish();
    if tab == 3 && handle.ui.lock().apps.is_empty() {
        spawn_network(handle);
    }
}

/// Flip a cleaner checkbox or a firewall block.
pub fn toggle_row(id: Uuid, row: &str) {
    let Some(handle) = live(id) else {
        return;
    };
    if let Some(cleaner) = CleanerId::parse(row) {
        if handle.busy.load(Ordering::Acquire) {
            return;
        }
        let mut ui = handle.ui.lock();
        let on = !ui.config.is_enabled(cleaner);
        ui.config.set_enabled(cleaner, on);
        drop(ui);
        handle.publish();
        return;
    }
    spawn_firewall_toggle(handle, row.to_string());
}

/// Measure every cleaner without deleting.
pub fn scan(id: Uuid) {
    let Some(handle) = live(id) else {
        return;
    };
    spawn_scan(handle);
}

/// Delete the checked cleaners on the current tab.
pub fn clean(id: Uuid) {
    let Some(handle) = live(id) else {
        return;
    };
    let tab = handle.ui.lock().tab;
    if tab == 0 || tab == 1 {
        spawn_clean(handle, tab);
    }
}

/// Choose the volume used by the next wipe.
pub fn select_drive(id: Uuid, index: i32) {
    let Some(handle) = live(id) else {
        return;
    };
    let chosen = {
        let ui = handle.ui.lock();
        ui.drives
            .get(index.max(0) as usize)
            .map(|drive| (drive.mount.clone(), drive.free))
    };
    if let Some((mount, free)) = chosen {
        let mut ui = handle.ui.lock();
        ui.config.drive = mount;
        ui.free_bytes = free;
        drop(ui);
        handle.publish();
    }
}

/// Set the overwrite pass count to 1 or 3.
pub fn set_passes(id: Uuid, passes: i32) {
    let Some(handle) = live(id) else {
        return;
    };
    handle.ui.lock().config.passes = clamp_passes(passes as u8);
    handle.publish();
}

/// Overwrite free space on the selected volume.
pub fn start_wipe(id: Uuid) {
    let Some(handle) = live(id) else {
        return;
    };
    spawn_wipe(handle);
}

/// Ask the running wipe to delete its filler and stop.
pub fn cancel_wipe(id: Uuid) {
    let Some(handle) = live(id) else {
        return;
    };
    handle.cancel.store(true, Ordering::Relaxed);
}

/// Re-read running programs and Orchid firewall rules.
pub fn refresh_network(id: Uuid) {
    let Some(handle) = live(id) else {
        return;
    };
    spawn_network(handle);
}

fn live(id: Uuid) -> Option<Arc<ProtectHandle>> {
    LIVE.get(&id).map(|handle| Arc::clone(handle.value()))
}

fn spawn_named(name: &str, work: impl FnOnce() + Send + 'static) -> bool {
    thread::Builder::new()
        .name(name.to_string())
        .spawn(work)
        .is_ok()
}

fn spawn_scan(handle: Arc<ProtectHandle>) {
    if !try_begin(&handle, "protect-status-scanning") {
        return;
    }
    let worker = Arc::clone(&handle);
    if !spawn_named("orchid-protect-scan", move || {
        let layout = HostLayout::from_env();
        let stats = CleanerId::ALL
            .iter()
            .copied()
            .map(|id| (id, scan_cleaner(id, &layout)))
            .collect::<Vec<_>>();
        let drives = collect_drives();
        {
            let mut ui = worker.ui.lock();
            ui.stats = stats;
            if !drives.is_empty() {
                ui.drives = drives;
            }
            let index = selected_drive(&ui) as usize;
            ui.free_bytes = ui.drives.get(index).map(|drive| drive.free).unwrap_or(0);
            if ui.config.drive.is_empty() {
                if let Some(drive) = ui.drives.first() {
                    ui.config.drive = drive.mount.clone();
                }
            }
            ui.scanned = true;
            ui.status_key = "protect-status-scanned".to_string();
        }
        worker.busy.store(false, Ordering::Release);
        worker.publish();
    }) {
        finish_idle(&handle, "protect-status-error");
    }
}

fn spawn_clean(handle: Arc<ProtectHandle>, tab: i32) {
    if !try_begin(&handle, "protect-status-cleaning") {
        return;
    }
    let enabled: Vec<CleanerId> = {
        let ui = handle.ui.lock();
        CleanerId::ALL
            .iter()
            .copied()
            .filter(|id| tab_index(id.tab()) == tab && ui.config.is_enabled(*id))
            .collect()
    };
    let worker = Arc::clone(&handle);
    if !spawn_named("orchid-protect-clean", move || {
        let layout = HostLayout::from_env();
        let reports = enabled
            .into_iter()
            .map(|id| (id, clean_cleaner(id, &layout)))
            .collect::<Vec<_>>();
        apply_clean_reports(&worker, &reports);
        worker.busy.store(false, Ordering::Release);
        worker.publish();
    }) {
        finish_idle(&handle, "protect-status-error");
    }
}

fn apply_clean_reports(handle: &ProtectHandle, reports: &[(CleanerId, CleanReport)]) {
    let mut ui = handle.ui.lock();
    if reports.is_empty() {
        ui.status_key = "protect-status-empty".to_string();
        return;
    }
    let mut files = 0u64;
    let mut bytes = 0u64;
    let mut need_admin = false;
    let mut other = None;
    for (id, report) in reports {
        files += report.stats.files;
        bytes += report.stats.bytes;
        if let Some(existing) = ui.stats.iter_mut().find(|(row, _)| row == id) {
            existing.1 = CleanStats::default();
        }
        match &report.error {
            Some(PlatformError::NeedAdmin) => need_admin = true,
            Some(err) => other = Some(err.to_string()),
            None => {}
        }
    }
    ui.status_files = files;
    ui.status_bytes = bytes;
    if let Some(message) = other {
        ui.status_key = "protect-status-error".to_string();
        ui.status_detail = message;
    } else if files == 0 && need_admin {
        ui.status_key = "protect-status-need-admin".to_string();
    } else {
        ui.status_key = "protect-status-done".to_string();
    }
}

fn spawn_wipe(handle: Arc<ProtectHandle>) {
    if !try_begin(&handle, "protect-status-wiping") {
        return;
    }
    let (mount, passes) = {
        let mut ui = handle.ui.lock();
        if ui.drives.is_empty() {
            ui.drives = collect_drives();
        }
        let index = selected_drive(&ui) as usize;
        if ui.config.drive.is_empty() {
            if let Some(mount) = ui.drives.get(index).map(|drive| drive.mount.clone()) {
                ui.config.drive = mount;
            }
        }
        let mount = ui
            .drives
            .get(index)
            .map(|drive| drive.mount.clone())
            .unwrap_or_else(|| ui.config.drive.clone());
        ui.wipe_percent = 0;
        ui.free_bytes = ui
            .drives
            .get(index)
            .map(|drive| drive.free)
            .unwrap_or(ui.free_bytes);
        (mount, ui.config.passes)
    };
    if mount.is_empty() {
        handle.ui.lock().wipe_percent = -1;
        finish_idle(&handle, "protect-status-empty");
        return;
    }
    handle.cancel.store(false, Ordering::Relaxed);
    let cancel = Arc::new(AtomicBool::new(false));
    let progress = Arc::new(AtomicU64::new(0));
    let watcher = Arc::clone(&handle);
    let watch_cancel = Arc::clone(&cancel);
    let watch_progress = Arc::clone(&progress);
    let _ = spawn_named("orchid-protect-wipe-ui", move || loop {
        thread::sleep(Duration::from_millis(400));
        if watcher.cancel.load(Ordering::Relaxed) {
            watch_cancel.store(true, Ordering::Relaxed);
        }
        let written = watch_progress.load(Ordering::Relaxed);
        let mut ui = watcher.ui.lock();
        if ui.status_key != "protect-status-wiping" {
            break;
        }
        let budget = ui.free_bytes.saturating_sub(RESERVE_BYTES).max(1);
        ui.wipe_percent = ((written.saturating_mul(100)) / budget).min(100) as i32;
        drop(ui);
        watcher.publish();
    });
    let worker = Arc::clone(&handle);
    let cancel_flag = Arc::clone(&cancel);
    let progress_flag = Arc::clone(&progress);
    if !spawn_named("orchid-protect-wipe", move || {
        let temp = std::env::temp_dir();
        let dir = filler_directory(std::path::Path::new(&mount), &temp);
        let free = disk_free(&mount).unwrap_or_else(|| worker.ui.lock().free_bytes);
        let budget = wipe_budget(free);
        let report = wipe_free_space(
            &dir,
            budget,
            passes,
            8 * 1024 * 1024,
            4,
            cancel_flag.as_ref(),
            progress_flag.as_ref(),
        );
        {
            let mut ui = worker.ui.lock();
            ui.wipe_percent = -1;
            match report {
                Ok(report) if report.cancelled => {
                    ui.status_key = "protect-status-cancelled".to_string();
                }
                Ok(_) => ui.status_key = "protect-status-protected".to_string(),
                Err(err) => {
                    ui.status_key = "protect-status-error".to_string();
                    ui.status_detail = err.to_string();
                }
            }
        }
        worker.busy.store(false, Ordering::Release);
        worker.publish();
    }) {
        handle.ui.lock().wipe_percent = -1;
        finish_idle(&handle, "protect-status-error");
    }
}

fn spawn_network(handle: Arc<ProtectHandle>) {
    if !try_begin(&handle, "protect-status-scanning") {
        return;
    }
    let remembered = handle.ui.lock().config.blocked.clone();
    let worker = Arc::clone(&handle);
    if !spawn_named("orchid-protect-net", move || {
        let running = running_apps();
        let json = firewall_rules_json();
        let rules = if json.is_empty() {
            None
        } else {
            Some(super::firewall::parse_firewall_json(&json))
        };
        {
            let mut ui = worker.ui.lock();
            let blocked_paths;
            let blocked_ref: &[String] = if let Some(rules) = rules.as_ref() {
                blocked_paths = rules
                    .iter()
                    .map(|rule| rule.program.clone())
                    .collect::<Vec<_>>();
                ui.config.blocked.clone_from(&blocked_paths);
                &blocked_paths
            } else {
                &remembered
            };
            ui.apps = merge_app_rows(&running, blocked_ref)
                .into_iter()
                .map(|row| AppView {
                    name: row.name,
                    path: row.path,
                    blocked: row.blocked,
                })
                .collect();
            ui.status_key = "protect-status-scanned".to_string();
        }
        worker.busy.store(false, Ordering::Release);
        worker.publish();
    }) {
        finish_idle(&handle, "protect-status-error");
    }
}

fn spawn_firewall_toggle(handle: Arc<ProtectHandle>, path: String) {
    if !try_begin(&handle, "protect-status-scanning") {
        return;
    }
    let worker = Arc::clone(&handle);
    let target = path.clone();
    if !spawn_named("orchid-protect-fw", move || {
        let outcome = apply_firewall_toggle(&target);
        {
            let mut ui = worker.ui.lock();
            match outcome {
                Ok(blocked) => {
                    if let Some(app) = ui.apps.iter_mut().find(|app| app.path == path) {
                        app.blocked = blocked;
                    }
                    ui.config
                        .blocked
                        .retain(|row| !row.eq_ignore_ascii_case(&path));
                    if blocked {
                        ui.config.blocked.push(path);
                    }
                    ui.status_key.clear();
                }
                Err(PlatformError::NeedAdmin) => {
                    ui.status_key = "protect-status-need-admin".to_string();
                }
                Err(err) => {
                    ui.status_key = "protect-status-error".to_string();
                    ui.status_detail = err.to_string();
                }
            }
        }
        worker.busy.store(false, Ordering::Release);
        worker.publish();
    }) {
        finish_idle(&handle, "protect-status-error");
    }
}

fn apply_firewall_toggle(path: &str) -> Result<bool, PlatformError> {
    let program = PathBuf::from(path);
    if block_refusal(&program).is_some() {
        return Err(PlatformError::Other("refused".into()));
    }
    let name = rule_name(&program);
    let currently = super::firewall::parse_firewall_json(&firewall_rules_json())
        .iter()
        .any(|rule| rule.program.eq_ignore_ascii_case(path));
    if currently {
        let _ = run_netsh(&super::firewall::netsh_delete_args(&name));
        Ok(false)
    } else {
        run_netsh(&super::firewall::netsh_add_args(&name, path))?;
        Ok(true)
    }
}

fn running_apps() -> Vec<RunningApp> {
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::Always),
    );
    let mut apps = Vec::new();
    for process in system.processes().values() {
        let Some(exe) = process.exe() else {
            continue;
        };
        apps.push(RunningApp {
            name: process.name().to_string_lossy().into_owned(),
            path: exe.to_string_lossy().into_owned(),
        });
    }
    apps
}

fn collect_drives() -> Vec<DriveInfo> {
    Disks::new_with_refreshed_list()
        .iter()
        .filter(|disk| disk.total_space() > 0 && !disk.is_removable())
        .map(|disk| {
            let mount = disk.mount_point().to_string_lossy().into_owned();
            let name = disk.name().to_string_lossy().into_owned();
            let label = if name.is_empty() {
                mount.clone()
            } else {
                format!("{name} ({mount})")
            };
            DriveInfo {
                label,
                mount,
                free: disk.available_space(),
            }
        })
        .collect()
}

fn disk_free(mount: &str) -> Option<u64> {
    let trimmed = mount.trim_end_matches(['\\', '/']);
    Disks::new_with_refreshed_list()
        .iter()
        .find(|disk| {
            let point = disk.mount_point().to_string_lossy();
            point.eq_ignore_ascii_case(mount) || point.eq_ignore_ascii_case(trimmed)
        })
        .map(|disk| disk.available_space())
}
