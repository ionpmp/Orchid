//! Common archive types shared by every format-specific reader.

use std::path::Path;

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
    TarBz2,
    Gzip,
    Bzip2,
    Xz,
    Rar,
    Cab,
    Iso,
    Ace,
    Arj,
    Lzh,
}

impl ArchiveFormat {
    /// File extension used when creating this format (`zip`, `7z`, `tar.gz`, …).
    #[must_use]
    pub fn ext(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::SevenZ => "7z",
            Self::Tar => "tar",
            Self::TarGz => "tar.gz",
            Self::TarXz => "tar.xz",
            Self::TarBz2 => "tar.bz2",
            Self::Gzip => "gz",
            Self::Bzip2 => "bz2",
            Self::Xz => "xz",
            Self::Rar => "rar",
            Self::Cab => "cab",
            Self::Iso => "iso",
            Self::Ace => "ace",
            Self::Arj => "arj",
            Self::Lzh => "lzh",
        }
    }

    /// Guess a format from a file name (handles `.tar.gz` and similar).
    #[must_use]
    pub fn from_file_name(name: &str) -> Option<Self> {
        let n = name.to_ascii_lowercase();
        if n.ends_with(".tar.gz") || n.ends_with(".tgz") {
            return Some(Self::TarGz);
        }
        if n.ends_with(".tar.xz") || n.ends_with(".txz") {
            return Some(Self::TarXz);
        }
        if n.ends_with(".tar.bz2") || n.ends_with(".tbz2") || n.ends_with(".tbz") {
            return Some(Self::TarBz2);
        }
        let ext = n.rsplit_once('.')?.1;
        Some(match ext {
            "zip" => Self::Zip,
            "7z" => Self::SevenZ,
            "tar" => Self::Tar,
            "gz" => Self::TarGz,
            "bz2" => Self::TarBz2,
            "xz" => Self::TarXz,
            "rar" => Self::Rar,
            "cab" => Self::Cab,
            "iso" | "img" => Self::Iso,
            "ace" => Self::Ace,
            "arj" => Self::Arj,
            "lzh" | "lha" => Self::Lzh,
            _ => return None,
        })
    }

    /// Formats that the bundled Rust readers can open without 7-Zip.
    #[must_use]
    pub fn native_readable(self) -> bool {
        matches!(
            self,
            Self::Zip | Self::SevenZ | Self::Tar | Self::TarGz | Self::TarXz | Self::TarBz2
        )
    }

    /// Formats that can be created without 7-Zip (no password / volumes / SFX).
    #[must_use]
    pub fn native_writable(self) -> bool {
        matches!(
            self,
            Self::Zip | Self::Tar | Self::TarGz | Self::TarXz | Self::TarBz2
        )
    }
}

impl std::fmt::Display for ArchiveFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.ext())
    }
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

/// Options for creating an archive.
#[derive(Debug, Clone, Default)]
pub struct CreateArchiveOptions {
    /// Optional encryption password.
    pub password: Option<String>,
    /// Split into volumes of this many bytes (7-Zip).
    pub volume_bytes: Option<u64>,
    /// Build a self-extracting archive (7-Zip `-sfx`).
    pub sfx: bool,
}

/// Result of `7z t` / CRC verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveTestReport {
    /// Human-readable summary.
    pub summary: String,
    /// True when the archive tested clean.
    pub ok: bool,
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

/// True when `name` looks like a supported archive (including nested ones).
#[must_use]
pub fn looks_like_archive_name(name: &str) -> bool {
    ArchiveFormat::from_file_name(name).is_some()
}

/// True when `path`'s file name looks like a supported archive.
#[must_use]
pub fn looks_like_archive_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(looks_like_archive_name)
}
