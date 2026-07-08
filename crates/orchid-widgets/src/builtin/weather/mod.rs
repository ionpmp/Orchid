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
    orchid_config: Arc<RwLock<orchid_storage::OrchidConfig>>,
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
        orchid_config: Arc<RwLock<orchid_storage::OrchidConfig>>,
    ) -> Self {
        let refresh_interval = std::time::Duration::from_secs(
            (config.refresh_interval_minutes as u64).max(1) * 60,
        );
        Self {
            instance_id,
            config: RwLock::new(config),
            orchid_config,
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
        let location = self.location();
        let provider = self.provider.clone();
        let data_slot = self.data.clone();
        let last_error = self.last_error.clone();
        let bus = self.bus.clone();
        let instance_id = self.instance_id;
        Self::fetch_once(
            provider,
            location,
            data_slot,
            last_error,
            bus,
            instance_id,
        )
        .await;
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
                let locale = self.orchid_config.read().locale.clone();
                let payload = render_payload(&config, &data, status, &locale);
                let title = data.location.name.clone();
                (payload, title)
            }
            (None, Some(_err)) => (
                crate::widget::payloads::WeatherPayload {
                    location_name: config.location.name.clone(),
                    current_temp_text: "—".into(),
                    feels_like_temp: None,
                    condition_key: WeatherCondition::Unknown.ftl_key(),
                    condition_icon: WeatherCondition::Unknown.icon(),
                    humidity_percent: None,
                    wind_speed_kph: None,
                    wind_direction: None,
                    forecast: Vec::new(),
                    fetched_at: None,
                    is_loading: false,
                    status: crate::widget::payloads::WeatherStatusTag::Error,
                },
                config.location.name.clone(),
            ),
            (None, None) => (
                crate::widget::payloads::WeatherPayload {
                    location_name: config.location.name.clone(),
                    current_temp_text: "—".into(),
                    feels_like_temp: None,
                    condition_key: WeatherCondition::Unknown.ftl_key(),
                    condition_icon: WeatherCondition::Unknown.icon(),
                    humidity_percent: None,
                    wind_speed_kph: None,
                    wind_direction: None,
                    forecast: Vec::new(),
                    fetched_at: None,
                    is_loading: true,
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
    locale: &orchid_storage::LocaleConfig,
) -> crate::widget::payloads::WeatherPayload {
    let temp_text = format_temperature(data.current.temperature_c, config.units);
    let feels_like_temp = data
        .current
        .feels_like_c
        .map(|t| format_temperature(t, config.units));
    let forecast = data
        .forecast
        .iter()
        .enumerate()
        .map(|(i, d)| render_forecast_day(i, d, config, locale))
        .collect();

    crate::widget::payloads::WeatherPayload {
        location_name: data.location.name.clone(),
        current_temp_text: temp_text,
        feels_like_temp,
        condition_key: data.current.condition.ftl_key(),
        condition_icon: data.current.condition.icon(),
        humidity_percent: data.current.humidity,
        wind_speed_kph: data.current.wind_speed_kph,
        wind_direction: data
            .current
            .wind_direction_deg
            .map(|deg| wind_direction_label(deg).to_string()),
        forecast,
        fetched_at: Some(data.fetched_at),
        is_loading: false,
        status,
    }
}

fn render_forecast_day(
    idx: usize,
    day: &DailyForecast,
    config: &WeatherConfig,
    locale: &orchid_storage::LocaleConfig,
) -> crate::widget::payloads::WeatherForecastDay {
    crate::widget::payloads::WeatherForecastDay {
        day_index: idx as u8,
        weekday_label: if idx >= 2 {
            Some(locale.format_weekday(day.date))
        } else {
            None
        },
        high_text: format_temperature(day.high_c, config.units),
        low_text: format_temperature(day.low_c, config.units),
        condition_icon: day.condition.icon(),
        precipitation_probability: day.precipitation_probability,
    }
}

fn format_temperature(c: f32, units: TemperatureUnit) -> String {
    match units {
        TemperatureUnit::Celsius => format!("{:.0}°C", c),
        TemperatureUnit::Fahrenheit => format!("{:.0}°F", celsius_to_fahrenheit(c)),
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
        let widget = WeatherWidget::new(
            ctx.instance_id,
            cfg,
            provider.clone(),
            ctx.bus.clone(),
            ctx.config.clone(),
        );
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
