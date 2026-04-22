//! Persistent configuration for the weather widget.

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

use super::types::Location;

/// Temperature unit shown in the widget.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
#[serde(rename_all = "kebab-case")]
pub enum TemperatureUnit {
    #[default]
    Celsius,
    Fahrenheit,
}

/// Persistent weather-widget config.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
#[allow(missing_docs)]
pub struct WeatherConfig {
    #[bincode(with_serde)]
    pub location: Location,
    pub units: TemperatureUnit,
    pub refresh_interval_minutes: u32,
}

impl Default for WeatherConfig {
    fn default() -> Self {
        Self {
            location: Location::default(),
            units: TemperatureUnit::Celsius,
            refresh_interval_minutes: 30,
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
}
