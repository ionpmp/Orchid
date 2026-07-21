//! Jyotish widget — Vedic panchanga from local astronomical calculations.

pub mod astro;
pub mod config;

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use dashmap::DashMap;
use parking_lot::RwLock;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::{JyotishPayload, JyotishPlanetRow};
use crate::widget::refresh::PeriodicRefresh;
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};
use orchid_storage::{LifecycleState, LocaleConfig, WidgetSize};

pub use astro::{compute_jyotish, JyotishData};
pub use config::{AyanamsaSystem, JyotishConfig};

/// Stable type id.
pub const TYPE_ID: &str = "jyotish";

static JYOTISH_LIVE: LazyLock<DashMap<Uuid, Arc<JyotishHandle>>> = LazyLock::new(DashMap::new);

struct JyotishHandle {
    instance_id: Uuid,
    config: Arc<RwLock<JyotishConfig>>,
    data: Arc<RwLock<Option<JyotishData>>>,
    bus: Arc<orchid_core::EventBus>,
}

impl JyotishHandle {
    fn publish(&self) {
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    fn recalculate(&self) {
        let mut cfg = self.config.read().clone();
        cfg.normalize();
        let at = Utc::now() + ChronoDuration::days(i64::from(cfg.day_offset));
        let data = compute_jyotish(cfg.latitude, cfg.longitude, at, cfg.ayanamsa);
        *self.data.write() = Some(data);
    }
}

/// Snapshot the live config for the settings dialog.
#[must_use]
pub fn current_config(instance_id: Uuid) -> Option<JyotishConfig> {
    JYOTISH_LIVE
        .get(&instance_id)
        .map(|h| h.config.read().clone())
}

/// Apply a settings-dialog mutation to the live config.
pub fn update_config(instance_id: Uuid, mutate: impl FnOnce(&mut JyotishConfig)) {
    let Some(h) = JYOTISH_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        mutate(&mut cfg);
        cfg.normalize();
    }
    h.recalculate();
    h.publish();
}

/// Shift the viewed day by `delta` (−1 / +1) and refresh.
pub fn shift_day(instance_id: Uuid, delta: i32) {
    update_config(instance_id, |cfg| {
        cfg.day_offset = cfg.day_offset.saturating_add(delta);
    });
}

/// Jump back to today.
pub fn go_today(instance_id: Uuid) {
    update_config(instance_id, |cfg| {
        cfg.day_offset = 0;
    });
}

/// Jyotish widget implementation.
pub struct JyotishWidget {
    instance_id: Uuid,
    handle: Arc<JyotishHandle>,
    orchid_config: Arc<RwLock<orchid_storage::OrchidConfig>>,
    refresh: PeriodicRefresh,
}

impl std::fmt::Debug for JyotishWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JyotishWidget")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl JyotishWidget {
    /// Construct with config.
    pub fn new(
        instance_id: Uuid,
        mut config: JyotishConfig,
        bus: Arc<orchid_core::EventBus>,
        orchid_config: Arc<RwLock<orchid_storage::OrchidConfig>>,
    ) -> Self {
        config.normalize();
        let handle = Arc::new(JyotishHandle {
            instance_id,
            config: Arc::new(RwLock::new(config)),
            data: Arc::new(RwLock::new(None)),
            bus,
        });
        JYOTISH_LIVE.insert(instance_id, Arc::clone(&handle));
        Self {
            instance_id,
            handle,
            orchid_config,
            refresh: PeriodicRefresh::new(Duration::from_secs(10 * 60)),
        }
    }

    fn recalculate(&self) {
        self.handle.recalculate();
    }
}

#[async_trait]
impl Widget for JyotishWidget {
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
        let handle = Arc::clone(&self.handle);
        self.refresh.start(move || {
            let handle = Arc::clone(&handle);
            async move {
                handle.recalculate();
                handle.publish();
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
        JYOTISH_LIVE.remove(&self.instance_id);
        Ok(())
    }

    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }

    fn snapshot(&self) -> Option<WidgetSnapshot> {
        let cfg = self.handle.config.read().clone();
        let locale = self.orchid_config.read().locale.clone();
        let payload = match self.handle.data.read().clone() {
            Some(data) => render_payload(&cfg, &data, &locale),
            None => loading_payload(&cfg),
        };
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title: cfg.location_name.clone(),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::Jyotish(payload),
        })
    }

    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        let cfg = self.handle.config.read().clone();
        state_codec::save_state(&cfg)
    }

    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let mut cfg: JyotishConfig = state_codec::restore_state(bytes)?;
        cfg.normalize();
        *self.handle.config.write() = cfg;
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

fn loading_payload(cfg: &JyotishConfig) -> JyotishPayload {
    JyotishPayload {
        date_text: String::new(),
        location_name: cfg.location_name.clone(),
        ayanamsa_key: cfg.ayanamsa.ftl_key(),
        ayanamsa_deg_text: String::new(),
        day_offset: cfg.day_offset,
        is_today: cfg.day_offset == 0,
        tithi_key: "jyotish-tithi-pratipada",
        paksha_key: "jyotish-paksha-shukla",
        tithi_end_text: None,
        nakshatra_key: "jyotish-nakshatra-ashwini",
        pada: 1,
        nakshatra_end_text: None,
        yoga_key: "jyotish-yoga-vishkambha",
        karana_key: "jyotish-karana-bava",
        vara_key: "jyotish-vara-ravi",
        sunrise_time: None,
        sunset_time: None,
        planets: Vec::new(),
        show_planets: cfg.show_planets,
        is_loading: true,
    }
}

fn render_payload(cfg: &JyotishConfig, data: &JyotishData, locale: &LocaleConfig) -> JyotishPayload {
    let at = data.calculated_at;
    let date_text = locale.format_date(at);
    let fmt_time = |t: chrono::DateTime<Utc>| locale.format_time(t);

    let planets = if cfg.show_planets {
        data.planets
            .iter()
            .map(|p| JyotishPlanetRow {
                graha_key: p.graha.ftl_key(),
                rashi_key: astro::rashi_ftl_key(p.rashi_index),
                degree_text: astro::format_degree_in_rashi(p.degree_in_rashi),
                is_retrograde: p.is_retrograde,
            })
            .collect()
    } else {
        Vec::new()
    };

    JyotishPayload {
        date_text,
        location_name: cfg.location_name.clone(),
        ayanamsa_key: cfg.ayanamsa.ftl_key(),
        ayanamsa_deg_text: format!("{:.2}°", data.ayanamsa_deg),
        day_offset: cfg.day_offset,
        is_today: cfg.day_offset == 0,
        tithi_key: astro::tithi_ftl_key(data.tithi_index),
        paksha_key: astro::paksha_ftl_key(data.paksha_shukla),
        tithi_end_text: None,
        nakshatra_key: astro::nakshatra_ftl_key(data.nakshatra_index),
        pada: data.pada,
        nakshatra_end_text: None,
        yoga_key: astro::yoga_ftl_key(data.yoga_index),
        karana_key: astro::karana_ftl_key(data.karana_index),
        vara_key: astro::vara_ftl_key(data.vara_index),
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
        planets,
        show_planets: cfg.show_planets,
        is_loading: false,
    }
}

/// Descriptor ready to register on a widget registry.
#[must_use]
pub fn descriptor() -> WidgetDescriptor {
    let factory: WidgetFactory = Arc::new(|ctx: WidgetContext, state_bytes| {
        let cfg = match state_bytes {
            Some(bytes) => state_codec::restore_state::<JyotishConfig>(bytes).unwrap_or_default(),
            None => JyotishConfig::default(),
        };
        Ok(Box::new(JyotishWidget::new(
            ctx.instance_id,
            cfg,
            ctx.bus.clone(),
            ctx.config.clone(),
        )) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-jyotish-name",
        description_key: "widget-jyotish-desc",
        icon_name: "jyotish",
        category: WidgetCategory::Astronomy,
        default_size: WidgetSize::Medium,
        min_size: Some(WidgetSize::Small),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: true,
        factory,
    }
}
