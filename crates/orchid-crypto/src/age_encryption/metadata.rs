//! Sidecar metadata for age-encrypted payloads.

use bincode::{Decode, Encode};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::age_encryption::identity::Identity;
use crate::error::{CryptoError, Result};

/// Current metadata schema version. Bump when layout changes.
pub const METADATA_VERSION: u32 = 1;

/// Metadata written alongside an age-encrypted payload.
///
/// For single files it lives in `<name>.age.meta`; for directories it lives
/// as `.orchid-encrypted.meta` next to the archive.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct EncryptedFileMeta {
    /// Schema version (`METADATA_VERSION`).
    pub version: u32,
    /// Unique metadata id.
    #[bincode(with_serde)]
    pub id: Uuid,
    /// Original filename.
    pub original_name: String,
    /// Original plaintext size in bytes.
    pub original_size: u64,
    /// When encryption completed.
    #[bincode(with_serde)]
    pub encrypted_at: DateTime<Utc>,
    /// Kind of identity that encrypted the payload.
    pub identity_kind: IdentityKind,
    /// BLAKE3 of the plaintext, used for post-decrypt verification.
    pub blake3_of_plaintext: [u8; 32],
    /// Optional MIME type or `"directory"`.
    pub content_type_hint: Option<String>,
}

/// Which [`Identity`] variant produced an encrypted payload.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    Encode,
    Decode,
)]
#[serde(rename_all = "kebab-case")]
pub enum IdentityKind {
    /// Passphrase / scrypt.
    Passphrase,
    /// Asymmetric age X25519.
    X25519,
}

impl EncryptedFileMeta {
    /// Build a fresh metadata record with `encrypted_at = now`.
    #[must_use]
    pub fn new(
        original_name: &str,
        original_size: u64,
        identity: &Identity,
        plaintext_hash: [u8; 32],
    ) -> Self {
        Self {
            version: METADATA_VERSION,
            id: crate::random::random_uuid(),
            original_name: original_name.to_string(),
            original_size,
            encrypted_at: Utc::now(),
            identity_kind: identity.kind(),
            blake3_of_plaintext: plaintext_hash,
            content_type_hint: None,
        }
    }

    /// Attach a content-type hint (MIME or `"directory"`).
    #[must_use]
    pub fn with_content_type_hint(mut self, hint: impl Into<String>) -> Self {
        self.content_type_hint = Some(hint.into());
        self
    }

    /// Bincode encode to a byte vector.
    ///
    /// # Errors
    ///
    /// Propagates [`CryptoError::Encoding`] on encode failure.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        bincode::encode_to_vec(self, bincode::config::standard())
            .map_err(|e| CryptoError::Encoding(e.to_string()))
    }

    /// Bincode decode from a byte slice.
    ///
    /// # Errors
    ///
    /// Propagates [`CryptoError::Encoding`] on decode failure.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let (v, _) = bincode::decode_from_slice(bytes, bincode::config::standard())
            .map_err(|e| CryptoError::Encoding(e.to_string()))?;
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bincode_round_trip() {
        let id = Identity::passphrase("pw");
        let meta = EncryptedFileMeta::new("a.txt", 1024, &id, [7u8; 32])
            .with_content_type_hint("text/plain");
        let bytes = meta.to_bytes().unwrap();
        let back = EncryptedFileMeta::from_bytes(&bytes).unwrap();
        assert_eq!(back.version, METADATA_VERSION);
        assert_eq!(back.original_name, "a.txt");
        assert_eq!(back.original_size, 1024);
        assert_eq!(back.identity_kind, IdentityKind::Passphrase);
        assert_eq!(back.content_type_hint.as_deref(), Some("text/plain"));
    }
}
