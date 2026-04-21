//! Filesystem entries and metadata.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::path::FsPath;

/// A single filesystem entry: path + metadata + name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsEntry {
    /// Canonical path.
    pub path: FsPath,
    /// Last path segment, cached for UI sorts.
    pub name: String,
    /// Metadata snapshot taken at discovery time.
    pub metadata: FsMetadata,
}

/// Per-entry metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsMetadata {
    /// Kind of filesystem object.
    pub kind: FsEntryKind,
    /// Size in bytes (0 for directories).
    pub size: u64,
    /// Creation time, if reported by the OS.
    pub created: Option<DateTime<Utc>>,
    /// Last-modified time, if reported by the OS.
    pub modified: Option<DateTime<Utc>>,
    /// Last-access time, if reported by the OS.
    pub accessed: Option<DateTime<Utc>>,
    /// Whether the OS reports the entry as read-only.
    pub readonly: bool,
    /// Whether the OS hides this entry by default.
    pub hidden: bool,
    /// Whether the OS marks this entry as system-owned.
    pub system: bool,
    /// Detected MIME type, if known.
    pub mime: Option<String>,
    /// Orchid-specific attributes derived from storage tables.
    pub extended: ExtendedAttributes,
}

/// Whether an entry is a file, directory, symlink, or something exotic.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FsEntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

/// Orchid-specific per-entry annotations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtendedAttributes {
    /// Path is registered in the encrypted-folder index.
    pub is_encrypted: bool,
    /// Path is covered by a managed-folder declaration.
    pub is_managed: bool,
    /// Id of the chunk-store manifest describing the payload (if any).
    pub manifest_id: Option<uuid::Uuid>,
    /// User-defined tags.
    pub tags: Vec<String>,
    /// Optional colour label.
    pub color_label: Option<orchid_storage::ColorLabel>,
    /// Whether the user has starred this entry.
    pub starred: bool,
}
