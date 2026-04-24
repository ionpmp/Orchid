//! Widget-type registry.

use dashmap::DashMap;

use crate::error::{Result, WidgetError};
use crate::widget::descriptor::{WidgetCategory, WidgetDescriptor};

/// Directory of registered widget types keyed by `type_id`.
#[derive(Default)]
pub struct WidgetRegistry {
    descriptors: DashMap<&'static str, WidgetDescriptor>,
}

impl std::fmt::Debug for WidgetRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WidgetRegistry")
            .field("count", &self.descriptors.len())
            .finish()
    }
}

impl WidgetRegistry {
    /// Empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a widget type.
    ///
    /// # Errors
    ///
    /// Returns [`WidgetError::UnknownWidgetType`] when the id already exists.
    /// (We reuse the variant for brevity; a dedicated `DuplicateType` can
    /// land in a future pass if we want richer diagnostics.)
    pub fn register(&self, desc: WidgetDescriptor) -> Result<()> {
        if self.descriptors.contains_key(desc.type_id) {
            return Err(WidgetError::UnknownWidgetType(format!(
                "duplicate widget type: {}",
                desc.type_id
            )));
        }
        self.descriptors.insert(desc.type_id, desc);
        Ok(())
    }

    /// Remove a widget type from the registry.
    pub fn unregister(&self, type_id: &str) -> bool {
        self.descriptors.remove(type_id).is_some()
    }

    /// Fetch a descriptor by type id.
    ///
    /// Accepts UI aliases such as `"search"` for the universal search widget
    /// ([`crate::builtin::search::TYPE_ID`]).
    #[must_use]
    pub fn get(&self, type_id: &str) -> Option<WidgetDescriptor> {
        let key = match type_id {
            "search" => "universal-search",
            other => other,
        };
        self.descriptors.get(key).map(|e| e.value().clone())
    }

    /// Every registered descriptor, in no particular order.
    #[must_use]
    pub fn list(&self) -> Vec<WidgetDescriptor> {
        self.descriptors.iter().map(|e| e.value().clone()).collect()
    }

    /// Descriptors that belong to `category`.
    #[must_use]
    pub fn list_by_category(&self, category: WidgetCategory) -> Vec<WidgetDescriptor> {
        self.descriptors
            .iter()
            .filter(|e| e.value().category == category)
            .map(|e| e.value().clone())
            .collect()
    }
}
