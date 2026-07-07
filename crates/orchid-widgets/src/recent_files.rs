//! Application-wide recent-files list shared by the file manager and the
//! recent-files widget.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::SystemTime;

use orchid_core::{Event, EventBus, EventSource};
use orchid_fs::FsPath;
use parking_lot::RwLock;

/// One recently opened file path.
#[derive(Debug, Clone)]
pub struct RecentFileEntry {
    /// Normalized filesystem path.
    pub path: String,
    /// When the path was last opened.
    pub opened_at: SystemTime,
}

/// Published when the recent-files list changes.
#[derive(Debug, Clone, Copy, Default)]
pub struct RecentFilesUpdated;

impl Event for RecentFilesUpdated {
    fn event_type() -> &'static str {
        "recent_files.updated"
    }
}

/// Thread-safe recent-files store (MRU, capped length).
#[derive(Debug)]
pub struct RecentFilesStore {
    inner: RwLock<VecDeque<RecentFileEntry>>,
    max_entries: usize,
}

impl RecentFilesStore {
    /// Create a store retaining at most `max_entries` paths.
    #[must_use]
    pub fn new(max_entries: usize) -> Arc<Self> {
        Arc::new(Self {
            inner: RwLock::new(VecDeque::new()),
            max_entries: max_entries.max(1),
        })
    }

    /// Record a file path as recently opened. Directories and virtual paths are ignored.
    pub fn touch(&self, path: &FsPath, bus: Option<&EventBus>) {
        if is_ignored_path(path) {
            return;
        }
        let s = path.as_str().to_string();
        let now = SystemTime::now();
        let mut changed = false;
        {
            let mut recent = self.inner.write();
            if recent.front().map(|e| e.path.as_str()) != Some(s.as_str()) {
                recent.retain(|e| e.path != s);
                recent.push_front(RecentFileEntry {
                    path: s,
                    opened_at: now,
                });
                while recent.len() > self.max_entries {
                    recent.pop_back();
                }
                changed = true;
            }
        }
        if changed {
            if let Some(bus) = bus {
                bus.publish(EventSource::System, RecentFilesUpdated);
            }
        }
    }

    /// Return the most recent paths, newest first.
    #[must_use]
    pub fn list(&self, limit: usize) -> Vec<RecentFileEntry> {
        self.inner
            .read()
            .iter()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Paths only, for virtual-folder listing.
    #[must_use]
    pub fn paths(&self) -> Vec<String> {
        self.inner.read().iter().map(|e| e.path.clone()).collect()
    }
}

fn is_ignored_path(path: &FsPath) -> bool {
    let raw = path.as_str();
    if raw.starts_with("virtual:") {
        return true;
    }
    raw.ends_with('/') || raw.ends_with('\\')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_moves_path_to_front_and_caps_length() {
        let store = RecentFilesStore::new(2);
        let a = FsPath::new("local:/a.txt").unwrap();
        let b = FsPath::new("local:/b.txt").unwrap();
        let c = FsPath::new("local:/c.txt").unwrap();
        store.touch(&a, None);
        store.touch(&b, None);
        store.touch(&c, None);
        let paths: Vec<_> = store.paths();
        assert_eq!(paths, vec!["local:/c.txt", "local:/b.txt"]);
    }
}
