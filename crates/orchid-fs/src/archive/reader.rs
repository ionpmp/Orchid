//! Unified archive reader trait and factory.

use std::path::Path;

use async_trait::async_trait;

use crate::archive::sevenz::SevenZArchiveReader;
use crate::archive::tar::{TarGzReader, TarReader};
use crate::archive::types::ArchiveFormat;
use crate::archive::zip::ZipReader;
use crate::error::{FsError, Result};

/// Read-only access to an archive of any supported format.
#[async_trait]
pub trait ArchiveReader: Send + Sync {
    /// Format tag.
    fn format(&self) -> ArchiveFormat;

    /// List every entry.
    async fn list(&self) -> Result<Vec<crate::archive::types::ArchiveEntry>>;

    /// Read a single entry into memory.
    async fn read_entry(&self, path: &str) -> Result<Vec<u8>>;

    /// Extract a single entry to disk.
    async fn extract_entry(&self, path: &str, output: &Path) -> Result<()>;

    /// Extract every entry; returns the number of files written.
    async fn extract_all(&self, output: &Path) -> Result<u64>;
}

/// Sniff the archive format from magic bytes at the start of the stream.
#[must_use]
pub fn detect_format(bytes: &[u8]) -> Option<ArchiveFormat> {
    if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") || bytes.starts_with(b"PK\x07\x08") {
        return Some(ArchiveFormat::Zip);
    }
    if bytes.starts_with(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        return Some(ArchiveFormat::SevenZ);
    }
    if bytes.starts_with(&[0x1F, 0x8B]) {
        return Some(ArchiveFormat::TarGz);
    }
    if bytes.len() >= 262 && &bytes[257..262] == b"ustar" {
        return Some(ArchiveFormat::Tar);
    }
    None
}

/// Open the archive at `path`, choosing a reader by sniffed magic bytes.
/// Falls back to the file extension when magic bytes are ambiguous.
///
/// # Errors
///
/// Returns [`FsError::UnsupportedArchive`] when neither magic nor extension
/// identifies a supported format.
pub fn open_archive(path: &Path) -> Result<Box<dyn ArchiveReader>> {
    let format = sniff_format(path)?;
    Ok(match format {
        ArchiveFormat::Zip => Box::new(ZipReader::new(path.to_path_buf())),
        ArchiveFormat::Tar => Box::new(TarReader::new(path.to_path_buf())),
        ArchiveFormat::TarGz => Box::new(TarGzReader::new(path.to_path_buf())),
        ArchiveFormat::SevenZ => Box::new(SevenZArchiveReader::new(path.to_path_buf())),
    })
}

fn sniff_format(path: &Path) -> Result<ArchiveFormat> {
    use std::io::Read;
    let mut head = [0u8; 512];
    let n = {
        let mut f = std::fs::File::open(path)?;
        f.read(&mut head)?
    };
    if let Some(fmt) = detect_format(&head[..n]) {
        return Ok(fmt);
    }
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("zip") => Ok(ArchiveFormat::Zip),
        Some("7z") => Ok(ArchiveFormat::SevenZ),
        Some("tar") => Ok(ArchiveFormat::Tar),
        Some("tgz") | Some("gz") => Ok(ArchiveFormat::TarGz),
        _ => Err(FsError::UnsupportedArchive(path.display().to_string())),
    }
}
