//! Error types for `.orchid` format I/O.

use thiserror::Error;

/// Result alias for format operations.
pub type Result<T> = std::result::Result<T, FormatError>;

/// Failures while reading or writing a `.orchid` file.
#[derive(Debug, Error)]
pub enum FormatError {
    /// I/O failure.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Underlying `orchid-crypto` failure (age, chunk store, …).
    #[error("crypto error: {0}")]
    Crypto(#[from] orchid_crypto::CryptoError),

    /// Header or footer magic bytes were not `ORCD`.
    #[error("invalid file magic")]
    InvalidMagic,

    /// Region mini-header magic was not `ORCR`.
    #[error("invalid region magic at offset {0}")]
    InvalidRegionMagic(u64),

    /// Footer TOC BLAKE3 did not match the TOC bytes.
    #[error("TOC integrity check failed")]
    TocHashMismatch,

    /// FlatBuffers TOC failed verification or required fields were missing.
    #[error("invalid TOC: {0}")]
    InvalidToc(String),

    /// Region payload failed decompression or integrity checks.
    #[error("region decode failed: {0}")]
    RegionDecode(String),

    /// Requested region type was not present.
    #[error("region type 0x{0:04x} not found")]
    RegionNotFound(u16),

    /// Unsupported compression codec.
    #[error("unsupported compression codec {0}")]
    UnsupportedCompression(u8),

    /// Private region requires an [`orchid_crypto::Identity`].
    #[error("identity required to decrypt private region")]
    IdentityRequired,

    /// Feature not implemented on this code path.
    #[error("unsupported: {0}")]
    Unsupported(&'static str),
}
