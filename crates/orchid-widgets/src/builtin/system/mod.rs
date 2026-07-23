//! System-indicators widget — CPU / Memory / Disk / Network / Battery.

pub mod config;
#[cfg(windows)]
mod cpu_windows;
pub mod provider;
pub mod types;

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
    IndicatorStatus, SystemIndicator, SystemIndicatorKind, SystemPayload,
};
use crate::widget::refresh::PeriodicRefresh;
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};
use orchid_storage::{LifecycleState, WidgetSize};

pub use config::SystemConfig;
pub use provider::SystemProvider;
pub use types::{BatteryStatus, DiskUsage, NetworkRate, SystemSnapshot};

/// Stable type id.
pub const TYPE_ID: &str = "system";

static SYSTEM_LIVE: LazyLock<DashMap<Uuid, Arc<SystemHandle>>> = LazyLock::new(DashMap::new);

struct SystemHandle {
    instance_id: Uuid,
    config: Arc<RwLock<SystemConfig>>,
    snapshot: Arc<RwLock<Option<SystemSnapshot>>>,
    provider: Arc<SystemProvider>,
    refresh: Mutex<PeriodicRefresh>,
    bus: Arc<orchid_core::EventBus>,
    locale: Arc<orchid_i18n::LocaleManager>,
}

impl SystemHandle {
    fn publish(&self) {
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

    /// Start (or restart) the periodic sample loop with the current interval.
    fn schedule_refresh(self: &Arc<Self>) {
        let interval = self.refresh_interval();
        let mut refresh = self.refresh.lock();
        refresh.set_interval(interval);
        let provider = self.provider.clone();
        let snap_slot = self.snapshot.clone();
        let handle = Arc::clone(self);
        refresh.start(move || {
            let provider = provider.clone();
            let snap_slot = snap_slot.clone();
            let handle = Arc::clone(&handle);
            async move {
                let provider2 = provider.clone();
                let snap = match tokio::task::spawn_blocking(move || provider2.refresh()).await {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(error = %e, "system periodic refresh join failed");
                        return;
                    }
                };
                *snap_slot.write() = Some(snap);
                handle.publish();
            }
        });
    }

    fn stop_refresh(&self) {
        self.refresh.lock().stop();
    }
}

/// Snapshot the live system config for the settings dialog.
#[must_use]
pub fn current_config(instance_id: Uuid) -> Option<SystemConfig> {
    SYSTEM_LIVE
        .get(&instance_id)
        .map(|h| h.config.read().clone())
}

/// Apply a settings-dialog mutation to the live system config.
pub fn update_config(instance_id: Uuid, mutate: impl FnOnce(&mut SystemConfig)) {
    let Some(h) = SYSTEM_LIVE.get(&instance_id) else {
        return;
    };
    let before_interval = h.config.read().refresh_interval_seconds;
    {
        let mut cfg = h.config.write();
        mutate(&mut cfg);
        cfg.normalize();
    }
    let after_interval = h.config.read().refresh_interval_seconds;
    h.publish();
    if before_interval != after_interval && h.refresh.lock().is_running() {
        h.schedule_refresh();
    }
}

/// System widget.
pub struct SystemWidget {
    instance_id: Uuid,
    handle: Arc<SystemHandle>,
}

impl std::fmt::Debug for SystemWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemWidget")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl SystemWidget {
    /// Construct a system widget.
    pub fn new(
        instance_id: Uuid,
        cfg: SystemConfig,
        bus: Arc<orchid_core::EventBus>,
        locale: Arc<orchid_i18n::LocaleManager>,
    ) -> Self {
        let interval = Duration::from_secs(cfg.refresh_interval_seconds.max(1) as u64);
        let handle = Arc::new(SystemHandle {
            instance_id,
            config: Arc::new(RwLock::new(cfg)),
            snapshot: Arc::new(RwLock::new(None)),
            provider: Arc::new(SystemProvider::new()),
            refresh: Mutex::new(PeriodicRefresh::new(interval)),
            bus,
            locale,
        });
        SYSTEM_LIVE.insert(instance_id, Arc::clone(&handle));
        Self {
            instance_id,
            handle,
        }
    }
}

#[async_trait]
impl Widget for SystemWidget {
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
                WidgetError::CreationFailed(format!("system metrics initial refresh: {e}"))
            })?;
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
        SYSTEM_LIVE.remove(&self.instance_id);
        Ok(())
    }
    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }
    fn snapshot(&self) -> Option<WidgetSnapshot> {
        let cfg = self.handle.config.read().clone();
        let indicators = match self.handle.snapshot.read().clone() {
            Some(snap) => build_indicators(&cfg, &snap, &self.handle.locale),
            None => vec![SystemIndicator {
                kind: SystemIndicatorKind::Cpu,
                name_suffix: None,
                value_text: String::new(),
                network_up: None,
                network_down: None,
                percent: None,
                segments: Vec::new(),
                icon: "system-cpu",
                status: IndicatorStatus::Normal,
            }],
        };
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title: self.handle.locale.tr("widget-system-name").into(),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::SystemIndicators(SystemPayload {
                indicators,
                is_loading: self.handle.snapshot.read().is_none(),
            }),
        })
    }
    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        state_codec::save_state(&*self.handle.config.read())
    }
    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let mut cfg: SystemConfig = state_codec::restore_state(bytes)?;
        cfg.normalize();
        let was_running = self.handle.refresh.lock().is_running();
        let before = self.handle.config.read().refresh_interval_seconds;
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
            min_size: Some(WidgetSize::Small),
            max_size: None,
            preferred_size: Some(WidgetSize::Medium),
            allows_grouping: true,
            keeps_state_when_unloaded: true,
            has_settings_panel: true,
        }
    }
}

fn bucket_pct(pct: f32, warn: f32, crit: f32) -> IndicatorStatus {
    if pct >= crit {
        IndicatorStatus::Critical
    } else if pct >= warn {
        IndicatorStatus::Warning
    } else {
        IndicatorStatus::Normal
    }
}

fn bucket_low(pct: f32, warn: f32, crit: f32) -> IndicatorStatus {
    if pct <= crit {
        IndicatorStatus::Critical
    } else if pct <= warn {
        IndicatorStatus::Warning
    } else {
        IndicatorStatus::Normal
    }
}

fn build_indicators(
    cfg: &SystemConfig,
    snap: &SystemSnapshot,
    locale: &orchid_i18n::LocaleManager,
) -> Vec<SystemIndicator> {
    let mut out = Vec::new();

    if cfg.show_cpu {
        let segments = if cfg.show_cpu_cores {
            snap.cpu_per_core.clone()
        } else {
            Vec::new()
        };
        out.push(SystemIndicator {
            kind: SystemIndicatorKind::Cpu,
            name_suffix: None,
            value_text: format!("{:.0}%", snap.cpu_total_percent),
            network_up: None,
            network_down: None,
            percent: Some(snap.cpu_total_percent),
            segments,
            icon: "system-cpu",
            status: bucket_pct(snap.cpu_total_percent, 75.0, 90.0),
        });
    }

    if cfg.show_memory && snap.memory_total_bytes > 0 {
        let pct = (snap.memory_used_bytes as f32 / snap.memory_total_bytes as f32) * 100.0;
        let mut value_text = format!(
            "{} / {}",
            locale.format_byte_size(snap.memory_used_bytes),
            locale.format_byte_size(snap.memory_total_bytes)
        );
        if snap.swap_total_bytes > 0 {
            let swap = locale.tr_args(
                "system-swap-suffix",
                &orchid_i18n::FluentArgs::new()
                    .with("used", locale.format_byte_size(snap.swap_used_bytes))
                    .with("total", locale.format_byte_size(snap.swap_total_bytes)),
            );
            value_text.push_str(" · ");
            value_text.push_str(&swap);
        }
        out.push(SystemIndicator {
            kind: SystemIndicatorKind::Memory,
            name_suffix: None,
            value_text,
            network_up: None,
            network_down: None,
            percent: Some(pct),
            segments: Vec::new(),
            icon: "system-memory",
            status: bucket_pct(pct, 80.0, 95.0),
        });
    }

    if cfg.show_disks {
        let selector = &cfg.disks;
        for d in &snap.disks {
            let include = if selector.is_empty() {
                provider::is_default_disk(d, cfg.show_removable_disks)
            } else {
                selector.iter().any(|m| m == &d.mount)
            };
            if !include {
                continue;
            }
            let pct = (d.used_bytes as f32 / d.total_bytes as f32) * 100.0;
            out.push(SystemIndicator {
                kind: SystemIndicatorKind::Disk,
                name_suffix: Some(d.mount.clone()),
                value_text: format!(
                    "{} / {}",
                    locale.format_byte_size(d.used_bytes),
                    locale.format_byte_size(d.total_bytes)
                ),
                network_up: None,
                network_down: None,
                percent: Some(pct),
                segments: Vec::new(),
                icon: "system-disk",
                status: bucket_pct(pct, 85.0, 95.0),
            });
        }
    }

    if cfg.show_network {
        let selector = &cfg.network_interfaces;
        let mut nets: Vec<&NetworkRate> = snap
            .networks
            .iter()
            .filter(|n| {
                if !selector.is_empty() {
                    return selector.iter().any(|name| name == &n.interface);
                }
                !provider::is_noisy_nic(&n.interface)
                    && (n.total_uploaded_bytes > 0 || n.total_downloaded_bytes > 0)
            })
            .collect();
        nets.sort_by(|a, b| a.interface.cmp(&b.interface));

        if cfg.aggregate_network {
            if !nets.is_empty() {
                let upload: f64 = nets.iter().map(|n| n.upload_bps).sum();
                let download: f64 = nets.iter().map(|n| n.download_bps).sum();
                out.push(SystemIndicator {
                    kind: SystemIndicatorKind::Network,
                    name_suffix: None,
                    value_text: String::new(),
                    network_up: Some(locale.format_byte_size(upload.max(0.0) as u64)),
                    network_down: Some(locale.format_byte_size(download.max(0.0) as u64)),
                    percent: None,
                    segments: Vec::new(),
                    icon: "system-network",
                    status: IndicatorStatus::Normal,
                });
            }
        } else {
            for n in nets {
                out.push(SystemIndicator {
                    kind: SystemIndicatorKind::Network,
                    name_suffix: Some(n.interface.clone()),
                    value_text: String::new(),
                    network_up: Some(locale.format_byte_size(n.upload_bps.max(0.0) as u64)),
                    network_down: Some(locale.format_byte_size(n.download_bps.max(0.0) as u64)),
                    percent: None,
                    segments: Vec::new(),
                    icon: "system-network",
                    status: IndicatorStatus::Normal,
                });
            }
        }
    }

    if cfg.show_battery {
        if let Some(b) = &snap.battery {
            let pct = b.percent as f32;
            let mut value = format!("{}%", b.percent);
            if b.charging {
                value.push_str(" · ");
                value.push_str(&locale.tr("system-battery-charging"));
                if let Some(secs) = b.time_to_full_seconds {
                    let time = locale.format_duration_secs(secs);
                    let args = orchid_i18n::FluentArgs::new().with("time", time);
                    value.push_str(" · ");
                    value.push_str(&locale.tr_args("system-battery-time-remaining", &args));
                }
            } else if let Some(secs) = b.time_to_empty_seconds {
                let time = locale.format_duration_secs(secs);
                let args = orchid_i18n::FluentArgs::new().with("time", time);
                value.push_str(" · ");
                value.push_str(&locale.tr_args("system-battery-time-remaining", &args));
            }
            out.push(SystemIndicator {
                kind: SystemIndicatorKind::Battery,
                name_suffix: None,
                value_text: value,
                network_up: None,
                network_down: None,
                percent: Some(pct),
                segments: Vec::new(),
                icon: if b.charging {
                    "system-battery-charging"
                } else {
                    "system-battery"
                },
                status: bucket_low(pct, 20.0, 10.0),
            });
        }
    }

    if cfg.show_uptime {
        out.push(SystemIndicator {
            kind: SystemIndicatorKind::Uptime,
            name_suffix: None,
            value_text: locale.format_duration_secs(snap.uptime_seconds),
            network_up: None,
            network_down: None,
            percent: None,
            segments: Vec::new(),
            icon: "system-uptime",
            status: IndicatorStatus::Normal,
        });
    }

    out
}

/// Descriptor ready to register on a widget registry.
#[must_use]
pub fn descriptor() -> WidgetDescriptor {
    let factory: WidgetFactory = Arc::new(|ctx: WidgetContext, state_bytes| {
        let cfg = match state_bytes {
            Some(bytes) => {
                let mut cfg = state_codec::restore_state::<SystemConfig>(bytes).unwrap_or_default();
                cfg.normalize();
                cfg
            }
            None => SystemConfig::default(),
        };
        Ok(Box::new(SystemWidget::new(
            ctx.instance_id,
            cfg,
            ctx.bus.clone(),
            ctx.locale.clone(),
        )) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-system-name",
        description_key: "widget-system-desc",
        icon_name: "system",
        category: WidgetCategory::System,
        default_size: WidgetSize::Medium,
        min_size: Some(WidgetSize::Small),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: true,
        factory,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn snap(cpu: f32) -> SystemSnapshot {
        SystemSnapshot {
            cpu_total_percent: cpu,
            cpu_per_core: vec![10.0, 20.0, 30.0, 40.0],
            cpu_temp_c: None,
            memory_total_bytes: 16 * 1024 * 1024 * 1024,
            memory_used_bytes: 8 * 1024 * 1024 * 1024,
            swap_total_bytes: 0,
            swap_used_bytes: 0,
            disks: Vec::new(),
            networks: Vec::new(),
            battery: None,
            uptime_seconds: 3600,
            captured_at: Utc::now(),
        }
    }

    fn test_locale() -> orchid_i18n::LocaleManager {
        orchid_i18n::LocaleManager::new(orchid_i18n::default_language(), None).expect("locale")
    }

    #[test]
    fn cpu_thresholds_bucket_correctly() {
        let cfg = SystemConfig::default();
        let locale = test_locale();
        let indicators = build_indicators(&cfg, &snap(20.0), &locale);
        let cpu = indicators
            .iter()
            .find(|i| i.kind == SystemIndicatorKind::Cpu)
            .unwrap();
        assert_eq!(cpu.status, IndicatorStatus::Normal);
        assert_eq!(cpu.segments.len(), 4);

        let indicators = build_indicators(&cfg, &snap(80.0), &locale);
        assert_eq!(
            indicators
                .iter()
                .find(|i| i.kind == SystemIndicatorKind::Cpu)
                .unwrap()
                .status,
            IndicatorStatus::Warning
        );

        let indicators = build_indicators(&cfg, &snap(95.0), &locale);
        assert_eq!(
            indicators
                .iter()
                .find(|i| i.kind == SystemIndicatorKind::Cpu)
                .unwrap()
                .status,
            IndicatorStatus::Critical
        );
    }

    #[test]
    fn cpu_cores_respect_toggle() {
        let mut cfg = SystemConfig::default();
        cfg.show_cpu_cores = false;
        cfg.show_memory = false;
        cfg.show_disks = false;
        cfg.show_network = false;
        cfg.show_battery = false;
        cfg.show_uptime = false;
        let locale = test_locale();
        let indicators = build_indicators(&cfg, &snap(50.0), &locale);
        assert!(indicators[0].segments.is_empty());
    }

    #[test]
    fn network_aggregates_by_default() {
        let mut cfg = SystemConfig::default();
        cfg.show_cpu = false;
        cfg.show_memory = false;
        cfg.show_disks = false;
        cfg.show_battery = false;
        cfg.show_uptime = false;
        let mut s = snap(0.0);
        s.networks = vec![
            NetworkRate {
                interface: "Ethernet".into(),
                upload_bps: 1000.0,
                download_bps: 2000.0,
                total_uploaded_bytes: 10,
                total_downloaded_bytes: 20,
            },
            NetworkRate {
                interface: "Wi-Fi".into(),
                upload_bps: 500.0,
                download_bps: 1500.0,
                total_uploaded_bytes: 5,
                total_downloaded_bytes: 15,
            },
            NetworkRate {
                interface: "Loopback Pseudo-Interface 1".into(),
                upload_bps: 1.0,
                download_bps: 1.0,
                total_uploaded_bytes: 1,
                total_downloaded_bytes: 1,
            },
        ];
        let locale = test_locale();
        let indicators = build_indicators(&cfg, &s, &locale);
        assert_eq!(indicators.len(), 1);
        assert!(indicators[0].name_suffix.is_none());
    }

    #[test]
    fn memory_includes_swap_suffix_when_present() {
        let cfg = SystemConfig {
            show_cpu: false,
            show_disks: false,
            show_network: false,
            show_battery: false,
            show_uptime: false,
            ..SystemConfig::default()
        };
        let mut s = snap(0.0);
        s.swap_total_bytes = 4 * 1024 * 1024 * 1024;
        s.swap_used_bytes = 1024 * 1024 * 1024;
        let locale = test_locale();
        let indicators = build_indicators(&cfg, &s, &locale);
        let mem = indicators
            .iter()
            .find(|i| i.kind == SystemIndicatorKind::Memory)
            .unwrap();
        assert!(
            mem.value_text.to_ascii_lowercase().contains("swap"),
            "expected swap in {:?}",
            mem.value_text
        );
    }

    #[test]
    fn battery_indicator_when_present() {
        let cfg = SystemConfig {
            show_cpu: false,
            show_memory: false,
            show_disks: false,
            show_network: false,
            show_uptime: false,
            ..SystemConfig::default()
        };
        let mut s = snap(0.0);
        s.battery = Some(BatteryStatus {
            percent: 42,
            charging: true,
            time_to_empty_seconds: None,
            time_to_full_seconds: Some(3600),
        });
        let locale = test_locale();
        let indicators = build_indicators(&cfg, &s, &locale);
        assert_eq!(indicators.len(), 1);
        assert_eq!(indicators[0].kind, SystemIndicatorKind::Battery);
        assert_eq!(indicators[0].icon, "system-battery-charging");
        assert!(indicators[0].value_text.contains("42%"));
    }
}
