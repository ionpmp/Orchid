//! Value types for the system-indicators widget.

use chrono::{DateTime, Utc};

/// Full sampled snapshot.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct SystemSnapshot {
    pub cpu_total_percent: f32,
    pub cpu_per_core: Vec<f32>,
    pub cpu_temp_c: Option<f32>,
    pub memory_total_bytes: u64,
    pub memory_used_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
    pub disks: Vec<DiskUsage>,
    pub networks: Vec<NetworkRate>,
    pub battery: Option<BatteryStatus>,
    pub uptime_seconds: u64,
    pub captured_at: DateTime<Utc>,
}

/// One disk / filesystem.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct DiskUsage {
    pub mount: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub file_system: String,
    pub is_removable: bool,
}

/// One network interface with a rate sampled over the refresh interval.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct NetworkRate {
    pub interface: String,
    pub upload_bps: f64,
    pub download_bps: f64,
    pub total_uploaded_bytes: u64,
    pub total_downloaded_bytes: u64,
}

/// Battery status from [`starship_battery`], when a battery is present.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct BatteryStatus {
    pub percent: u8,
    pub charging: bool,
    pub time_to_empty_seconds: Option<u64>,
    pub time_to_full_seconds: Option<u64>,
}
