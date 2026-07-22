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
            // Varanasi — classical Jyotish reference city.
            latitude: 25.3176,
            longitude: 82.9739,
            location_name: "Varanasi".into(),
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
    /// Clamp coordinates and day offset to sane ranges.
    pub fn normalize(&mut self) {
        self.latitude = self.latitude.clamp(-90.0, 90.0);
        self.longitude = self.longitude.clamp(-180.0, 180.0);
        self.day_offset = self.day_offset.clamp(-3650, 3650);
        self.active_tab = self.active_tab.min(3);
        self.month_offset = self.month_offset.clamp(-1200, 1200);
        self.year_offset = self.year_offset.clamp(-100, 100);
        self.birth_utc_offset_minutes = self.birth_utc_offset_minutes.clamp(-14 * 60, 14 * 60);
        if self.location_name.trim().is_empty() {
            self.location_name = "Varanasi".into();
        }
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

    /// Whether birth date is set.
    #[must_use]
    pub fn has_birth_data(&self) -> bool {
        self.birth_date.is_some()
    }
}
