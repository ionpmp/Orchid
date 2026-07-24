//! Jyotish widget persistent configuration.

use bincode::{Decode, Encode};
use chrono::NaiveDate;
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

/// One saved Jyotish location (name + coordinates). Unlike weather, no
/// timezone is stored — sunrise/muhurta are derived from longitude alone.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode, PartialEq)]
#[allow(missing_docs)]
pub struct JyotishLocation {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
}

impl Default for JyotishLocation {
    fn default() -> Self {
        // Varanasi — classical Jyotish reference city.
        Self {
            name: "Varanasi".into(),
            latitude: 25.3176,
            longitude: 82.9739,
        }
    }
}

/// Persistent jyotish-widget config.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
#[allow(missing_docs)]
pub struct JyotishConfig {
    /// Saved locations; always non-empty after [`JyotishConfig::normalize`].
    #[serde(default)]
    pub locations: Vec<JyotishLocation>,
    /// Index into [`Self::locations`] for the place currently shown.
    #[serde(default)]
    pub active_index: usize,
    pub ayanamsa: AyanamsaSystem,
    /// Days relative to today (UTC date). `0` = today.
    pub day_offset: i32,
    pub show_planets: bool,
    pub show_sunrise_sunset: bool,
    pub birth_date: Option<String>,
    pub birth_time: Option<String>,
    pub birth_utc_offset_minutes: i32,
    pub birth_time_rectified: bool,
    pub active_tab: u8,
    pub month_offset: i32,
    pub year_offset: i32,
    #[serde(default = "default_true")]
    pub notify_day_color: bool,
    #[serde(default = "default_true")]
    pub notify_rahukalam: bool,
    /// Show Rahu Kalam / Yamagandam / Gulika windows on the Day tab.
    #[serde(default = "default_true")]
    pub show_rahukalam: bool,
    /// Apply natal tara/chandra layers (and life/dasha) when birth data is set.
    #[serde(default = "default_true")]
    pub enable_personal: bool,
}

fn default_true() -> bool {
    true
}

impl Default for JyotishConfig {
    fn default() -> Self {
        Self {
            locations: vec![JyotishLocation::default()],
            active_index: 0,
            ayanamsa: AyanamsaSystem::Lahiri,
            day_offset: 0,
            show_planets: true,
            show_sunrise_sunset: true,
            birth_date: None,
            birth_time: None,
            birth_utc_offset_minutes: 0,
            birth_time_rectified: false,
            active_tab: 0,
            month_offset: 0,
            year_offset: 0,
            notify_day_color: true,
            notify_rahukalam: true,
            show_rahukalam: true,
            enable_personal: true,
        }
    }
}

impl JyotishConfig {
    /// Fill in sane defaults; clamp coordinates, indices, and day offset.
    pub fn normalize(&mut self) {
        if self.locations.is_empty() {
            self.locations.push(JyotishLocation::default());
        }
        for loc in &mut self.locations {
            loc.latitude = loc.latitude.clamp(-90.0, 90.0);
            loc.longitude = loc.longitude.clamp(-180.0, 180.0);
            if loc.name.trim().is_empty() {
                loc.name = "Varanasi".into();
            }
        }
        if self.active_index >= self.locations.len() {
            self.active_index = 0;
        }
        self.day_offset = self.day_offset.clamp(-3650, 3650);
        self.active_tab = self.active_tab.min(3);
        self.month_offset = self.month_offset.clamp(-1200, 1200);
        self.year_offset = self.year_offset.clamp(-100, 100);
        self.birth_utc_offset_minutes = self.birth_utc_offset_minutes.clamp(-14 * 60, 14 * 60);
        if let Some(ref d) = self.birth_date {
            if NaiveDate::parse_from_str(d, "%Y-%m-%d").is_err() {
                self.birth_date = None;
            }
        }
        if let Some(ref t) = self.birth_time {
            if chrono::NaiveTime::parse_from_str(t, "%H:%M").is_err() {
                self.birth_time = None;
            }
        }
    }

    /// Active location (after normalize).
    #[must_use]
    pub fn active_location(&self) -> &JyotishLocation {
        &self.locations[self
            .active_index
            .min(self.locations.len().saturating_sub(1))]
    }

    /// Latitude of the active location.
    #[must_use]
    pub fn latitude(&self) -> f64 {
        self.active_location().latitude
    }

    /// Longitude of the active location.
    #[must_use]
    pub fn longitude(&self) -> f64 {
        self.active_location().longitude
    }

    /// Display name of the active location.
    #[must_use]
    pub fn location_name(&self) -> &str {
        &self.active_location().name
    }

    /// Whether birth date is set.
    #[must_use]
    pub fn has_birth_data(&self) -> bool {
        self.birth_date.is_some()
    }
}

/// Pre-multi-location shape (flat `latitude` / `longitude` / `location_name`).
#[derive(Debug, Serialize, Deserialize)]
struct LegacyJyotishConfig {
    latitude: f64,
    longitude: f64,
    location_name: String,
    ayanamsa: AyanamsaSystem,
    day_offset: i32,
    show_planets: bool,
    show_sunrise_sunset: bool,
    birth_date: Option<String>,
    birth_time: Option<String>,
    birth_utc_offset_minutes: i32,
    birth_time_rectified: bool,
    active_tab: u8,
    month_offset: i32,
    year_offset: i32,
    #[serde(default = "default_true")]
    notify_day_color: bool,
    #[serde(default = "default_true")]
    notify_rahukalam: bool,
    #[serde(default = "default_true")]
    show_rahukalam: bool,
    #[serde(default = "default_true")]
    enable_personal: bool,
}

/// Decode config, accepting both multi-location and legacy flat-location blobs.
pub fn decode_config(bytes: &[u8]) -> crate::error::Result<JyotishConfig> {
    match crate::widget::config::restore_state::<JyotishConfig>(bytes) {
        Ok(mut cfg) => {
            cfg.normalize();
            Ok(cfg)
        }
        Err(_) => {
            let legacy: LegacyJyotishConfig = crate::widget::config::restore_state(bytes)?;
            let mut cfg = JyotishConfig {
                locations: vec![JyotishLocation {
                    name: legacy.location_name,
                    latitude: legacy.latitude,
                    longitude: legacy.longitude,
                }],
                active_index: 0,
                ayanamsa: legacy.ayanamsa,
                day_offset: legacy.day_offset,
                show_planets: legacy.show_planets,
                show_sunrise_sunset: legacy.show_sunrise_sunset,
                birth_date: legacy.birth_date,
                birth_time: legacy.birth_time,
                birth_utc_offset_minutes: legacy.birth_utc_offset_minutes,
                birth_time_rectified: legacy.birth_time_rectified,
                active_tab: legacy.active_tab,
                month_offset: legacy.month_offset,
                year_offset: legacy.year_offset,
                notify_day_color: legacy.notify_day_color,
                notify_rahukalam: legacy.notify_rahukalam,
                show_rahukalam: legacy.show_rahukalam,
                enable_personal: legacy.enable_personal,
            };
            cfg.normalize();
            Ok(cfg)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_fills_empty_locations() {
        let mut cfg = JyotishConfig {
            locations: vec![],
            active_index: 9,
            ..JyotishConfig::default()
        };
        cfg.normalize();
        assert_eq!(cfg.locations.len(), 1);
        assert_eq!(cfg.active_index, 0);
        assert_eq!(cfg.location_name(), "Varanasi");
    }

    #[test]
    fn normalize_clamps_out_of_range_coordinates() {
        let mut cfg = JyotishConfig {
            locations: vec![JyotishLocation {
                name: "Nowhere".into(),
                latitude: 200.0,
                longitude: -400.0,
            }],
            active_index: 0,
            ..JyotishConfig::default()
        };
        cfg.normalize();
        assert_eq!(cfg.latitude(), 90.0);
        assert_eq!(cfg.longitude(), -180.0);
    }

    #[test]
    fn roundtrip_multi_location_config() {
        let cfg = JyotishConfig {
            locations: vec![
                JyotishLocation::default(),
                JyotishLocation {
                    name: "Ujjain".into(),
                    latitude: 23.1765,
                    longitude: 75.7885,
                },
            ],
            active_index: 1,
            ..JyotishConfig::default()
        };
        let bytes = crate::widget::config::save_state(&cfg).expect("encode");
        let decoded = decode_config(&bytes).expect("decode");
        assert_eq!(decoded.locations.len(), 2);
        assert_eq!(decoded.active_index, 1);
        assert_eq!(decoded.location_name(), "Ujjain");
    }

    #[test]
    fn decode_legacy_flat_location_blob() {
        let legacy = LegacyJyotishConfig {
            latitude: 28.6139,
            longitude: 77.2090,
            location_name: "Delhi".into(),
            ayanamsa: AyanamsaSystem::Lahiri,
            day_offset: 0,
            show_planets: true,
            show_sunrise_sunset: true,
            birth_date: None,
            birth_time: None,
            birth_utc_offset_minutes: 0,
            birth_time_rectified: false,
            active_tab: 0,
            month_offset: 0,
            year_offset: 0,
            notify_day_color: true,
            notify_rahukalam: true,
            show_rahukalam: true,
            enable_personal: true,
        };
        let bytes = crate::widget::config::save_state(&legacy).expect("encode legacy");
        let decoded = decode_config(&bytes).expect("decode legacy");
        assert_eq!(decoded.locations.len(), 1);
        assert_eq!(decoded.location_name(), "Delhi");
        assert_eq!(decoded.latitude(), 28.6139);
        assert_eq!(decoded.longitude(), 77.2090);
    }
}
