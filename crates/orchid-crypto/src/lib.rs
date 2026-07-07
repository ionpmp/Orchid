//! Cryptography layer for Orchid: file encryption, password database, and
//! content-addressed storage.
//!
//! # Layout
//!
//! * [`age_encryption`] — file / stream / directory encryption via the
//!   [`age`] crate, with a time-bounded [`RevealManager`] for plaintext
//!   access.
//! * [`kdbx`] — KDBX4 password database built on the `keepass` crate, with
//!   TOTP helpers and a [`SecureClipboard`] trait for the UI layer.
//! * [`content`] — BLAKE3 hashing, FastCDC chunking, and a refcount-aware
//!   [`ChunkStore`] backed by `orchid-storage`.
//! * [`secret`] — [`ZeroizingBytes`] and a Windows DPAPI wrapper.
//! * [`random`] — secure randomness helpers.
//!
//! # Threat model
//!
//! * File encryption (`age`) protects confidentiality at rest. A stolen
//!   `.age` file is useless without the passphrase or X25519 identity.
//! * Password databases (`KDBX4` with Argon2id) protect the password vault
//!   at rest; the vault is only in cleartext inside a running Orchid
//!   process.
//! * Content-addressed chunks are plaintext by design — deduplication
//!   across files requires stable content. Encrypted files that need
//!   deduplication must be encrypted *after* chunking, which is the
//!   responsibility of a higher layer (`orchid-fs`).
//! * DPAPI-protected blobs defend against an offline attacker without
//!   access to the user's Windows profile, but NOT against malware running
//!   as the same user.

#![warn(missing_docs)]
#![warn(clippy::all)]
#![deny(unsafe_code)]
// Aggregate error types are big; boxing them wholesale to please
// `result_large_err` would allocate on every error path for no real benefit.
#![allow(clippy::result_large_err)]

pub mod age_encryption;
pub mod biometric;
pub mod content;
pub mod error;
pub mod kdbx;
pub mod random;
pub mod secret;
pub mod vault;

pub use age_encryption::{
    Decryptor, EncryptedFileMeta, Encryptor, Identity, IdentityKind, RevealClosed,
    RevealDuration, RevealExpired, RevealManager, RevealSession, RevealStarted,
};
pub use biometric::{
    check_availability as check_biometric_availability, verify_user as verify_biometric_user,
    BiometricAvailability, BiometricVerification,
};
pub use content::{
    from_hex, hash_bytes, hash_file, hex, Chunk, ChunkRef, ChunkRefInfo, ChunkStore, Chunker,
    ChunkerConfig, Clock, DedupStats, Deduplicator, FileManifest, FixedClock, GcStats,
    StreamHasher, SystemClock,
};
pub use error::{CryptoError, Result};
pub use kdbx::{
    generate_code, parse_otpauth_uri, to_otpauth_uri, PasswordDatabase, PasswordEntry,
    PasswordGroup, SearchQuery, SearchResult, SecureClipboard, TotpAlgorithm, TotpCode,
    TotpConfig,
};
pub use random::{fill_secure, random_bytes, random_uuid};
pub use secret::{SecretBytes, ZeroizingBytes};
pub use vault::{FmPassphraseVault, PasswordVault};

/// Crate version.
///
/// # Examples
///
/// ```
/// assert!(!orchid_crypto::version().is_empty());
/// ```
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
