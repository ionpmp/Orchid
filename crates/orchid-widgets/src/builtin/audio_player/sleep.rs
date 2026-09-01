//! Sleep timer for the audio player (widget-owned; pauses the engine).

#![allow(missing_docs)]

use std::time::{Duration, Instant};

/// Sleep timer presets in minutes (`0` = off).
pub const SLEEP_PRESETS_MIN: &[u32] = &[0, 15, 30, 60, 90];

#[derive(Debug, Default)]
pub struct SleepTimer {
    pub index: u32,
    pub until: Option<Instant>,
    pub label: String,
}

impl SleepTimer {
    pub fn cycle(&mut self) {
        self.index = (self.index + 1) % SLEEP_PRESETS_MIN.len() as u32;
        self.apply_index();
    }

    pub fn set_minutes(&mut self, minutes: u32) {
        if let Some(i) = SLEEP_PRESETS_MIN.iter().position(|&m| m == minutes) {
            self.index = i as u32;
        } else if minutes == 0 {
            self.index = 0;
        } else {
            // Custom: store as nearest or as off+label — use exact deadline.
            self.index = 0;
            self.until = Some(Instant::now() + Duration::from_secs(u64::from(minutes) * 60));
            self.label = format!("Sleep {minutes}m");
            return;
        }
        self.apply_index();
    }

    fn apply_index(&mut self) {
        let mins = SLEEP_PRESETS_MIN[self.index as usize % SLEEP_PRESETS_MIN.len()];
        if mins == 0 {
            self.until = None;
            self.label.clear();
        } else {
            self.until = Some(Instant::now() + Duration::from_secs(u64::from(mins) * 60));
            self.label = format!("Sleep {mins}m");
        }
    }

    /// Update countdown label; returns `true` when the timer just fired.
    pub fn tick(&mut self) -> bool {
        let Some(until) = self.until else {
            return false;
        };
        if Instant::now() < until {
            let left = until.saturating_duration_since(Instant::now());
            let total = left.as_secs();
            let mins = total / 60;
            let secs = total % 60;
            self.label = if mins > 0 {
                format!("Sleep {mins}m{secs:02}s")
            } else {
                format!("Sleep {secs}s")
            };
            return false;
        }
        self.index = 0;
        self.until = None;
        self.label.clear();
        true
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        self.until.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_off_to_15() {
        let mut t = SleepTimer::default();
        t.cycle();
        assert_eq!(t.index, 1);
        assert!(t.is_active());
        assert!(t.label.contains("15"));
    }
}
