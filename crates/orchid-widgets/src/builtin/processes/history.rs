//! In-memory CPU and memory samples for live processes.
//!
//! Sixty samples, oldest first. The widget does not write this to disk.

use std::collections::{HashMap, HashSet, VecDeque};

use super::types::ProcessSample;

/// How many samples a process graph keeps.
const SAMPLES: usize = 60;

#[derive(Debug, Default)]
struct Series {
    cpu: VecDeque<f32>,
    memory: VecDeque<u64>,
}

/// Recent samples for each process that was alive on the last refresh.
#[derive(Debug, Default)]
pub struct ProcessHistory {
    by_pid: HashMap<u32, Series>,
}

impl ProcessHistory {
    /// Append one sample per live process. Processes missing from `processes` are dropped.
    pub fn record(&mut self, processes: &[ProcessSample]) {
        let mut seen = HashSet::new();
        for process in processes {
            if process.pid == 0 {
                continue;
            }
            seen.insert(process.pid);
            let series = self.by_pid.entry(process.pid).or_default();
            push_f32(&mut series.cpu, process.cpu_percent.clamp(0.0, 100.0));
            push_u64(&mut series.memory, process.memory_bytes);
        }
        self.by_pid.retain(|pid, _| seen.contains(pid));
    }

    /// CPU samples in `0..=100` for `pid`.
    #[must_use]
    pub fn cpu(&self, pid: u32) -> Vec<f32> {
        self.by_pid
            .get(&pid)
            .map(|series| series.cpu.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Working-set samples scaled so the peak in the window is 100.
    #[must_use]
    pub fn memory_percent(&self, pid: u32) -> Vec<f32> {
        let Some(series) = self.by_pid.get(&pid) else {
            return Vec::new();
        };
        let peak = series.memory.iter().copied().max().unwrap_or(0);
        if peak == 0 {
            return series.memory.iter().map(|_| 0.0).collect();
        }
        series
            .memory
            .iter()
            .map(|bytes| (*bytes as f32 / peak as f32) * 100.0)
            .collect()
    }
}

fn push_f32(series: &mut VecDeque<f32>, value: f32) {
    if series.len() == SAMPLES {
        series.pop_front();
    }
    series.push_back(value);
}

fn push_u64(series: &mut VecDeque<u64>, value: u64) {
    if series.len() == SAMPLES {
        series.pop_front();
    }
    series.push_back(value);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::payloads::ProcessGroup;

    fn sample(pid: u32, cpu: f32, memory: u64) -> ProcessSample {
        ProcessSample {
            pid,
            name: "orchid".into(),
            status: String::new(),
            cpu_percent: cpu,
            memory_bytes: memory,
            io_read_bytes: 0,
            io_write_bytes: 0,
            io_read_bps: 0,
            io_write_bps: 0,
            user: String::new(),
            path: String::new(),
            parent_pid: None,
            session_id: None,
            group: ProcessGroup::Apps,
        }
    }

    #[test]
    fn keeps_sixty_samples_and_scales_memory_to_the_peak() {
        let mut history = ProcessHistory::default();
        for n in 1..=61 {
            history.record(&[sample(7, n as f32, n)]);
        }
        let cpu = history.cpu(7);
        assert_eq!(cpu.len(), 60);
        assert_eq!(cpu[0], 2.0);
        assert_eq!(cpu[59], 61.0);
        let memory = history.memory_percent(7);
        assert_eq!(memory.len(), 60);
        assert!((memory[59] - 100.0).abs() < 0.01);
        history.record(&[sample(8, 1.0, 10)]);
        assert!(history.cpu(7).is_empty());
        assert_eq!(history.cpu(8), vec![1.0]);
    }
}
