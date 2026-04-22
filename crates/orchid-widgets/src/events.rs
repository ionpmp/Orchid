//! Bus events emitted by the widget framework.

use orchid_core::Event;
use orchid_storage::{GridPosition, LifecycleState, WidgetSize};
use uuid::Uuid;

/// A new widget instance was created.
#[derive(Debug, Clone)]
pub struct WidgetCreated {
    /// Widget instance id.
    pub instance_id: Uuid,
    /// Workspace the instance belongs to.
    pub workspace_id: Uuid,
    /// Widget type (`"terminal"` etc.).
    pub type_id: String,
}
impl Event for WidgetCreated {
    fn event_type() -> &'static str {
        "widget.created"
    }
}

/// A widget instance was closed.
#[derive(Debug, Clone)]
pub struct WidgetClosed {
    /// Widget instance id.
    pub instance_id: Uuid,
}
impl Event for WidgetClosed {
    fn event_type() -> &'static str {
        "widget.closed"
    }
}

/// A widget was moved.
#[derive(Debug, Clone)]
pub struct WidgetMoved {
    /// Widget instance id.
    pub instance_id: Uuid,
    /// Previous position.
    pub from: GridPosition,
    /// New position.
    pub to: GridPosition,
}
impl Event for WidgetMoved {
    fn event_type() -> &'static str {
        "widget.moved"
    }
}

/// A widget was resized.
#[derive(Debug, Clone)]
pub struct WidgetResized {
    /// Widget instance id.
    pub instance_id: Uuid,
    /// Previous size.
    pub from: WidgetSize,
    /// New size.
    pub to: WidgetSize,
}
impl Event for WidgetResized {
    fn event_type() -> &'static str {
        "widget.resized"
    }
}

/// Lifecycle state of a widget changed.
#[derive(Debug, Clone)]
pub struct WidgetLifecycleChanged {
    /// Widget instance id.
    pub instance_id: Uuid,
    /// Previous state.
    pub from: LifecycleState,
    /// New state.
    pub to: LifecycleState,
}
impl Event for WidgetLifecycleChanged {
    fn event_type() -> &'static str {
        "widget.lifecycle_changed"
    }
}

/// A widget produced a new snapshot (UI should re-render).
#[derive(Debug, Clone)]
pub struct WidgetSnapshotUpdated {
    /// Widget instance id.
    pub instance_id: Uuid,
}
impl Event for WidgetSnapshotUpdated {
    fn event_type() -> &'static str {
        "widget.snapshot_updated"
    }
}

/// A new workspace was created.
#[derive(Debug, Clone)]
pub struct WorkspaceCreated {
    /// Workspace id.
    pub workspace_id: Uuid,
    /// Workspace display name.
    pub name: String,
}
impl Event for WorkspaceCreated {
    fn event_type() -> &'static str {
        "workspace.created"
    }
}

/// A workspace was deleted.
#[derive(Debug, Clone)]
pub struct WorkspaceDeleted {
    /// Workspace id.
    pub workspace_id: Uuid,
}
impl Event for WorkspaceDeleted {
    fn event_type() -> &'static str {
        "workspace.deleted"
    }
}

/// Active workspace changed.
#[derive(Debug, Clone)]
pub struct WorkspaceSwitched {
    /// Previously active workspace (if any).
    pub from: Option<Uuid>,
    /// Newly active workspace.
    pub to: Uuid,
}
impl Event for WorkspaceSwitched {
    fn event_type() -> &'static str {
        "workspace.switched"
    }
}

/// A workspace was renamed.
#[derive(Debug, Clone)]
pub struct WorkspaceRenamed {
    /// Workspace id.
    pub workspace_id: Uuid,
    /// New name.
    pub name: String,
}
impl Event for WorkspaceRenamed {
    fn event_type() -> &'static str {
        "workspace.renamed"
    }
}

/// A widget group was created.
#[derive(Debug, Clone)]
pub struct GroupCreated {
    /// Group id.
    pub group_id: Uuid,
    /// Workspace the group belongs to.
    pub workspace_id: Uuid,
}
impl Event for GroupCreated {
    fn event_type() -> &'static str {
        "widget.group_created"
    }
}

/// A widget group was dissolved.
#[derive(Debug, Clone)]
pub struct GroupDissolved {
    /// Group id.
    pub group_id: Uuid,
}
impl Event for GroupDissolved {
    fn event_type() -> &'static str {
        "widget.group_dissolved"
    }
}

/// The active member of a widget group changed.
#[derive(Debug, Clone)]
pub struct GroupActiveChanged {
    /// Group id.
    pub group_id: Uuid,
    /// Now-active widget instance.
    pub instance_id: Uuid,
}
impl Event for GroupActiveChanged {
    fn event_type() -> &'static str {
        "widget.group_active_changed"
    }
}
