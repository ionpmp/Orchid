//! Visit frequency + recency for the file-manager path history menu.

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};

/// One recorded folder visit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
#[allow(missing_docs)]
pub struct PathVisit {
    pub path: String,
    pub visit_count: u32,
    pub last_seq: u64,
}

/// One row in the history dropdown.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct VisitMenuItem {
    pub path: String,
    pub frequent: bool,
    pub is_header: bool,
}

/// Running visit log for one file-manager instance.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VisitLog {
    entries: Vec<PathVisit>,
    seq: u64,
}

impl VisitLog {
    /// Keep this many distinct paths.
    pub const MAX_ENTRIES: usize = 40;
    /// Leading frequent rows in the menu.
    pub const FREQUENT_COUNT: usize = 5;
    /// Recent rows after the frequent block (excluding those already listed).
    pub const RECENT_COUNT: usize = 15;

    /// Restore from persisted visits.
    #[must_use]
    pub fn from_entries(entries: Vec<PathVisit>) -> Self {
        let seq = entries.iter().map(|e| e.last_seq).max().unwrap_or(0);
        Self { entries, seq }
    }

    /// Snapshot for persistence.
    #[must_use]
    pub fn entries(&self) -> &[PathVisit] {
        &self.entries
    }

    /// Record a visit to `path`. No-op for empty paths.
    pub fn record(&mut self, path: &str) {
        if path.is_empty() {
            return;
        }
        self.seq = self.seq.saturating_add(1);
        if let Some(existing) = self.entries.iter_mut().find(|e| e.path == path) {
            existing.visit_count = existing.visit_count.saturating_add(1);
            existing.last_seq = self.seq;
        } else {
            self.entries.push(PathVisit {
                path: path.to_string(),
                visit_count: 1,
                last_seq: self.seq,
            });
        }
        self.evict();
    }

    /// Menu rows: frequent header + top paths by count, then recent header + recency.
    #[must_use]
    pub fn menu_items(&self) -> Vec<VisitMenuItem> {
        if self.entries.is_empty() {
            return Vec::new();
        }
        let mut by_freq = self.entries.clone();
        by_freq.sort_by(|a, b| {
            b.visit_count
                .cmp(&a.visit_count)
                .then_with(|| b.last_seq.cmp(&a.last_seq))
        });
        let frequent: Vec<PathVisit> = by_freq.into_iter().take(Self::FREQUENT_COUNT).collect();
        let frequent_paths: std::collections::HashSet<&str> =
            frequent.iter().map(|e| e.path.as_str()).collect();

        let mut recent: Vec<PathVisit> = self
            .entries
            .iter()
            .filter(|e| !frequent_paths.contains(e.path.as_str()))
            .cloned()
            .collect();
        recent.sort_by_key(|e| std::cmp::Reverse(e.last_seq));
        recent.truncate(Self::RECENT_COUNT);

        let mut out = Vec::new();
        if !frequent.is_empty() {
            out.push(VisitMenuItem {
                path: String::new(),
                frequent: true,
                is_header: true,
            });
            for e in frequent {
                out.push(VisitMenuItem {
                    path: e.path,
                    frequent: true,
                    is_header: false,
                });
            }
        }
        if !recent.is_empty() {
            out.push(VisitMenuItem {
                path: String::new(),
                frequent: false,
                is_header: true,
            });
            for e in recent {
                out.push(VisitMenuItem {
                    path: e.path,
                    frequent: false,
                    is_header: false,
                });
            }
        }
        out
    }

    fn evict(&mut self) {
        if self.entries.len() <= Self::MAX_ENTRIES {
            return;
        }
        self.entries.sort_by_key(|e| std::cmp::Reverse(e.last_seq));
        self.entries.truncate(Self::MAX_ENTRIES);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequent_paths_lead_the_menu() {
        let mut log = VisitLog::default();
        log.record("local:/rare");
        for _ in 0..5 {
            log.record("local:/hot");
        }
        for _ in 0..3 {
            log.record("local:/warm");
        }
        log.record("local:/rare");
        let items = log.menu_items();
        let paths: Vec<&str> = items
            .iter()
            .filter(|i| !i.is_header)
            .map(|i| i.path.as_str())
            .collect();
        assert_eq!(paths[0], "local:/hot");
        assert_eq!(paths[1], "local:/warm");
        assert!(items[0].is_header && items[0].frequent);
    }

    #[test]
    fn recent_section_omits_frequent_paths() {
        let mut log = VisitLog::default();
        for p in ["local:/a", "local:/b", "local:/c", "local:/d", "local:/e"] {
            for _ in 0..3 {
                log.record(p);
            }
        }
        log.record("local:/f");
        log.record("local:/g");
        let items = log.menu_items();
        let recent: Vec<&str> = items
            .iter()
            .filter(|i| !i.is_header && !i.frequent)
            .map(|i| i.path.as_str())
            .collect();
        assert!(!recent.contains(&"local:/a"));
        assert_eq!(recent, vec!["local:/g", "local:/f"]);
    }

    #[test]
    fn empty_log_has_no_rows() {
        assert!(VisitLog::default().menu_items().is_empty());
    }
}
