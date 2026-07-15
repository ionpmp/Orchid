//! Derive which widget instances are currently visible on screen.
//!
//! Visibility is driven by the active workspace and the active tab inside
//! multi-member groups — the same rules the UI uses when building frames.

use uuid::Uuid;

use crate::group::GroupManager;
use crate::manager::WidgetManager;
use crate::workspace::WorkspaceManager;

/// Instance ids that occupy a visible layout slot right now.
///
/// An instance is visible when:
/// * it lives on the active workspace, and
/// * it is not a non-active member of a multi-tab group (≥ 2 members).
#[must_use]
pub fn visible_instance_ids(
    widget_manager: &WidgetManager,
    workspace_manager: &WorkspaceManager,
    group_manager: &GroupManager,
) -> Vec<Uuid> {
    let Ok(ws) = workspace_manager.active() else {
        return Vec::new();
    };
    let workspace_groups = group_manager.list_for_workspace(ws.id);
    let mut out = Vec::new();
    for inst in widget_manager.instances_for_workspace(ws.id) {
        let id = inst.id;
        let hidden_by_group = workspace_groups.iter().any(|g| {
            g.members.len() >= 2
                && g.members.contains(&id)
                && g.active_instance() != Some(id)
        });
        if !hidden_by_group {
            out.push(id);
        }
    }
    out
}
