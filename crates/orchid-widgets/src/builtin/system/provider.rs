//! `sysinfo`-backed system-metrics provider.

use std::time::Instant;

use chrono::Utc;
use parking_lot::Mutex;
use starship_battery::{Manager, State};
use sysinfo::{Disks, Networks, System, MINIMUM_CPU_UPDATE_INTERVAL};

use super::types::{BatteryStatus, DiskUsage, NetworkRate, SystemSnapshot};

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
    /// Last time [`System::refresh_cpu_usage`] ran. CPU % is a delta metric;
    /// Windows PDH in particular returns garbage (often 0% idle → 100% busy)
    /// when samples are taken closer than [`MINIMUM_CPU_UPDATE_INTERVAL`].
    last_cpu_refresh: Mutex<Option<Instant>>,
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
    ///
    /// Performs a baseline CPU sample only — that reading is not trusted.
    /// The next [`Self::refresh`] waits for [`MINIMUM_CPU_UPDATE_INTERVAL`]
    /// so the first published value is meaningful.
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
            last_cpu_refresh: Mutex::new(Some(Instant::now())),
        }
    }

    /// Refresh and produce a fresh snapshot.
    pub fn refresh(&self) -> SystemSnapshot {
        let captured_at = Utc::now();
        let now = Instant::now();

        // Wait outside the System lock so concurrent callers don't pile up
        // while holding the mutex during the mandatory CPU sample gap.
        if let Some(prev) = *self.last_cpu_refresh.lock() {
            let elapsed = prev.elapsed();
            if elapsed < MINIMUM_CPU_UPDATE_INTERVAL {
                std::thread::sleep(MINIMUM_CPU_UPDATE_INTERVAL - elapsed);
            }
        }

        let mut system = self.system.lock();
        system.refresh_cpu_usage();
        *self.last_cpu_refresh.lock() = Some(Instant::now());
        system.refresh_memory();

        // Clamp: PDH can theoretically return values slightly outside 0–100.
        let cpu_total = system.global_cpu_usage().clamp(0.0, 100.0);
        let cpu_per_core = system
            .cpus()
            .iter()
            .map(|c| c.cpu_usage().clamp(0.0, 100.0))
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
            battery: sample_battery(),
            uptime_seconds,
            captured_at,
        }
    }
}

fn sample_battery() -> Option<BatteryStatus> {
    use starship_battery::units::ratio::ratio;
    use starship_battery::units::time::second;

    let manager = Manager::new().ok()?;
    let mut batteries = manager.batteries().ok()?;
    let bat = batteries.next()?.ok()?;
    let percent = (bat.state_of_charge().get::<ratio>() * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8;
    let charging = matches!(bat.state(), State::Charging);
    let time_to_empty_seconds = bat
        .time_to_empty()
        .map(|t| t.get::<second>().max(0.0).round() as u64);
    let time_to_full_seconds = bat
        .time_to_full()
        .map(|t| t.get::<second>().max(0.0).round() as u64);
    Some(BatteryStatus {
        percent,
        charging,
        time_to_empty_seconds,
        time_to_full_seconds,
    })
}

/// Whether a NIC name looks like loopback / tunnel noise we should hide by default.
#[must_use]
pub fn is_noisy_nic(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "lo"
        || lower.starts_with("lo:")
        || lower.contains("loopback")
        || lower.contains("isatap")
        || lower.contains("teredo")
        || lower.starts_with("veth")
        || lower.starts_with("br-")
        || lower.starts_with("docker")
}

/// Whether a disk should appear when no explicit mount filter is set.
#[must_use]
pub fn is_default_disk(d: &DiskUsage, include_removable: bool) -> bool {
    const MIN_BYTES: u64 = 1 << 30; // 1 GiB
    if d.total_bytes < MIN_BYTES {
        return false;
    }
    if d.is_removable && !include_removable {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_returns_non_empty_snapshot() {
        let p = SystemProvider::new();
        let snap = p.refresh();
        assert!(snap.memory_total_bytes > 0);
        assert!(!snap.cpu_per_core.is_empty() || snap.cpu_total_percent.is_finite());
        assert!(
            (0.0..=100.0).contains(&snap.cpu_total_percent),
            "cpu_total_percent out of range: {}",
            snap.cpu_total_percent
        );
    }

    #[test]
    fn consecutive_refreshes_produce_valid_cpu() {
        let p = SystemProvider::new();
        let first = p.refresh();
        let again = p.refresh();
        assert!((0.0..=100.0).contains(&first.cpu_total_percent));
        assert!((0.0..=100.0).contains(&again.cpu_total_percent));
        let _ = MINIMUM_CPU_UPDATE_INTERVAL;
    }

    #[test]
    fn noisy_nic_filter_matches_loopback() {
        assert!(is_noisy_nic("lo"));
        assert!(is_noisy_nic("Loopback Pseudo-Interface 1"));
        assert!(is_noisy_nic("isatap.example"));
        assert!(!is_noisy_nic("eth0"));
        assert!(!is_noisy_nic("Wi-Fi"));
        assert!(!is_noisy_nic("Ethernet"));
    }

    #[test]
    fn default_disk_skips_tiny_and_removable() {
        let big = DiskUsage {
            mount: "C:\\".into(),
            total_bytes: 100 << 30,
            used_bytes: 50 << 30,
            file_system: "NTFS".into(),
            is_removable: false,
        };
        assert!(is_default_disk(&big, false));
        assert!(!is_default_disk(
            &DiskUsage {
                mount: "D:\\".into(),
                total_bytes: 100 << 20,
                used_bytes: 10 << 20,
                file_system: "FAT32".into(),
                is_removable: false,
            },
            false
        ));
        let usb = DiskUsage {
            mount: "E:\\".into(),
            total_bytes: 64 << 30,
            used_bytes: 1 << 30,
            file_system: "exFAT".into(),
            is_removable: true,
        };
        assert!(!is_default_disk(&usb, false));
        assert!(is_default_disk(&usb, true));
    }
}
