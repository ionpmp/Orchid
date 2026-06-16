//! Serializable tab / split layout for terminal widget persistence.

use orchid_terminal::{LayoutRoot, SplitDirection, SplitNode, Tab, TabSet};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Persisted tab + split tree (session ids are assigned on restore).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredLayoutRoot {
    /// Active tab index.
    pub active_tab: usize,
    /// Ordered tabs.
    pub tabs: Vec<StoredTab>,
}

/// One persisted tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredTab {
    /// Stable tab id.
    pub id: Uuid,
    /// Last known display title.
    pub title: String,
    /// Split tree without live session ids.
    pub root: StoredSplitNode,
    /// Focused leaf index in left-to-right order, if any.
    pub focus_leaf: Option<usize>,
}

/// Persisted split node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StoredSplitNode {
    /// Terminal pane (maps to the next session id on restore).
    Leaf,
    /// Internal split.
    Split {
        /// Side-by-side vs stacked.
        direction: StoredSplitDirection,
        /// Fraction for the first child.
        ratio: f32,
        /// First child.
        first: Box<StoredSplitNode>,
        /// Second child.
        second: Box<StoredSplitNode>,
    },
}

/// Serializable [`SplitDirection`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoredSplitDirection {
    /// Side-by-side panes.
    Horizontal,
    /// Stacked panes.
    Vertical,
}

impl StoredLayoutRoot {
    /// Total pane count across all tabs.
    #[must_use]
    pub fn leaf_count(&self) -> usize {
        self.tabs.iter().map(|t| t.root.leaf_count()).sum()
    }
}

impl StoredSplitNode {
    #[must_use]
    fn leaf_count(&self) -> usize {
        match self {
            Self::Leaf => 1,
            Self::Split { first, second, .. } => first.leaf_count() + second.leaf_count(),
        }
    }
}

/// Capture the live layout for persistence.
#[must_use]
pub fn layout_to_stored(layout: &LayoutRoot) -> StoredLayoutRoot {
    StoredLayoutRoot {
        active_tab: layout.active_tab,
        tabs: layout
            .tabs
            .tabs
            .iter()
            .map(|tab| {
                let mut leaves = Vec::new();
                tab.root.leaves(&mut leaves);
                let focus_leaf = tab
                    .focus
                    .and_then(|focus| leaves.iter().position(|s| *s == focus));
                StoredTab {
                    id: tab.id,
                    title: tab.title.clone(),
                    root: split_to_stored(&tab.root),
                    focus_leaf,
                }
            })
            .collect(),
    }
}

fn split_to_stored(node: &SplitNode) -> StoredSplitNode {
    match node {
        SplitNode::Leaf { .. } => StoredSplitNode::Leaf,
        SplitNode::Split {
            direction,
            ratio,
            first,
            second,
        } => StoredSplitNode::Split {
            direction: match direction {
                SplitDirection::Horizontal => StoredSplitDirection::Horizontal,
                SplitDirection::Vertical => StoredSplitDirection::Vertical,
            },
            ratio: *ratio,
            first: Box::new(split_to_stored(first)),
            second: Box::new(split_to_stored(second)),
        },
    }
}

/// Rebuild a [`LayoutRoot`] using freshly spawned session ids (one per leaf,
/// tabs in order, leaves left-to-right within each tab).
#[must_use]
pub fn stored_to_layout(stored: &StoredLayoutRoot, sessions: &[Uuid]) -> LayoutRoot {
    let mut iter = sessions.iter().copied();
    let tabs: Vec<Tab> = stored
        .tabs
        .iter()
        .map(|tab| {
            let root = stored_to_split(&tab.root, &mut iter);
            let mut leaves = Vec::new();
            root.leaves(&mut leaves);
            let focus = tab
                .focus_leaf
                .and_then(|i| leaves.get(i).copied())
                .or_else(|| leaves.first().copied());
            Tab {
                id: tab.id,
                title: tab.title.clone(),
                root,
                focus,
            }
        })
        .collect();
    let active_tab = stored.active_tab.min(tabs.len().saturating_sub(1));
    LayoutRoot {
        tabs: TabSet { tabs },
        active_tab,
    }
}

fn stored_to_split(node: &StoredSplitNode, sessions: &mut impl Iterator<Item = Uuid>) -> SplitNode {
    match node {
        StoredSplitNode::Leaf => SplitNode::leaf(
            sessions
                .next()
                .expect("stored layout leaf count must match session list"),
        ),
        StoredSplitNode::Split {
            direction,
            ratio,
            first,
            second,
        } => SplitNode::Split {
            direction: match direction {
                StoredSplitDirection::Horizontal => SplitDirection::Horizontal,
                StoredSplitDirection::Vertical => SplitDirection::Vertical,
            },
            ratio: *ratio,
            first: Box::new(stored_to_split(first, sessions)),
            second: Box::new(stored_to_split(second, sessions)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchid_terminal::LayoutRoot;

    #[test]
    fn roundtrip_preserves_split_ratio() {
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();
        let mut layout = LayoutRoot::new(s1);
        layout.split(SplitDirection::Horizontal, s2).unwrap();
        layout.set_split_ratio(s1, s2, 0.65).unwrap();

        let stored = layout_to_stored(&layout);
        let s3 = Uuid::new_v4();
        let s4 = Uuid::new_v4();
        let restored = stored_to_layout(&stored, &[s3, s4]);

        assert_eq!(restored.tabs.tabs.len(), 1);
        assert_eq!(restored.active_tab, 0);
        let snap = restored.snapshot();
        assert_eq!(snap.tabs[0].panes.len(), 2);
        assert!((snap.tabs[0].dividers[0].ratio - 0.65).abs() < 0.001);
        assert_eq!(restored.focused_session(), Some(s4));
    }

    #[test]
    fn roundtrip_multi_tab() {
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();
        let mut layout = LayoutRoot::new(s1);
        layout.add_tab(s2);
        layout.active_tab = 1;

        let stored = layout_to_stored(&layout);
        assert_eq!(stored.leaf_count(), 2);
        let new_ids: Vec<Uuid> = (0..2).map(|_| Uuid::new_v4()).collect();
        let restored = stored_to_layout(&stored, &new_ids);
        assert_eq!(restored.tabs.tabs.len(), 2);
        assert_eq!(restored.active_tab, 1);
        assert_eq!(restored.focused_session(), Some(new_ids[1]));
    }
}
