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

    /// Unsupported compression codec for Phase 1.
    #[error("unsupported compression codec {0}")]
    UnsupportedCompression(u8),

    /// Feature not implemented in this Phase 1 sealed path.
    #[error("unsupported: {0}")]
    Unsupported(&'static str),
}
