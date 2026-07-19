//! Orchid-facing representation of a KDBX entry.
//!
//! The raw `keepass::db::Entry` type is powerful but exposes details we do
//! not want to propagate through the rest of the workspace. [`PasswordEntry`]
//! is the stable, ergonomic shape that the password-manager widget consumes.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use secrecy::SecretString;
use uuid::Uuid;

/// A single password entry.
#[derive(Debug, Clone)]
pub struct PasswordEntry {
    /// Stable identifier, preserved across saves.
    pub id: Uuid,
    /// Human-readable title.
    pub title: String,
    /// Username / login.
    pub username: String,
    /// Password.
    pub password: SecretString,
    /// Optional URL.
    pub url: Option<String>,
    /// Optional free-form notes.
    pub notes: Option<String>,
    /// Free-form tags.
    pub tags: Vec<String>,
    /// Custom fields. Values are kept secret because users may store tokens
    /// or answers to security questions here.
    pub custom_fields: BTreeMap<String, SecretString>,
    /// Optional TOTP configuration.
    pub totp: Option<TotpConfig>,
    /// Creation timestamp (server-side).
    pub created_at: DateTime<Utc>,
    /// Last-modification timestamp (server-side).
    pub modified_at: DateTime<Utc>,
    /// Id of the containing [`crate::PasswordGroup`].
    pub group_id: Uuid,
}

/// Time-based one-time password configuration.
#[derive(Debug, Clone)]
pub struct TotpConfig {
    /// Base32-encoded shared secret.
    pub secret: SecretString,
    /// HMAC algorithm.
    pub algorithm: TotpAlgorithm,
    /// Number of digits in a generated code (typically 6).
    pub digits: u8,
    /// Code refresh period in seconds (typically 30).
    pub period_seconds: u32,
    /// Optional issuer shown in the UI.
    pub issuer: Option<String>,
    /// Optional account shown in the UI.
    pub account: Option<String>,
}

/// Supported HMAC algorithms for TOTP.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TotpAlgorithm {
    Sha1,
    Sha256,
    Sha512,
}

impl Default for TotpConfig {
    fn default() -> Self {
        Self {
            secret: SecretString::from(String::new()),
            algorithm: TotpAlgorithm::Sha1,
            digits: 6,
            period_seconds: 30,
            issuer: None,
            account: None,
        }
    }
}
