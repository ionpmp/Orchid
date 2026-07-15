//! Weather-data providers.
//!
//! The widget talks to the abstract [`WeatherProvider`] trait. An
//! [`OpenMeteoProvider`] implementation hits
//! <https://api.open-meteo.com/v1/forecast> and
//! <https://geocoding-api.open-meteo.com/v1/search>.

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use serde::Deserialize;

use super::types::{CurrentWeather, DailyForecast, Location, WeatherCondition, WeatherData};

/// Number of daily forecast rows requested from Open-Meteo (max free: 16).
pub const FORECAST_DAYS: u8 = 16;

/// Error type returned from providers.
#[derive(thiserror::Error, Debug)]
#[allow(missing_docs)]
pub enum WeatherError {
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error("API returned error: {0}")]
    Api(String),
    #[error("no data for location")]
    NoData,
    #[error("response parse error: {0}")]
    Parse(String),
}

/// Result alias used by this module.
pub type Result<T> = std::result::Result<T, WeatherError>;

/// One row from the geocoding search API.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct GeocodingHit {
    pub name: String,
    pub detail: String,
    pub latitude: f64,
    pub longitude: f64,
    pub timezone: Option<String>,
}

impl GeocodingHit {
    /// Convert into a saved [`Location`].
    #[must_use]
    pub fn into_location(self) -> Location {
        Location {
            name: self.name,
            latitude: self.latitude,
            longitude: self.longitude,
            timezone: self.timezone,
        }
    }
}

/// Abstract weather-data source.
#[async_trait]
pub trait WeatherProvider: Send + Sync + std::fmt::Debug {
    /// Fetch current conditions + multi-day forecast for `location`.
    async fn fetch(&self, location: &Location) -> Result<WeatherData>;

    /// Search cities by name (Open-Meteo geocoding).
    async fn search_cities(&self, query: &str) -> Result<Vec<GeocodingHit>>;
}

/// `api.open-meteo.com`-backed provider. Uses `rustls`-only HTTPS (no
/// OpenSSL runtime dependency).
#[derive(Debug, Clone)]
pub struct OpenMeteoProvider {
    client: reqwest::Client,
}

impl OpenMeteoProvider {
    /// Build a provider using an externally-configured HTTP client. The
    /// caller is expected to set timeouts / user-agent on `client`.
    #[must_use]
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    /// Build a provider with a default HTTP client (30 s timeout, Orchid
    /// User-Agent).
    ///
    /// # Errors
    ///
    /// Propagates [`reqwest::Error`] when the client builder fails.
    pub fn default_client() -> std::result::Result<reqwest::Client, reqwest::Error> {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("orchid-widgets/", env!("CARGO_PKG_VERSION")))
            .build()
    }
}

#[async_trait]
impl WeatherProvider for OpenMeteoProvider {
    async fn fetch(&self, location: &Location) -> Result<WeatherData> {
        let url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&current_weather=true&hourly=relativehumidity_2m,apparent_temperature,precipitation&daily=temperature_2m_max,temperature_2m_min,weather_code,precipitation_probability_mean,sunrise,sunset&forecast_days={days}&timezone=auto",
            lat = location.latitude,
            lon = location.longitude,
            days = FORECAST_DAYS,
        );
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(WeatherError::Api(format!("HTTP {}", resp.status())));
        }
        let body: OpenMeteoResponse = resp
            .json()
            .await
            .map_err(|e| WeatherError::Parse(e.to_string()))?;
        let data = body.into_weather_data(location.clone())?;
        Ok(data)
    }

    async fn search_cities(&self, query: &str) -> Result<Vec<GeocodingHit>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let url = format!(
            "https://geocoding-api.open-meteo.com/v1/search?name={name}&count=8&language=en&format=json",
            name = urlencoding_path(q),
        );
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(WeatherError::Api(format!("HTTP {}", resp.status())));
        }
        let body: GeocodingResponse = resp
            .json()
            .await
            .map_err(|e| WeatherError::Parse(e.to_string()))?;
        Ok(body
            .results
            .unwrap_or_default()
            .into_iter()
            .map(|r| {
                let detail = [r.admin1.as_deref(), r.country.as_deref()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(", ");
                GeocodingHit {
                    name: r.name,
                    detail,
                    latitude: r.latitude,
                    longitude: r.longitude,
                    timezone: r.timezone,
                }
            })
            .collect())
    }
}

/// Minimal path-segment encoding for geocoding queries (spaces → `%20`).
fn urlencoding_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => {
                out.push('%');
                out.push(char::from(b"0123456789ABCDEF"[(b >> 4) as usize]));
                out.push(char::from(b"0123456789ABCDEF"[(b & 0xf) as usize]));
            }
        }
    }
    out
}

/// Map a WMO weather code (as returned by Open-Meteo) to our canonical
/// [`WeatherCondition`]. The table mirrors the official
/// [WMO 4677 codes](https://open-meteo.com/en/docs) commonly used in the
/// API responses.
#[must_use]
pub fn map_wmo_code(code: u32) -> WeatherCondition {
    match code {
        0 => WeatherCondition::Clear,
        1 | 2 => WeatherCondition::PartlyCloudy,
        3 => WeatherCondition::Overcast,
        45 | 48 => WeatherCondition::Fog,
        51..=57 => WeatherCondition::Drizzle,
        61..=67 | 80..=82 => WeatherCondition::Rain,
        71..=77 | 85 | 86 => WeatherCondition::Snow,
        95 => WeatherCondition::Thunderstorm,
        96 | 99 => WeatherCondition::Hail,
        _ => WeatherCondition::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Open-Meteo response parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct OpenMeteoResponse {
    #[serde(default)]
    current_weather: Option<OmCurrent>,
    #[serde(default)]
    hourly: Option<OmHourly>,
    #[serde(default)]
    daily: Option<OmDaily>,
}

#[derive(Debug, Deserialize)]
struct OmCurrent {
    temperature: f32,
    windspeed: f32,
    winddirection: u16,
    weathercode: u32,
    time: String,
}

#[derive(Debug, Deserialize)]
struct OmHourly {
    time: Vec<String>,
    #[serde(default)]
    relativehumidity_2m: Vec<Option<u8>>,
    #[serde(default)]
    apparent_temperature: Vec<Option<f32>>,
    #[serde(default)]
    precipitation: Vec<Option<f32>>,
}

#[derive(Debug, Deserialize)]
struct OmDaily {
    time: Vec<String>,
    temperature_2m_max: Vec<Option<f32>>,
    temperature_2m_min: Vec<Option<f32>>,
    weather_code: Vec<Option<u32>>,
    #[serde(default)]
    precipitation_probability_mean: Vec<Option<u8>>,
    #[serde(default)]
    sunrise: Vec<Option<String>>,
    #[serde(default)]
    sunset: Vec<Option<String>>,
}

#[derive(Debug, Deserialize)]
struct GeocodingResponse {
    #[serde(default)]
    results: Option<Vec<GeocodingResult>>,
}

#[derive(Debug, Deserialize)]
struct GeocodingResult {
    name: String,
    latitude: f64,
    longitude: f64,
    #[serde(default)]
    country: Option<String>,
    #[serde(default)]
    admin1: Option<String>,
    #[serde(default)]
    timezone: Option<String>,
}

impl OpenMeteoResponse {
    fn into_weather_data(self, location: Location) -> Result<WeatherData> {
        let current = self.current_weather.ok_or(WeatherError::NoData)?;
        let observed_at = parse_naive_local_or_utc(&current.time).unwrap_or_else(Utc::now);

        let (humidity, feels_like, precip) = match self.hourly {
            Some(h) => closest_hourly(&h, observed_at),
            None => (None, None, None),
        };

        let cw = CurrentWeather {
            temperature_c: current.temperature,
            feels_like_c: feels_like,
            humidity,
            wind_speed_kph: Some(current.windspeed),
            wind_direction_deg: Some(current.winddirection),
            precipitation_mm: precip,
            condition: map_wmo_code(current.weathercode),
            observed_at,
        };

        let forecast = match self.daily {
            Some(d) => days_from_daily(d)?,
            None => Vec::new(),
        };

        Ok(WeatherData {
            location,
            current: cw,
            forecast,
            fetched_at: Utc::now(),
        })
    }
}

fn closest_hourly(h: &OmHourly, at: DateTime<Utc>) -> (Option<u8>, Option<f32>, Option<f32>) {
    let mut best: Option<(usize, i64)> = None;
    for (i, s) in h.time.iter().enumerate() {
        if let Some(ts) = parse_naive_local_or_utc(s) {
            let diff = (ts - at).num_seconds().abs();
            if best.map(|(_, d)| diff < d).unwrap_or(true) {
                best = Some((i, diff));
            }
        }
    }
    let Some((idx, _)) = best else {
        return (None, None, None);
    };
    let humidity = h.relativehumidity_2m.get(idx).copied().flatten();
    let feels = h.apparent_temperature.get(idx).copied().flatten();
    let precip = h.precipitation.get(idx).copied().flatten();
    (humidity, feels, precip)
}

fn days_from_daily(d: OmDaily) -> Result<Vec<DailyForecast>> {
    let mut out = Vec::with_capacity(d.time.len());
    for (i, day_str) in d.time.iter().enumerate() {
        let date = NaiveDate::parse_from_str(day_str, "%Y-%m-%d")
            .map_err(|e| WeatherError::Parse(format!("invalid daily.time {day_str}: {e}")))?;
        let high = d
            .temperature_2m_max
            .get(i)
            .copied()
            .flatten()
            .unwrap_or(0.0);
        let low = d
            .temperature_2m_min
            .get(i)
            .copied()
            .flatten()
            .unwrap_or(0.0);
        let code = d.weather_code.get(i).copied().flatten().unwrap_or(0);
        let probability = d.precipitation_probability_mean.get(i).copied().flatten();
        let sunrise = d
            .sunrise
            .get(i)
            .and_then(|s| s.as_ref())
            .and_then(|s| parse_naive_local_or_utc(s));
        let sunset = d
            .sunset
            .get(i)
            .and_then(|s| s.as_ref())
            .and_then(|s| parse_naive_local_or_utc(s));
        out.push(DailyForecast {
            date,
            high_c: high,
            low_c: low,
            condition: map_wmo_code(code),
            precipitation_probability: probability,
            sunrise,
            sunset,
        });
    }
    Ok(out)
}

fn parse_naive_local_or_utc(s: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M") {
        return Utc.from_utc_datetime(&dt).into();
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Utc.from_utc_datetime(&dt).into();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wmo_codes_map_to_expected_conditions() {
        assert_eq!(map_wmo_code(0), WeatherCondition::Clear);
        assert_eq!(map_wmo_code(1), WeatherCondition::PartlyCloudy);
        assert_eq!(map_wmo_code(3), WeatherCondition::Overcast);
        assert_eq!(map_wmo_code(45), WeatherCondition::Fog);
        assert_eq!(map_wmo_code(53), WeatherCondition::Drizzle);
        assert_eq!(map_wmo_code(63), WeatherCondition::Rain);
        assert_eq!(map_wmo_code(73), WeatherCondition::Snow);
        assert_eq!(map_wmo_code(95), WeatherCondition::Thunderstorm);
        assert_eq!(map_wmo_code(99), WeatherCondition::Hail);
        assert_eq!(map_wmo_code(999), WeatherCondition::Unknown);
    }

    #[test]
    fn condition_icon_and_label_exhaustive() {
        use WeatherCondition::*;
        for c in [
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
        ] {
            assert!(!c.icon().is_empty());
            assert!(!c.default_label().is_empty());
            assert!(!c.ftl_key().is_empty());
        }
    }

    #[test]
    fn parses_naive_time_formats() {
        assert!(parse_naive_local_or_utc("2026-04-22T12:00").is_some());
        assert!(parse_naive_local_or_utc("2026-04-22T12:00:00").is_some());
        assert!(parse_naive_local_or_utc("not-a-date").is_none());
    }

    #[test]
    fn encodes_geocoding_query() {
        assert_eq!(urlencoding_path("New York"), "New%20York");
        assert_eq!(urlencoding_path("São"), "S%C3%A3o");
    }
}
