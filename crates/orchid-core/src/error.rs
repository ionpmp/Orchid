//! Error type for [`orchid_core`](crate).

use crate::event::SubscriptionId;

/// Unified error type for every operation exposed by `orchid-core`.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum CoreError {
    /// The event bus is no longer accepting new publishes or subscriptions.
    #[error("event bus is shut down")]
    BusShutdown,

    /// Attempted to unsubscribe an id the bus does not know about.
    #[error("subscription not found: {0}")]
    SubscriptionNotFound(SubscriptionId),

    /// A command id was not present in the [`crate::CommandRegistry`].
    #[error("command not found: {0}")]
    CommandNotFound(String),

    /// Attempted to register a command whose id already exists.
    #[error("duplicate command id: {0}")]
    DuplicateCommand(String),

    /// A command-line string could not be parsed.
    #[error("invalid command syntax: {reason}")]
    InvalidCommandSyntax {
        /// Human-readable explanation.
        reason: String,
    },

    /// A shortcut string could not be parsed.
    #[error("invalid shortcut: {input}")]
    InvalidShortcut {
        /// The raw input string that failed to parse.
        input: String,
    },

    /// An [`crate::Action`] returned a non-success outcome, or an internal
    /// failure (panic, join error) happened during dispatch.
    #[error("action execution failed: {0}")]
    ActionFailed(String),

    /// An action was requested to be reversed but declares itself
    /// irreversible.
    #[error("action is not reversible")]
    NotReversible,

    /// The [`crate::HistoryRecorder`] middleware was unable to persist an
    /// entry.
    #[error("history recording failed: {0}")]
    HistoryRecording(String),

    /// Propagated error from the [`orchid_storage`] layer.
    #[error(transparent)]
    Storage(#[from] orchid_storage::StorageError),

    /// Two input bindings claim the same trigger.
    #[error("input mapping conflict: {0}")]
    InputMappingConflict(String),
}

/// `Result` alias with [`CoreError`] as the default error type.
///
/// # Examples
///
/// ```
/// use orchid_core::Result;
///
/// fn trivial() -> Result<()> { Ok(()) }
/// ```
pub type Result<T, E = CoreError> = std::result::Result<T, E>;
