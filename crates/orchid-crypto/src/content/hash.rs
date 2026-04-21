//! BLAKE3 hashing over bytes, files, and streams.

use std::path::Path;

use tokio::fs::File;
use tokio::io::AsyncReadExt;

use crate::error::{CryptoError, Result};

/// File-size threshold at which [`hash_file`] switches to a memory-mapped
/// parallel hasher. Below this, we use the streaming read path.
const MMAP_THRESHOLD_BYTES: u64 = 16 * 1024 * 1024;

/// Size of each streaming read chunk.
const STREAM_CHUNK_BYTES: usize = 1024 * 1024;

/// BLAKE3 hash of an in-memory byte slice.
///
/// # Examples
///
/// ```
/// use orchid_crypto::hash_bytes;
/// let h = hash_bytes(b"");
/// assert_eq!(h.len(), 32);
/// ```
#[must_use]
pub fn hash_bytes(data: &[u8]) -> [u8; 32] {
    *blake3::hash(data).as_bytes()
}

/// BLAKE3 hash of a file on disk.
///
/// For files larger than ~16 MiB this uses `update_mmap_rayon` for parallel
/// hashing; below that threshold it streams 1 MiB chunks asynchronously.
///
/// # Errors
///
/// * [`CryptoError::Io`] if the file cannot be opened or read.
pub async fn hash_file(path: &Path) -> Result<[u8; 32]> {
    let meta = tokio::fs::metadata(path).await?;
    if meta.len() >= MMAP_THRESHOLD_BYTES {
        let path_owned = path.to_path_buf();
        let digest = tokio::task::spawn_blocking(move || {
            let mut hasher = blake3::Hasher::new();
            hasher.update_mmap(&path_owned)?;
            Ok::<_, std::io::Error>(*hasher.finalize().as_bytes())
        })
        .await
        .map_err(|e| CryptoError::Encoding(format!("join error while hashing: {e}")))??;
        return Ok(digest);
    }

    let mut file = File::open(path).await?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; STREAM_CHUNK_BYTES];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(*hasher.finalize().as_bytes())
}

/// Reusable streaming BLAKE3 hasher. Convenient when encryption and hashing
/// happen in lockstep over the same bytes.
#[derive(Debug, Default)]
pub struct StreamHasher(blake3::Hasher);

impl StreamHasher {
    /// Construct a fresh hasher.
    #[must_use]
    pub fn new() -> Self {
        Self(blake3::Hasher::new())
    }

    /// Feed more bytes.
    pub fn update(&mut self, chunk: &[u8]) {
        self.0.update(chunk);
    }

    /// Finalise and return the 32-byte digest.
    #[must_use]
    pub fn finalize(self) -> [u8; 32] {
        *self.0.finalize().as_bytes()
    }
}

/// Hex-encode a 32-byte digest.
#[must_use]
pub fn hex(digest: &[u8; 32]) -> String {
    ::hex::encode(digest)
}

/// Decode a 64-character hex string into a 32-byte digest.
///
/// # Errors
///
/// Returns [`CryptoError::Encoding`] if the input is not 64 hex characters.
pub fn from_hex(s: &str) -> Result<[u8; 32]> {
    let bytes = ::hex::decode(s).map_err(|e| CryptoError::Encoding(e.to_string()))?;
    if bytes.len() != 32 {
        return Err(CryptoError::Encoding(format!(
            "expected 32 bytes, got {}",
            bytes.len()
        )));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_bytes_of_empty_matches_known_constant() {
        let h = hash_bytes(b"");
        // BLAKE3 empty hash — well-defined constant.
        let expected =
            ::hex::decode("af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262")
                .unwrap();
        assert_eq!(h.as_slice(), expected.as_slice());
    }

    #[test]
    fn hex_roundtrip() {
        let digest = hash_bytes(b"orchid");
        let s = hex(&digest);
        assert_eq!(from_hex(&s).unwrap(), digest);
    }

    #[tokio::test]
    async fn hash_file_matches_hash_bytes() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let data = b"some reasonably sized payload that lives on disk";
        std::fs::write(tmp.path(), data).unwrap();
        let a = hash_file(tmp.path()).await.unwrap();
        let b = hash_bytes(data);
        assert_eq!(a, b);
    }

    #[test]
    fn stream_hasher_matches_one_shot() {
        let mut h = StreamHasher::new();
        h.update(b"hello ");
        h.update(b"world");
        assert_eq!(h.finalize(), hash_bytes(b"hello world"));
    }
}
