//! Lock-free snapshot cache for widget instances.
//!
//! Widgets emit [`WidgetSnapshot`] values into a cache from a background task.
//! The UI thread reads the most recent snapshot with a single [`DashMap`]
//! lookup, without awaiting async widget locks.

use std::sync::Arc;

use dashmap::DashMap;
use uuid::Uuid;

use crate::widget::snapshot::WidgetSnapshot;

/// Latest rendered snapshot per instance id, updated by a background task.
pub struct WidgetSnapshotCache {
    inner: DashMap<Uuid, Arc<WidgetSnapshot>>,
}

impl Default for WidgetSnapshotCache {
    fn default() -> Self {
        Self::new()
    }
}

impl WidgetSnapshotCache {
    /// Empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: DashMap::new(),
        }
    }

    /// Store a snapshot, replacing any previous entry for this id.
    pub fn put(&self, id: Uuid, snapshot: WidgetSnapshot) {
        self.inner.insert(id, Arc::new(snapshot));
    }

    /// Returns the most recent snapshot if the UI has not yet been updated.
    #[must_use]
    pub fn get(&self, id: Uuid) -> Option<Arc<WidgetSnapshot>> {
        self.inner.get(&id).map(|r| r.value().clone())
    }

    /// Remove the entry for a closed instance to avoid unbounded growth.
    pub fn remove(&self, id: Uuid) {
        self.inner.remove(&id);
    }

    /// How many instance ids currently have a cached frame.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// `true` when there are no cached entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::snapshot::{WidgetPayload, WidgetStatus};

    fn sample_snapshot(id: Uuid) -> WidgetSnapshot {
        WidgetSnapshot {
            instance_id: id,
            widget_type: "terminal",
            title: "T".to_string(),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::Empty,
        }
    }

    #[test]
    fn put_get_remove() {
        let cache = WidgetSnapshotCache::new();
        let id = Uuid::new_v4();
        assert!(cache.get(id).is_none());

        cache.put(id, sample_snapshot(id));
        assert!(cache.get(id).is_some());

        cache.remove(id);
        assert!(cache.get(id).is_none());
    }

    #[test]
    fn overwrites() {
        let cache = WidgetSnapshotCache::new();
        let id = Uuid::new_v4();
        cache.put(id, sample_snapshot(id));
        cache.put(id, sample_snapshot(id));
        assert_eq!(cache.len(), 1);
    }
}
