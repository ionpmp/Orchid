//! Moon widget — built-in, uses local astronomical calculations, no network.

pub mod astro;
pub mod config;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use parking_lot::RwLock;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::MoonPayload;
use crate::widget::refresh::PeriodicRefresh;
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory};
use orchid_storage::{LifecycleState, WidgetSize};

pub use astro::{compute_moon, MoonData, MoonPhase};
pub use config::MoonConfig;

/// Stable type id.
pub const TYPE_ID: &str = "moon";

/// Moon widget implementation.
pub struct MoonWidget {
    instance_id: Uuid,
    config: RwLock<MoonConfig>,
    data: Arc<RwLock<Option<MoonData>>>,
    refresh: PeriodicRefresh,
    bus: Arc<orchid_core::EventBus>,
}

impl std::fmt::Debug for MoonWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MoonWidget")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl MoonWidget {
    /// Construct a moon widget with the given config.
    pub fn new(instance_id: Uuid, config: MoonConfig, bus: Arc<orchid_core::EventBus>) -> Self {
        Self {
            instance_id,
            config: RwLock::new(config),
            data: Arc::new(RwLock::new(None)),
            refresh: PeriodicRefresh::new(Duration::from_secs(10 * 60)),
            bus,
        }
    }

    fn recalculate(&self) {
        let cfg = self.config.read().clone();
        let data = compute_moon(cfg.latitude, cfg.longitude, Utc::now());
        *self.data.write() = Some(data);
    }
}

#[async_trait]
impl Widget for MoonWidget {
    fn type_id(&self) -> &'static str {
        TYPE_ID
    }

    fn instance_id(&self) -> Uuid {
        self.instance_id
    }

    async fn on_create(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.recalculate();
        Ok(())
    }

    async fn on_activate(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        let cfg = self.config.read().clone();
        let data_slot = self.data.clone();
        let bus = self.bus.clone();
        let instance_id = self.instance_id;
        self.refresh.start(move || {
            let lat = cfg.latitude;
            let lon = cfg.longitude;
            let data_slot = data_slot.clone();
            let bus = bus.clone();
            async move {
                *data_slot.write() = Some(compute_moon(lat, lon, Utc::now()));
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
        let payload = match self.data.read().clone() {
            Some(data) => render_payload(&cfg, &data),
            None => MoonPayload {
                phase_key: MoonPhase::NewMoon.ftl_key(),
                phase_icon: MoonPhase::NewMoon.icon(),
                illumination_percent: None,
                age_days: None,
                distance_km: None,
                next_full_date: None,
                next_new_date: None,
                moonrise_time: None,
                moonset_time: None,
                sunrise_time: None,
                sunset_time: None,
                libration_lat_deg: None,
                libration_lon_deg: None,
                is_loading: true,
            },
        };
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title: cfg.location_name.clone(),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::Moon(payload),
        })
    }

    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        let cfg = self.config.read().clone();
        state_codec::save_state(&cfg)
    }

    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let cfg: MoonConfig = state_codec::restore_state(bytes)?;
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

fn render_payload(cfg: &MoonConfig, data: &MoonData) -> MoonPayload {
    let fmt_time = |t: chrono::DateTime<Utc>| t.format("%H:%M").to_string();
    let fmt_date = |t: chrono::DateTime<Utc>| t.format("%b %d").to_string();
    MoonPayload {
        phase_key: data.phase_name.ftl_key(),
        phase_icon: data.phase_name.icon(),
        illumination_percent: Some(data.illumination_percent),
        age_days: Some(data.age_days),
        distance_km: Some(data.distance_km),
        next_full_date: Some(fmt_date(data.next_full_moon)),
        next_new_date: Some(fmt_date(data.next_new_moon)),
        moonrise_time: data.moonrise.map(fmt_time),
        moonset_time: data.moonset.map(fmt_time),
        sunrise_time: if cfg.show_sunrise_sunset {
            data.sunrise.map(fmt_time)
        } else {
            None
        },
        sunset_time: if cfg.show_sunrise_sunset {
            data.sunset.map(fmt_time)
        } else {
            None
        },
        libration_lat_deg: if cfg.show_libration {
            Some(data.libration_lat_deg)
        } else {
            None
        },
        libration_lon_deg: if cfg.show_libration {
            Some(data.libration_lon_deg)
        } else {
            None
        },
        is_loading: false,
    }
}

/// Descriptor ready to register on a widget registry.
#[must_use]
pub fn descriptor() -> WidgetDescriptor {
    let factory: WidgetFactory = Arc::new(|ctx: WidgetContext, state_bytes| {
        let cfg = match state_bytes {
            Some(bytes) => state_codec::restore_state::<MoonConfig>(bytes).unwrap_or_default(),
            None => MoonConfig::default(),
        };
        Ok(Box::new(MoonWidget::new(ctx.instance_id, cfg, ctx.bus.clone())) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-moon-name",
        description_key: "widget-moon-desc",
        icon_name: "moon",
        category: WidgetCategory::Astronomy,
        default_size: WidgetSize::Medium,
        min_size: Some(WidgetSize::Small),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: true,
        factory,
    }
}
