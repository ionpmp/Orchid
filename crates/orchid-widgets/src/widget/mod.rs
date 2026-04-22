//! The [`Widget`] trait and its immediate collaborators.

pub mod config;
pub mod descriptor;
pub mod instance;
pub mod lifecycle;
pub mod snapshot;

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;
use uuid::Uuid;

use crate::error::Result;

pub use descriptor::{WidgetCategory, WidgetDescriptor, WidgetFactory};
pub use instance::WidgetInstanceRuntime;
pub use lifecycle::LifecycleController;
pub use snapshot::{
    TerminalPayload, TerminalPayloadCell, WidgetPayload, WidgetSnapshot, WidgetStatus,
};

/// Capabilities reported by a widget at runtime. The manager consults these
/// before attempting resize / group / unload operations.
#[derive(Debug, Clone, Default)]
pub struct WidgetCapabilities {
    /// Widget accepts arbitrary resize operations.
    pub supports_resize: bool,
    /// Optional floor on the widget's size.
    pub min_size: Option<orchid_storage::WidgetSize>,
    /// Optional ceiling on the widget's size.
    pub max_size: Option<orchid_storage::WidgetSize>,
    /// Preferred size used when the user picks "reset size".
    pub preferred_size: Option<orchid_storage::WidgetSize>,
    /// Widget can be stacked with others in a group.
    pub allows_grouping: bool,
    /// Widget implements [`Widget::save_state`] meaningfully and would like
    /// to be rehydrated rather than reconstructed after `Unloaded`.
    pub keeps_state_when_unloaded: bool,
    /// Widget has a settings panel.
    pub has_settings_panel: bool,
}

/// Shared state handed to widget lifecycle callbacks.
#[derive(Clone)]
pub struct WidgetContext {
    /// Global event bus.
    pub bus: Arc<orchid_core::EventBus>,
    /// State store for persistent data.
    pub storage: Arc<orchid_storage::StateStore>,
    /// Shared configuration.
    pub config: Arc<RwLock<orchid_storage::OrchidConfig>>,
    /// Instance id (stable for the widget's lifetime).
    pub instance_id: Uuid,
    /// Workspace this widget currently lives on.
    pub workspace_id: Uuid,
}

impl std::fmt::Debug for WidgetContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WidgetContext")
            .field("instance_id", &self.instance_id)
            .field("workspace_id", &self.workspace_id)
            .finish_non_exhaustive()
    }
}

/// A widget — the unit of functionality on a workspace.
///
/// Widgets do not render themselves directly. They produce a
/// [`WidgetSnapshot`] that the UI layer consumes to draw the matching Slint
/// component for the widget type.
#[async_trait]
pub trait Widget: Send + Sync + 'static {
    /// Stable widget type identifier (`"terminal"`, `"weather"`, ...).
    fn type_id(&self) -> &'static str;

    /// Unique instance id, assigned at creation by the manager.
    fn instance_id(&self) -> Uuid;

    /// Called once when the widget is created.
    async fn on_create(&mut self, ctx: &WidgetContext) -> Result<()>;

    /// Called when the widget transitions to `Active`.
    async fn on_activate(&mut self, ctx: &WidgetContext) -> Result<()>;

    /// Called when the widget enters `Sleeping`.
    async fn on_sleep(&mut self, ctx: &WidgetContext) -> Result<()>;

    /// Called when the widget enters `Unloaded`.
    async fn on_unload(&mut self, ctx: &WidgetContext) -> Result<()>;

    /// Called immediately before the widget is destroyed.
    async fn on_close(&mut self, ctx: &WidgetContext) -> Result<()>;

    /// Called when the manager resizes the widget.
    async fn on_resize(
        &mut self,
        ctx: &WidgetContext,
        size: orchid_storage::WidgetSize,
    ) -> Result<()>;

    /// Produce a cheap snapshot of the widget's current display state.
    /// Called every frame; returning `None` signals "nothing new" and the
    /// previous snapshot is kept.
    fn snapshot(&self) -> Option<WidgetSnapshot>;

    /// Serialise the widget's persistent state.
    fn save_state(&self) -> Result<Vec<u8>>;

    /// Rehydrate state produced by a previous [`Widget::save_state`] call.
    fn restore_state(&mut self, bytes: &[u8]) -> Result<()>;

    /// Optional per-instance display name. `None` falls back to the
    /// descriptor's `display_name_key`.
    fn display_name(&self) -> Option<String> {
        None
    }

    /// Capabilities the widget reports to the framework.
    fn capabilities(&self) -> WidgetCapabilities {
        WidgetCapabilities::default()
    }
}
