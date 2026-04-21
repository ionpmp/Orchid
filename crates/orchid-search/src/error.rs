//! Error type for [`orchid_search`](crate).

/// Unified error type for every operation in `orchid-search`.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum SearchError {
    /// A Tantivy operation failed.
    #[error(transparent)]
    Tantivy(#[from] tantivy::TantivyError),

    /// An underlying I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Propagated from `orchid-fs`.
    #[error(transparent)]
    Fs(#[from] orchid_fs::FsError),

    /// Propagated from `orchid-storage`.
    #[error(transparent)]
    Storage(#[from] orchid_storage::StorageError),

    /// Propagated from `orchid-core`.
    #[error(transparent)]
    Core(#[from] orchid_core::CoreError),

    /// Content extraction failed.
    #[error("extraction failed for {path}: {reason}")]
    Extraction {
        /// Path that failed extraction.
        path: String,
        /// Human-readable explanation.
        reason: String,
    },

    /// Query string was not parseable.
    #[error("query parse error: {0}")]
    QueryParse(String),

    /// Operation attempted on a closed / shut-down index.
    #[error("index closed")]
    IndexClosed,
}

/// Crate-wide `Result` alias defaulting to [`SearchError`].
pub type Result<T, E = SearchError> = std::result::Result<T, E>;
