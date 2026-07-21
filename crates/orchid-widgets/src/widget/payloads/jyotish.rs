//! Payload for the Jyotish (Vedic panchanga) widget.

/// One graha (planet) row for the sidereal table.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct JyotishPlanetRow {
    /// Fluent key for the graha name (`jyotish-graha-*`).
    pub graha_key: &'static str,
    /// Fluent key for the rashi (`jyotish-rashi-*`).
    pub rashi_key: &'static str,
    /// Degrees within the rashi, e.g. `"12°34'"`.
    pub degree_text: String,
    /// Retrograde marker when applicable.
    pub is_retrograde: bool,
}

/// Render-ready Jyotish payload.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct JyotishPayload {
    pub date_text: String,
    pub location_name: String,
    pub ayanamsa_key: &'static str,
    pub ayanamsa_deg_text: String,
    pub day_offset: i32,
    pub is_today: bool,

    pub tithi_key: &'static str,
    pub paksha_key: &'static str,
    pub tithi_end_text: Option<String>,
    pub nakshatra_key: &'static str,
    pub pada: u8,
    pub nakshatra_end_text: Option<String>,
    pub yoga_key: &'static str,
    pub karana_key: &'static str,
    pub vara_key: &'static str,

    pub sunrise_time: Option<String>,
    pub sunset_time: Option<String>,

    pub planets: Vec<JyotishPlanetRow>,
    pub show_planets: bool,
    pub is_loading: bool,
}
