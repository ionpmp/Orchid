//! File-manager selection model.

use std::collections::HashSet;

/// Selection state for one tab. Paths stored as canonical strings.
#[derive(Debug, Clone, Default)]
pub struct SelectionModel {
    selected: HashSet<String>,
    anchor: Option<String>,
}

impl SelectionModel {
    /// Empty selection.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop every selection.
    pub fn clear(&mut self) {
        self.selected.clear();
        self.anchor = None;
    }

    /// Replace the selection with a single path.
    pub fn select_single(&mut self, path: &str) {
        self.selected.clear();
        self.selected.insert(path.to_string());
        self.anchor = Some(path.to_string());
    }

    /// Toggle a path.
    pub fn toggle(&mut self, path: &str) {
        if !self.selected.remove(path) {
            self.selected.insert(path.to_string());
            self.anchor = Some(path.to_string());
        }
    }

    /// Set an anchor without changing the selection.
    pub fn set_anchor(&mut self, path: &str) {
        self.anchor = Some(path.to_string());
    }

    /// Extend the selection to `path`, treating the current anchor as the
    /// other end of the range. No-op when no anchor is set.
    pub fn extend_to(&mut self, ordered: &[String], to: &str) {
        let Some(anchor) = self.anchor.clone() else {
            self.select_single(to);
            return;
        };
        let ia = ordered.iter().position(|p| p == &anchor);
        let ib = ordered.iter().position(|p| p == to);
        if let (Some(a), Some(b)) = (ia, ib) {
            let (lo, hi) = (a.min(b), a.max(b));
            self.selected.clear();
            for p in &ordered[lo..=hi] {
                self.selected.insert(p.clone());
            }
        }
    }

    /// Whether the path is currently selected.
    #[must_use]
    pub fn is_selected(&self, path: &str) -> bool {
        self.selected.contains(path)
    }

    /// Snapshot of selected paths. Order is unspecified.
    #[must_use]
    pub fn selected_paths(&self) -> Vec<String> {
        self.selected.iter().cloned().collect()
    }

    /// Number of selected paths.
    #[must_use]
    pub fn count(&self) -> usize {
        self.selected.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_flips_membership() {
        let mut s = SelectionModel::new();
        s.toggle("a");
        assert!(s.is_selected("a"));
        s.toggle("a");
        assert!(!s.is_selected("a"));
    }

    #[test]
    fn extend_to_builds_range_from_anchor() {
        let mut s = SelectionModel::new();
        let ordered = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        s.select_single("b");
        s.extend_to(&ordered, "d");
        assert_eq!(s.count(), 3);
        assert!(s.is_selected("b"));
        assert!(s.is_selected("c"));
        assert!(s.is_selected("d"));
    }

    #[test]
    fn select_single_replaces() {
        let mut s = SelectionModel::new();
        s.toggle("a");
        s.toggle("b");
        s.select_single("c");
        assert_eq!(s.count(), 1);
        assert!(s.is_selected("c"));
    }
}
