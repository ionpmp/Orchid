//! File tagging backed by the [`orchid_storage`] file-tag table.
//!
//! Every mutation publishes a [`TagsChangedEvent`] on the bus so that the
//! search index can re-materialise affected documents.

pub mod manager;

use chrono::{DateTime, Utc};

use crate::path::FsPath;

pub use manager::TagManager;

/// Fired whenever a file's tag / colour / starred state changes.
#[derive(Debug, Clone)]
pub struct TagsChangedEvent {
    /// Path whose tags were updated.
    pub path: FsPath,
    /// When the change was recorded.
    pub at: DateTime<Utc>,
}
impl orchid_core::Event for TagsChangedEvent {
    fn event_type() -> &'static str {
        "fs.tags_changed"
    }
}
