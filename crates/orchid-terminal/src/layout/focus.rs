//! Focus traversal within a tab.

use uuid::Uuid;

use crate::layout::splits::SplitNode;
use crate::layout::tabs::Tab;

/// Cycle focus to the next leaf in left-to-right, top-to-bottom order.
/// Wraps around at the end.
pub fn focus_next_in_tab(tab: &mut Tab) {
    let leaves = collect_leaves(&tab.root);
    if leaves.is_empty() {
        return;
    }
    let current = focused_session(tab).unwrap_or(leaves[0]);
    let pos = leaves.iter().position(|s| *s == current).unwrap_or(0);
    let next = (pos + 1) % leaves.len();
    set_focus(tab, leaves[next]);
}

/// Cycle focus to the previous leaf.
pub fn focus_previous_in_tab(tab: &mut Tab) {
    let leaves = collect_leaves(&tab.root);
    if leaves.is_empty() {
        return;
    }
    let current = focused_session(tab).unwrap_or(leaves[0]);
    let pos = leaves.iter().position(|s| *s == current).unwrap_or(0);
    let prev = (pos + leaves.len() - 1) % leaves.len();
    set_focus(tab, leaves[prev]);
}

/// Session currently focused in `tab`, or `None` if empty.
#[must_use]
pub fn focused_session(tab: &Tab) -> Option<Uuid> {
    if let Some(focus) = tab.focus {
        return Some(focus);
    }
    let mut leaves = Vec::new();
    tab.root.leaves(&mut leaves);
    leaves.first().copied()
}

fn collect_leaves(root: &SplitNode) -> Vec<Uuid> {
    let mut out = Vec::new();
    root.leaves(&mut out);
    out
}

fn set_focus(tab: &mut Tab, session: Uuid) {
    tab.focus = Some(session);
}
