//! Payload for the weather widget.

use chrono::{DateTime, Utc};

/// Render-ready weather payload.
#[derive(Debug, Clone)]
pub struct WeatherPayload {
    /// Display name of the configured location.
    pub location_name: String,
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
    /// Wind compass direction label (e.g. `"NE"`), if known.
    pub wind_direction: Option<String>,
    /// 3-day forecast.
    pub forecast: Vec<WeatherForecastDay>,
    /// When the provider last fetched data.
    pub fetched_at: Option<DateTime<Utc>>,
    /// `true` until the first fetch attempt completes.
    pub is_loading: bool,
    /// Cache / freshness tag.
    pub status: WeatherStatusTag,
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
