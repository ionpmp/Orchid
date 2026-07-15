//! Persistent configuration for the weather widget.

use serde::{Deserialize, Serialize};

use super::types::Location;

/// Temperature unit shown in the widget.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TemperatureUnit {
    #[default]
    Celsius,
    Fahrenheit,
}

/// Persistent weather-widget config (one or more cities).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct WeatherConfig {
    /// Saved cities; always non-empty after [`WeatherConfig::normalize`].
    #[serde(default)]
    pub locations: Vec<Location>,
    /// Index into [`Self::locations`] for the city currently shown.
    #[serde(default)]
    pub active_index: usize,
    pub units: TemperatureUnit,
    pub refresh_interval_minutes: u32,
}

impl Default for WeatherConfig {
    fn default() -> Self {
        Self {
            locations: vec![Location::default()],
            active_index: 0,
            units: TemperatureUnit::Celsius,
            refresh_interval_minutes: 30,
        }
    }
}

impl WeatherConfig {
    /// Fill in sane defaults.
    pub fn normalize(&mut self) {
        if self.locations.is_empty() {
            self.locations.push(Location::default());
        }
        if self.active_index >= self.locations.len() {
            self.active_index = 0;
        }
        if self.refresh_interval_minutes == 0 {
            self.refresh_interval_minutes = 30;
        }
    }

    /// Active city (after normalize).
    #[must_use]
    pub fn active_location(&self) -> &Location {
        &self.locations[self.active_index.min(self.locations.len().saturating_sub(1))]
    }
}

/// Pre-bincode/serde shape when only a single `location` field existed.
#[derive(Debug, Serialize, Deserialize)]
struct LegacyWeatherConfig {
    location: Location,
    units: TemperatureUnit,
    refresh_interval_minutes: u32,
}

/// Decode config, accepting both multi-city and legacy single-city blobs.
pub fn decode_config(bytes: &[u8]) -> crate::error::Result<WeatherConfig> {
    match crate::widget::config::restore_state::<WeatherConfig>(bytes) {
        Ok(mut cfg) => {
            cfg.normalize();
            Ok(cfg)
        }
        Err(_) => {
            let legacy: LegacyWeatherConfig = crate::widget::config::restore_state(bytes)?;
            let mut cfg = WeatherConfig {
                locations: vec![legacy.location],
                active_index: 0,
                units: legacy.units,
                refresh_interval_minutes: legacy.refresh_interval_minutes,
            };
            cfg.normalize();
            Ok(cfg)
        }
    }
}

/// Convert °C to °F.
#[inline]
#[must_use]
pub fn celsius_to_fahrenheit(c: f32) -> f32 {
    c * 9.0 / 5.0 + 32.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fahrenheit_conversions() {
        assert!((celsius_to_fahrenheit(0.0) - 32.0).abs() < 1e-4);
        assert!((celsius_to_fahrenheit(100.0) - 212.0).abs() < 1e-4);
        assert!((celsius_to_fahrenheit(-40.0) + 40.0).abs() < 1e-4);
    }

    #[test]
    fn normalize_fills_empty_locations() {
        let mut cfg = WeatherConfig {
            locations: vec![],
            active_index: 9,
            units: TemperatureUnit::Celsius,
            refresh_interval_minutes: 0,
        };
        cfg.normalize();
        assert_eq!(cfg.locations.len(), 1);
        assert_eq!(cfg.active_index, 0);
        assert_eq!(cfg.refresh_interval_minutes, 30);
    }

    #[test]
    fn roundtrip_multi_city_config() {
        let cfg = WeatherConfig {
            locations: vec![
                Location::default(),
                Location {
                    name: "Berlin".into(),
                    latitude: 52.5,
                    longitude: 13.4,
                    timezone: Some("Europe/Berlin".into()),
                },
            ],
            active_index: 1,
            units: TemperatureUnit::Fahrenheit,
            refresh_interval_minutes: 15,
        };
        let bytes = crate::widget::config::save_state(&cfg).expect("encode");
        let decoded = decode_config(&bytes).expect("decode");
        assert_eq!(decoded.locations.len(), 2);
        assert_eq!(decoded.active_index, 1);
        assert_eq!(decoded.units, TemperatureUnit::Fahrenheit);
    }

    #[test]
    fn decode_legacy_single_location_blob() {
        let legacy = LegacyWeatherConfig {
            location: Location {
                name: "Paris".into(),
                latitude: 48.8,
                longitude: 2.3,
                timezone: Some("Europe/Paris".into()),
            },
            units: TemperatureUnit::Celsius,
            refresh_interval_minutes: 20,
        };
        let bytes = crate::widget::config::save_state(&legacy).expect("encode legacy");
        let decoded = decode_config(&bytes).expect("decode legacy");
        assert_eq!(decoded.locations.len(), 1);
        assert_eq!(decoded.locations[0].name, "Paris");
        assert_eq!(decoded.refresh_interval_minutes, 20);
    }
}
