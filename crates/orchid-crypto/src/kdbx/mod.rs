//! KDBX4 password database integration built on the `keepass` crate.
//!
//! * [`PasswordDatabase`] — owned handle to a KDBX4 file.
//! * [`PasswordEntry`] / [`PasswordGroup`] — Orchid-facing data types.
//! * [`SearchQuery`] / [`SearchResult`] — built-in filtering / scoring.
//! * [`TotpConfig`] + [`parse_otpauth_uri`] / [`generate_code`] — TOTP support.
//! * [`SecureClipboard`] — trait the password widget depends on; the
//!   concrete implementation lives in `orchid-ui`.

pub mod clipboard;
pub mod database;
pub mod entry;
pub mod group;
pub mod search;
pub mod totp;

pub use clipboard::SecureClipboard;
pub use database::PasswordDatabase;
pub use entry::{PasswordEntry, TotpAlgorithm, TotpConfig};
pub use group::PasswordGroup;
pub use search::{SearchQuery, SearchResult};
pub use totp::{generate_code, parse_otpauth_uri, to_otpauth_uri, TotpCode};
