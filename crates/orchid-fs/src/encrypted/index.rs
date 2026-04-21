//! On-disk index of encrypted paths.
//!
//! The index intentionally stores only the [`orchid_crypto::IdentityKind`]
//! — never the underlying secret material. The user re-supplies the
//! passphrase / key at every reveal.

use bincode::{Decode, Encode};
use orchid_crypto::{IdentityKind, RevealDuration};
use redb::TableDefinition;
use serde::{Deserialize, Serialize};

use crate::path::FsPath;

/// Persisted encrypted-folder record. Mirrors [`EncryptedFolderConfig`]
/// from the public API minus the key material.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct EncryptedFolderRecord {
    /// Filesystem path that carries encrypted data.
    pub path: FsPath,
    /// Kind of identity that produced the payload.
    pub identity_kind: IdentityKind,
    /// Reveal-window policy applied on open.
    pub reveal_duration: RevealDurationCompat,
    /// Whether the declaration is active.
    pub enabled: bool,
}

/// Local shadow of [`orchid_crypto::RevealDuration`]: we need our own `Encode`/
/// `Decode` derives for bincode round-trips. Converted back and forth at the
/// API boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Encode, Decode)]
pub enum RevealDurationCompat {
    /// 5 minutes.
    FiveMinutes,
    /// 30 minutes.
    ThirtyMinutes,
    /// 1 hour.
    OneHour,
    /// Persist until explicitly closed.
    UntilClosed,
}

impl From<RevealDuration> for RevealDurationCompat {
    fn from(d: RevealDuration) -> Self {
        match d {
            RevealDuration::FiveMinutes => Self::FiveMinutes,
            RevealDuration::ThirtyMinutes => Self::ThirtyMinutes,
            RevealDuration::OneHour => Self::OneHour,
            RevealDuration::UntilClosed => Self::UntilClosed,
        }
    }
}

impl From<RevealDurationCompat> for RevealDuration {
    fn from(d: RevealDurationCompat) -> Self {
        match d {
            RevealDurationCompat::FiveMinutes => Self::FiveMinutes,
            RevealDurationCompat::ThirtyMinutes => Self::ThirtyMinutes,
            RevealDurationCompat::OneHour => Self::OneHour,
            RevealDurationCompat::UntilClosed => Self::UntilClosed,
        }
    }
}

/// redb table of registered encrypted paths.
pub(crate) const ENCRYPTED_PATHS: TableDefinition<
    &str,
    orchid_storage::Value<EncryptedFolderRecord>,
> = TableDefinition::new("fs_encrypted_paths");
