//! Common archive types shared by every format-specific reader.

use chrono::{DateTime, Utc};

/// Archive format enum. Reader implementations are chosen based on this.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    SevenZ,
    Tar,
    TarGz,
    TarXz,
}

/// A single entry in an archive.
#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    /// Forward-slash separated path inside the archive.
    pub path: String,
    /// Uncompressed size in bytes.
    pub size: u64,
    /// Compressed size in bytes, if known.
    pub compressed_size: Option<u64>,
    /// Last-modified timestamp, if recorded by the archive.
    pub modified: Option<DateTime<Utc>>,
    /// Whether the entry is a directory rather than a file.
    pub is_dir: bool,
    /// CRC32 of the uncompressed data, if recorded (only zip stores this).
    pub crc32: Option<u32>,
}

/// Reject entries whose path contains `..` or is absolute. Returns the
/// normalised forward-slashed path on success.
pub(crate) fn sanitise_entry_path(raw: &str) -> Option<String> {
    let normalised = raw.replace('\\', "/");
    let trimmed = normalised.trim_start_matches('/');
    for seg in trimmed.split('/') {
        if seg == ".." {
            return None;
        }
    }
    if normalised.starts_with('/') {
        return None;
    }
    Some(trimmed.to_string())
}
