//! Error types for the mail engine.

use thiserror::Error;

/// Result alias for this crate.
pub type Result<T> = std::result::Result<T, MailError>;

/// Failures from account storage, protocols, or autodiscover.
#[derive(Debug, Error)]
pub enum MailError {
    /// I/O failure.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// JSON (de)serialisation.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    /// SQLite cache.
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Crypto / DPAPI.
    #[error("crypto: {0}")]
    Crypto(#[from] orchid_crypto::CryptoError),

    /// IMAP protocol or session.
    #[error("imap: {0}")]
    Imap(String),

    /// SMTP send.
    #[error("smtp: {0}")]
    Smtp(String),

    /// Autodiscover could not find usable servers.
    #[error("autodiscover: {0}")]
    Autodiscover(String),

    /// OAuth flow failed.
    #[error("oauth: {0}")]
    Oauth(String),

    /// Account or folder missing.
    #[error("not found: {0}")]
    NotFound(String),

    /// Invalid user input.
    #[error("invalid: {0}")]
    Invalid(String),

    /// HTTP failure during ISPDB / OAuth.
    #[error("http: {0}")]
    Http(String),

    /// MIME parse / build.
    #[error("mime: {0}")]
    Mime(String),
}

impl From<async_imap::error::Error> for MailError {
    fn from(value: async_imap::error::Error) -> Self {
        Self::Imap(value.to_string())
    }
}

impl From<lettre::error::Error> for MailError {
    fn from(value: lettre::error::Error) -> Self {
        Self::Smtp(value.to_string())
    }
}

impl From<lettre::transport::smtp::Error> for MailError {
    fn from(value: lettre::transport::smtp::Error) -> Self {
        Self::Smtp(value.to_string())
    }
}

impl From<reqwest::Error> for MailError {
    fn from(value: reqwest::Error) -> Self {
        Self::Http(value.to_string())
    }
}
