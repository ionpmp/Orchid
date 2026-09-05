//! zstd helpers for Clean-Text and Structured regions.

use crate::{FormatError, Result};

/// Compress `plaintext` with zstd (default level).
pub fn compress_zstd(plaintext: &[u8]) -> Result<Vec<u8>> {
    zstd::encode_all(plaintext, 3).map_err(|e| FormatError::RegionDecode(e.to_string()))
}

/// Decompress zstd `payload` into owned bytes.
pub fn decompress_zstd(payload: &[u8]) -> Result<Vec<u8>> {
    zstd::decode_all(payload).map_err(|e| FormatError::RegionDecode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zstd_roundtrip() {
        let src = b"hello orchid clean-text\n".repeat(40);
        let c = compress_zstd(&src).unwrap();
        assert!(c.len() < src.len());
        assert_eq!(decompress_zstd(&c).unwrap(), src);
    }
}
