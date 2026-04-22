//! Payload for the weather widget.

/// Render-ready weather payload.
#[derive(Debug, Clone)]
pub struct WeatherPayload {
    /// Display name of the configured location.
    pub location_name: String,
    /// Pre-formatted current temperature (e.g. `"24°C"`).
    pub current_temp_text: String,
    /// Optional "feels like" text.
    pub feels_like_text: Option<String>,
    /// Localised condition label.
    pub condition_label: String,
    /// Icon name (`"weather-clear"`, `"weather-rain"`, ...).
    pub condition_icon: &'static str,
    /// Localised humidity text (e.g. `"68%"`).
    pub humidity_text: Option<String>,
    /// Localised wind text (e.g. `"12 km/h NE"`).
    pub wind_text: Option<String>,
    /// 3-day forecast.
    pub forecast: Vec<WeatherForecastDay>,
    /// Localised "last updated" line.
    pub last_updated_text: String,
    /// Cache / freshness tag.
    pub status: WeatherStatusTag,
}

/// A single forecast day as shown in the UI.
#[derive(Debug, Clone)]
pub struct WeatherForecastDay {
    /// Localised day label (`"Today"`, `"Tomorrow"`, `"Wed"`).
    pub day_label: String,
    /// Pre-formatted high temperature.
    pub high_text: String,
    /// Pre-formatted low temperature.
    pub low_text: String,
    /// Icon name.
    pub condition_icon: &'static str,
    /// Optional precipitation-probability label (e.g. `"45%"`).
    pub precipitation_probability_text: Option<String>,
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
