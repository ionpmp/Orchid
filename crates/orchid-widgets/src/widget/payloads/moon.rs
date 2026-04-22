//! Payload for the moon widget.

/// Render-ready moon payload.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct MoonPayload {
    pub phase_label: String,
    pub phase_icon: &'static str,
    pub illumination_text: String,
    pub age_text: String,
    pub distance_text: String,
    pub next_full_text: String,
    pub next_new_text: String,
    pub moonrise_text: Option<String>,
    pub moonset_text: Option<String>,
    pub sunrise_text: Option<String>,
    pub sunset_text: Option<String>,
    pub libration_text: Option<String>,
}
