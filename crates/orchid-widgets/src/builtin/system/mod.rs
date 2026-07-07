//! System-indicators widget — CPU / Memory / Disk / Network / Battery.

pub mod config;
pub mod provider;
pub mod types;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::RwLock;
use uuid::Uuid;

use crate::error::{Result as WidgetResult, WidgetError};
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::{IndicatorStatus, SystemIndicator, SystemIndicatorKind, SystemPayload};
use crate::widget::refresh::PeriodicRefresh;
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory};
use orchid_storage::{LifecycleState, WidgetSize};

pub use config::SystemConfig;
pub use provider::SystemProvider;
pub use types::{BatteryStatus, DiskUsage, NetworkRate, SystemSnapshot};

/// Stable type id.
pub const TYPE_ID: &str = "system";

/// System widget.
pub struct SystemWidget {
    instance_id: Uuid,
    config: RwLock<SystemConfig>,
    provider: Arc<SystemProvider>,
    snapshot: Arc<RwLock<Option<SystemSnapshot>>>,
    refresh: PeriodicRefresh,
    bus: Arc<orchid_core::EventBus>,
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
    pub fn new(instance_id: Uuid, cfg: SystemConfig, bus: Arc<orchid_core::EventBus>) -> Self {
        let interval = Duration::from_secs(cfg.refresh_interval_seconds.max(1) as u64);
        Self {
            instance_id,
            config: RwLock::new(cfg),
            provider: Arc::new(SystemProvider::new()),
            snapshot: Arc::new(RwLock::new(None)),
            refresh: PeriodicRefresh::new(interval),
            bus,
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
        let provider = self.provider.clone();
        let snap = tokio::task::spawn_blocking(move || provider.refresh())
            .await
            .map_err(|e| {
                WidgetError::CreationFailed(format!("system metrics initial refresh: {e}"))
            })?;
        *self.snapshot.write() = Some(snap);
        Ok(())
    }
    async fn on_activate(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        let provider = self.provider.clone();
        let snap_slot = self.snapshot.clone();
        let bus = self.bus.clone();
        let instance_id = self.instance_id;
        self.refresh.start(move || {
            let provider = provider.clone();
            let snap_slot = snap_slot.clone();
            let bus = bus.clone();
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
                bus.publish(
                    orchid_core::EventSource::Widget(instance_id),
                    WidgetSnapshotUpdated { instance_id },
                );
            }
        });
        Ok(())
    }
    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.refresh.stop();
        Ok(())
    }
    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.refresh.stop();
        Ok(())
    }
    async fn on_close(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.refresh.stop();
        Ok(())
    }
    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }
    fn snapshot(&self) -> Option<WidgetSnapshot> {
        let cfg = self.config.read().clone();
        let indicators = match self.snapshot.read().clone() {
            Some(snap) => build_indicators(&cfg, &snap),
            None => vec![SystemIndicator {
                kind: SystemIndicatorKind::Cpu,
                name_suffix: None,
                value_text: String::new(),
                network_up: None,
                network_down: None,
                percent: None,
                icon: "system-cpu",
                status: IndicatorStatus::Normal,
            }],
        };
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title: "System".into(),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::SystemIndicators(SystemPayload {
                indicators,
                is_loading: self.snapshot.read().is_none(),
            }),
        })
    }
    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        state_codec::save_state(&*self.config.read())
    }
    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let cfg: SystemConfig = state_codec::restore_state(bytes)?;
        *self.config.write() = cfg;
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

fn build_indicators(cfg: &SystemConfig, snap: &SystemSnapshot) -> Vec<SystemIndicator> {
    let mut out = Vec::new();

    if cfg.show_cpu {
        out.push(SystemIndicator {
            kind: SystemIndicatorKind::Cpu,
            name_suffix: None,
            value_text: format!("{:.0}%", snap.cpu_total_percent),
            network_up: None,
            network_down: None,
            percent: Some(snap.cpu_total_percent),
            icon: "system-cpu",
            status: bucket_pct(snap.cpu_total_percent, 75.0, 90.0),
        });
    }

    if cfg.show_memory && snap.memory_total_bytes > 0 {
        let pct = (snap.memory_used_bytes as f32 / snap.memory_total_bytes as f32) * 100.0;
        out.push(SystemIndicator {
            kind: SystemIndicatorKind::Memory,
            name_suffix: None,
            value_text: format!(
                "{} / {}",
                format_bytes(snap.memory_used_bytes),
                format_bytes(snap.memory_total_bytes)
            ),
            network_up: None,
            network_down: None,
            percent: Some(pct),
            icon: "system-memory",
            status: bucket_pct(pct, 80.0, 95.0),
        });
    }

    if cfg.show_disks {
        let selector = &cfg.disks;
        for d in &snap.disks {
            if !selector.is_empty() && !selector.iter().any(|m| m == &d.mount) {
                continue;
            }
            if d.total_bytes == 0 {
                continue;
            }
            let pct = (d.used_bytes as f32 / d.total_bytes as f32) * 100.0;
            out.push(SystemIndicator {
                kind: SystemIndicatorKind::Disk,
                name_suffix: Some(d.mount.clone()),
                value_text: format!(
                    "{} / {}",
                    format_bytes(d.used_bytes),
                    format_bytes(d.total_bytes)
                ),
                network_up: None,
                network_down: None,
                percent: Some(pct),
                icon: "system-disk",
                status: bucket_pct(pct, 85.0, 95.0),
            });
        }
    }

    if cfg.show_network {
        let selector = &cfg.network_interfaces;
        for n in &snap.networks {
            if !selector.is_empty() && !selector.iter().any(|name| name == &n.interface) {
                continue;
            }
            out.push(SystemIndicator {
                kind: SystemIndicatorKind::Network,
                name_suffix: Some(n.interface.clone()),
                value_text: String::new(),
                network_up: Some(format_bytes(n.upload_bps as u64)),
                network_down: Some(format_bytes(n.download_bps as u64)),
                percent: None,
                icon: "system-network",
                status: IndicatorStatus::Normal,
            });
        }
    }

    if cfg.show_battery {
        if let Some(b) = &snap.battery {
            let pct = b.percent as f32;
            out.push(SystemIndicator {
                kind: SystemIndicatorKind::Battery,
                name_suffix: None,
                value_text: format!("{}%", b.percent),
                network_up: None,
                network_down: None,
                percent: Some(pct),
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
            value_text: format_duration_secs(snap.uptime_seconds),
            network_up: None,
            network_down: None,
            percent: None,
            icon: "system-uptime",
            status: IndicatorStatus::Normal,
        });
    }

    out
}

fn format_bytes(b: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;
    let f = b as f64;
    if f >= TB {
        format!("{:.1} TB", f / TB)
    } else if f >= GB {
        format!("{:.1} GB", f / GB)
    } else if f >= MB {
        format!("{:.1} MB", f / MB)
    } else if f >= KB {
        format!("{:.0} KB", f / KB)
    } else {
        format!("{} B", b)
    }
}

fn format_duration_secs(s: u64) -> String {
    let days = s / 86400;
    let hours = (s % 86400) / 3600;
    let minutes = (s % 3600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// Descriptor ready to register on a widget registry.
#[must_use]
pub fn descriptor() -> WidgetDescriptor {
    let factory: WidgetFactory = Arc::new(|ctx: WidgetContext, state_bytes| {
        let cfg = match state_bytes {
            Some(bytes) => state_codec::restore_state::<SystemConfig>(bytes).unwrap_or_default(),
            None => SystemConfig::default(),
        };
        Ok(Box::new(SystemWidget::new(ctx.instance_id, cfg, ctx.bus.clone())) as Box<dyn Widget>)
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
            cpu_per_core: Vec::new(),
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

    #[test]
    fn cpu_thresholds_bucket_correctly() {
        let cfg = SystemConfig::default();
        let indicators = build_indicators(&cfg, &snap(20.0));
        let cpu = indicators.iter().find(|i| i.kind == SystemIndicatorKind::Cpu).unwrap();
        assert_eq!(cpu.status, IndicatorStatus::Normal);

        let indicators = build_indicators(&cfg, &snap(80.0));
        assert_eq!(
            indicators.iter().find(|i| i.kind == SystemIndicatorKind::Cpu).unwrap().status,
            IndicatorStatus::Warning
        );

        let indicators = build_indicators(&cfg, &snap(95.0));
        assert_eq!(
            indicators.iter().find(|i| i.kind == SystemIndicatorKind::Cpu).unwrap().status,
            IndicatorStatus::Critical
        );
    }

    #[test]
    fn format_bytes_scales() {
        assert_eq!(format_bytes(512), "512 B");
        assert!(format_bytes(1024 * 1024).ends_with("MB"));
        assert!(format_bytes(1024_u64.pow(3)).ends_with("GB"));
    }
}
