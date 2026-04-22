//! Error type for [`orchid_widgets`](crate).

/// Unified error type for widget-framework operations.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum WidgetError {
    /// Attempted to instantiate an unregistered widget type.
    #[error("widget type not registered: {0}")]
    UnknownWidgetType(String),

    /// No instance with the given id is currently registered.
    #[error("widget instance not found: {0}")]
    InstanceNotFound(uuid::Uuid),

    /// No workspace with the given id exists.
    #[error("workspace not found: {0}")]
    WorkspaceNotFound(uuid::Uuid),

    /// Workspace count has reached [`crate::MAX_WORKSPACES`].
    #[error("workspace limit reached: maximum is {max}")]
    WorkspaceLimitReached {
        /// Limit that was exceeded.
        max: usize,
    },

    /// Position falls outside the workspace grid.
    #[error("invalid position: ({col},{row}) does not fit in workspace bounds")]
    InvalidPosition {
        /// Requested column.
        col: u16,
        /// Requested row.
        row: u16,
    },

    /// Placement collides with an existing widget.
    #[error("position collision with widget {0}")]
    PositionCollision(uuid::Uuid),

    /// Invalid size request (e.g. below `min_size`).
    #[error("invalid size: {reason}")]
    InvalidSize {
        /// Human-readable explanation.
        reason: String,
    },

    /// Widget factory failed to produce an instance.
    #[error("widget creation failed: {0}")]
    CreationFailed(String),

    /// Widget is in a lifecycle state that does not permit the operation.
    #[error("widget is in an invalid state for this operation: {0}")]
    InvalidStateForOperation(String),

    /// Group with the given id does not exist.
    #[error("group not found: {0}")]
    GroupNotFound(uuid::Uuid),

    /// Widget is not a member of any group.
    #[error("widget not in any group")]
    WidgetNotInGroup,

    /// A group-level move / resize was rejected.
    #[error("cannot move group: {0}")]
    GroupMoveError(String),

    /// Generic layout error.
    #[error("layout error: {0}")]
    Layout(String),

    /// Propagated from [`orchid_storage`].
    #[error(transparent)]
    Storage(#[from] orchid_storage::StorageError),

    /// Propagated from [`orchid_core`].
    #[error(transparent)]
    Core(#[from] orchid_core::CoreError),
}

/// `Result` alias with [`WidgetError`] as the default error type.
pub type Result<T, E = WidgetError> = std::result::Result<T, E>;
