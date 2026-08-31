//! Jyotish widget persistent configuration.

use bincode_reloaded::{Decode, Encode};
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

/// Birth profile gender (UI / future narrative; does not affect score math).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Encode, Decode, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Gender {
    /// Not specified.
    #[default]
    Unspecified,
    /// Female.
    Female,
    /// Male.
    Male,
}

impl Gender {
    /// Stable wire / UI value (`0` / `1` / `2`).
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            Self::Unspecified => 0,
            Self::Female => 1,
            Self::Male => 2,
        }
    }

    /// Parse from UI index.
    #[must_use]
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Female,
            2 => Self::Male,
            _ => Self::Unspecified,
        }
    }

    /// Fluent key for the label.
    #[must_use]
    pub fn ftl_key(self) -> &'static str {
        match self {
            Self::Unspecified => "jyotish-gender-unspecified",
            Self::Female => "jyotish-gender-female",
            Self::Male => "jyotish-gender-male",
        }
    }
}

/// One birth profile (person) with its own birth place — separate from
/// observation locations used for sunrise / muhurta.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode, PartialEq)]
#[allow(missing_docs)]
pub struct JyotishProfile {
    pub name: String,
    #[serde(default)]
    pub gender: Gender,
    pub birth_date: Option<String>,
    pub birth_time: Option<String>,
    pub birth_utc_offset_minutes: i32,
    pub birth_time_rectified: bool,
    pub birth_place: JyotishLocation,
}

impl Default for JyotishProfile {
    fn default() -> Self {
        Self {
            name: String::new(),
            gender: Gender::Unspecified,
            birth_date: None,
            birth_time: None,
            birth_utc_offset_minutes: 0,
            birth_time_rectified: false,
            birth_place: JyotishLocation::default(),
        }
    }
}

impl JyotishProfile {
    /// Whether this profile has a usable birth date.
    #[must_use]
    pub fn has_birth_data(&self) -> bool {
        self.birth_date.is_some()
    }

    fn normalize(&mut self) {
        self.birth_place.latitude = self.birth_place.latitude.clamp(-90.0, 90.0);
        self.birth_place.longitude = self.birth_place.longitude.clamp(-180.0, 180.0);
        if self.birth_place.name.trim().is_empty() {
            self.birth_place.name = "Varanasi".into();
        }
        if self.name.trim().is_empty() {
            self.name = "Profile".into();
        }
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
}

/// Persistent jyotish-widget config.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
#[allow(missing_docs)]
pub struct JyotishConfig {
    /// Saved observation locations; always non-empty after [`JyotishConfig::normalize`].
    #[serde(default)]
    pub locations: Vec<JyotishLocation>,
    /// Index into [`Self::locations`] for the place currently shown.
    /// Ignored for calculations when [`Self::use_current`] is true.
    #[serde(default)]
    pub active_index: usize,
    /// When true, observation coords come from the session-resolved
    /// "Current location" (GPS/IP), not [`Self::locations`][`Self::active_index`].
    #[serde(default)]
    pub use_current: bool,
    pub ayanamsa: AyanamsaSystem,
    /// Days relative to today (UTC date). `0` = today.
    pub day_offset: i32,
    pub show_planets: bool,
    pub show_sunrise_sunset: bool,
    /// Birth profiles (people); empty means no personal layer.
    #[serde(default)]
    pub profiles: Vec<JyotishProfile>,
    /// Index into [`Self::profiles`].
    #[serde(default)]
    pub active_profile_index: usize,
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
            use_current: false,
            ayanamsa: AyanamsaSystem::Lahiri,
            day_offset: 0,
            show_planets: true,
            show_sunrise_sunset: true,
            profiles: Vec::new(),
            active_profile_index: 0,
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
        for profile in &mut self.profiles {
            profile.normalize();
        }
        if self.profiles.is_empty() {
            self.active_profile_index = 0;
        } else if self.active_profile_index >= self.profiles.len() {
            self.active_profile_index = self.profiles.len() - 1;
        }
        self.day_offset = self.day_offset.clamp(-3650, 3650);
        self.active_tab = self.active_tab.min(3);
        self.month_offset = self.month_offset.clamp(-1200, 1200);
        self.year_offset = self.year_offset.clamp(-100, 100);
    }

    /// Active observation location (after normalize).
    #[must_use]
    pub fn active_location(&self) -> &JyotishLocation {
        &self.locations[self
            .active_index
            .min(self.locations.len().saturating_sub(1))]
    }

    /// Active birth profile, if any.
    #[must_use]
    pub fn active_profile(&self) -> Option<&JyotishProfile> {
        self.profiles.get(self.active_profile_index)
    }

    /// Mutable active birth profile, if any.
    pub fn active_profile_mut(&mut self) -> Option<&mut JyotishProfile> {
        let idx = self.active_profile_index;
        self.profiles.get_mut(idx)
    }

    /// Latitude of the active observation location.
    #[must_use]
    pub fn latitude(&self) -> f64 {
        self.active_location().latitude
    }

    /// Longitude of the active observation location.
    #[must_use]
    pub fn longitude(&self) -> f64 {
        self.active_location().longitude
    }

    /// Display name of the active observation location.
    #[must_use]
    pub fn location_name(&self) -> &str {
        &self.active_location().name
    }

    /// Whether the active profile has a birth date.
    #[must_use]
    pub fn has_birth_data(&self) -> bool {
        self.active_profile()
            .is_some_and(JyotishProfile::has_birth_data)
    }
}

fn profiles_from_flat_birth(
    birth_date: Option<String>,
    birth_time: Option<String>,
    birth_utc_offset_minutes: i32,
    birth_time_rectified: bool,
    locations: &[JyotishLocation],
) -> Vec<JyotishProfile> {
    if birth_date.is_none() && birth_time.is_none() && !birth_time_rectified {
        return Vec::new();
    }
    let birth_place = locations
        .first()
        .cloned()
        .unwrap_or_else(JyotishLocation::default);
    vec![JyotishProfile {
        name: "Profile".into(),
        gender: Gender::Unspecified,
        birth_date,
        birth_time,
        birth_utc_offset_minutes,
        birth_time_rectified,
        birth_place,
    }]
}

/// Shape with `use_current` + flat birth_* (pre-profile migration).
#[derive(Debug, Serialize, Deserialize)]
struct FlatBirthJyotishConfig {
    #[serde(default)]
    locations: Vec<JyotishLocation>,
    #[serde(default)]
    active_index: usize,
    #[serde(default)]
    use_current: bool,
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

impl FlatBirthJyotishConfig {
    fn into_config(self) -> JyotishConfig {
        let profiles = profiles_from_flat_birth(
            self.birth_date,
            self.birth_time,
            self.birth_utc_offset_minutes,
            self.birth_time_rectified,
            &self.locations,
        );
        JyotishConfig {
            locations: self.locations,
            active_index: self.active_index,
            use_current: self.use_current,
            ayanamsa: self.ayanamsa,
            day_offset: self.day_offset,
            show_planets: self.show_planets,
            show_sunrise_sunset: self.show_sunrise_sunset,
            profiles,
            active_profile_index: 0,
            active_tab: self.active_tab,
            month_offset: self.month_offset,
            year_offset: self.year_offset,
            notify_day_color: self.notify_day_color,
            notify_rahukalam: self.notify_rahukalam,
            show_rahukalam: self.show_rahukalam,
            enable_personal: self.enable_personal,
        }
    }
}

/// Multi-location shape before [`JyotishConfig::use_current`] existed.
#[derive(Debug, Serialize, Deserialize)]
struct MultiLocJyotishConfig {
    #[serde(default)]
    locations: Vec<JyotishLocation>,
    #[serde(default)]
    active_index: usize,
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

impl MultiLocJyotishConfig {
    fn into_config(self) -> JyotishConfig {
        FlatBirthJyotishConfig {
            locations: self.locations,
            active_index: self.active_index,
            use_current: false,
            ayanamsa: self.ayanamsa,
            day_offset: self.day_offset,
            show_planets: self.show_planets,
            show_sunrise_sunset: self.show_sunrise_sunset,
            birth_date: self.birth_date,
            birth_time: self.birth_time,
            birth_utc_offset_minutes: self.birth_utc_offset_minutes,
            birth_time_rectified: self.birth_time_rectified,
            active_tab: self.active_tab,
            month_offset: self.month_offset,
            year_offset: self.year_offset,
            notify_day_color: self.notify_day_color,
            notify_rahukalam: self.notify_rahukalam,
            show_rahukalam: self.show_rahukalam,
            enable_personal: self.enable_personal,
        }
        .into_config()
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

/// Decode config, accepting current, flat-birth, pre-`use_current`, and legacy blobs.
pub fn decode_config(bytes: &[u8]) -> crate::error::Result<JyotishConfig> {
    if let Ok(mut cfg) = crate::widget::config::restore_state::<JyotishConfig>(bytes) {
        cfg.normalize();
        return Ok(cfg);
    }
    if let Ok(flat) = crate::widget::config::restore_state::<FlatBirthJyotishConfig>(bytes) {
        let mut cfg = flat.into_config();
        cfg.normalize();
        return Ok(cfg);
    }
    if let Ok(multi) = crate::widget::config::restore_state::<MultiLocJyotishConfig>(bytes) {
        let mut cfg = multi.into_config();
        cfg.normalize();
        return Ok(cfg);
    }
    let legacy: LegacyJyotishConfig = crate::widget::config::restore_state(bytes)?;
    let locations = vec![JyotishLocation {
        name: legacy.location_name,
        latitude: legacy.latitude,
        longitude: legacy.longitude,
    }];
    let mut cfg = FlatBirthJyotishConfig {
        locations,
        active_index: 0,
        use_current: false,
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
    }
    .into_config();
    cfg.normalize();
    Ok(cfg)
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
        assert!(!decoded.use_current);
        assert!(decoded.profiles.is_empty());
    }

    #[test]
    fn decode_pre_use_current_multi_location_defaults_flag_false() {
        let prior = MultiLocJyotishConfig {
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
        };
        let bytes = crate::widget::config::save_state(&prior).expect("encode prior");
        let decoded = decode_config(&bytes).expect("decode prior");
        assert!(!decoded.use_current);
        assert_eq!(decoded.location_name(), "Varanasi");
    }

    #[test]
    fn default_config_use_current_is_false() {
        assert!(!JyotishConfig::default().use_current);
        assert!(JyotishConfig::default().profiles.is_empty());
    }

    #[test]
    fn decode_flat_birth_migrates_to_profile_with_observation_birth_place() {
        let prior = FlatBirthJyotishConfig {
            locations: vec![JyotishLocation {
                name: "Delhi".into(),
                latitude: 28.6139,
                longitude: 77.2090,
            }],
            active_index: 0,
            use_current: true,
            ayanamsa: AyanamsaSystem::Lahiri,
            day_offset: 0,
            show_planets: true,
            show_sunrise_sunset: true,
            birth_date: Some("1990-05-15".into()),
            birth_time: Some("14:30".into()),
            birth_utc_offset_minutes: 330,
            birth_time_rectified: true,
            active_tab: 0,
            month_offset: 0,
            year_offset: 0,
            notify_day_color: true,
            notify_rahukalam: true,
            show_rahukalam: true,
            enable_personal: true,
        };
        let bytes = crate::widget::config::save_state(&prior).expect("encode flat birth");
        let decoded = decode_config(&bytes).expect("decode flat birth");
        assert!(decoded.use_current);
        assert_eq!(decoded.profiles.len(), 1);
        assert_eq!(decoded.active_profile_index, 0);
        let p = &decoded.profiles[0];
        assert_eq!(p.birth_date.as_deref(), Some("1990-05-15"));
        assert_eq!(p.birth_time.as_deref(), Some("14:30"));
        assert_eq!(p.birth_utc_offset_minutes, 330);
        assert!(p.birth_time_rectified);
        assert_eq!(p.birth_place.name, "Delhi");
        assert_eq!(p.birth_place.latitude, 28.6139);
        assert!(decoded.has_birth_data());
    }

    #[test]
    fn roundtrip_profiles_config() {
        let cfg = JyotishConfig {
            profiles: vec![
                JyotishProfile {
                    name: "Ada".into(),
                    gender: Gender::Female,
                    birth_date: Some("1991-01-01".into()),
                    birth_time: Some("08:15".into()),
                    birth_utc_offset_minutes: 60,
                    birth_time_rectified: false,
                    birth_place: JyotishLocation {
                        name: "London".into(),
                        latitude: 51.5,
                        longitude: -0.12,
                    },
                },
                JyotishProfile {
                    name: "Bob".into(),
                    gender: Gender::Male,
                    birth_date: Some("1985-12-25".into()),
                    ..JyotishProfile::default()
                },
            ],
            active_profile_index: 1,
            ..JyotishConfig::default()
        };
        let bytes = crate::widget::config::save_state(&cfg).expect("encode");
        let decoded = decode_config(&bytes).expect("decode");
        assert_eq!(decoded.profiles.len(), 2);
        assert_eq!(decoded.active_profile_index, 1);
        assert_eq!(
            decoded.active_profile().map(|p| p.name.as_str()),
            Some("Bob")
        );
    }
}
