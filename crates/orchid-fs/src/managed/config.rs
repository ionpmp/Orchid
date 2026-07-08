//! Configuration types for managed folders.

use bincode::{Decode, Encode};
use orchid_crypto::ChunkerConfig;
use serde::{Deserialize, Serialize};

use crate::managed::policy::ManagedFolderPolicy;
use crate::path::FsPath;

/// Declaration of a managed folder.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct ManagedFolderConfig {
    /// Root of the managed tree.
    pub path: FsPath,
    /// Chunker tuning used when ingesting files.
    pub chunk_size: ChunkerConfig,
    /// Whether the folder is currently active.
    pub enabled: bool,
    /// Ingest new / modified files automatically.
    pub auto_ingest: bool,
    /// Optional retention, quota, and exclude rules.
    #[serde(default)]
    pub policy: Option<ManagedFolderPolicy>,
}

/// Aggregate statistics for a managed folder.
#[derive(Debug, Clone, Copy, Default)]
pub struct ManagedFolderStats {
    /// Distinct files tracked in the manifest index.
    pub files_tracked: u64,
    /// Sum of original file sizes.
    pub logical_bytes: u64,
    /// Unique chunks in the chunk store that belong to this folder.
    pub unique_chunks: u64,
    /// Bytes on disk for those chunks.
    pub physical_bytes: u64,
}
