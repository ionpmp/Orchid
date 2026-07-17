//! Persistent configuration for the clock / world-clocks widget.

use serde::{Deserialize, Serialize};

/// One saved world-clock city.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct ClockCity {
    pub name: String,
    /// IANA timezone id (`America/New_York`, …).
    pub timezone: String,
}

/// Persistent clock-widget config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(missing_docs)]
pub struct ClockConfig {
    /// World clocks (local time is always shown separately).
    #[serde(default)]
    pub cities: Vec<ClockCity>,
    /// Include seconds in time strings.
    #[serde(default = "default_true")]
    pub show_seconds: bool,
    /// Show the date under each city time.
    #[serde(default = "default_true")]
    pub show_dates: bool,
    /// Show UTC offsets (`UTC+7`).
    #[serde(default = "default_true")]
    pub show_offsets: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            cities: default_cities(),
            show_seconds: true,
            show_dates: true,
            show_offsets: true,
        }
    }
}

impl ClockConfig {
    /// Drop empty / duplicate timezones and keep a sane list.
    pub fn normalize(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.cities.retain(|c| {
            let tz = c.timezone.trim();
            if tz.is_empty() || !seen.insert(tz.to_string()) {
                return false;
            }
            true
        });
        for c in &mut self.cities {
            c.name = c.name.trim().to_string();
            c.timezone = c.timezone.trim().to_string();
            if c.name.is_empty() {
                c.name = c.timezone.clone();
            }
        }
    }
}

fn default_cities() -> Vec<ClockCity> {
    vec![
        ClockCity {
            name: "New York".into(),
            timezone: "America/New_York".into(),
        },
        ClockCity {
            name: "London".into(),
            timezone: "Europe/London".into(),
        },
        ClockCity {
            name: "Tokyo".into(),
            timezone: "Asia/Tokyo".into(),
        },
        ClockCity {
            name: "Sydney".into(),
            timezone: "Australia/Sydney".into(),
        },
    ]
}

/// Decode config from saved widget state.
pub fn decode_config(bytes: &[u8]) -> crate::error::Result<ClockConfig> {
    let mut cfg = crate::widget::config::restore_state::<ClockConfig>(bytes)?;
    cfg.normalize();
    Ok(cfg)
}
