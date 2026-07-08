//! Unified archive reader trait and factory.

use std::path::Path;

use async_trait::async_trait;

use crate::archive::sevenz::SevenZArchiveReader;
use crate::archive::tar::{TarGzReader, TarReader, TarXzReader};
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

/// XZ stream magic: `FD 37 7A 58 5A 00`.
const XZ_MAGIC: &[u8] = &[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00];

/// Sniff the archive format from magic bytes at the start of the stream.
#[must_use]
pub fn detect_format(bytes: &[u8]) -> Option<ArchiveFormat> {
    if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") || bytes.starts_with(b"PK\x07\x08") {
        return Some(ArchiveFormat::Zip);
    }
    if bytes.starts_with(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        return Some(ArchiveFormat::SevenZ);
    }
    if bytes.starts_with(XZ_MAGIC) {
        return Some(ArchiveFormat::TarXz);
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
        ArchiveFormat::TarXz => Box::new(TarXzReader::new(path.to_path_buf())),
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
    format_from_extension(path)
        .ok_or_else(|| FsError::UnsupportedArchive(path.display().to_string()))
}

/// Map a file path's extension(s) to an archive format.
fn format_from_extension(path: &Path) -> Option<ArchiveFormat> {
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    if name.ends_with(".tar.xz") || name.ends_with(".txz") || name.ends_with(".xz") {
        return Some(ArchiveFormat::TarXz);
    }
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") || name.ends_with(".gz") {
        return Some(ArchiveFormat::TarGz);
    }
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("zip") => Some(ArchiveFormat::Zip),
        Some("7z") => Some(ArchiveFormat::SevenZ),
        Some("tar") => Some(ArchiveFormat::Tar),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn detect_xz_magic() {
        let mut sample = vec![0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00];
        sample.extend_from_slice(b"rest");
        assert_eq!(detect_format(&sample), Some(ArchiveFormat::TarXz));
    }

    #[test]
    fn detect_gz_still_works() {
        assert_eq!(
            detect_format(&[0x1F, 0x8B, 0x08]),
            Some(ArchiveFormat::TarGz)
        );
    }

    #[test]
    fn extension_tar_xz() {
        assert_eq!(
            format_from_extension(Path::new("pack.tar.xz")),
            Some(ArchiveFormat::TarXz)
        );
        assert_eq!(
            format_from_extension(Path::new("pack.txz")),
            Some(ArchiveFormat::TarXz)
        );
        assert_eq!(
            format_from_extension(Path::new("pack.xz")),
            Some(ArchiveFormat::TarXz)
        );
    }

    #[test]
    fn open_tar_xz_roundtrip() {
        let td = tempfile::tempdir().unwrap();
        let path = td.path().join("sample.tar.xz");

        // Build a .tar.xz in memory: tar → xz.
        let mut tar_buf = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_buf);
            let mut header = tar::Header::new_gnu();
            header.set_path("hello.txt").unwrap();
            header.set_size(5);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, b"world".as_slice()).unwrap();
            builder.finish().unwrap();
        }
        {
            let file = std::fs::File::create(&path).unwrap();
            let mut enc = xz2::write::XzEncoder::new(file, 6);
            enc.write_all(&tar_buf).unwrap();
            enc.finish().unwrap();
        }

        assert_eq!(
            detect_format(&std::fs::read(&path).unwrap()[..6]),
            Some(ArchiveFormat::TarXz)
        );

        let reader = open_archive(&path).unwrap();
        assert_eq!(reader.format(), ArchiveFormat::TarXz);

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let entries = reader.list().await.unwrap();
            assert!(entries.iter().any(|e| e.path == "hello.txt"));
            let bytes = reader.read_entry("hello.txt").await.unwrap();
            assert_eq!(bytes, b"world");
        });
    }
}
