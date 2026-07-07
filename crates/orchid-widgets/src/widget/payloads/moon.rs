//! Payload for the moon widget.

/// Render-ready moon payload.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct MoonPayload {
    /// Fluent key for the phase label (`moon-phase-*`).
    pub phase_key: &'static str,
    pub phase_icon: &'static str,
    pub illumination_percent: Option<f32>,
    pub age_days: Option<f32>,
    pub distance_km: Option<f64>,
    pub next_full_date: Option<String>,
    pub next_new_date: Option<String>,
    pub moonrise_time: Option<String>,
    pub moonset_time: Option<String>,
    pub sunrise_time: Option<String>,
    pub sunset_time: Option<String>,
    pub libration_lat_deg: Option<f64>,
    pub libration_lon_deg: Option<f64>,
    /// `true` until the first calculation completes.
    pub is_loading: bool,
}
