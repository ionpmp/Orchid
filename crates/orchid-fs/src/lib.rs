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
    acl_grant, acl_reset, acl_text, add_folder_share, apply_attr_patch, apply_name_case,
    bitlocker_drive_letter, bitlocker_lock, bitlocker_status, bitlocker_unlock, chmod, chown,
    compare_dirs, compare_files, copy, copy_previous_version, copy_with_control, create_hard_link,
    create_junction, create_symlink, decode_base64, decode_uue, decoded_path, delete,
    discover_parts, empty_recycle, encode_base64, encode_uue, exact_user_share, format_hash_report,
    format_recovery_key, format_verify_report, hash_path, hash_paths, inspect_path,
    inspect_signature, is_admin_share_name, is_hash_sidecar, is_recycle_item, is_recycle_listing,
    is_recycle_virtual, join_files, join_output_name, list_recycle, looks_like_recovery_key, move_,
    open_bitlocker_os, open_previous_versions_tab, open_sharing_tab, parse_mode, parse_timestamp,
    pick_previous_version, previous_version_file_stamp, previous_versions, purge_recycle,
    recycle_entries, recycle_item_id, recycle_listing_supported, recycle_original_path,
    remove_folder_share, restore_previous_version, restore_recycle, set_mtime, share_covers_path,
    shares_for_path, sidecar_path, split_file, sync_dirs, verify_hash_file, write_hash_file,
    AttrPatch, BitLockerConversion, BitLockerLock, BitLockerProtection, BitLockerStatus,
    CopyOptions, DeleteOptions, DiffKind, DirDiff, FileCompare, FileProperties, FolderShare,
    HashAlgo, HashRecord, NameCase, OperationProgress, PreviousVersion, ProgressSink, RecycleItem,
    SignatureReport, SyncMode, SyncStats, TransferControl, VerifyRow, RECYCLE_PATH,
};
pub use path::FsPath;
pub use provider::{
    is_rclone_scheme, list_local_with_preview, normalize_mount_uri, rclone_backend, rclone_sync,
    read_prefix, register_archive_provider, register_rclone_providers, ArchiveProvider,
    FsCapabilities, FsChange, FsChangeKind, FsProvider, FsProviderRegistry, FsWatcherHandle,
    LocalProvider, ProviderId, RcloneProvider, RCLONE_SCHEMES,
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
