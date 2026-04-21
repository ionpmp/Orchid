//! Bus events published by [`crate::FileWatcher`].

use chrono::{DateTime, Utc};

use crate::path::FsPath;

/// Published when a new entry appears.
#[derive(Debug, Clone)]
pub struct FsCreatedEvent {
    /// Path that was created.
    pub path: FsPath,
    /// When the change was detected.
    pub at: DateTime<Utc>,
}
impl orchid_core::Event for FsCreatedEvent {
    fn event_type() -> &'static str {
        "fs.created"
    }
}

/// Published when an entry's contents change.
#[derive(Debug, Clone)]
pub struct FsModifiedEvent {
    /// Path whose contents changed.
    pub path: FsPath,
    /// When the change was detected.
    pub at: DateTime<Utc>,
}
impl orchid_core::Event for FsModifiedEvent {
    fn event_type() -> &'static str {
        "fs.modified"
    }
}

/// Published when an entry is removed.
#[derive(Debug, Clone)]
pub struct FsDeletedEvent {
    /// Path that was deleted.
    pub path: FsPath,
    /// When the change was detected.
    pub at: DateTime<Utc>,
}
impl orchid_core::Event for FsDeletedEvent {
    fn event_type() -> &'static str {
        "fs.deleted"
    }
}

/// Published when an entry is renamed. Not every provider can report the
/// rename atomically; some will emit delete+create instead.
#[derive(Debug, Clone)]
pub struct FsRenamedEvent {
    /// Old path.
    pub from: FsPath,
    /// New path.
    pub to: FsPath,
    /// When the change was detected.
    pub at: DateTime<Utc>,
}
impl orchid_core::Event for FsRenamedEvent {
    fn event_type() -> &'static str {
        "fs.renamed"
    }
}
