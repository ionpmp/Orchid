//! Filesystem layer for Orchid: providers, watching, tags, managed folders,
//! encrypted paths, archives, and file operations.
//!
//! See module-level docs for each subsystem.

#![warn(missing_docs)]
#![warn(clippy::all)]
#![allow(clippy::result_large_err)]

pub mod archive;
pub mod encrypted;
pub mod entry;
pub mod error;
pub mod icon;
pub mod managed;
pub mod mime;
pub mod operations;
pub mod path;
pub mod provider;
pub mod tag;
pub mod watcher;

pub use archive::{detect_format, open_archive, ArchiveEntry, ArchiveFormat, ArchiveReader};
pub use encrypted::{
    EncryptedFolderConfig, EncryptedFolderEngine, EncryptedFolderRecord, EncryptedPathRegistered,
};
pub use entry::{ExtendedAttributes, FsEntry, FsEntryKind, FsMetadata};
pub use error::{FsError, Result};
pub use icon::{shell_icon, ShellIcon, ShellIconSize};
pub use managed::{
    ManagedFileIngestFailedEvent, ManagedFileIngestStartedEvent, ManagedFileIngestedEvent,
    ManagedFolderConfig, ManagedFolderEngine, ManagedFolderPolicy, ManagedFolderStats,
};
pub use mime::guess_mime;
pub use operations::{
    copy, delete, move_, CopyOptions, DeleteOptions, OperationProgress, ProgressSink,
};
pub use path::FsPath;
pub use provider::{
    normalize_mount_uri, read_prefix, register_rclone_providers, FsCapabilities, FsChange,
    FsChangeKind, FsProvider, FsProviderRegistry, FsWatcherHandle, LocalProvider, ProviderId,
    RcloneProvider, RCLONE_SCHEMES,
};
pub use tag::{TagManager, TagsChangedEvent};
pub use watcher::{
    events::{FsCreatedEvent, FsDeletedEvent, FsModifiedEvent, FsRenamedEvent},
    FileWatcher, WatchHandle,
};

/// Returns the crate version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
