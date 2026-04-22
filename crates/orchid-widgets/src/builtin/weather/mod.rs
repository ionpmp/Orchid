//! Weather widget — built-in.

pub mod config;
pub mod provider;
pub mod types;

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use parking_lot::RwLock;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::error::{Result as WidgetResult, WidgetError};
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::refresh::PeriodicRefresh;
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor,
    WidgetFactory,
};
use orchid_storage::{LifecycleState, WidgetSize};

pub use config::{celsius_to_fahrenheit, TemperatureUnit, WeatherConfig};
pub use provider::{
    map_wmo_code, OpenMeteoProvider, WeatherError, WeatherProvider,
};
pub use types::{CurrentWeather, DailyForecast, Location, WeatherCondition, WeatherData};

/// Stable type id for the weather widget.
pub const TYPE_ID: &str = "weather";

/// Weather widget implementation.
pub struct WeatherWidget {
    instance_id: Uuid,
    config: RwLock<WeatherConfig>,
    provider: Arc<dyn WeatherProvider>,
    data: Arc<RwLock<Option<WeatherData>>>,
    last_error: Arc<RwLock<Option<String>>>,
    refresh: PeriodicRefresh,
    bus: Arc<orchid_core::EventBus>,
}

impl std::fmt::Debug for WeatherWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WeatherWidget")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl WeatherWidget {
    /// Construct a weather widget.
    pub fn new(
        instance_id: Uuid,
        config: WeatherConfig,
        provider: Arc<dyn WeatherProvider>,
        bus: Arc<orchid_core::EventBus>,
    ) -> Self {
        let refresh_interval = std::time::Duration::from_secs(
            (config.refresh_interval_minutes as u64).max(1) * 60,
        );
        Self {
            instance_id,
            config: RwLock::new(config),
            provider,
            data: Arc::new(RwLock::new(None)),
            last_error: Arc::new(RwLock::new(None)),
            refresh: PeriodicRefresh::new(refresh_interval),
            bus,
        }
    }

    fn location(&self) -> Location {
        self.config.read().location.clone()
    }

    /// Fetch once and update shared state; publishes a snapshot-updated event.
    async fn fetch_once(
        provider: Arc<dyn WeatherProvider>,
        location: Location,
        data_slot: Arc<RwLock<Option<WeatherData>>>,
        last_error: Arc<RwLock<Option<String>>>,
        bus: Arc<orchid_core::EventBus>,
        instance_id: Uuid,
    ) {
        match provider.fetch(&location).await {
            Ok(wd) => {
                *data_slot.write() = Some(wd);
                *last_error.write() = None;
                bus.publish(
                    orchid_core::EventSource::Widget(instance_id),
                    WidgetSnapshotUpdated { instance_id },
                );
            }
            Err(e) => {
                warn!(%instance_id, error = %e, "weather fetch failed");
                *last_error.write() = Some(e.to_string());
                bus.publish(
                    orchid_core::EventSource::Widget(instance_id),
                    WidgetSnapshotUpdated { instance_id },
                );
            }
        }
    }
}

#[async_trait]
impl Widget for WeatherWidget {
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
        let location = self.location();
        let provider = self.provider.clone();
        let data_slot = self.data.clone();
        let last_error = self.last_error.clone();
        let bus = self.bus.clone();
        let instance_id = self.instance_id;

        // First tick fires immediately via tokio::interval semantics.
        self.refresh.start(move || {
            let provider = provider.clone();
            let location = location.clone();
            let data_slot = data_slot.clone();
            let last_error = last_error.clone();
            let bus = bus.clone();
            async move {
                Self::fetch_once(provider, location, data_slot, last_error, bus, instance_id).await;
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
        let config = self.config.read().clone();
        let data_opt = self.data.read().clone();
        let last_err = self.last_error.read().clone();
        let now = Utc::now();

        let (payload, title) = match (data_opt, last_err) {
            (Some(data), err) => {
                let status = if err.is_some() {
                    crate::widget::payloads::WeatherStatusTag::Stale
                } else if (now - data.fetched_at).num_seconds() < 60 * 60 {
                    crate::widget::payloads::WeatherStatusTag::Fresh
                } else {
                    crate::widget::payloads::WeatherStatusTag::Stale
                };
                let payload = render_payload(&config, &data, status, now);
                let title = data.location.name.clone();
                (payload, title)
            }
            (None, Some(_err)) => (
                crate::widget::payloads::WeatherPayload {
                    location_name: config.location.name.clone(),
                    current_temp_text: "—".into(),
                    feels_like_text: None,
                    condition_label: WeatherCondition::Unknown.default_label().into(),
                    condition_icon: WeatherCondition::Unknown.icon(),
                    humidity_text: None,
                    wind_text: None,
                    forecast: Vec::new(),
                    last_updated_text: "Error loading weather".into(),
                    status: crate::widget::payloads::WeatherStatusTag::Error,
                },
                config.location.name.clone(),
            ),
            (None, None) => (
                crate::widget::payloads::WeatherPayload {
                    location_name: config.location.name.clone(),
                    current_temp_text: "—".into(),
                    feels_like_text: None,
                    condition_label: "Loading…".into(),
                    condition_icon: WeatherCondition::Unknown.icon(),
                    humidity_text: None,
                    wind_text: None,
                    forecast: Vec::new(),
                    last_updated_text: "Loading…".into(),
                    status: crate::widget::payloads::WeatherStatusTag::Offline,
                },
                config.location.name.clone(),
            ),
        };

        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title,
            status: WidgetStatus::Ready,
            payload: WidgetPayload::Weather(payload),
        })
    }

    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        let cfg = self.config.read().clone();
        state_codec::save_state(&cfg)
    }

    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let cfg: WeatherConfig = state_codec::restore_state(bytes)?;
        debug!(%self.instance_id, "restored weather config");
        *self.config.write() = cfg;
        Ok(())
    }

    fn capabilities(&self) -> WidgetCapabilities {
        WidgetCapabilities {
            supports_resize: true,
            min_size: Some(WidgetSize::Medium),
            max_size: None,
            preferred_size: Some(WidgetSize::Medium),
            allows_grouping: true,
            keeps_state_when_unloaded: true,
            has_settings_panel: true,
        }
    }
}

fn render_payload(
    config: &WeatherConfig,
    data: &WeatherData,
    status: crate::widget::payloads::WeatherStatusTag,
    now: chrono::DateTime<Utc>,
) -> crate::widget::payloads::WeatherPayload {
    let temp_text = format_temperature(data.current.temperature_c, config.units);
    let feels_like_text = data
        .current
        .feels_like_c
        .map(|t| format!("Feels {}", format_temperature(t, config.units)));
    let humidity_text = data.current.humidity.map(|h| format!("{h}%"));
    let wind_text = data.current.wind_speed_kph.map(|kph| {
        let dir = data.current.wind_direction_deg.map(wind_direction_label).unwrap_or("");
        if dir.is_empty() {
            format!("{:.0} km/h", kph)
        } else {
            format!("{:.0} km/h {}", kph, dir)
        }
    });
    let forecast = data
        .forecast
        .iter()
        .enumerate()
        .map(|(i, d)| render_forecast_day(i, d, config))
        .collect();
    let last_updated_text = format_relative(now, data.fetched_at);

    crate::widget::payloads::WeatherPayload {
        location_name: data.location.name.clone(),
        current_temp_text: temp_text,
        feels_like_text,
        condition_label: data.current.condition.default_label().into(),
        condition_icon: data.current.condition.icon(),
        humidity_text,
        wind_text,
        forecast,
        last_updated_text,
        status,
    }
}

fn render_forecast_day(
    idx: usize,
    day: &DailyForecast,
    config: &WeatherConfig,
) -> crate::widget::payloads::WeatherForecastDay {
    let day_label = match idx {
        0 => "Today".to_string(),
        1 => "Tomorrow".to_string(),
        _ => day.date.format("%a").to_string(),
    };
    crate::widget::payloads::WeatherForecastDay {
        day_label,
        high_text: format_temperature(day.high_c, config.units),
        low_text: format_temperature(day.low_c, config.units),
        condition_icon: day.condition.icon(),
        precipitation_probability_text: day
            .precipitation_probability
            .map(|p| format!("{p}%")),
    }
}

fn format_temperature(c: f32, units: TemperatureUnit) -> String {
    match units {
        TemperatureUnit::Celsius => format!("{:.0}°C", c),
        TemperatureUnit::Fahrenheit => format!("{:.0}°F", celsius_to_fahrenheit(c)),
    }
}

fn format_relative(now: chrono::DateTime<Utc>, then: chrono::DateTime<Utc>) -> String {
    let secs = (now - then).num_seconds().max(0);
    if secs < 60 {
        "Updated just now".into()
    } else if secs < 60 * 60 {
        format!("Updated {}m ago", secs / 60)
    } else if secs < 60 * 60 * 24 {
        format!("Updated {}h ago", secs / 3600)
    } else {
        format!("Updated {}d ago", secs / 86400)
    }
}

fn wind_direction_label(deg: u16) -> &'static str {
    const DIRS: [&str; 16] = [
        "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW",
        "NW", "NNW",
    ];
    let idx = (((deg as f32 + 11.25) / 22.5) as usize) % 16;
    DIRS[idx]
}

/// Build the descriptor ready to register on a
/// [`crate::WidgetRegistry`].
#[must_use]
pub fn descriptor(http_client: reqwest::Client) -> WidgetDescriptor {
    let provider: Arc<dyn WeatherProvider> = Arc::new(OpenMeteoProvider::new(http_client));
    let factory: WidgetFactory = Arc::new(move |ctx: WidgetContext, state_bytes| {
        let cfg = match state_bytes {
            Some(bytes) => state_codec::restore_state::<WeatherConfig>(bytes)
                .unwrap_or_default(),
            None => WeatherConfig::default(),
        };
        let widget = WeatherWidget::new(ctx.instance_id, cfg, provider.clone(), ctx.bus.clone());
        Ok(Box::new(widget) as Box<dyn Widget>)
    });

    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-weather-name",
        description_key: "widget-weather-desc",
        icon_name: "weather",
        category: WidgetCategory::Information,
        default_size: WidgetSize::Medium,
        min_size: Some(WidgetSize::Medium),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: true,
        factory,
    }
}

// Used by the error match arm above; satisfies the compiler that we carried
// the variant through.
#[allow(dead_code)]
fn _assert_error_type(_: &WidgetError) {}
