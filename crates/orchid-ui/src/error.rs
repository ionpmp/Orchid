//! Unified error type for the UI layer.

/// Errors surfaced by `orchid-ui`.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum UiError {
    /// Slint reported a failure (window creation, event loop, ...).
    #[error("Slint error: {0}")]
    Slint(String),

    /// A theme with the given id is not registered.
    #[error("theme not found: {0}")]
    ThemeNotFound(String),

    /// Propagated from `orchid-storage`.
    #[error(transparent)]
    Storage(#[from] orchid_storage::StorageError),

    /// Propagated from `orchid-core`.
    #[error(transparent)]
    Core(#[from] orchid_core::CoreError),

    /// Propagated from `orchid-i18n`.
    #[error(transparent)]
    I18n(#[from] orchid_i18n::I18nError),

    /// Propagated IO failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// `Result` alias that defaults to [`UiError`].
pub type Result<T, E = UiError> = std::result::Result<T, E>;
