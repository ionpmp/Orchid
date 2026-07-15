//! Shared data types for the weather widget.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// User-facing location with its coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct Location {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub timezone: Option<String>,
}

impl Default for Location {
    fn default() -> Self {
        // Semarang; users can add/replace cities from the in-widget picker.
        Self {
            name: "Semarang".into(),
            latitude: -6.9667,
            longitude: 110.4167,
            timezone: Some("Asia/Jakarta".into()),
        }
    }
}

/// Current-conditions report.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct CurrentWeather {
    pub temperature_c: f32,
    pub feels_like_c: Option<f32>,
    pub humidity: Option<u8>,
    pub wind_speed_kph: Option<f32>,
    pub wind_direction_deg: Option<u16>,
    pub precipitation_mm: Option<f32>,
    pub condition: WeatherCondition,
    pub observed_at: DateTime<Utc>,
}

/// One day of forecast.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct DailyForecast {
    pub date: NaiveDate,
    pub high_c: f32,
    pub low_c: f32,
    pub condition: WeatherCondition,
    pub precipitation_probability: Option<u8>,
    pub sunrise: Option<DateTime<Utc>>,
    pub sunset: Option<DateTime<Utc>>,
}

/// Canonical condition enum — providers normalise their labels / codes to
/// this set.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WeatherCondition {
    Clear,
    PartlyCloudy,
    Cloudy,
    Overcast,
    Fog,
    Drizzle,
    Rain,
    Snow,
    Sleet,
    Thunderstorm,
    Hail,
    Windy,
    Unknown,
}

impl WeatherCondition {
    /// Stable icon name emitted into payloads.
    #[must_use]
    pub fn icon(self) -> &'static str {
        match self {
            Self::Clear => "weather-clear",
            Self::PartlyCloudy => "weather-partly-cloudy",
            Self::Cloudy => "weather-cloudy",
            Self::Overcast => "weather-overcast",
            Self::Fog => "weather-fog",
            Self::Drizzle => "weather-drizzle",
            Self::Rain => "weather-rain",
            Self::Snow => "weather-snow",
            Self::Sleet => "weather-sleet",
            Self::Thunderstorm => "weather-thunderstorm",
            Self::Hail => "weather-hail",
            Self::Windy => "weather-windy",
            Self::Unknown => "weather-unknown",
        }
    }

    /// English label used when no locale bundle is loaded. The i18n layer
    /// will override this with the `weather-condition-*` Fluent keys once
    /// it is in place.
    #[must_use]
    pub fn default_label(self) -> &'static str {
        match self {
            Self::Clear => "Clear",
            Self::PartlyCloudy => "Partly cloudy",
            Self::Cloudy => "Cloudy",
            Self::Overcast => "Overcast",
            Self::Fog => "Fog",
            Self::Drizzle => "Drizzle",
            Self::Rain => "Rain",
            Self::Snow => "Snow",
            Self::Sleet => "Sleet",
            Self::Thunderstorm => "Thunderstorm",
            Self::Hail => "Hail",
            Self::Windy => "Windy",
            Self::Unknown => "Unknown",
        }
    }

    /// Fluent key used once the i18n layer resolves labels.
    #[must_use]
    pub fn ftl_key(self) -> &'static str {
        match self {
            Self::Clear => "weather-condition-clear",
            Self::PartlyCloudy => "weather-condition-partly-cloudy",
            Self::Cloudy => "weather-condition-cloudy",
            Self::Overcast => "weather-condition-overcast",
            Self::Fog => "weather-condition-fog",
            Self::Drizzle => "weather-condition-drizzle",
            Self::Rain => "weather-condition-rain",
            Self::Snow => "weather-condition-snow",
            Self::Sleet => "weather-condition-sleet",
            Self::Thunderstorm => "weather-condition-thunderstorm",
            Self::Hail => "weather-condition-hail",
            Self::Windy => "weather-condition-windy",
            Self::Unknown => "weather-condition-unknown",
        }
    }
}

/// Full weather payload returned from providers.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct WeatherData {
    pub location: Location,
    pub current: CurrentWeather,
    pub forecast: Vec<DailyForecast>,
    pub fetched_at: DateTime<Utc>,
}
