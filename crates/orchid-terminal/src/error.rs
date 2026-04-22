//! Error type for [`orchid_terminal`](crate).

/// Unified error type for terminal operations.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum TerminalError {
    /// Child process could not be started.
    #[error("failed to spawn shell: {0}")]
    SpawnFailed(String),

    /// Underlying PTY operation failed.
    #[error("PTY error: {0}")]
    Pty(String),

    /// No session with the given id is registered.
    #[error("session not found: {0}")]
    SessionNotFound(uuid::Uuid),

    /// A session with that id is already registered.
    #[error("session already exists: {0}")]
    SessionAlreadyExists(uuid::Uuid),

    /// The session has been closed.
    #[error("session is closed")]
    SessionClosed,

    /// The requested backend is not available on this platform.
    #[error("backend not available: {0}")]
    BackendUnavailable(String),

    /// Invalid terminal size passed to a resize call.
    #[error("invalid resize: cols={cols}, rows={rows}")]
    InvalidResize {
        /// Requested columns.
        cols: u16,
        /// Requested rows.
        rows: u16,
    },

    /// Writing user input bytes to the PTY failed.
    #[error("write to PTY failed: {0}")]
    WriteFailed(String),

    /// Internal emulator failure.
    #[error("emulator error: {0}")]
    Emulator(String),

    /// Layout operation produced an inconsistent tree.
    #[error("layout invariant violated: {0}")]
    LayoutInvariant(String),

    /// Pasted text looked like an injection attempt.
    #[error("paste rejected: contains control sequence")]
    PasteRejected,

    /// Plain I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Propagated from [`orchid_storage`].
    #[error(transparent)]
    Storage(#[from] orchid_storage::StorageError),

    /// Propagated from [`orchid_core`].
    #[error(transparent)]
    Core(#[from] orchid_core::CoreError),
}

/// Crate-wide `Result` alias.
pub type Result<T, E = TerminalError> = std::result::Result<T, E>;
