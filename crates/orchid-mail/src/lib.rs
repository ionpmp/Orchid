//! Mail engine for Orchid: accounts, autodiscover, IMAP/SMTP, and local cache.
//!
//! The UI talks to [`MailEngine`]. Secrets never enter widget state; they live
//! in a DPAPI blob under `data/mail/`. Message bodies and headers are cached in
//! SQLite (`data/mail/cache.db`).

#![warn(missing_docs)]
#![warn(clippy::all)]
#![allow(clippy::result_large_err)]

pub mod account;
pub mod autodiscover;
pub mod cache;
pub mod engine;
pub mod error;
pub mod imap;
pub mod mime_util;
pub mod oauth;
pub mod secrets;
pub mod smtp;
pub mod store;
pub mod tls;

pub use account::{
    AttachmentMeta, AuthKind, ComposeMessage, MailAccount, MailFolder, MessageBody, MessageHeader,
    ServerEndpoint, ServerSuggestion, TlsMode,
};
pub use autodiscover::discover;
pub use engine::MailEngine;
pub use error::{MailError, Result};
pub use mime_util::{html_to_text, sanitize_html_for_webview};
pub use secrets::{AccountSecrets, MailSecretsStore};

/// Crate version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
