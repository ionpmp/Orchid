//! Moon widget persistent configuration.

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

/// Persistent moon-widget config.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
#[allow(missing_docs)]
pub struct MoonConfig {
    pub latitude: f64,
    pub longitude: f64,
    pub location_name: String,
    pub show_sunrise_sunset: bool,
    pub show_libration: bool,
}

impl Default for MoonConfig {
    fn default() -> Self {
        Self {
            latitude: -6.9667,
            longitude: 110.4167,
            location_name: "Semarang".into(),
            show_sunrise_sunset: true,
            show_libration: false,
        }
    }
}
