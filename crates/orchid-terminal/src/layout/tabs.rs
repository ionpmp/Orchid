//! Ordered tab model.

use uuid::Uuid;

use crate::layout::splits::SplitNode;

/// A single tab containing one split tree.
#[derive(Debug, Clone)]
pub struct Tab {
    /// Stable id of the tab.
    pub id: Uuid,
    /// Display title (mutable; updated from OSC 0 / 2 of the focused
    /// session).
    pub title: String,
    /// Root of this tab's split tree.
    pub root: SplitNode,
    /// Currently focused session within the tab.
    pub focus: Option<Uuid>,
}

impl Tab {
    /// Fresh tab with a single leaf.
    #[must_use]
    pub fn single(title: impl Into<String>, session: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            root: SplitNode::leaf(session),
            focus: Some(session),
        }
    }
}

/// Ordered set of tabs.
#[derive(Debug, Clone, Default)]
pub struct TabSet {
    /// Ordered list of tabs.
    pub tabs: Vec<Tab>,
}
