//! Error types for [`orchid_storage`](crate).
//!
//! All fallible operations in this crate surface a [`StorageError`]. Callers
//! either pattern-match on specific variants (e.g. [`StorageError::UnsupportedSchemaVersion`]
//! when they need to decide whether to bail out early) or just propagate
//! through [`Result`].

use std::path::PathBuf;

/// Unified error type for every operation exposed by `orchid-storage`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StorageError {
    /// A filesystem I/O operation failed.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    /// A generic [`redb`] error. Specific transaction / table /
    /// commit / storage errors have dedicated variants below.
    #[error("redb error: {0}")]
    Redb(#[from] ::redb::Error),

    /// Opening or creating the underlying redb database file failed.
    #[error("redb database error: {0}")]
    RedbDatabase(#[from] ::redb::DatabaseError),

    /// Starting a redb transaction failed.
    #[error("redb transaction error: {0}")]
    RedbTransaction(#[from] ::redb::TransactionError),

    /// Opening or creating a table inside a redb transaction failed.
    #[error("redb table error: {0}")]
    RedbTable(#[from] ::redb::TableError),

    /// A redb storage-level error (disk corruption, capacity limits, ...).
    #[error("redb storage error: {0}")]
    RedbStorage(#[from] ::redb::StorageError),

    /// Committing a redb write transaction failed.
    #[error("redb commit error: {0}")]
    RedbCommit(#[from] ::redb::CommitError),

    /// Compacting the redb database failed.
    #[error("redb compaction error: {0}")]
    RedbCompaction(#[from] ::redb::CompactionError),

    /// A TOML file could not be parsed. Carries the path of the offending
    /// file so diagnostics stay actionable.
    #[error("failed to parse TOML at {path}: {source}")]
    Toml {
        /// Filesystem path of the TOML file that failed to parse.
        path: PathBuf,
        /// Underlying `toml` deserialisation error.
        #[source]
        source: toml::de::Error,
    },

    /// A value could not be re-serialised to TOML (e.g. while saving
    /// configuration).
    #[error("failed to serialise TOML: {0}")]
    TomlSerialize(#[from] toml::ser::Error),

    /// A bincode encoding error. In practice this only happens if a value
    /// exceeds an encoder limit since all of our types are infallibly encodable.
    #[error("bincode encode error: {0}")]
    Bincode(#[from] ::bincode::error::EncodeError),

    /// A bincode decoding error, typically signalling corruption of the
    /// underlying redb database.
    #[error("bincode decode error: {0}")]
    BincodeDecode(#[from] ::bincode::error::DecodeError),

    /// A schema migration failed while opening the state database.
    #[error("migration from v{from} to v{to} failed: {reason}")]
    MigrationFailed {
        /// Version the migration started from.
        from: u32,
        /// Version the migration was trying to reach.
        to: u32,
        /// Human-readable failure reason.
        reason: String,
    },

    /// The database on disk was written by a newer build of Orchid than this
    /// one supports.
    #[error(
        "database schema version {found} is newer than supported maximum {supported_max}"
    )]
    UnsupportedSchemaVersion {
        /// Version read from the on-disk schema metadata.
        found: u32,
        /// Highest schema version this build understands.
        supported_max: u32,
    },

    /// A loaded configuration failed semantic validation.
    #[error("configuration validation error: {0}")]
    ConfigValidation(String),

    /// OS path resolution failed (e.g. the user has no home directory).
    #[error("path resolution error: {0}")]
    PathResolution(String),

    /// A filesystem watcher error bubbled up from [`notify`].
    #[error("file watcher error: {0}")]
    Watcher(#[from] ::notify::Error),
}

/// `Result` alias with [`StorageError`] as the default error type.
///
/// # Examples
///
/// ```
/// use orchid_storage::Result;
///
/// fn make_dir() -> Result<()> {
///     // any fallible operation that returns `StorageError` can be propagated
///     // with `?` here.
///     Ok(())
/// }
/// ```
pub type Result<T, E = StorageError> = std::result::Result<T, E>;
