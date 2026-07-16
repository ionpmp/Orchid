//! Weather widget — built-in.

pub mod config;
pub mod provider;
pub mod types;

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use parking_lot::RwLock;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};
use orchid_storage::{LifecycleState, WidgetSize};

pub use config::{celsius_to_fahrenheit, decode_config, TemperatureUnit, WeatherConfig};
pub use provider::{
    map_wmo_code, GeocodingHit, OpenMeteoProvider, WeatherError, WeatherProvider, FORECAST_DAYS,
};
pub use types::{CurrentWeather, DailyForecast, Location, WeatherCondition, WeatherData};

/// Stable type id for the weather widget.
pub const TYPE_ID: &str = "weather";

fn job_key(instance_id: Uuid) -> String {
    format!("weather:{instance_id}")
}

/// Live weather handles for UI callbacks (city picker / switch).
static WEATHER_LIVE: LazyLock<DashMap<Uuid, Arc<WeatherHandle>>> = LazyLock::new(DashMap::new);

/// Open / close the city picker overlay.
pub fn set_picker_open(instance_id: Uuid, open: bool) {
    if let Some(h) = WEATHER_LIVE.get(&instance_id) {
        h.set_picker_open(open);
    }
}

/// Select which configured city is shown.
pub fn select_city(instance_id: Uuid, index: usize) {
    if let Some(h) = WEATHER_LIVE.get(&instance_id) {
        h.select_city(index);
    }
}

/// Remove a configured city (keeps at least one).
pub fn remove_city(instance_id: Uuid, index: usize) {
    if let Some(h) = WEATHER_LIVE.get(&instance_id) {
        h.remove_city(index);
    }
}

/// Update the city-search query and kick off geocoding.
pub fn search_cities(instance_id: Uuid, query: String) {
    if let Some(h) = WEATHER_LIVE.get(&instance_id) {
        h.search_cities(query);
    }
}

/// Add a city from a geocoding hit and make it active.
pub fn add_city(
    instance_id: Uuid,
    name: String,
    latitude: f64,
    longitude: f64,
    timezone: String,
) {
    if let Some(h) = WEATHER_LIVE.get(&instance_id) {
        h.add_city(Location {
            name,
            latitude,
            longitude,
            timezone: if timezone.is_empty() {
                None
            } else {
                Some(timezone)
            },
        });
    }
}

/// Highlight a forecast day in the strip and show its detail line.
pub fn select_day(instance_id: Uuid, index: usize) {
    if let Some(h) = WEATHER_LIVE.get(&instance_id) {
        h.select_day(index);
    }
}

/// Snapshot the live weather config for the settings dialog.
#[must_use]
pub fn current_config(instance_id: Uuid) -> Option<WeatherConfig> {
    WEATHER_LIVE
        .get(&instance_id)
        .map(|h| h.config.read().clone())
}

/// Apply a settings-dialog mutation to the live weather config.
pub fn update_config(instance_id: Uuid, mutate: impl FnOnce(&mut WeatherConfig)) {
    let Some(h) = WEATHER_LIVE.get(&instance_id) else {
        return;
    };
    let before_interval = h.config.read().refresh_interval_minutes;
    {
        let mut cfg = h.config.write();
        mutate(&mut cfg);
        cfg.normalize();
    }
    let after_interval = h.config.read().refresh_interval_minutes;
    h.publish();
    if before_interval != after_interval {
        h.schedule_job();
    }
}

/// Coords cache key (centi-degrees) so float noise does not duplicate entries.
fn location_key(loc: &Location) -> (i32, i32) {
    (
        (loc.latitude * 100.0).round() as i32,
        (loc.longitude * 100.0).round() as i32,
    )
}

#[derive(Clone, Default)]
struct UiState {
    picker_open: bool,
    search_query: String,
    search_results: Vec<GeocodingHit>,
    search_busy: bool,
    search_generation: u64,
    /// Forecast day shown in the detail row (clamped on snapshot).
    selected_day_index: usize,
}

struct WeatherHandle {
    instance_id: Uuid,
    config: Arc<RwLock<WeatherConfig>>,
    provider: Arc<dyn WeatherProvider>,
    cache: Arc<RwLock<HashMap<(i32, i32), WeatherData>>>,
    last_error: Arc<RwLock<Option<String>>>,
    is_fetching: Arc<RwLock<bool>>,
    ui: Arc<RwLock<UiState>>,
    bus: Arc<orchid_core::EventBus>,
    orchid_config: Arc<RwLock<orchid_storage::OrchidConfig>>,
    jobs: Arc<orchid_core::BackgroundJobQueue>,
}

impl std::fmt::Debug for WeatherHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WeatherHandle")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl WeatherHandle {
    fn publish(&self) {
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    fn refresh_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(
            (self.config.read().refresh_interval_minutes as u64).max(1) * 60,
        )
    }

    fn schedule_job(self: &Arc<Self>) {
        let handle = Arc::clone(self);
        let interval = self.refresh_interval();
        self.jobs
            .schedule(job_key(self.instance_id), interval, move || {
                let provider = handle.provider.clone();
                let config = handle.config.clone();
                let cache = handle.cache.clone();
                let last_error = handle.last_error.clone();
                let is_fetching = handle.is_fetching.clone();
                let bus = handle.bus.clone();
                let instance_id = handle.instance_id;
                async move {
                    fetch_all_locations(
                        provider, config, cache, last_error, is_fetching, bus, instance_id,
                    )
                    .await;
                }
            });
    }

    fn cancel_job(&self) {
        self.jobs.cancel(&job_key(self.instance_id));
    }

    fn set_picker_open(&self, open: bool) {
        let mut ui = self.ui.write();
        ui.picker_open = open;
        if !open {
            ui.search_query.clear();
            ui.search_results.clear();
            ui.search_busy = false;
        }
        drop(ui);
        self.publish();
    }

    fn select_city(&self, index: usize) {
        {
            let mut cfg = self.config.write();
            if index < cfg.locations.len() {
                cfg.active_index = index;
            }
        }
        self.ui.write().selected_day_index = 0;
        self.publish();
        self.spawn_fetch_all();
    }

    fn select_day(&self, index: usize) {
        self.ui.write().selected_day_index = index;
        self.publish();
    }

    fn remove_city(&self, index: usize) {
        {
            let mut cfg = self.config.write();
            if cfg.locations.len() <= 1 || index >= cfg.locations.len() {
                return;
            }
            let removed = cfg.locations.remove(index);
            self.cache.write().remove(&location_key(&removed));
            if cfg.active_index > index {
                cfg.active_index -= 1;
            } else if cfg.active_index >= cfg.locations.len() {
                cfg.active_index = cfg.locations.len().saturating_sub(1);
            }
            cfg.normalize();
        }
        self.publish();
        self.spawn_fetch_all();
    }

    fn add_city(&self, location: Location) {
        {
            let mut cfg = self.config.write();
            if let Some(existing) = cfg
                .locations
                .iter()
                .position(|l| location_key(l) == location_key(&location))
            {
                cfg.active_index = existing;
            } else {
                cfg.locations.push(location);
                cfg.active_index = cfg.locations.len() - 1;
            }
            cfg.normalize();
        }
        {
            let mut ui = self.ui.write();
            ui.picker_open = false;
            ui.search_query.clear();
            ui.search_results.clear();
            ui.search_busy = false;
            ui.selected_day_index = 0;
        }
        self.publish();
        self.spawn_fetch_all();
    }

    fn search_cities(&self, query: String) {
        let generation = {
            let mut ui = self.ui.write();
            ui.search_query = query.clone();
            ui.search_generation = ui.search_generation.wrapping_add(1);
            ui.search_busy = !query.trim().is_empty();
            if query.trim().is_empty() {
                ui.search_results.clear();
                ui.search_busy = false;
            }
            ui.search_generation
        };
        self.publish();
        if query.trim().is_empty() {
            return;
        }

        let provider = self.provider.clone();
        let ui = self.ui.clone();
        let bus = self.bus.clone();
        let instance_id = self.instance_id;
        tokio::spawn(async move {
            // Debounce keystrokes so we do not hammer the geocoding API.
            tokio::time::sleep(std::time::Duration::from_millis(280)).await;
            if ui.read().search_generation != generation {
                return;
            }
            let result = provider.search_cities(&query).await;
            let mut slot = ui.write();
            if slot.search_generation != generation {
                return;
            }
            slot.search_busy = false;
            match result {
                Ok(hits) => slot.search_results = hits,
                Err(e) => {
                    warn!(%instance_id, error = %e, "weather geocoding failed");
                    slot.search_results.clear();
                }
            }
            drop(slot);
            bus.publish(
                orchid_core::EventSource::Widget(instance_id),
                WidgetSnapshotUpdated { instance_id },
            );
        });
    }

    fn spawn_fetch_all(&self) {
        let provider = self.provider.clone();
        let config = self.config.clone();
        let cache = self.cache.clone();
        let last_error = self.last_error.clone();
        let is_fetching = self.is_fetching.clone();
        let bus = self.bus.clone();
        let instance_id = self.instance_id;
        tokio::spawn(async move {
            fetch_all_locations(
                provider,
                config,
                cache,
                last_error,
                is_fetching,
                bus,
                instance_id,
            )
            .await;
        });
    }
}

/// Weather widget implementation.
pub struct WeatherWidget {
    handle: Arc<WeatherHandle>,
}

impl std::fmt::Debug for WeatherWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WeatherWidget")
            .field("instance_id", &self.handle.instance_id)
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
        jobs: Arc<orchid_core::BackgroundJobQueue>,
    ) -> Self {
        let mut config = config;
        config.normalize();
        let handle = Arc::new(WeatherHandle {
            instance_id,
            config: Arc::new(RwLock::new(config)),
            provider,
            cache: Arc::new(RwLock::new(HashMap::new())),
            last_error: Arc::new(RwLock::new(None)),
            is_fetching: Arc::new(RwLock::new(false)),
            ui: Arc::new(RwLock::new(UiState::default())),
            bus,
            orchid_config,
            jobs,
        });
        WEATHER_LIVE.insert(instance_id, Arc::clone(&handle));
        Self { handle }
    }
}

impl Drop for WeatherWidget {
    fn drop(&mut self) {
        self.handle.cancel_job();
        WEATHER_LIVE.remove(&self.handle.instance_id);
    }
}

async fn fetch_all_locations(
    provider: Arc<dyn WeatherProvider>,
    config: Arc<RwLock<WeatherConfig>>,
    cache: Arc<RwLock<HashMap<(i32, i32), WeatherData>>>,
    last_error: Arc<RwLock<Option<String>>>,
    is_fetching: Arc<RwLock<bool>>,
    bus: Arc<orchid_core::EventBus>,
    instance_id: Uuid,
) {
    let locations = config.read().locations.clone();
    *is_fetching.write() = true;
    bus.publish(
        orchid_core::EventSource::Widget(instance_id),
        WidgetSnapshotUpdated { instance_id },
    );

    let mut first_err: Option<String> = None;
    let mut any_ok = false;
    for loc in locations {
        match provider.fetch(&loc).await {
            Ok(wd) => {
                cache.write().insert(location_key(&loc), wd);
                any_ok = true;
            }
            Err(e) => {
                warn!(%instance_id, city = %loc.name, error = %e, "weather fetch failed");
                if first_err.is_none() {
                    first_err = Some(e.to_string());
                }
            }
        }
    }
    *last_error.write() = if any_ok { None } else { first_err };
    *is_fetching.write() = false;
    bus.publish(
        orchid_core::EventSource::Widget(instance_id),
        WidgetSnapshotUpdated { instance_id },
    );
}

#[async_trait]
impl Widget for WeatherWidget {
    fn type_id(&self) -> &'static str {
        TYPE_ID
    }

    fn instance_id(&self) -> Uuid {
        self.handle.instance_id
    }

    async fn on_create(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        // Always-on fetch: first tick runs immediately via BackgroundJobQueue.
        self.handle.schedule_job();
        Ok(())
    }

    async fn on_activate(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_close(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.handle.cancel_job();
        Ok(())
    }

    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }

    fn snapshot(&self) -> Option<WidgetSnapshot> {
        let config = self.handle.config.read().clone();
        let ui = self.handle.ui.read().clone();
        let active = config.active_location().clone();
        let data_opt = self
            .handle
            .cache
            .read()
            .get(&location_key(&active))
            .cloned();
        let last_err = self.handle.last_error.read().clone();
        let fetching = *self.handle.is_fetching.read();
        let now = Utc::now();

        let cities: Vec<_> = config
            .locations
            .iter()
            .enumerate()
            .map(|(i, loc)| crate::widget::payloads::WeatherCityEntry {
                name: loc.name.clone(),
                active: i == config.active_index,
            })
            .collect();

        let search_results: Vec<_> = ui
            .search_results
            .iter()
            .map(|h| crate::widget::payloads::WeatherSearchHit {
                name: h.name.clone(),
                detail: h.detail.clone(),
                latitude: h.latitude,
                longitude: h.longitude,
                timezone: h.timezone.clone().unwrap_or_default(),
            })
            .collect();

        let (payload, title) = match (data_opt, last_err) {
            (Some(data), err) => {
                let status = if err.is_some() {
                    crate::widget::payloads::WeatherStatusTag::Stale
                } else if (now - data.fetched_at).num_seconds() < 60 * 60 {
                    crate::widget::payloads::WeatherStatusTag::Fresh
                } else {
                    crate::widget::payloads::WeatherStatusTag::Stale
                };
                let locale = self.handle.orchid_config.read().locale.clone();
                let mut payload =
                    render_payload(&config, &data, status, &locale, ui.selected_day_index);
                payload.cities = cities;
                payload.active_city_index = config.active_index;
                payload.picker_open = ui.picker_open;
                payload.search_query = ui.search_query.clone();
                payload.search_results = search_results;
                payload.search_busy = ui.search_busy;
                let title = data.location.name.clone();
                (payload, title)
            }
            (None, Some(_err)) => (
                empty_payload(
                    &config,
                    cities,
                    search_results,
                    &ui,
                    fetching,
                    crate::widget::payloads::WeatherStatusTag::Error,
                ),
                active.name.clone(),
            ),
            (None, None) => (
                empty_payload(
                    &config,
                    cities,
                    search_results,
                    &ui,
                    true,
                    crate::widget::payloads::WeatherStatusTag::Offline,
                ),
                active.name.clone(),
            ),
        };

        Some(WidgetSnapshot {
            instance_id: self.handle.instance_id,
            widget_type: TYPE_ID,
            title,
            status: WidgetStatus::Ready,
            payload: WidgetPayload::Weather(payload),
        })
    }

    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        let cfg = self.handle.config.read().clone();
        state_codec::save_state(&cfg)
    }

    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let cfg = decode_config(bytes)?;
        debug!(%self.handle.instance_id, "restored weather config");
        *self.handle.config.write() = cfg;
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

fn empty_payload(
    config: &WeatherConfig,
    cities: Vec<crate::widget::payloads::WeatherCityEntry>,
    search_results: Vec<crate::widget::payloads::WeatherSearchHit>,
    ui: &UiState,
    is_loading: bool,
    status: crate::widget::payloads::WeatherStatusTag,
) -> crate::widget::payloads::WeatherPayload {
    crate::widget::payloads::WeatherPayload {
        location_name: config.active_location().name.clone(),
        cities,
        active_city_index: config.active_index,
        picker_open: ui.picker_open,
        search_query: ui.search_query.clone(),
        search_results,
        search_busy: ui.search_busy,
        selected_day_index: ui.selected_day_index,
        current_temp_text: "—".into(),
        feels_like_temp: None,
        condition_key: WeatherCondition::Unknown.ftl_key(),
        condition_icon: WeatherCondition::Unknown.icon(),
        humidity_percent: None,
        wind_speed_kph: None,
        wind_direction: None,
        forecast: Vec::new(),
        fetched_at: None,
        is_loading,
        status,
    }
}

fn render_payload(
    config: &WeatherConfig,
    data: &WeatherData,
    status: crate::widget::payloads::WeatherStatusTag,
    locale: &orchid_storage::LocaleConfig,
    selected_day_index: usize,
) -> crate::widget::payloads::WeatherPayload {
    let temp_text = format_temperature(data.current.temperature_c, config.units);
    let feels_like_temp = data
        .current
        .feels_like_c
        .map(|t| format_temperature(t, config.units));
    let selected = if data.forecast.is_empty() {
        0
    } else {
        selected_day_index.min(data.forecast.len() - 1)
    };
    let forecast = data
        .forecast
        .iter()
        .enumerate()
        .map(|(i, d)| render_forecast_day(i, d, config, locale, i == selected))
        .collect();

    crate::widget::payloads::WeatherPayload {
        location_name: data.location.name.clone(),
        cities: Vec::new(),
        active_city_index: config.active_index,
        picker_open: false,
        search_query: String::new(),
        search_results: Vec::new(),
        search_busy: false,
        selected_day_index: selected,
        current_temp_text: temp_text,
        feels_like_temp,
        condition_key: data.current.condition.ftl_key(),
        condition_icon: data.current.condition.icon(),
        humidity_percent: data.current.humidity,
        wind_speed_kph: data.current.wind_speed_kph,
        wind_direction: data
            .current
            .wind_direction_deg
            .map(|deg| wind_direction_ftl_key(deg).to_string()),
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
    selected: bool,
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
        selected,
        sunrise_text: day.sunrise.map(|t| locale.format_time(t)),
        sunset_text: day.sunset.map(|t| locale.format_time(t)),
    }
}

fn format_temperature(c: f32, units: TemperatureUnit) -> String {
    match units {
        TemperatureUnit::Celsius => format!("{:.0}°C", c),
        TemperatureUnit::Fahrenheit => format!("{:.0}°F", celsius_to_fahrenheit(c)),
    }
}

/// Fluent key for a compass bearing (`weather-wind-n` … `weather-wind-nnw`).
#[must_use]
pub fn wind_direction_ftl_key(deg: u16) -> &'static str {
    const KEYS: [&str; 16] = [
        "weather-wind-n",
        "weather-wind-nne",
        "weather-wind-ne",
        "weather-wind-ene",
        "weather-wind-e",
        "weather-wind-ese",
        "weather-wind-se",
        "weather-wind-sse",
        "weather-wind-s",
        "weather-wind-ssw",
        "weather-wind-sw",
        "weather-wind-wsw",
        "weather-wind-w",
        "weather-wind-wnw",
        "weather-wind-nw",
        "weather-wind-nnw",
    ];
    KEYS[wind_direction_index(deg)]
}

fn wind_direction_index(deg: u16) -> usize {
    (((deg as f32 + 11.25) / 22.5) as usize) % 16
}

#[cfg(test)]
mod wind_tests {
    use super::*;

    #[test]
    fn wind_direction_buckets_cardinals_and_intermediates() {
        assert_eq!(wind_direction_ftl_key(0), "weather-wind-n");
        assert_eq!(wind_direction_ftl_key(45), "weather-wind-ne");
        assert_eq!(wind_direction_ftl_key(90), "weather-wind-e");
        assert_eq!(wind_direction_ftl_key(180), "weather-wind-s");
        assert_eq!(wind_direction_ftl_key(270), "weather-wind-w");
        assert_eq!(wind_direction_ftl_key(359), "weather-wind-n");
        assert_eq!(wind_direction_index(22), 1); // NNE
        assert_eq!(wind_direction_index(337), 15); // NNW
    }
}

/// Build the descriptor ready to register on a
/// [`crate::WidgetRegistry`].
#[must_use]
pub fn descriptor(http_client: reqwest::Client) -> WidgetDescriptor {
    let provider: Arc<dyn WeatherProvider> = Arc::new(OpenMeteoProvider::new(http_client));
    let factory: WidgetFactory = Arc::new(move |ctx: WidgetContext, state_bytes| {
        let cfg = match state_bytes {
            Some(bytes) => decode_config(bytes).unwrap_or_default(),
            None => WeatherConfig::default(),
        };
        let widget = WeatherWidget::new(
            ctx.instance_id,
            cfg,
            provider.clone(),
            ctx.bus.clone(),
            ctx.config.clone(),
            ctx.jobs.clone(),
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
