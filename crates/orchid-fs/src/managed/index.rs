//! redb tables owned by the managed-folder engine.
//!
//! Tables:
//! * `fs_managed_folders` — managed-folder declarations keyed by root path.
//! * `fs_managed_manifests` — per-file manifests keyed by file path.

use orchid_crypto::FileManifest;
use orchid_storage::Value;
use redb::TableDefinition;

use crate::managed::config::ManagedFolderConfig;

/// Managed-folder declarations, keyed by the folder's canonical path.
pub(crate) const MANAGED_FOLDERS: TableDefinition<&str, Value<ManagedFolderConfig>> =
    TableDefinition::new("fs_managed_folders");

/// File manifests produced by the managed-folder engine.
pub(crate) const MANAGED_MANIFESTS: TableDefinition<&str, Value<FileManifest>> =
    TableDefinition::new("fs_managed_manifests");
