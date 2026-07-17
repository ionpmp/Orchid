//! Config for the system-indicators widget.

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

/// Persistent system-widget config.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
#[allow(missing_docs)]
pub struct SystemConfig {
    pub show_cpu: bool,
    pub show_memory: bool,
    pub show_disks: bool,
    pub show_network: bool,
    pub show_battery: bool,
    pub show_uptime: bool,
    pub refresh_interval_seconds: u32,
    pub network_interfaces: Vec<String>,
    pub disks: Vec<String>,
    /// Show per-core utilisation bars under the CPU row.
    #[serde(default = "default_true")]
    pub show_cpu_cores: bool,
    /// Collapse all NICs into a single up/down total (recommended).
    #[serde(default = "default_true")]
    pub aggregate_network: bool,
    /// Include removable volumes when no explicit disk filter is set.
    #[serde(default)]
    pub show_removable_disks: bool,
}

fn default_true() -> bool {
    true
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            show_cpu: true,
            show_memory: true,
            show_disks: true,
            show_network: true,
            show_battery: true,
            show_uptime: true,
            refresh_interval_seconds: 2,
            network_interfaces: Vec::new(),
            disks: Vec::new(),
            show_cpu_cores: true,
            aggregate_network: true,
            show_removable_disks: false,
        }
    }
}

impl SystemConfig {
    /// Clamp invalid values after restore / settings edits.
    pub fn normalize(&mut self) {
        if self.refresh_interval_seconds == 0 {
            self.refresh_interval_seconds = 1;
        }
    }
}
