//! File encryption built on the [`age`] crate.
//!
//! * [`Identity`] — how the user authenticates (passphrase or X25519).
//! * [`Encryptor`] / [`Decryptor`] — synchronous-on-blocking wrappers around
//!   age's pipelines, with async-friendly file, stream, and directory APIs.
//! * [`EncryptedFileMeta`] — sidecar record written alongside every encrypted
//!   payload.
//! * [`RevealManager`] — time-bounded plaintext access with automatic wipe.

pub mod decryptor;
pub mod encryptor;
pub mod identity;
pub mod metadata;
pub mod reveal;

pub use decryptor::Decryptor;
pub use encryptor::Encryptor;
pub use identity::Identity;
pub use metadata::{EncryptedFileMeta, IdentityKind, METADATA_VERSION};
pub use reveal::{
    RevealClosed, RevealDuration, RevealExpired, RevealManager, RevealSession, RevealStarted,
};
