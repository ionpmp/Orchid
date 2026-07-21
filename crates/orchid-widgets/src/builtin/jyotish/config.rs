//! Jyotish widget persistent configuration.

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

/// Ayanamsa system used to convert tropical → sidereal longitudes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Encode, Decode, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AyanamsaSystem {
    /// Chitra-paksha (Lahiri) — standard in Indian calendars.
    #[default]
    Lahiri,
    /// Krishnamurti (KP) — ~1° ahead of Lahiri.
    Krishnamurti,
    /// B.V. Raman.
    Raman,
}

impl AyanamsaSystem {
    /// Stable settings / Fluent key fragment.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lahiri => "lahiri",
            Self::Krishnamurti => "krishnamurti",
            Self::Raman => "raman",
        }
    }

    /// Parse from settings combo value.
    #[must_use]
    pub fn from_str_value(s: &str) -> Self {
        match s {
            "krishnamurti" => Self::Krishnamurti,
            "raman" => Self::Raman,
            _ => Self::Lahiri,
        }
    }

    /// Fluent key for the label.
    #[must_use]
    pub fn ftl_key(self) -> &'static str {
        match self {
            Self::Lahiri => "jyotish-ayanamsa-lahiri",
            Self::Krishnamurti => "jyotish-ayanamsa-krishnamurti",
            Self::Raman => "jyotish-ayanamsa-raman",
        }
    }
}

/// Persistent jyotish-widget config.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
#[allow(missing_docs)]
pub struct JyotishConfig {
    pub latitude: f64,
    pub longitude: f64,
    pub location_name: String,
    pub ayanamsa: AyanamsaSystem,
    /// Days relative to today (UTC date). `0` = today.
    pub day_offset: i32,
    pub show_planets: bool,
    pub show_sunrise_sunset: bool,
}

impl Default for JyotishConfig {
    fn default() -> Self {
        Self {
            // Varanasi — classical Jyotish reference city.
            latitude: 25.3176,
            longitude: 82.9739,
            location_name: "Varanasi".into(),
            ayanamsa: AyanamsaSystem::Lahiri,
            day_offset: 0,
            show_planets: true,
            show_sunrise_sunset: true,
        }
    }
}

impl JyotishConfig {
    /// Clamp coordinates and day offset to sane ranges.
    pub fn normalize(&mut self) {
        self.latitude = self.latitude.clamp(-90.0, 90.0);
        self.longitude = self.longitude.clamp(-180.0, 180.0);
        self.day_offset = self.day_offset.clamp(-3650, 3650);
        if self.location_name.trim().is_empty() {
            self.location_name = "Varanasi".into();
        }
    }
}
