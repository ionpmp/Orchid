//! Windows Optimize widget — update, privacy, Explorer, and shell settings.

mod catalog;
mod config;
mod platform;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::thread;

use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::RwLock;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::{OptimizePayload, OptimizeRow};
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};
use orchid_storage::{LifecycleState, WidgetSize};

pub use catalog::ApplyStatus;
pub use config::OptimizeConfig;

use catalog::{
    apply_ops, plan_for, preset_ops, probe_matches, select_index, tweak_by_id, tweaks, TAB_STARTUP,
};

/// Stable type id.
pub const TYPE_ID: &str = "optimize";

static OPTIMIZE_LIVE: LazyLock<DashMap<Uuid, Arc<OptimizeHandle>>> = LazyLock::new(DashMap::new);

struct OptimizeHandle {
    instance_id: Uuid,
    config: RwLock<OptimizeConfig>,
    status_key: RwLock<String>,
    query: RwLock<String>,
    cache: RwLock<Option<OptimizePayload>>,
    busy: AtomicBool,
    bus: Arc<orchid_core::EventBus>,
    locale: Arc<orchid_i18n::LocaleManager>,
}

impl OptimizeHandle {
    fn publish(&self) {
        *self.cache.write() = None;
        self.emit();
    }

    fn emit(&self) {
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    fn payload(&self) -> OptimizePayload {
        let base = if let Some(cached) = self.cache.read().clone() {
            cached
        } else {
            let built = self.build_payload();
            *self.cache.write() = Some(built.clone());
            built
        };
        self.filter_query(base)
    }

    fn filter_query(&self, mut payload: OptimizePayload) -> OptimizePayload {
        let query = self.query.read().clone();
        payload.query = query.clone();
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return payload;
        }
        payload.rows.retain(|row| {
            let title = if row.title_text.is_empty() {
                self.locale.tr(&row.title_key)
            } else {
                row.title_text.clone()
            };
            let detail = if row.detail_text.is_empty() {
                self.locale.tr(&row.detail_key)
            } else {
                row.detail_text.clone()
            };
            title.to_lowercase().contains(&needle) || detail.to_lowercase().contains(&needle)
        });
        payload
    }

    fn build_payload(&self) -> OptimizePayload {
        let tab = i32::from(self.config.read().tab);
        let unsupported = !cfg!(windows);
        let rows = if unsupported {
            Vec::new()
        } else if tab == i32::from(TAB_STARTUP) {
            startup_rows(&self.locale)
        } else {
            tweaks()
                .iter()
                .filter(|tweak| i32::from(tweak.tab) == tab)
                .map(|tweak| {
                    let selected = select_index(tweak, probe_matches);
                    OptimizeRow {
                        id: tweak.id.to_string(),
                        title_key: tweak.title_key.to_string(),
                        detail_key: tweak.detail_key.to_string(),
                        option_keys: tweak
                            .option_keys
                            .iter()
                            .map(|key| (*key).to_string())
                            .collect(),
                        selected,
                        needs_admin: tweak.needs_admin,
                        changed: selected != tweak.fallback,
                        needs_restart: tweak.restarts_explorer,
                        title_text: String::new(),
                        detail_text: String::new(),
                        can_toggle: true,
                    }
                })
                .collect()
        };
        OptimizePayload {
            tab,
            rows,
            query: String::new(),
            status_key: self.status_key.read().clone(),
            unsupported,
        }
    }
}

fn startup_rows(locale: &orchid_i18n::LocaleManager) -> Vec<OptimizeRow> {
    let Ok(list) = crate::builtin::processes::startup::list_startup() else {
        return Vec::new();
    };
    list.into_iter()
        .map(|row| {
            let place = if row.location.contains('\\') {
                row.location.clone()
            } else {
                locale.tr(&row.location)
            };
            let machine = row.id.starts_with("registry:hklm")
                || row.location == "processes-startup-common-folder";
            OptimizeRow {
                id: row.id,
                title_key: String::new(),
                detail_key: String::new(),
                option_keys: Vec::new(),
                selected: u8::from(row.enabled),
                needs_admin: machine,
                changed: !row.enabled,
                needs_restart: false,
                title_text: row.name,
                detail_text: format!("{place}\n{}", row.command),
                can_toggle: row.can_toggle,
            }
        })
        .collect()
}

/// Snapshot the live config for tests and the settings dialog.
#[must_use]
pub fn current_config(instance_id: Uuid) -> Option<OptimizeConfig> {
    OPTIMIZE_LIVE
        .get(&instance_id)
        .map(|handle| handle.config.read().clone())
}

/// Switch tabs and publish. Does not write the registry.
pub fn set_tab(instance_id: Uuid, tab: i32) {
    let Some(handle) = OPTIMIZE_LIVE.get(&instance_id) else {
        return;
    };
    if tab < 0 {
        return;
    }
    {
        let mut cfg = handle.config.write();
        cfg.tab = u8::try_from(tab).unwrap_or(0);
        cfg.normalize();
    }
    handle.publish();
}

/// Apply a switch or a choice. Unknown ids are ignored.
///
/// The registry write runs off the UI thread. An administrator setting asks
/// Windows to confirm before it is stored.
pub fn set_choice(instance_id: Uuid, tweak_id: &str, index: i32) {
    let Some(def) = tweak_by_id(tweak_id) else {
        return;
    };
    let Some(plan) = plan_for(tweak_id, index) else {
        return;
    };
    let Some(handle) = OPTIMIZE_LIVE.get(&instance_id) else {
        return;
    };
    if handle
        .busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    *handle.status_key.write() = "optimize-status-working".to_string();
    handle.publish();
    let restarts = def.restarts_explorer;
    let plan = plan.to_vec();
    let worker = Arc::clone(&handle);
    let spawned = thread::Builder::new()
        .name("orchid-optimize".to_string())
        .spawn(move || {
            let status = apply_ops(&plan);
            let key = match status {
                ApplyStatus::Applied if restarts => "optimize-status-explorer",
                ApplyStatus::Applied => "optimize-status-applied",
                ApplyStatus::Denied => "optimize-status-denied",
                ApplyStatus::Failed => "optimize-status-failed",
                ApplyStatus::Unsupported => "optimize-unsupported",
            };
            *worker.status_key.write() = key.to_string();
            worker.busy.store(false, Ordering::Release);
            worker.publish();
        });
    if spawned.is_err() {
        handle.busy.store(false, Ordering::Release);
        *handle.status_key.write() = "optimize-status-failed".to_string();
        handle.publish();
    }
}

/// Remember the search box. Does not read the registry again.
pub fn set_query(instance_id: Uuid, query: &str) {
    let Some(handle) = OPTIMIZE_LIVE.get(&instance_id) else {
        return;
    };
    *handle.query.write() = query.to_string();
    handle.emit();
}

/// Apply a named set. Unknown ids are ignored.
///
/// Machine policies in the set share one administrator prompt.
pub fn apply_preset(instance_id: Uuid, preset_id: &str) {
    let Some((ops, restarts)) = preset_ops(preset_id) else {
        return;
    };
    let Some(handle) = OPTIMIZE_LIVE.get(&instance_id) else {
        return;
    };
    if handle
        .busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    *handle.status_key.write() = "optimize-status-working".to_string();
    handle.publish();
    let worker = Arc::clone(&handle);
    let spawned = thread::Builder::new()
        .name("orchid-optimize".to_string())
        .spawn(move || {
            let status = apply_ops(&ops);
            let key = match status {
                ApplyStatus::Applied if restarts => "optimize-status-explorer",
                ApplyStatus::Applied => "optimize-status-preset",
                ApplyStatus::Denied => "optimize-status-denied",
                ApplyStatus::Failed => "optimize-status-failed",
                ApplyStatus::Unsupported => "optimize-unsupported",
            };
            *worker.status_key.write() = key.to_string();
            worker.busy.store(false, Ordering::Release);
            worker.publish();
        });
    if spawned.is_err() {
        handle.busy.store(false, Ordering::Release);
        *handle.status_key.write() = "optimize-status-failed".to_string();
        handle.publish();
    }
}

/// Flip a switch. Choice rows are left unchanged.
pub fn toggle(instance_id: Uuid, tweak_id: &str) {
    if tweak_id.starts_with("registry:") || tweak_id.starts_with("folder:") {
        toggle_startup(instance_id, tweak_id);
        return;
    }
    let Some(def) = tweak_by_id(tweak_id) else {
        return;
    };
    if !def.option_keys.is_empty() {
        return;
    }
    let current = select_index(def, probe_matches);
    let next = if current == 0 { 1 } else { 0 };
    set_choice(instance_id, tweak_id, i32::from(next));
}

fn toggle_startup(instance_id: Uuid, id: &str) {
    let Ok(list) = crate::builtin::processes::startup::list_startup() else {
        return;
    };
    let Some(row) = list.into_iter().find(|row| row.id == id) else {
        return;
    };
    if !row.can_toggle {
        return;
    }
    let Some(handle) = OPTIMIZE_LIVE.get(&instance_id) else {
        return;
    };
    if handle
        .busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    *handle.status_key.write() = "optimize-status-working".to_string();
    handle.publish();
    let enable = !row.enabled;
    let id = id.to_string();
    let worker = Arc::clone(&handle);
    let spawned = thread::Builder::new()
        .name("orchid-optimize".to_string())
        .spawn(move || {
            let key = match crate::builtin::processes::startup::set_startup_enabled(&id, enable) {
                Ok(()) => "optimize-status-applied",
                Err(err) => {
                    let lower = err.to_ascii_lowercase();
                    if lower.contains("denied") || lower.contains("access") {
                        "optimize-status-denied"
                    } else {
                        "optimize-status-failed"
                    }
                }
            };
            *worker.status_key.write() = key.to_string();
            worker.busy.store(false, Ordering::Release);
            worker.publish();
        });
    if spawned.is_err() {
        handle.busy.store(false, Ordering::Release);
        *handle.status_key.write() = "optimize-status-failed".to_string();
        handle.publish();
    }
}

/// Re-read Windows and clear the status line.
pub fn refresh(instance_id: Uuid) {
    let Some(handle) = OPTIMIZE_LIVE.get(&instance_id) else {
        return;
    };
    *handle.status_key.write() = "optimize-status-refreshed".to_string();
    handle.publish();
}

/// Restart Explorer so shell settings show up. Does nothing off Windows.
pub fn restart_explorer(instance_id: Uuid) {
    let Some(handle) = OPTIMIZE_LIVE.get(&instance_id) else {
        return;
    };
    if !cfg!(windows) {
        *handle.status_key.write() = "optimize-unsupported".to_string();
        handle.publish();
        return;
    }
    if handle
        .busy
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    *handle.status_key.write() = "optimize-status-working".to_string();
    handle.publish();
    let worker = Arc::clone(&handle);
    let spawned = thread::Builder::new()
        .name("orchid-optimize".to_string())
        .spawn(move || {
            let ok = restart_explorer_process();
            *worker.status_key.write() = if ok {
                "optimize-status-restarted".to_string()
            } else {
                "optimize-status-failed".to_string()
            };
            worker.busy.store(false, Ordering::Release);
            worker.publish();
        });
    if spawned.is_err() {
        handle.busy.store(false, Ordering::Release);
        *handle.status_key.write() = "optimize-status-failed".to_string();
        handle.publish();
    }
}

fn restart_explorer_process() -> bool {
    #[cfg(windows)]
    {
        let killed = std::process::Command::new("taskkill")
            .args(["/f", "/im", "explorer.exe"])
            .status();
        let started = std::process::Command::new("explorer.exe").spawn();
        killed.is_ok() && started.is_ok()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

struct OptimizeWidget {
    instance_id: Uuid,
    handle: Arc<OptimizeHandle>,
}

impl OptimizeWidget {
    fn new(
        instance_id: Uuid,
        cfg: OptimizeConfig,
        bus: Arc<orchid_core::EventBus>,
        locale: Arc<orchid_i18n::LocaleManager>,
    ) -> Self {
        let handle = Arc::new(OptimizeHandle {
            instance_id,
            config: RwLock::new(cfg),
            status_key: RwLock::new(String::new()),
            query: RwLock::new(String::new()),
            cache: RwLock::new(None),
            busy: AtomicBool::new(false),
            bus,
            locale,
        });
        OPTIMIZE_LIVE.insert(instance_id, Arc::clone(&handle));
        Self {
            instance_id,
            handle,
        }
    }
}

#[async_trait]
impl Widget for OptimizeWidget {
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
        self.handle.publish();
        Ok(())
    }

    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_close(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        OPTIMIZE_LIVE.remove(&self.instance_id);
        Ok(())
    }

    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }

    fn snapshot(&self) -> Option<WidgetSnapshot> {
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title: self.handle.locale.tr("widget-optimize-name"),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::Optimize(self.handle.payload()),
        })
    }

    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        let cfg = self.handle.config.read().clone();
        state_codec::save_state(&cfg)
    }

    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let mut cfg: OptimizeConfig = state_codec::restore_state(bytes).unwrap_or_default();
        cfg.normalize();
        *self.handle.config.write() = cfg;
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
        let mut cfg = match state_bytes {
            Some(bytes) => state_codec::restore_state::<OptimizeConfig>(bytes).unwrap_or_default(),
            None => OptimizeConfig::default(),
        };
        cfg.normalize();
        Ok(Box::new(OptimizeWidget::new(
            ctx.instance_id,
            cfg,
            ctx.bus.clone(),
            ctx.locale.clone(),
        )) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-optimize-name",
        description_key: "widget-optimize-desc",
        icon_name: "system",
        category: WidgetCategory::System,
        default_size: WidgetSize::Large,
        min_size: Some(WidgetSize::Medium),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: false,
        factory,
    }
}
