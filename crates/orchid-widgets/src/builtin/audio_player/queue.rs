//! Play queue with shuffle / repeat.

#![allow(missing_docs)]

use super::config::RepeatMode;

/// Mutable play queue state (in-memory; paths also mirrored to config).
#[derive(Debug, Clone, Default)]
pub struct PlayQueue {
    pub paths: Vec<String>,
    pub index: usize,
    /// Shuffle order: permutation of indices into `paths`.
    pub order: Vec<usize>,
    pub shuffle: bool,
    pub repeat: RepeatMode,
}

impl PlayQueue {
    #[must_use]
    pub fn from_paths(paths: Vec<String>, index: usize, shuffle: bool, repeat: RepeatMode) -> Self {
        let mut q = Self {
            index: index.min(paths.len().saturating_sub(1)),
            paths,
            order: Vec::new(),
            shuffle,
            repeat,
        };
        q.rebuild_order();
        q
    }

    pub fn rebuild_order(&mut self) {
        self.order = (0..self.paths.len()).collect();
        if self.shuffle && self.paths.len() > 1 {
            // Fisher–Yates with a simple LCG seeded from length + index.
            let mut state = (self.paths.len() as u64)
                .wrapping_mul(0x9E37_79B9)
                .wrapping_add(self.index as u64);
            for i in (1..self.order.len()).rev() {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1);
                let j = (state as usize) % (i + 1);
                self.order.swap(i, j);
            }
            // Keep current track first in shuffle order when possible.
            if let Some(pos) = self.order.iter().position(|&i| i == self.index) {
                self.order.swap(0, pos);
            }
        }
    }

    #[must_use]
    pub fn current(&self) -> Option<&str> {
        self.paths.get(self.index).map(String::as_str)
    }

    /// Path that would play after [`Self::next`] without mutating state.
    #[must_use]
    pub fn peek_next(&self) -> Option<&str> {
        if self.paths.is_empty() {
            return None;
        }
        match self.repeat {
            RepeatMode::One => return self.current(),
            RepeatMode::Off | RepeatMode::All => {}
        }
        if self.shuffle {
            let pos = self
                .order
                .iter()
                .position(|&i| i == self.index)
                .unwrap_or(0);
            if pos + 1 < self.order.len() {
                return self.paths.get(self.order[pos + 1]).map(String::as_str);
            }
            if matches!(self.repeat, RepeatMode::All) && !self.order.is_empty() {
                // After rebuild the first of a new order is unpredictable; skip prefetch.
                return None;
            }
            return None;
        }
        if self.index + 1 < self.paths.len() {
            return self.paths.get(self.index + 1).map(String::as_str);
        }
        if matches!(self.repeat, RepeatMode::All) {
            return self.paths.first().map(String::as_str);
        }
        None
    }

    /// Advance to next track. Returns the new path, or `None` if playback should stop.
    pub fn next(&mut self) -> Option<&str> {
        if self.paths.is_empty() {
            return None;
        }
        match self.repeat {
            RepeatMode::One => return self.current(),
            RepeatMode::Off | RepeatMode::All => {}
        }
        if self.shuffle {
            let pos = self
                .order
                .iter()
                .position(|&i| i == self.index)
                .unwrap_or(0);
            if pos + 1 < self.order.len() {
                self.index = self.order[pos + 1];
                return self.current();
            }
            if matches!(self.repeat, RepeatMode::All) {
                self.rebuild_order();
                self.index = self.order.first().copied().unwrap_or(0);
                return self.current();
            }
            return None;
        }
        if self.index + 1 < self.paths.len() {
            self.index += 1;
            return self.current();
        }
        if matches!(self.repeat, RepeatMode::All) {
            self.index = 0;
            return self.current();
        }
        None
    }

    /// Go to previous track (or restart current).
    pub fn previous(&mut self) -> Option<&str> {
        if self.paths.is_empty() {
            return None;
        }
        if self.shuffle {
            let pos = self
                .order
                .iter()
                .position(|&i| i == self.index)
                .unwrap_or(0);
            if pos > 0 {
                self.index = self.order[pos - 1];
            }
            return self.current();
        }
        if self.index > 0 {
            self.index -= 1;
        }
        self.current()
    }

    pub fn replace(&mut self, paths: Vec<String>, start: usize) {
        self.paths = paths;
        self.index = start.min(self.paths.len().saturating_sub(1));
        self.rebuild_order();
    }

    pub fn play_at(&mut self, path: &str) -> bool {
        if let Some(i) = self.paths.iter().position(|p| p == path) {
            self.index = i;
            true
        } else {
            self.paths.push(path.to_string());
            self.index = self.paths.len() - 1;
            self.rebuild_order();
            true
        }
    }

    pub fn set_shuffle(&mut self, shuffle: bool) {
        self.shuffle = shuffle;
        self.rebuild_order();
    }

    pub fn cycle_repeat(&mut self) {
        self.repeat = self.repeat.cycle();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_with_repeat_all_wraps() {
        let mut q = PlayQueue::from_paths(
            vec!["a".into(), "b".into()],
            1,
            false,
            RepeatMode::All,
        );
        assert_eq!(q.next(), Some("a"));
    }

    #[test]
    fn next_off_stops_at_end() {
        let mut q = PlayQueue::from_paths(
            vec!["a".into(), "b".into()],
            1,
            false,
            RepeatMode::Off,
        );
        assert!(q.next().is_none());
    }

    #[test]
    fn repeat_one_stays() {
        let mut q = PlayQueue::from_paths(
            vec!["a".into(), "b".into()],
            0,
            false,
            RepeatMode::One,
        );
        assert_eq!(q.next(), Some("a"));
    }

    #[test]
    fn peek_next_follows_linear_order() {
        let q = PlayQueue::from_paths(
            vec!["a".into(), "b".into(), "c".into()],
            0,
            false,
            RepeatMode::Off,
        );
        assert_eq!(q.peek_next(), Some("b"));
        let mut q = q;
        assert_eq!(q.next(), Some("b"));
        assert_eq!(q.peek_next(), Some("c"));
    }
}
