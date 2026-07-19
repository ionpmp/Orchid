//! age-based encryption front-end.

use std::io::Write;
use std::path::{Path, PathBuf};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::age_encryption::identity::Identity;
use crate::age_encryption::metadata::EncryptedFileMeta;
use crate::content::hash::hash_bytes;
use crate::error::{CryptoError, Result};

/// Extension applied to the encrypted payload.
const AGE_EXT: &str = "age";

/// Extension for the sidecar metadata file.
const META_EXT: &str = "age.meta";

/// Name of the per-directory metadata sidecar.
pub(crate) const DIR_META_NAME: &str = ".orchid-encrypted.meta";

/// Name of the per-directory archive (tar.age) file, relative to the output
/// directory. Prefixed with a leading dot so it doesn't collide with user
/// files that might live next to it.
pub(crate) const DIR_ARCHIVE_NAME: &str = ".orchid-encrypted.tar.age";

/// Stateless front-end for encrypting payloads with a fixed [`Identity`].
#[derive(Clone)]
pub struct Encryptor {
    identity: Identity,
}

impl std::fmt::Debug for Encryptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Encryptor")
            .field("identity", &self.identity)
            .finish()
    }
}

impl Encryptor {
    /// Construct an encryptor bound to `identity`.
    #[must_use]
    pub fn new(identity: Identity) -> Self {
        Self { identity }
    }

    /// Encrypt an in-memory byte buffer.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::AgeEncrypt`] if the age pipeline fails (for
    /// example because no recipients were supplied).
    pub fn encrypt_bytes(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        encrypt_bytes_sync(&self.identity, plaintext)
    }

    /// Encrypt `input_path` into `output_path`, writing a sidecar `.age.meta`
    /// file. The encrypted bytes are written to `<output>.tmp` first and
    /// renamed atomically on success.
    ///
    /// # Errors
    ///
    /// Propagates I/O errors; [`CryptoError::AgeEncrypt`] for encryption
    /// failures.
    pub async fn encrypt_file(
        &self,
        input_path: &Path,
        output_path: &Path,
    ) -> Result<EncryptedFileMeta> {
        let identity = self.identity.clone();
        let input_path = input_path.to_path_buf();
        let output_path = output_path.to_path_buf();

        tokio::task::spawn_blocking(move || {
            encrypt_file_blocking(&identity, &input_path, &output_path)
        })
        .await
        .map_err(|e| CryptoError::AgeEncrypt(format!("join error: {e}")))?
    }

    /// Encrypt an async stream to another async stream. Returns the BLAKE3
    /// hash of the plaintext that flowed through.
    ///
    /// Currently buffers the plaintext in memory. For very large payloads
    /// prefer [`Encryptor::encrypt_file`].
    ///
    /// # Errors
    ///
    /// Propagates I/O and age errors.
    pub async fn encrypt_stream<R, W>(
        &self,
        mut reader: R,
        mut writer: W,
    ) -> Result<[u8; 32]>
    where
        R: tokio::io::AsyncRead + Unpin + Send,
        W: tokio::io::AsyncWrite + Unpin + Send,
    {
        let mut plaintext = Vec::new();
        reader.read_to_end(&mut plaintext).await?;
        let hash = hash_bytes(&plaintext);

        let identity = self.identity.clone();
        let ciphertext = tokio::task::spawn_blocking(move || {
            encrypt_bytes_sync(&identity, &plaintext)
        })
        .await
        .map_err(|e| CryptoError::AgeEncrypt(format!("join error: {e}")))??;

        writer.write_all(&ciphertext).await?;
        writer.flush().await?;
        Ok(hash)
    }

    /// Encrypt a directory tree by tar-ing it in memory, then encrypting the
    /// tar. Writes `<output_dir>/.orchid-encrypted.tar.age` and
    /// `<output_dir>/.orchid-encrypted.meta`.
    ///
    /// The caller is responsible for removing `input_dir` after success.
    ///
    /// # Errors
    ///
    /// Propagates I/O and age errors.
    pub async fn encrypt_directory(
        &self,
        input_dir: &Path,
        output_dir: &Path,
    ) -> Result<EncryptedFileMeta> {
        let identity = self.identity.clone();
        let input_dir = input_dir.to_path_buf();
        let output_dir = output_dir.to_path_buf();

        tokio::task::spawn_blocking(move || {
            encrypt_directory_blocking(&identity, &input_dir, &output_dir)
        })
        .await
        .map_err(|e| CryptoError::AgeEncrypt(format!("join error: {e}")))?
    }
}

// ---------------------------------------------------------------------------
// Synchronous helpers. These live inside `tokio::task::spawn_blocking` calls.
// ---------------------------------------------------------------------------

fn encrypt_bytes_sync(identity: &Identity, plaintext: &[u8]) -> Result<Vec<u8>> {
    match identity {
        Identity::Passphrase(pw) => {
            let encryptor = age::Encryptor::with_user_passphrase(pw.clone());
            let mut out = Vec::with_capacity(plaintext.len() + 256);
            {
                let mut writer = encryptor
                    .wrap_output(&mut out)
                    .map_err(|e| CryptoError::AgeEncrypt(format!("wrap_output: {e}")))?;
                writer
                    .write_all(plaintext)
                    .map_err(|e| CryptoError::AgeEncrypt(format!("write: {e}")))?;
                writer
                    .finish()
                    .map_err(|e| CryptoError::AgeEncrypt(format!("finish: {e}")))?;
            }
            Ok(out)
        }
        Identity::X25519(id) => age::encrypt(&id.to_public(), plaintext)
            .map_err(|e| CryptoError::AgeEncrypt(e.to_string())),
    }
}

fn encrypt_file_blocking(
    identity: &Identity,
    input_path: &Path,
    output_path: &Path,
) -> Result<EncryptedFileMeta> {
    let plaintext = std::fs::read(input_path)?;
    let plaintext_hash = hash_bytes(&plaintext);
    let original_size = plaintext.len() as u64;

    let original_name = input_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unnamed")
        .to_string();

    let ciphertext = encrypt_bytes_sync(identity, &plaintext)?;

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp: PathBuf = output_path.with_extension(format!("{AGE_EXT}.tmp"));
    std::fs::write(&tmp, &ciphertext)?;
    std::fs::rename(&tmp, output_path)?;

    let meta = EncryptedFileMeta::new(&original_name, original_size, identity, plaintext_hash);
    let meta_path = output_path.with_extension(META_EXT);
    std::fs::write(&meta_path, meta.to_bytes()?)?;

    Ok(meta)
}

fn encrypt_directory_blocking(
    identity: &Identity,
    input_dir: &Path,
    output_dir: &Path,
) -> Result<EncryptedFileMeta> {
    // Build a tarball in memory. For MVP this buffers the entire directory;
    // `orchid-fs` will later replace this with a streaming implementation
    // backed by temp files.
    let mut tar_bytes: Vec<u8> = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut tar_bytes);
        builder
            .append_dir_all(".", input_dir)
            .map_err(|e| CryptoError::Io(std::io::Error::other(e)))?;
        builder
            .finish()
            .map_err(|e| CryptoError::Io(std::io::Error::other(e)))?;
    }

    let plaintext_hash = hash_bytes(&tar_bytes);
    let original_size = tar_bytes.len() as u64;
    let original_name = input_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("directory")
        .to_string();

    let ciphertext = encrypt_bytes_sync(identity, &tar_bytes)?;

    std::fs::create_dir_all(output_dir)?;
    let archive_path = output_dir.join(DIR_ARCHIVE_NAME);
    let tmp = archive_path.with_extension("age.tmp");
    std::fs::write(&tmp, &ciphertext)?;
    std::fs::rename(&tmp, &archive_path)?;

    let meta =
        EncryptedFileMeta::new(&original_name, original_size, identity, plaintext_hash)
            .with_content_type_hint("directory");
    let meta_path = output_dir.join(DIR_META_NAME);
    std::fs::write(&meta_path, meta.to_bytes()?)?;

    Ok(meta)
}
