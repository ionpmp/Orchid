//! Tab + split-tree data model (UI-agnostic).

pub mod focus;
pub mod splits;
pub mod tabs;

use uuid::Uuid;

use crate::error::{Result, TerminalError};

pub use focus::{focus_next_in_tab, focus_previous_in_tab, focused_session};
pub use splits::{SplitDirection, SplitNode};
pub use tabs::{Tab, TabSet};

/// Top-level layout model: an ordered list of tabs plus the active tab index.
#[derive(Debug, Clone)]
pub struct LayoutRoot {
    /// Tab set.
    pub tabs: TabSet,
    /// Index of the active tab.
    pub active_tab: usize,
}

impl LayoutRoot {
    /// Build a fresh layout with a single tab containing one session.
    #[must_use]
    pub fn new(initial_session: Uuid) -> Self {
        let mut tabs = TabSet::default();
        tabs.tabs.push(Tab::single("Terminal", initial_session));
        Self {
            tabs,
            active_tab: 0,
        }
    }

    /// Append a new tab with a single leaf. Returns its index.
    pub fn add_tab(&mut self, session: Uuid) -> usize {
        self.tabs.tabs.push(Tab::single("Terminal", session));
        self.tabs.tabs.len() - 1
    }

    /// Close the tab at `index`.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::LayoutInvariant`] for out-of-range indices
    /// or when trying to close the last remaining tab.
    pub fn close_tab(&mut self, index: usize) -> Result<()> {
        if index >= self.tabs.tabs.len() {
            return Err(TerminalError::LayoutInvariant(format!(
                "tab index {index} out of range"
            )));
        }
        if self.tabs.tabs.len() == 1 {
            return Err(TerminalError::LayoutInvariant(
                "cannot close last remaining tab".into(),
            ));
        }
        self.tabs.tabs.remove(index);
        if self.active_tab >= self.tabs.tabs.len() {
            self.active_tab = self.tabs.tabs.len() - 1;
        }
        Ok(())
    }

    /// Reorder tabs.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::LayoutInvariant`] on invalid indices.
    pub fn move_tab(&mut self, from: usize, to: usize) -> Result<()> {
        if from >= self.tabs.tabs.len() || to >= self.tabs.tabs.len() {
            return Err(TerminalError::LayoutInvariant("tab index out of range".into()));
        }
        let tab = self.tabs.tabs.remove(from);
        self.tabs.tabs.insert(to, tab);
        Ok(())
    }

    /// Split the focused pane, attaching `new_session` as the second half.
    ///
    /// # Errors
    ///
    /// Propagates [`TerminalError::LayoutInvariant`] when the layout has no
    /// active tab.
    pub fn split(
        &mut self,
        direction: SplitDirection,
        new_session: Uuid,
    ) -> Result<()> {
        let tab = self
            .tabs
            .tabs
            .get_mut(self.active_tab)
            .ok_or_else(|| TerminalError::LayoutInvariant("no active tab".into()))?;
        let focus = tab.focus.unwrap_or_else(|| {
            let mut leaves = Vec::new();
            tab.root.leaves(&mut leaves);
            leaves.first().copied().unwrap_or(new_session)
        });
        replace_leaf(&mut tab.root, focus, direction, new_session);
        tab.focus = Some(new_session);
        Ok(())
    }

    /// Close the focused leaf. Collapses splits when there's only one pane
    /// left.
    ///
    /// # Errors
    ///
    /// Propagates [`TerminalError::LayoutInvariant`] on invalid state.
    pub fn close_focus(&mut self) -> Result<()> {
        let tab = self
            .tabs
            .tabs
            .get_mut(self.active_tab)
            .ok_or_else(|| TerminalError::LayoutInvariant("no active tab".into()))?;
        let focus = tab.focus.ok_or_else(|| {
            TerminalError::LayoutInvariant("cannot close without a focused pane".into())
        })?;
        let collapsed = close_leaf(&mut tab.root, focus);
        if collapsed {
            tab.focus = None;
        }
        if tab.root.leaf_count() == 0 {
            return Err(TerminalError::LayoutInvariant(
                "tab became empty after close".into(),
            ));
        }
        Ok(())
    }

    /// Cycle focus within the active tab.
    pub fn focus_next(&mut self) {
        if let Some(tab) = self.tabs.tabs.get_mut(self.active_tab) {
            focus_next_in_tab(tab);
        }
    }

    /// Cycle focus backwards.
    pub fn focus_previous(&mut self) {
        if let Some(tab) = self.tabs.tabs.get_mut(self.active_tab) {
            focus_previous_in_tab(tab);
        }
    }

    /// Session currently focused, if any.
    #[must_use]
    pub fn focused_session(&self) -> Option<Uuid> {
        self.tabs
            .tabs
            .get(self.active_tab)
            .and_then(focused_session)
    }

    /// UI-friendly snapshot of the whole layout.
    #[must_use]
    pub fn snapshot(&self) -> LayoutSnapshot {
        let tabs = self
            .tabs
            .tabs
            .iter()
            .map(|t| TabSnapshot {
                id: t.id,
                title: t.title.clone(),
                panes: pane_snapshot(&t.root, (0.0, 0.0), (1.0, 1.0)),
                focused: t.focus,
            })
            .collect();
        LayoutSnapshot {
            tabs,
            active_tab: self.active_tab,
        }
    }
}

/// Fractional pane coordinates, for UI layout.
#[derive(Debug, Clone, Copy)]
pub struct PaneBounds {
    /// Left edge as a fraction of the tab area (0..=1).
    pub left: f32,
    /// Top edge.
    pub top: f32,
    /// Right edge.
    pub right: f32,
    /// Bottom edge.
    pub bottom: f32,
}

/// A single leaf in a layout snapshot.
#[derive(Debug, Clone)]
pub struct PaneSnapshot {
    /// Session backing this pane.
    pub session: Uuid,
    /// Fractional bounds within the tab area.
    pub bounds: PaneBounds,
}

/// Snapshot of one tab for rendering.
#[derive(Debug, Clone)]
pub struct TabSnapshot {
    /// Stable id.
    pub id: Uuid,
    /// Display title.
    pub title: String,
    /// Flat list of panes.
    pub panes: Vec<PaneSnapshot>,
    /// Focused session.
    pub focused: Option<Uuid>,
}

/// Top-level layout snapshot.
#[derive(Debug, Clone)]
pub struct LayoutSnapshot {
    /// Ordered tabs.
    pub tabs: Vec<TabSnapshot>,
    /// Active tab index.
    pub active_tab: usize,
}

// ---------------------------------------------------------------------------
// Tree mutation helpers
// ---------------------------------------------------------------------------

fn replace_leaf(
    node: &mut SplitNode,
    focus: Uuid,
    direction: SplitDirection,
    new_session: Uuid,
) -> bool {
    match node {
        SplitNode::Leaf { session } if *session == focus => {
            let old = *session;
            *node = SplitNode::Split {
                direction,
                ratio: 0.5,
                first: Box::new(SplitNode::leaf(old)),
                second: Box::new(SplitNode::leaf(new_session)),
            };
            true
        }
        SplitNode::Leaf { .. } => false,
        SplitNode::Split { first, second, .. } => {
            if replace_leaf(first, focus, direction, new_session) {
                return true;
            }
            replace_leaf(second, focus, direction, new_session)
        }
    }
}

/// Remove the leaf with the given session; collapse the parent split into
/// the sibling if that makes the split trivial.
fn close_leaf(node: &mut SplitNode, target: Uuid) -> bool {
    match node {
        SplitNode::Leaf { .. } => false,
        SplitNode::Split { first, second, .. } => {
            let first_is_leaf = matches!(first.as_ref(), SplitNode::Leaf { session } if *session == target);
            let second_is_leaf = matches!(second.as_ref(), SplitNode::Leaf { session } if *session == target);
            if first_is_leaf {
                let keep = std::mem::replace(
                    second.as_mut(),
                    SplitNode::leaf(Uuid::nil()),
                );
                *node = keep;
                return true;
            }
            if second_is_leaf {
                let keep = std::mem::replace(
                    first.as_mut(),
                    SplitNode::leaf(Uuid::nil()),
                );
                *node = keep;
                return true;
            }
            close_leaf(first, target) || close_leaf(second, target)
        }
    }
}

fn pane_snapshot(
    node: &SplitNode,
    start: (f32, f32),
    end: (f32, f32),
) -> Vec<PaneSnapshot> {
    match node {
        SplitNode::Leaf { session } => vec![PaneSnapshot {
            session: *session,
            bounds: PaneBounds {
                left: start.0,
                top: start.1,
                right: end.0,
                bottom: end.1,
            },
        }],
        SplitNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            let r = ratio.clamp(0.05, 0.95);
            let mut out = Vec::new();
            match direction {
                SplitDirection::Horizontal => {
                    let mid = start.0 + (end.0 - start.0) * r;
                    out.extend(pane_snapshot(first, start, (mid, end.1)));
                    out.extend(pane_snapshot(second, (mid, start.1), end));
                }
                SplitDirection::Vertical => {
                    let mid = start.1 + (end.1 - start.1) * r;
                    out.extend(pane_snapshot(first, start, (end.0, mid)));
                    out.extend(pane_snapshot(second, (start.0, mid), end));
                }
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_and_close_returns_to_single_leaf() {
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();
        let mut layout = LayoutRoot::new(s1);
        layout.split(SplitDirection::Horizontal, s2).unwrap();
        assert_eq!(layout.snapshot().tabs[0].panes.len(), 2);
        layout.tabs.tabs[0].focus = Some(s2);
        layout.close_focus().unwrap();
        assert_eq!(layout.snapshot().tabs[0].panes.len(), 1);
    }

    #[test]
    fn add_and_close_tab() {
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();
        let mut layout = LayoutRoot::new(s1);
        let idx = layout.add_tab(s2);
        assert_eq!(idx, 1);
        assert_eq!(layout.tabs.tabs.len(), 2);
        layout.close_tab(1).unwrap();
        assert_eq!(layout.tabs.tabs.len(), 1);
    }

    #[test]
    fn cannot_close_last_tab() {
        let mut layout = LayoutRoot::new(Uuid::new_v4());
        assert!(layout.close_tab(0).is_err());
    }

    #[test]
    fn focus_next_cycles() {
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();
        let s3 = Uuid::new_v4();
        let mut layout = LayoutRoot::new(s1);
        layout.split(SplitDirection::Horizontal, s2).unwrap();
        layout.split(SplitDirection::Vertical, s3).unwrap();
        // Force focus to s1.
        layout.tabs.tabs[0].focus = Some(s1);
        layout.focus_next();
        assert_ne!(layout.focused_session(), Some(s1));
    }
}
