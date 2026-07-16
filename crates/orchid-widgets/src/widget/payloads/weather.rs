//! Payload for the weather widget.

use chrono::{DateTime, Utc};

/// Render-ready weather payload.
#[derive(Debug, Clone)]
pub struct WeatherPayload {
    /// Display name of the active city.
    pub location_name: String,
    /// All configured city names (for chips / picker list).
    pub cities: Vec<WeatherCityEntry>,
    /// Index of the active city in [`Self::cities`].
    pub active_city_index: usize,
    /// City-picker overlay visibility.
    pub picker_open: bool,
    /// Current city-search query.
    pub search_query: String,
    /// Geocoding hits for the current query.
    pub search_results: Vec<WeatherSearchHit>,
    /// True while a geocoding request is in flight.
    pub search_busy: bool,
    /// Index of the forecast day highlighted in the strip.
    pub selected_day_index: usize,
    /// Pre-formatted current temperature (e.g. `"24°C"`).
    pub current_temp_text: String,
    /// Optional pre-formatted "feels like" temperature (without prefix).
    pub feels_like_temp: Option<String>,
    /// Fluent key for the condition label (`weather-condition-*`).
    pub condition_key: &'static str,
    /// Icon name (`"weather-clear"`, `"weather-rain"`, ...).
    pub condition_icon: &'static str,
    /// Humidity percentage, if known.
    pub humidity_percent: Option<u8>,
    /// Wind speed in km/h, if known.
    pub wind_speed_kph: Option<f32>,
    /// Wind compass direction Fluent key (e.g. `"weather-wind-ne"`), if known.
    pub wind_direction: Option<String>,
    /// Multi-day forecast (swipeable in the UI).
    pub forecast: Vec<WeatherForecastDay>,
    /// When the provider last fetched data for the active city.
    pub fetched_at: Option<DateTime<Utc>>,
    /// `true` until the first fetch attempt completes.
    pub is_loading: bool,
    /// Cache / freshness tag.
    pub status: WeatherStatusTag,
}

/// One configured city chip / picker row.
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherCityEntry {
    /// Display name.
    pub name: String,
    /// Whether this is the active city.
    pub active: bool,
}

/// One geocoding search result shown in the city picker.
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherSearchHit {
    /// City name.
    pub name: String,
    /// Secondary line (region, country).
    pub detail: String,
    /// WGS84 latitude.
    pub latitude: f64,
    /// WGS84 longitude.
    pub longitude: f64,
    /// IANA timezone id when known.
    pub timezone: String,
}

/// A single forecast day as shown in the UI.
#[derive(Debug, Clone)]
pub struct WeatherForecastDay {
    /// `0` = today, `1` = tomorrow, else weekday formatting in the UI.
    pub day_index: u8,
    /// Short weekday label for `day_index >= 2` (e.g. `"Wed"`).
    pub weekday_label: Option<String>,
    /// Pre-formatted high temperature.
    pub high_text: String,
    /// Pre-formatted low temperature.
    pub low_text: String,
    /// Icon name.
    pub condition_icon: &'static str,
    /// Optional precipitation probability (0–100).
    pub precipitation_probability: Option<u8>,
    /// True when this day is the strip selection.
    pub selected: bool,
    /// Pre-formatted sunrise clock time, if known.
    pub sunrise_text: Option<String>,
    /// Pre-formatted sunset clock time, if known.
    pub sunset_text: Option<String>,
}

/// Coarse freshness tag used by the view to colour the widget frame.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeatherStatusTag {
    Fresh,
    Stale,
    Offline,
    Error,
}
