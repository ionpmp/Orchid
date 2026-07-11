//! File-manager selection model.

use std::collections::HashSet;

/// Selection state for one tab. Paths stored as canonical strings.
#[derive(Debug, Clone, Default)]
pub struct SelectionModel {
    selected: HashSet<String>,
    anchor: Option<String>,
    /// Keyboard / range endpoint opposite the anchor (Shift+Arrow lead).
    lead: Option<String>,
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
        self.lead = None;
    }

    /// Replace the selection with a single path.
    pub fn select_single(&mut self, path: &str) {
        self.selected.clear();
        self.selected.insert(path.to_string());
        self.anchor = Some(path.to_string());
        self.lead = Some(path.to_string());
    }

    /// Select every path in `ordered` (typically the visible listing).
    pub fn select_all(&mut self, ordered: &[String]) {
        self.selected.clear();
        for p in ordered {
            self.selected.insert(p.clone());
        }
        if let Some(first) = ordered.first() {
            self.anchor = Some(first.clone());
            self.lead = ordered.last().cloned();
        } else {
            self.anchor = None;
            self.lead = None;
        }
    }

    /// Toggle a path.
    pub fn toggle(&mut self, path: &str) {
        if !self.selected.remove(path) {
            self.selected.insert(path.to_string());
            self.anchor = Some(path.to_string());
            self.lead = Some(path.to_string());
        } else if self.lead.as_deref() == Some(path) {
            self.lead = self.anchor.clone();
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
            self.lead = Some(to.to_string());
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

    /// Current selection anchor, if any.
    #[must_use]
    pub fn anchor(&self) -> Option<&str> {
        self.anchor.as_deref()
    }

    /// Move the selection by `delta` steps within `ordered` (visible listing).
    ///
    /// When nothing is selected, a positive `delta` selects the first entry and a
    /// negative `delta` selects the last. With `extend`, grows a range from the
    /// existing anchor while moving the lead (Shift+Arrow).
    pub fn select_relative(&mut self, ordered: &[String], delta: i32, extend: bool) {
        if ordered.is_empty() {
            return;
        }
        let current = self
            .lead
            .as_ref()
            .or(self.anchor.as_ref())
            .and_then(|a| ordered.iter().position(|p| p == a))
            .or_else(|| ordered.iter().position(|p| self.selected.contains(p)));
        let next = match current {
            None => {
                if delta >= 0 {
                    0
                } else {
                    ordered.len().saturating_sub(1)
                }
            }
            Some(i) => {
                let ni = (i as i32).saturating_add(delta);
                ni.clamp(0, ordered.len().saturating_sub(1) as i32) as usize
            }
        };
        let target = &ordered[next];
        if extend {
            if self.anchor.is_none() {
                if let Some(i) = current {
                    self.anchor = Some(ordered[i].clone());
                } else {
                    self.anchor = Some(target.clone());
                }
            }
            self.extend_to(ordered, target);
        } else {
            self.select_single(target);
        }
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

    #[test]
    fn select_all_replaces_with_every_path() {
        let mut s = SelectionModel::new();
        s.toggle("a");
        let ordered = vec!["x".into(), "y".into(), "z".into()];
        s.select_all(&ordered);
        assert_eq!(s.count(), 3);
        assert!(s.is_selected("z"));
    }

    #[test]
    fn select_relative_moves_and_clamps() {
        let mut s = SelectionModel::new();
        let ordered = vec!["a".into(), "b".into(), "c".into()];
        s.select_relative(&ordered, 1, false);
        assert!(s.is_selected("a"));
        s.select_relative(&ordered, 1, false);
        assert!(s.is_selected("b"));
        s.select_relative(&ordered, 10, false);
        assert!(s.is_selected("c"));
        s.select_relative(&ordered, -1, false);
        assert!(s.is_selected("b"));
    }

    #[test]
    fn select_relative_extend_moves_lead() {
        let mut s = SelectionModel::new();
        let ordered = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        s.select_single("b");
        s.select_relative(&ordered, 1, true);
        assert_eq!(s.count(), 2);
        assert!(s.is_selected("b"));
        assert!(s.is_selected("c"));
        s.select_relative(&ordered, 1, true);
        assert_eq!(s.count(), 3);
        assert!(s.is_selected("d"));
        assert_eq!(s.anchor(), Some("b"));
        s.select_relative(&ordered, -1, true);
        assert_eq!(s.count(), 2);
        assert!(s.is_selected("b"));
        assert!(s.is_selected("c"));
        assert!(!s.is_selected("d"));
    }

    #[test]
    fn select_relative_empty_list_is_noop() {
        let mut s = SelectionModel::new();
        s.select_relative(&[], 1, false);
        assert_eq!(s.count(), 0);
    }
}
