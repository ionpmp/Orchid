//! Payload for the system-indicators widget.

/// Render-ready indicator payload.
#[derive(Debug, Clone)]
pub struct SystemPayload {
    /// Ordered indicator rows.
    pub indicators: Vec<SystemIndicator>,
}

/// Single indicator row: label + value + optional progress + status colour.
#[derive(Debug, Clone)]
pub struct SystemIndicator {
    /// Localised label (`"CPU"`, `"Memory"`, …).
    pub label: String,
    /// Pre-formatted value text (`"42%"`, `"1.2 GB / 8 GB"`).
    pub value_text: String,
    /// Progress-bar fraction in `0..=100`, or `None` when a bar is not
    /// appropriate for this indicator.
    pub percent: Option<f32>,
    /// Icon name.
    pub icon: &'static str,
    /// Threshold-based status.
    pub status: IndicatorStatus,
}

/// Threshold bucket driving the indicator's colour.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndicatorStatus {
    Normal,
    Warning,
    Critical,
}
