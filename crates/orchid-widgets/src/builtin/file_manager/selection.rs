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

/// How a name/attribute mask is applied to the current selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskOp {
    /// Replace the selection with matches.
    Replace,
    /// Add matches (`+`).
    Add,
    /// Remove matches (`-`).
    Subtract,
}

/// Predicate for [`SelectionModel::apply_matching`] over listing entries.
#[derive(Debug, Clone)]
pub struct SelectFilter {
    /// Glob against the file name (`*` / `?`). Empty matches nothing.
    pub pattern: String,
    /// Include files.
    pub files: bool,
    /// Include folders.
    pub folders: bool,
    /// Minimum file size (bytes). Folders ignore size unless both bounds are unset.
    pub min_size: Option<u64>,
    /// Maximum file size (bytes).
    pub max_size: Option<u64>,
    /// When true, only hidden entries.
    pub hidden: bool,
    /// When true, only read-only entries.
    pub readonly: bool,
    /// When set, only entries modified within this many days.
    pub newer_than_days: Option<u32>,
}

impl Default for SelectFilter {
    fn default() -> Self {
        Self {
            pattern: "*".into(),
            files: true,
            folders: true,
            min_size: None,
            max_size: None,
            hidden: false,
            readonly: false,
            newer_than_days: None,
        }
    }
}

impl SelectFilter {
    /// Name-only glob used by `+` / `-`.
    #[must_use]
    pub fn name_mask(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            ..Self::default()
        }
    }
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

    /// Invert membership for every path in `ordered` (visible listing).
    pub fn invert(&mut self, ordered: &[String]) {
        let mut next = HashSet::new();
        for p in ordered {
            if !self.selected.contains(p) {
                next.insert(p.clone());
            }
        }
        self.selected = next;
        self.sync_ends(ordered);
    }

    /// Apply `pred` to visible paths according to `op`.
    pub fn apply_matching(&mut self, ordered: &[String], op: MaskOp, pred: impl Fn(&str) -> bool) {
        match op {
            MaskOp::Replace => {
                self.selected.clear();
                for p in ordered {
                    if pred(p) {
                        self.selected.insert(p.clone());
                    }
                }
            }
            MaskOp::Add => {
                for p in ordered {
                    if pred(p) {
                        self.selected.insert(p.clone());
                    }
                }
            }
            MaskOp::Subtract => {
                for p in ordered {
                    if pred(p) {
                        self.selected.remove(p);
                    }
                }
            }
        }
        self.sync_ends(ordered);
    }

    /// Select the inclusive index range in `ordered`.
    ///
    /// When `additive`, existing members are kept (Ctrl+marquee).
    pub fn select_index_range(
        &mut self,
        ordered: &[String],
        from: usize,
        to: usize,
        additive: bool,
    ) {
        if ordered.is_empty() {
            return;
        }
        let last = ordered.len() - 1;
        let lo = from.min(to).min(last);
        let hi = from.max(to).min(last);
        if !additive {
            self.selected.clear();
        }
        for p in &ordered[lo..=hi] {
            self.selected.insert(p.clone());
        }
        self.anchor = Some(ordered[lo].clone());
        self.lead = Some(ordered[hi].clone());
    }

    /// Select tiles in the inclusive bounding rectangle of `from` and `to`.
    ///
    /// `columns` is the icon/grid column count. `columns <= 1` is a linear range
    /// (list / details). When `additive`, existing members are kept (Ctrl+marquee).
    pub fn select_index_rect(
        &mut self,
        ordered: &[String],
        from: usize,
        to: usize,
        columns: usize,
        additive: bool,
    ) {
        let cols = columns.max(1);
        if cols == 1 {
            self.select_index_range(ordered, from, to, additive);
            return;
        }
        if ordered.is_empty() {
            return;
        }
        let last = ordered.len() - 1;
        let a = from.min(last);
        let b = to.min(last);
        let r_lo = (a / cols).min(b / cols);
        let r_hi = (a / cols).max(b / cols);
        let c_lo = (a % cols).min(b % cols);
        let c_hi = (a % cols).max(b % cols);
        if !additive {
            self.selected.clear();
        }
        for r in r_lo..=r_hi {
            for c in c_lo..=c_hi {
                let i = r * cols + c;
                if i <= last {
                    self.selected.insert(ordered[i].clone());
                }
            }
        }
        self.anchor = Some(ordered[a].clone());
        self.lead = Some(ordered[b].clone());
    }

    fn sync_ends(&mut self, ordered: &[String]) {
        self.anchor = ordered
            .iter()
            .find(|p| self.selected.contains(p.as_str()))
            .cloned();
        self.lead = ordered
            .iter()
            .rev()
            .find(|p| self.selected.contains(p.as_str()))
            .cloned()
            .or_else(|| self.anchor.clone());
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

/// Case-insensitive glob against a file name (`*` and `?`). `*.*` matches all.
#[must_use]
pub fn name_glob_match(pattern: &str, name: &str) -> bool {
    let pat = pattern.trim();
    if pat.is_empty() {
        return false;
    }
    if pat == "*" || pat == "*.*" {
        return true;
    }
    glob_ci(pat.as_bytes(), name.as_bytes())
}

fn glob_ci(p: &[u8], t: &[u8]) -> bool {
    match (p.first(), t.first()) {
        (None, None) => true,
        (Some(b'*'), _) => {
            for i in 0..=t.len() {
                if glob_ci(&p[1..], &t[i..]) {
                    return true;
                }
            }
            false
        }
        (Some(b'?'), Some(_)) => glob_ci(&p[1..], &t[1..]),
        (Some(a), Some(b)) if a.eq_ignore_ascii_case(b) => glob_ci(&p[1..], &t[1..]),
        _ => false,
    }
}

/// Parse a human size (`10`, `10k`, `10kb`, `2.5mb`, `1g`). Empty → `None`.
#[must_use]
pub fn parse_byte_size(s: &str) -> Option<u64> {
    let t = s.trim().to_ascii_lowercase().replace(' ', "");
    if t.is_empty() {
        return None;
    }
    let (num, mul) = if let Some(n) = t.strip_suffix("kib") {
        (n, 1024.0)
    } else if let Some(n) = t.strip_suffix("kb") {
        (n, 1024.0)
    } else if let Some(n) = t.strip_suffix('k') {
        (n, 1024.0)
    } else if let Some(n) = t.strip_suffix("mib") {
        (n, 1024.0 * 1024.0)
    } else if let Some(n) = t.strip_suffix("mb") {
        (n, 1024.0 * 1024.0)
    } else if let Some(n) = t.strip_suffix('m') {
        (n, 1024.0 * 1024.0)
    } else if let Some(n) = t.strip_suffix("gib") {
        (n, 1024.0 * 1024.0 * 1024.0)
    } else if let Some(n) = t.strip_suffix("gb") {
        (n, 1024.0 * 1024.0 * 1024.0)
    } else if let Some(n) = t.strip_suffix('g') {
        (n, 1024.0 * 1024.0 * 1024.0)
    } else if let Some(n) = t.strip_suffix('b') {
        (n, 1.0)
    } else {
        (t.as_str(), 1.0)
    };
    let v: f64 = num.parse().ok()?;
    if !v.is_finite() || v < 0.0 {
        return None;
    }
    Some((v * mul) as u64)
}

/// Whether `entry` matches `filter` (name glob, kind, size, hidden, readonly, age).
#[must_use]
pub fn entry_matches_filter(entry: &orchid_fs::FsEntry, filter: &SelectFilter) -> bool {
    let is_dir = matches!(entry.metadata.kind, orchid_fs::FsEntryKind::Directory);
    if is_dir {
        if !filter.folders {
            return false;
        }
    } else if !filter.files {
        return false;
    }
    if !name_glob_match(&filter.pattern, &entry.name) {
        return false;
    }
    if !is_dir {
        if let Some(min) = filter.min_size {
            if entry.metadata.size < min {
                return false;
            }
        }
        if let Some(max) = filter.max_size {
            if entry.metadata.size > max {
                return false;
            }
        }
    } else if filter.min_size.is_some() || filter.max_size.is_some() {
        return false;
    }
    if filter.hidden && !entry.metadata.hidden {
        return false;
    }
    if filter.readonly && !entry.metadata.readonly {
        return false;
    }
    if let Some(days) = filter.newer_than_days {
        let Some(modified) = entry.metadata.modified else {
            return false;
        };
        let age = chrono::Utc::now().signed_duration_since(modified);
        if age.num_days() > i64::from(days) {
            return false;
        }
    }
    true
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

    #[test]
    fn invert_flips_visible_paths() {
        let mut s = SelectionModel::new();
        let ordered = vec!["a".into(), "b".into(), "c".into()];
        s.select_single("b");
        s.invert(&ordered);
        assert_eq!(s.count(), 2);
        assert!(s.is_selected("a"));
        assert!(!s.is_selected("b"));
        assert!(s.is_selected("c"));
    }

    #[test]
    fn mask_add_and_subtract() {
        let mut s = SelectionModel::new();
        let ordered = vec!["a.txt".into(), "b.rs".into(), "c.txt".into()];
        s.apply_matching(&ordered, MaskOp::Add, |p| p.ends_with(".txt"));
        assert_eq!(s.count(), 2);
        s.apply_matching(&ordered, MaskOp::Subtract, |p| p == "c.txt");
        assert_eq!(s.count(), 1);
        assert!(s.is_selected("a.txt"));
        s.apply_matching(&ordered, MaskOp::Replace, |p| p.ends_with(".rs"));
        assert_eq!(s.count(), 1);
        assert!(s.is_selected("b.rs"));
    }

    #[test]
    fn select_index_range_additive() {
        let mut s = SelectionModel::new();
        let ordered = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        s.select_index_range(&ordered, 0, 1, false);
        assert_eq!(s.count(), 2);
        s.select_index_range(&ordered, 3, 3, true);
        assert_eq!(s.count(), 3);
        assert!(s.is_selected("d"));
        assert!(!s.is_selected("c"));
    }

    #[test]
    fn select_index_rect_uses_column_bounding_box() {
        let mut s = SelectionModel::new();
        let ordered: Vec<String> = (0..8).map(|i| i.to_string()).collect();
        s.select_index_rect(&ordered, 0, 5, 4, false);
        assert!(s.is_selected("0"));
        assert!(s.is_selected("1"));
        assert!(!s.is_selected("2"));
        assert!(s.is_selected("4"));
        assert!(s.is_selected("5"));
        assert_eq!(s.count(), 4);
    }

    #[test]
    fn name_glob_star_and_question() {
        assert!(name_glob_match("*.txt", "readme.txt"));
        assert!(name_glob_match("*.TXT", "readme.txt"));
        assert!(!name_glob_match("*.txt", "readme.rs"));
        assert!(name_glob_match("*.*", "noext"));
        assert!(name_glob_match("file?.rs", "file1.rs"));
        assert!(!name_glob_match("file?.rs", "file12.rs"));
    }

    #[test]
    fn parse_byte_size_units() {
        assert_eq!(parse_byte_size(""), None);
        assert_eq!(parse_byte_size("10"), Some(10));
        assert_eq!(parse_byte_size("1kb"), Some(1024));
        assert_eq!(parse_byte_size("2 MB"), Some(2 * 1024 * 1024));
    }
}
