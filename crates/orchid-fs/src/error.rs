//! Error type for [`orchid_fs`](crate).

/// Unified error type for every fallible operation exposed by `orchid-fs`.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum FsError {
    /// The path does not exist.
    #[error("path not found: {0}")]
    NotFound(String),

    /// The path already exists and cannot be overwritten without permission.
    #[error("path already exists: {0}")]
    AlreadyExists(String),

    /// The OS refused access to the path.
    #[error("permission denied: {0}")]
    PermissionDenied(String),

    /// The path string is malformed.
    #[error("invalid path: {reason}")]
    InvalidPath {
        /// Human-readable explanation.
        reason: String,
    },

    /// A long-running operation was cancelled.
    #[error("operation cancelled")]
    Cancelled,

    /// No provider with this id is registered.
    #[error("provider not found: {0}")]
    ProviderNotFound(String),

    /// No provider is registered for the scheme of this path.
    #[error("provider not mounted at {0}")]
    ProviderNotMounted(String),

    /// Unsupported archive format (unknown magic bytes).
    #[error("archive format not supported: {0}")]
    UnsupportedArchive(String),

    /// Entry not found inside an archive.
    #[error("archive entry not found: {0}")]
    ArchiveEntryNotFound(String),

    /// Archive header / data was invalid or malformed.
    #[error("corrupt archive: {0}")]
    CorruptArchive(String),

    /// Managed-folder configuration conflicts with another declaration.
    #[error("managed folder conflict: {0}")]
    ManagedFolderConflict(String),

    /// Path is not known to the managed-folder index.
    #[error("not a managed folder: {0}")]
    NotManagedFolder(String),

    /// Path is not registered as encrypted.
    #[error("not an encrypted path: {0}")]
    NotEncryptedPath(String),

    /// Generic encrypted-path operation failure.
    #[error("encrypted path operation failed: {0}")]
    EncryptedOp(String),

    /// Filesystem I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Propagated from [`notify`](::notify).
    #[error(transparent)]
    Notify(#[from] notify::Error),

    /// Propagated from [`walkdir`](::walkdir).
    #[error(transparent)]
    Walk(#[from] walkdir::Error),

    /// Propagated from the `zip` crate.
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),

    /// Propagated from [`orchid_storage`].
    #[error(transparent)]
    Storage(#[from] orchid_storage::StorageError),

    /// Propagated from [`orchid_crypto`].
    #[error(transparent)]
    Crypto(#[from] orchid_crypto::CryptoError),

    /// Propagated from [`orchid_core`].
    #[error(transparent)]
    Core(#[from] orchid_core::CoreError),
}

/// Crate-wide `Result` alias defaulting to [`FsError`].
pub type Result<T, E = FsError> = std::result::Result<T, E>;
