//! Payload for the system-indicators widget.

/// Indicator row kind — used by the UI layer for i18n.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemIndicatorKind {
    /// CPU utilisation.
    Cpu,
    /// RAM usage.
    Memory,
    /// Disk usage for a mount point.
    Disk,
    /// Network throughput for an interface.
    Network,
    /// Battery level.
    Battery,
    /// System uptime.
    Uptime,
}

/// Render-ready indicator payload.
#[derive(Debug, Clone)]
pub struct SystemPayload {
    /// Ordered indicator rows.
    pub indicators: Vec<SystemIndicator>,
    /// `true` until the first metrics sample is available.
    pub is_loading: bool,
}

/// Single indicator row: label + value + optional progress + status colour.
#[derive(Debug, Clone)]
pub struct SystemIndicator {
    /// Row kind — drives label i18n in the UI layer.
    pub kind: SystemIndicatorKind,
    /// Mount point or interface name when relevant.
    pub name_suffix: Option<String>,
    /// Pre-formatted value text for simple indicators.
    pub value_text: String,
    /// Network upload rate (formatted), when [`SystemIndicatorKind::Network`].
    pub network_up: Option<String>,
    /// Network download rate (formatted), when [`SystemIndicatorKind::Network`].
    pub network_down: Option<String>,
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
