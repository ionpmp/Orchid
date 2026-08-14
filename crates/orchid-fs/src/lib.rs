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

pub use archive::{
    add_to_archive, create_archive, default_extract_dir, delete_from_archive, detect_format,
    extract_archive, is_archive_file, looks_like_archive_name, open_archive, test_archive,
    ArchiveEntry, ArchiveFormat, ArchiveReader, ArchiveTestReport, CreateArchiveOptions,
};
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
    acl_grant, acl_reset, acl_text, apply_attr_patch, apply_name_case, chmod, chown, compare_dirs,
    compare_files, copy, copy_with_control, create_hard_link, create_junction, create_symlink,
    decode_base64, decode_uue, decoded_path, delete, discover_parts, encode_base64, encode_uue,
    format_hash_report, format_verify_report, hash_path, hash_paths, is_hash_sidecar, join_files,
    join_output_name, move_, parse_mode, parse_timestamp, set_mtime, sidecar_path, split_file,
    sync_dirs, verify_hash_file, write_hash_file, AttrPatch, CopyOptions, DeleteOptions, DiffKind,
    DirDiff, FileCompare, HashAlgo, HashRecord, NameCase, OperationProgress, ProgressSink,
    SyncMode, SyncStats, TransferControl, VerifyRow,
};
pub use path::FsPath;
pub use provider::{
    is_rclone_scheme, normalize_mount_uri, rclone_backend, rclone_sync, read_prefix,
    register_archive_provider, register_rclone_providers, ArchiveProvider, FsCapabilities,
    FsChange, FsChangeKind, FsProvider, FsProviderRegistry, FsWatcherHandle, LocalProvider,
    ProviderId, RcloneProvider, RCLONE_SCHEMES,
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
