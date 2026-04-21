//! age-based decryption front-end.

use std::io::Read;
use std::path::Path;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::age_encryption::encryptor::{DIR_ARCHIVE_NAME, DIR_META_NAME};
use crate::age_encryption::identity::Identity;
use crate::age_encryption::metadata::EncryptedFileMeta;
use crate::content::hash::{hash_bytes, hex};
use crate::error::{CryptoError, Result};
use crate::secret::zeroizing::ZeroizingBytes;

const META_EXT: &str = "age.meta";

/// Stateless front-end for decrypting payloads with a fixed [`Identity`].
#[derive(Clone)]
pub struct Decryptor {
    identity: Identity,
}

impl std::fmt::Debug for Decryptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Decryptor")
            .field("identity", &self.identity)
            .finish()
    }
}

impl Decryptor {
    /// Construct a decryptor bound to `identity`.
    #[must_use]
    pub fn new(identity: Identity) -> Self {
        Self { identity }
    }

    /// Decrypt an in-memory ciphertext.
    ///
    /// # Errors
    ///
    /// * [`CryptoError::InvalidPassphrase`] if the identity is a passphrase
    ///   that does not unwrap the payload.
    /// * [`CryptoError::AgeDecrypt`] for any other decryption failure.
    pub fn decrypt_bytes(&self, ciphertext: &[u8]) -> Result<ZeroizingBytes> {
        decrypt_bytes_sync(&self.identity, ciphertext)
    }

    /// Decrypt `input_path` into `output_path`. On success the sidecar
    /// `<input>.age.meta` is consulted to verify the plaintext hash and the
    /// returned metadata reflects what was recorded at encryption time.
    ///
    /// On plaintext-hash mismatch the partially-written output is removed
    /// and [`CryptoError::AgeDecrypt`] is returned.
    ///
    /// # Errors
    ///
    /// Propagates I/O and age errors.
    pub async fn decrypt_file(
        &self,
        input_path: &Path,
        output_path: &Path,
    ) -> Result<EncryptedFileMeta> {
        let identity = self.identity.clone();
        let input = input_path.to_path_buf();
        let output = output_path.to_path_buf();
        tokio::task::spawn_blocking(move || decrypt_file_blocking(&identity, &input, &output))
            .await
            .map_err(|e| CryptoError::AgeDecrypt(format!("join error: {e}")))?
    }

    /// Decrypt an async stream to another async stream. Returns the metadata
    /// recovered from the sidecar (or a synthetic empty record when the
    /// caller did not provide one).
    ///
    /// Currently buffers the plaintext in memory. For very large payloads
    /// prefer [`Decryptor::decrypt_file`].
    ///
    /// # Errors
    ///
    /// Propagates I/O and age errors.
    pub async fn decrypt_stream<R, W>(
        &self,
        mut reader: R,
        mut writer: W,
    ) -> Result<EncryptedFileMeta>
    where
        R: tokio::io::AsyncRead + Unpin + Send,
        W: tokio::io::AsyncWrite + Unpin + Send,
    {
        let mut ciphertext = Vec::new();
        reader.read_to_end(&mut ciphertext).await?;

        let identity = self.identity.clone();
        let plaintext = tokio::task::spawn_blocking(move || {
            decrypt_bytes_sync(&identity, &ciphertext)
        })
        .await
        .map_err(|e| CryptoError::AgeDecrypt(format!("join error: {e}")))??;

        let hash = hash_bytes(plaintext.as_slice());
        writer.write_all(plaintext.as_slice()).await?;
        writer.flush().await?;

        Ok(EncryptedFileMeta {
            version: crate::age_encryption::metadata::METADATA_VERSION,
            id: crate::random::random_uuid(),
            original_name: "<stream>".into(),
            original_size: plaintext.as_slice().len() as u64,
            encrypted_at: chrono::Utc::now(),
            identity_kind: self.identity.kind(),
            blake3_of_plaintext: hash,
            content_type_hint: None,
        })
    }

    /// Decrypt a directory previously produced by
    /// [`crate::Encryptor::encrypt_directory`].
    ///
    /// # Errors
    ///
    /// Propagates I/O and age errors; returns [`CryptoError::AgeDecrypt`] if
    /// the recovered tar fails plaintext hash verification.
    pub async fn decrypt_directory(
        &self,
        input_dir: &Path,
        output_dir: &Path,
    ) -> Result<EncryptedFileMeta> {
        let identity = self.identity.clone();
        let input = input_dir.to_path_buf();
        let output = output_dir.to_path_buf();
        tokio::task::spawn_blocking(move || {
            decrypt_directory_blocking(&identity, &input, &output)
        })
        .await
        .map_err(|e| CryptoError::AgeDecrypt(format!("join error: {e}")))?
    }

    /// Read the sidecar metadata file without decrypting the payload.
    ///
    /// # Errors
    ///
    /// [`CryptoError::MetadataUnreadable`] on parse failure or missing
    /// sidecar file.
    pub async fn read_metadata(&self, input_path: &Path) -> Result<EncryptedFileMeta> {
        let meta_path = input_path.with_extension(META_EXT);
        let bytes = tokio::fs::read(&meta_path)
            .await
            .map_err(|e| CryptoError::MetadataUnreadable(format!("{}: {e}", meta_path.display())))?;
        EncryptedFileMeta::from_bytes(&bytes)
    }
}

// ---------------------------------------------------------------------------
// Synchronous helpers
// ---------------------------------------------------------------------------

fn decrypt_bytes_sync(identity: &Identity, ciphertext: &[u8]) -> Result<ZeroizingBytes> {
    let decryptor = age::Decryptor::new(ciphertext)
        .map_err(|e| CryptoError::AgeDecrypt(format!("decryptor init: {e}")))?;

    let mut plaintext = Vec::new();
    match decryptor {
        age::Decryptor::Passphrase(d) => {
            let pw = identity.as_passphrase().ok_or_else(|| {
                CryptoError::AgeDecrypt("passphrase-encrypted file requires a passphrase identity".into())
            })?;
            let mut reader = d
                .decrypt(&pw, None)
                .map_err(|e| map_decrypt_error(e, identity))?;
            reader
                .read_to_end(&mut plaintext)
                .map_err(|e| CryptoError::AgeDecrypt(format!("read: {e}")))?;
        }
        age::Decryptor::Recipients(d) => {
            let x25519 = identity.as_age_identity().ok_or_else(|| {
                CryptoError::AgeDecrypt(
                    "recipient-encrypted file requires an X25519 identity".into(),
                )
            })?;
            let ident_ref: &age::x25519::Identity = &x25519;
            let mut reader = d
                .decrypt(std::iter::once(ident_ref as &dyn age::Identity))
                .map_err(|e| map_decrypt_error(e, identity))?;
            reader
                .read_to_end(&mut plaintext)
                .map_err(|e| CryptoError::AgeDecrypt(format!("read: {e}")))?;
        }
    }

    Ok(ZeroizingBytes::new(plaintext))
}

fn map_decrypt_error(e: age::DecryptError, identity: &Identity) -> CryptoError {
    let msg = e.to_string();
    // `age` does not have a strongly-typed "wrong passphrase" variant in
    // public API, so we detect it by string match. False positives
    // downgrade to the generic `AgeDecrypt` variant.
    let is_passphrase = matches!(identity, Identity::Passphrase(_));
    if is_passphrase && (msg.contains("incorrect passphrase") || msg.contains("no matching keys"))
    {
        CryptoError::InvalidPassphrase
    } else {
        CryptoError::AgeDecrypt(msg)
    }
}

fn decrypt_file_blocking(
    identity: &Identity,
    input: &Path,
    output: &Path,
) -> Result<EncryptedFileMeta> {
    let ciphertext = std::fs::read(input)?;
    let plaintext = decrypt_bytes_sync(identity, &ciphertext)?;

    // Load and verify metadata.
    let meta_path = input.with_extension(META_EXT);
    let meta_bytes = std::fs::read(&meta_path)
        .map_err(|e| CryptoError::MetadataUnreadable(format!("{}: {e}", meta_path.display())))?;
    let meta = EncryptedFileMeta::from_bytes(&meta_bytes)?;
    let actual = hash_bytes(plaintext.as_slice());
    if actual != meta.blake3_of_plaintext {
        return Err(CryptoError::AgeDecrypt(format!(
            "plaintext integrity check failed (expected {}, got {})",
            hex(&meta.blake3_of_plaintext),
            hex(&actual)
        )));
    }

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = output.with_extension("orchid-decrypting");
    std::fs::write(&tmp, plaintext.as_slice())?;
    std::fs::rename(&tmp, output)?;
    Ok(meta)
}

fn decrypt_directory_blocking(
    identity: &Identity,
    input_dir: &Path,
    output_dir: &Path,
) -> Result<EncryptedFileMeta> {
    let archive_path = input_dir.join(DIR_ARCHIVE_NAME);
    let meta_path = input_dir.join(DIR_META_NAME);
    let ciphertext = std::fs::read(&archive_path)?;
    let plaintext = decrypt_bytes_sync(identity, &ciphertext)?;
    let meta_bytes = std::fs::read(&meta_path)
        .map_err(|e| CryptoError::MetadataUnreadable(format!("{}: {e}", meta_path.display())))?;
    let meta = EncryptedFileMeta::from_bytes(&meta_bytes)?;

    let actual = hash_bytes(plaintext.as_slice());
    if actual != meta.blake3_of_plaintext {
        return Err(CryptoError::AgeDecrypt(format!(
            "plaintext integrity check failed (expected {}, got {})",
            hex(&meta.blake3_of_plaintext),
            hex(&actual)
        )));
    }

    std::fs::create_dir_all(output_dir)?;
    let mut archive = tar::Archive::new(std::io::Cursor::new(plaintext.as_slice()));
    archive
        .unpack(output_dir)
        .map_err(|e| CryptoError::Io(std::io::Error::other(e)))?;
    Ok(meta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::age_encryption::encryptor::Encryptor;

    #[test]
    fn round_trip_bytes_passphrase() {
        let id = Identity::passphrase("pw");
        let enc = Encryptor::new(id.clone());
        let dec = Decryptor::new(id);
        let payload = b"hello encrypted world";
        let ct = enc.encrypt_bytes(payload).unwrap();
        let pt = dec.decrypt_bytes(&ct).unwrap();
        assert_eq!(pt.as_slice(), payload);
    }

    #[test]
    fn wrong_passphrase_is_rejected() {
        let enc = Encryptor::new(Identity::passphrase("right"));
        let dec = Decryptor::new(Identity::passphrase("wrong"));
        let ct = enc.encrypt_bytes(b"secret").unwrap();
        let err = dec.decrypt_bytes(&ct).unwrap_err();
        assert!(matches!(
            err,
            CryptoError::InvalidPassphrase | CryptoError::AgeDecrypt(_)
        ));
    }

    #[test]
    fn round_trip_bytes_x25519() {
        let id = Identity::generate_x25519();
        let enc = Encryptor::new(id.clone());
        let dec = Decryptor::new(id);
        let ct = enc.encrypt_bytes(b"asymmetric payload").unwrap();
        let pt = dec.decrypt_bytes(&ct).unwrap();
        assert_eq!(pt.as_slice(), b"asymmetric payload");
    }

    #[test]
    fn tampered_ciphertext_errors() {
        let id = Identity::passphrase("pw");
        let enc = Encryptor::new(id.clone());
        let dec = Decryptor::new(id);
        let mut ct = enc.encrypt_bytes(b"orig").unwrap();
        // Flip some trailing bits — within the HMAC-protected section.
        let n = ct.len();
        ct[n - 1] ^= 0xFF;
        assert!(dec.decrypt_bytes(&ct).is_err());
    }
}
