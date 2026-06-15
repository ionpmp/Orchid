//! Filesystem provider abstraction.
//!
//! Providers plug into [`FsProviderRegistry`] and serve reads / writes /
//! listings under a specific URL scheme. The built-in [`LocalProvider`]
//! covers `"local:"` paths; future network backends will slot in here
//! transparently.

pub mod local;
pub mod rclone;
pub mod registry;

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::entry::{FsEntry, FsMetadata};
use crate::error::Result;
use crate::operations::copy::CopyOptions;
use crate::operations::progress::ProgressSink;
use crate::path::FsPath;

pub use local::LocalProvider;
pub use rclone::{normalize_mount_uri, register_rclone_providers, RcloneProvider, RCLONE_SCHEMES};
pub use registry::FsProviderRegistry;

/// Identifier for a registered provider.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProviderId(pub String);

impl ProviderId {
    /// Construct an id from any string-like value.
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the inner string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Feature matrix reported by a provider.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FsCapabilities {
    /// Server-side rename without falling back to copy+delete.
    pub supports_rename: bool,
    /// Creating and following symlinks.
    pub supports_symlinks: bool,
    /// POSIX-style permission bits.
    pub supports_permissions: bool,
    /// Extended attributes (xattrs, ADS, ...).
    pub supports_extended_attrs: bool,
    /// Has a native change watcher.
    pub supports_native_watch: bool,
    /// Case-sensitive path comparisons.
    pub case_sensitive: bool,
    /// Random-access writes (seek + write mid-file).
    pub supports_random_write: bool,
}

/// A change streamed from a provider's watcher.
#[derive(Debug, Clone)]
pub struct FsChange {
    /// Affected path.
    pub path: FsPath,
    /// Kind of change.
    pub kind: FsChangeKind,
    /// When the change was detected.
    pub timestamp: DateTime<Utc>,
}

/// Kind of change an [`FsWatcherHandle`] may report.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum FsChangeKind {
    Created,
    Modified,
    Deleted,
    /// The file was renamed from `from` to `path`.
    Renamed {
        /// Source path of the rename.
        from: FsPath,
    },
}

/// Handle to a live native watcher. Drop or call [`FsWatcherHandle::shutdown`]
/// to stop.
#[async_trait]
pub trait FsWatcherHandle: Send + Sync {
    /// Receive the next batch of changes. Returns `None` once the watcher
    /// has been shut down or its underlying backend closed.
    async fn recv(&mut self) -> Option<Vec<FsChange>>;

    /// Cleanly shut down the watcher.
    async fn shutdown(self: Box<Self>);
}

/// Core trait every provider implements. All methods operate on [`FsPath`]s
/// whose scheme is expected to match [`FsProvider::scheme`].
#[async_trait]
pub trait FsProvider: Send + Sync + 'static {
    /// Stable identifier assigned when the provider was registered.
    fn id(&self) -> &ProviderId;

    /// URL scheme this provider serves (e.g. `"local"`).
    fn scheme(&self) -> &'static str;

    /// List direct children of `path`.
    ///
    /// # Errors
    ///
    /// Propagates provider-specific errors and [`crate::FsError::NotFound`]
    /// if the directory does not exist.
    async fn list(&self, path: &FsPath) -> Result<Vec<FsEntry>>;

    /// Fetch metadata for a single entry.
    async fn metadata(&self, path: &FsPath) -> Result<FsMetadata>;

    /// Test whether the path exists.
    async fn exists(&self, path: &FsPath) -> Result<bool>;

    /// Read a file fully into memory. For large files use [`read_stream`].
    ///
    /// [`read_stream`]: FsProvider::read_stream
    async fn read(&self, path: &FsPath) -> Result<Vec<u8>>;

    /// Open a file for streaming async reads.
    async fn read_stream(
        &self,
        path: &FsPath,
    ) -> Result<Box<dyn tokio::io::AsyncRead + Unpin + Send>>;

    /// Write `bytes` to `path`, creating the file if missing.
    async fn write(&self, path: &FsPath, bytes: &[u8]) -> Result<()>;

    /// Open a file for streaming async writes.
    async fn write_stream(
        &self,
        path: &FsPath,
    ) -> Result<Box<dyn tokio::io::AsyncWrite + Unpin + Send>>;

    /// Create a directory (and parents if `recursive`).
    async fn create_dir(&self, path: &FsPath, recursive: bool) -> Result<()>;

    /// Rename / move. May fail across volumes.
    async fn rename(&self, from: &FsPath, to: &FsPath) -> Result<()>;

    /// Delete an entry; `recursive = true` removes directory contents.
    async fn remove(&self, path: &FsPath, recursive: bool) -> Result<()>;

    /// Start a native watcher on `path` if supported.
    async fn watch(
        &self,
        path: &FsPath,
    ) -> Result<Option<Box<dyn FsWatcherHandle>>>;

    /// Feature matrix.
    fn capabilities(&self) -> FsCapabilities;

    /// Copy across schemes when this provider can delegate natively (e.g.
    /// `rclone copyto` between local disk and a network mount). Returns
    /// `Ok(true)` when handled; `Ok(false)` to fall back to generic streaming.
    async fn copy_cross_scheme(
        &self,
        _registry: &FsProviderRegistry,
        _from: &FsPath,
        _to: &FsPath,
        _options: CopyOptions,
        _progress: Option<&ProgressSink>,
    ) -> Result<bool> {
        Ok(false)
    }
}

/// Convenience alias for shared providers.
pub type SharedProvider = Arc<dyn FsProvider>;
