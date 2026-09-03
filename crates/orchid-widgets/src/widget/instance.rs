//! Runtime wrapper around a widget instance.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use orchid_storage::{WindowPlacement, WindowState};
use parking_lot::RwLock;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::layout::PixelBounds;
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
    /// Grid vs floating window placement.
    pub placement: RwLock<WindowPlacement>,
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
    /// Last `Widget::save_state` bytes written to (or loaded from) storage.
    /// Layout-only mutations reuse this instead of re-serializing the widget.
    pub last_config: RwLock<Vec<u8>>,
}

impl WidgetInstanceRuntime {
    /// Snapshot this runtime into the persistable [`orchid_storage::WidgetInstance`]
    /// shape. Callers supplement with `config` bytes from
    /// [`Widget::save_state`] before writing to storage.
    pub fn to_storage(&self, config_bytes: Vec<u8>) -> orchid_storage::WidgetInstance {
        orchid_storage::WidgetInstance {
            id: self.id,
            widget_type: self.type_id.clone(),
            workspace_id: self.workspace_id,
            position: *self.position.read(),
            size: *self.size.read(),
            lifecycle: *self.lifecycle.read(),
            placement: self.placement.read().clone(),
            config: config_bytes,
            created_at: self.created_at,
            updated_at: *self.updated_at.read(),
        }
    }

    /// `true` when this instance uses floating-window placement.
    #[must_use]
    pub fn is_windowed(&self) -> bool {
        self.placement.read().is_windowed()
    }

    /// `true` when painted in the floating overlay (not minimized / not grid).
    #[must_use]
    pub fn is_visible_floating(&self) -> bool {
        self.placement.read().is_visible_floating()
    }

    /// Viewport-relative bounds when visible as a floating window.
    #[must_use]
    pub fn floating_bounds(&self) -> Option<PixelBounds> {
        self.placement
            .read()
            .visible_bounds()
            .map(pixel_bounds_from_rect)
    }

    /// Current window state when floating, else `None`.
    #[must_use]
    pub fn window_state(&self) -> Option<WindowState> {
        match *self.placement.read() {
            WindowPlacement::Floating { state, .. } => Some(state),
            WindowPlacement::Grid => None,
        }
    }
}

/// Convert storage [`orchid_storage::PixelRect`] → layout [`PixelBounds`].
#[must_use]
pub fn pixel_bounds_from_rect(r: orchid_storage::PixelRect) -> PixelBounds {
    PixelBounds {
        x: r.x,
        y: r.y,
        width: r.width,
        height: r.height,
    }
}

/// Convert layout [`PixelBounds`] → storage [`orchid_storage::PixelRect`].
#[must_use]
pub fn pixel_rect_from_bounds(b: PixelBounds) -> orchid_storage::PixelRect {
    orchid_storage::PixelRect {
        x: b.x,
        y: b.y,
        width: b.width,
        height: b.height,
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
            .field("placement", &*self.placement.read())
            .finish_non_exhaustive()
    }
}

/// Convenience alias — runtimes are always shared through an `Arc`.
pub type SharedInstance = Arc<WidgetInstanceRuntime>;
