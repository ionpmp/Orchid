//! Runtime wrapper around a widget instance.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::widget::snapshot::WidgetSnapshot;
use crate::widget::Widget;

/// Owning, lock-friendly container for a live widget instance.
///
/// The widget itself lives inside a [`tokio::sync::Mutex`] rather than a
/// [`parking_lot::RwLock`] because lifecycle callbacks are `async fn`s and
/// therefore need an async-aware mutex that can be held across await points.
pub struct WidgetInstanceRuntime {
    /// Stable instance id.
    pub id: Uuid,
    /// Workspace this instance currently lives on.
    pub workspace_id: Uuid,
    /// Widget type identifier.
    pub type_id: String,
    /// Position inside the workspace grid.
    pub position: RwLock<orchid_storage::GridPosition>,
    /// Current size.
    pub size: RwLock<orchid_storage::WidgetSize>,
    /// Current lifecycle state.
    pub lifecycle: RwLock<orchid_storage::LifecycleState>,
    /// Group the widget belongs to, if any.
    pub group_id: RwLock<Option<Uuid>>,
    /// When the instance was created.
    pub created_at: DateTime<Utc>,
    /// When the instance was last mutated.
    pub updated_at: RwLock<DateTime<Utc>>,
    /// Widget object itself.
    pub widget: Mutex<Box<dyn Widget>>,
    /// Cached last snapshot (updated by the manager on a schedule).
    pub last_snapshot: RwLock<Option<WidgetSnapshot>>,
    /// Monotonic "last touched" timestamp, used by the idle sweeper.
    pub last_touched: RwLock<DateTime<Utc>>,
}

impl WidgetInstanceRuntime {
    /// Snapshot this runtime into the persistable [`orchid_storage::WidgetInstance`]
    /// shape. Callers supplement with `config` bytes from
    /// [`Widget::save_state`] before writing to storage.
    pub fn to_storage(
        &self,
        config_bytes: Vec<u8>,
    ) -> orchid_storage::WidgetInstance {
        orchid_storage::WidgetInstance {
            id: self.id,
            widget_type: self.type_id.clone(),
            workspace_id: self.workspace_id,
            position: *self.position.read(),
            size: *self.size.read(),
            lifecycle: *self.lifecycle.read(),
            config: config_bytes,
            created_at: self.created_at,
            updated_at: *self.updated_at.read(),
        }
    }
}

impl std::fmt::Debug for WidgetInstanceRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WidgetInstanceRuntime")
            .field("id", &self.id)
            .field("workspace_id", &self.workspace_id)
            .field("type_id", &self.type_id)
            .field("position", &*self.position.read())
            .field("size", &*self.size.read())
            .field("lifecycle", &*self.lifecycle.read())
            .finish_non_exhaustive()
    }
}

/// Convenience alias — runtimes are always shared through an `Arc`.
pub type SharedInstance = Arc<WidgetInstanceRuntime>;
