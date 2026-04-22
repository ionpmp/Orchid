//! `sysinfo`-backed system-metrics provider.

use std::time::Instant;

use chrono::Utc;
use parking_lot::Mutex;
use sysinfo::{Disks, Networks, System};

use super::types::{DiskUsage, NetworkRate, SystemSnapshot};

/// Snapshot of the previous network counters, used to compute rates.
#[derive(Debug, Clone, Default)]
struct NetworkPrev {
    totals: std::collections::HashMap<String, (u64, u64)>, // (recv, sent)
    at: Option<Instant>,
}

/// Provider owning a [`sysinfo::System`] handle plus previous network
/// counters for rate calculation. Not `Clone` (the inner `System` holds a
/// large buffer).
pub struct SystemProvider {
    system: Mutex<System>,
    disks: Mutex<Disks>,
    networks: Mutex<Networks>,
    previous: Mutex<NetworkPrev>,
}

impl std::fmt::Debug for SystemProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemProvider").finish_non_exhaustive()
    }
}

impl Default for SystemProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemProvider {
    /// New provider with refreshed disks + networks lists.
    #[must_use]
    pub fn new() -> Self {
        let mut system = System::new();
        system.refresh_all();
        let disks = Disks::new_with_refreshed_list();
        let networks = Networks::new_with_refreshed_list();
        Self {
            system: Mutex::new(system),
            disks: Mutex::new(disks),
            networks: Mutex::new(networks),
            previous: Mutex::new(NetworkPrev::default()),
        }
    }

    /// Refresh and produce a fresh snapshot.
    pub fn refresh(&self) -> SystemSnapshot {
        let captured_at = Utc::now();
        let now = Instant::now();

        let mut system = self.system.lock();
        system.refresh_cpu_usage();
        system.refresh_memory();

        let cpu_total = system.global_cpu_usage();
        let cpu_per_core = system
            .cpus()
            .iter()
            .map(|c| c.cpu_usage())
            .collect::<Vec<_>>();

        let memory_total = system.total_memory();
        let memory_used = system.used_memory();
        let swap_total = system.total_swap();
        let swap_used = system.used_swap();
        let uptime_seconds = System::uptime();
        drop(system);

        let mut disks = self.disks.lock();
        disks.refresh();
        let disk_usages = disks
            .iter()
            .map(|d| DiskUsage {
                mount: d.mount_point().to_string_lossy().into_owned(),
                total_bytes: d.total_space(),
                used_bytes: d.total_space().saturating_sub(d.available_space()),
                file_system: d.file_system().to_string_lossy().into_owned(),
                is_removable: d.is_removable(),
            })
            .collect::<Vec<_>>();
        drop(disks);

        let mut networks = self.networks.lock();
        networks.refresh();
        let mut prev = self.previous.lock();
        let elapsed = prev
            .at
            .map(|t| now.duration_since(t).as_secs_f64())
            .unwrap_or(0.0)
            .max(1e-3);
        let mut new_totals = std::collections::HashMap::new();
        let mut network_rates = Vec::new();
        for (name, data) in networks.iter() {
            let total_rx = data.total_received();
            let total_tx = data.total_transmitted();
            let (prev_rx, prev_tx) = prev
                .totals
                .get(name)
                .copied()
                .unwrap_or((total_rx, total_tx));
            let rx_bps = (total_rx.saturating_sub(prev_rx)) as f64 / elapsed;
            let tx_bps = (total_tx.saturating_sub(prev_tx)) as f64 / elapsed;
            new_totals.insert(name.clone(), (total_rx, total_tx));
            network_rates.push(NetworkRate {
                interface: name.clone(),
                upload_bps: tx_bps,
                download_bps: rx_bps,
                total_uploaded_bytes: total_tx,
                total_downloaded_bytes: total_rx,
            });
        }
        prev.totals = new_totals;
        prev.at = Some(now);
        drop(prev);
        drop(networks);

        SystemSnapshot {
            cpu_total_percent: cpu_total,
            cpu_per_core,
            cpu_temp_c: None,
            memory_total_bytes: memory_total,
            memory_used_bytes: memory_used,
            swap_total_bytes: swap_total,
            swap_used_bytes: swap_used,
            disks: disk_usages,
            networks: network_rates,
            battery: None,
            uptime_seconds,
            captured_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_returns_non_empty_snapshot() {
        let p = SystemProvider::new();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let snap = p.refresh();
        assert!(snap.memory_total_bytes > 0);
        assert!(!snap.cpu_per_core.is_empty() || snap.cpu_total_percent.is_finite());
    }
}
