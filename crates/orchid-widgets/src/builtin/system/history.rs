//! In-memory sample window for the system widget graphs.
//!
//! Sixty samples, oldest first. The widget does not write this to disk.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::builtin::system::types::SystemSnapshot;

/// How many samples a graph keeps.
pub const HISTORY_SAMPLES: usize = 60;

/// Recent CPU, memory, disk, network, and battery samples.
#[derive(Debug, Default)]
pub struct ResourceHistory {
    cpu: VecDeque<f32>,
    memory: VecDeque<f32>,
    battery: VecDeque<f32>,
    network: HashMap<String, VecDeque<f32>>,
    disks: HashMap<String, VecDeque<f32>>,
}

impl ResourceHistory {
    /// Append one snapshot. Missing disks and interfaces are dropped.
    pub fn record(&mut self, snap: &SystemSnapshot) {
        push(&mut self.cpu, snap.cpu_total_percent.clamp(0.0, 100.0));
        if snap.memory_total_bytes > 0 {
            let pct = (snap.memory_used_bytes as f32 / snap.memory_total_bytes as f32) * 100.0;
            push(&mut self.memory, pct.clamp(0.0, 100.0));
        }
        if let Some(battery) = &snap.battery {
            push(
                &mut self.battery,
                f32::from(battery.percent).clamp(0.0, 100.0),
            );
        }

        let mut nics = HashSet::new();
        for nic in &snap.networks {
            let bps = (nic.upload_bps.max(0.0) + nic.download_bps.max(0.0)) as f32;
            let series = self.network.entry(nic.interface.clone()).or_default();
            push(series, bps);
            nics.insert(nic.interface.clone());
        }
        self.network.retain(|name, _| nics.contains(name));

        let mut mounts = HashSet::new();
        for disk in &snap.disks {
            if disk.total_bytes == 0 {
                continue;
            }
            let pct = (disk.used_bytes as f32 / disk.total_bytes as f32) * 100.0;
            let series = self.disks.entry(disk.mount.clone()).or_default();
            push(series, pct.clamp(0.0, 100.0));
            mounts.insert(disk.mount.clone());
        }
        self.disks.retain(|mount, _| mounts.contains(mount));
    }

    /// CPU samples in `0..=100`.
    #[must_use]
    pub fn cpu(&self) -> Vec<f32> {
        self.cpu.iter().copied().collect()
    }

    /// Memory samples in `0..=100`.
    #[must_use]
    pub fn memory(&self) -> Vec<f32> {
        self.memory.iter().copied().collect()
    }

    /// Battery samples in `0..=100`.
    #[must_use]
    pub fn battery(&self) -> Vec<f32> {
        self.battery.iter().copied().collect()
    }

    /// Disk samples in `0..=100` for one mount.
    #[must_use]
    pub fn disk(&self, mount: &str) -> Vec<f32> {
        self.disks
            .get(mount)
            .map(|series| series.iter().copied().collect())
            .unwrap_or_default()
    }

    /// One interface, scaled so the peak in the window is 100.
    #[must_use]
    pub fn network(&self, interface: &str) -> Vec<f32> {
        self.network
            .get(interface)
            .map(|series| scale_peak(series.iter().copied().collect()))
            .unwrap_or_default()
    }

    /// Sum of the named interfaces, then scaled so the peak is 100.
    #[must_use]
    pub fn network_sum(&self, interfaces: &[String]) -> Vec<f32> {
        let series: Vec<&VecDeque<f32>> = interfaces
            .iter()
            .filter_map(|name| self.network.get(name))
            .collect();
        scale_peak(sum_aligned(&series))
    }
}

fn push(series: &mut VecDeque<f32>, value: f32) {
    if series.len() == HISTORY_SAMPLES {
        series.pop_front();
    }
    series.push_back(value);
}

fn sum_aligned(series: &[&VecDeque<f32>]) -> Vec<f32> {
    let len = series.iter().map(|s| s.len()).max().unwrap_or(0);
    let mut out = vec![0.0; len];
    for one in series {
        let pad = len - one.len();
        for (i, value) in one.iter().enumerate() {
            out[pad + i] += *value;
        }
    }
    out
}

fn scale_peak(samples: Vec<f32>) -> Vec<f32> {
    let peak = samples.iter().copied().fold(0.0_f32, f32::max);
    if peak <= f32::EPSILON {
        return samples.iter().map(|_| 0.0).collect();
    }
    samples
        .into_iter()
        .map(|value| (value / peak) * 100.0)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::system::types::{BatteryStatus, DiskUsage, NetworkRate};
    use chrono::Utc;

    fn snap(cpu: f32, down: f64) -> SystemSnapshot {
        SystemSnapshot {
            cpu_total_percent: cpu,
            cpu_per_core: Vec::new(),
            cpu_temp_c: None,
            memory_total_bytes: 100,
            memory_used_bytes: 40,
            swap_total_bytes: 0,
            swap_used_bytes: 0,
            disks: vec![DiskUsage {
                mount: "C:".into(),
                total_bytes: 200,
                used_bytes: 50,
                file_system: "NTFS".into(),
                is_removable: false,
            }],
            networks: vec![NetworkRate {
                interface: "Ethernet".into(),
                upload_bps: 0.0,
                download_bps: down,
                total_uploaded_bytes: 1,
                total_downloaded_bytes: 1,
            }],
            battery: Some(BatteryStatus {
                percent: 80,
                charging: false,
                time_to_empty_seconds: None,
                time_to_full_seconds: None,
            }),
            uptime_seconds: 1,
            captured_at: Utc::now(),
        }
    }

    #[test]
    fn window_keeps_sixty_and_scales_network_to_the_peak() {
        let mut history = ResourceHistory::default();
        history.record(&snap(10.0, 25.0));
        history.record(&snap(50.0, 100.0));
        assert_eq!(history.cpu(), vec![10.0, 50.0]);
        assert_eq!(history.memory(), vec![40.0, 40.0]);
        assert_eq!(history.disk("C:"), vec![25.0, 25.0]);
        assert_eq!(history.battery(), vec![80.0, 80.0]);
        let net = history.network_sum(&["Ethernet".into()]);
        assert!((net[0] - 25.0).abs() < 0.01);
        assert!((net[1] - 100.0).abs() < 0.01);

        for i in 0..HISTORY_SAMPLES {
            history.record(&snap(i as f32, 1.0));
        }
        assert_eq!(history.cpu().len(), HISTORY_SAMPLES);
        assert_eq!(*history.cpu().last().unwrap(), (HISTORY_SAMPLES - 1) as f32);
    }
}
