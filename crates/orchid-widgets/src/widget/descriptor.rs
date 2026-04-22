//! Widget-type metadata held by the registry.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::widget::{Widget, WidgetContext};
use orchid_storage::{LifecycleState, WidgetSize};

/// Factory function used by the manager to spawn / revive widget instances.
///
/// The factory receives a [`WidgetContext`] and optionally serialised state
/// bytes produced by a previous [`Widget::save_state`] call.
pub type WidgetFactory = Arc<
    dyn Fn(WidgetContext, Option<&[u8]>) -> Result<Box<dyn Widget>> + Send + Sync + 'static,
>;

/// Category shown in the dock / palette to group related widgets.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WidgetCategory {
    System,
    Productivity,
    Information,
    Media,
    Security,
    Astronomy,
    Developer,
    Custom,
}

/// Static metadata registered for a widget type.
#[derive(Clone)]
pub struct WidgetDescriptor {
    /// Stable identifier.
    pub type_id: &'static str,
    /// i18n key for the display name.
    pub display_name_key: &'static str,
    /// i18n key for a longer description.
    pub description_key: &'static str,
    /// Icon-pack key.
    pub icon_name: &'static str,
    /// Category the widget belongs to.
    pub category: WidgetCategory,
    /// Default size used when the user creates an instance without specifying
    /// one.
    pub default_size: WidgetSize,
    /// Minimum size the widget tolerates.
    pub min_size: Option<WidgetSize>,
    /// Maximum size the widget tolerates.
    pub max_size: Option<WidgetSize>,
    /// Lifecycle state a freshly-created instance starts in.
    pub default_lifecycle: LifecycleState,
    /// Whether multiple instances may coexist on the same workspace.
    pub allows_multiple_instances: bool,
    /// Factory used to spawn instances.
    pub factory: WidgetFactory,
}

impl std::fmt::Debug for WidgetDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WidgetDescriptor")
            .field("type_id", &self.type_id)
            .field("category", &self.category)
            .field("default_size", &self.default_size)
            .field("allows_multiple_instances", &self.allows_multiple_instances)
            .finish_non_exhaustive()
    }
}
