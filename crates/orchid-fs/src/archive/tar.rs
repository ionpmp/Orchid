//! TAR and TAR.GZ archive readers.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::archive::reader::ArchiveReader;
use crate::archive::types::{sanitise_entry_path, ArchiveEntry, ArchiveFormat};
use crate::error::{FsError, Result};

/// Reader around a plain `.tar` archive.
pub struct TarReader {
    path: PathBuf,
}

impl TarReader {
    /// Open a reader over `path`.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

/// Reader around a `.tar.gz` / `.tgz` archive.
pub struct TarGzReader {
    path: PathBuf,
}

impl TarGzReader {
    /// Open a reader over `path`.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

#[async_trait]
impl ArchiveReader for TarReader {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::Tar
    }

    async fn list(&self) -> Result<Vec<ArchiveEntry>> {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            let f = File::open(&path)?;
            list_tar(f)
        })
        .await
        .map_err(|e| FsError::CorruptArchive(format!("join: {e}")))?
    }

    async fn read_entry(&self, inner: &str) -> Result<Vec<u8>> {
        let path = self.path.clone();
        let target = inner.to_string();
        tokio::task::spawn_blocking(move || {
            let f = File::open(&path)?;
            read_tar_entry(f, &target)
        })
        .await
        .map_err(|e| FsError::CorruptArchive(format!("join: {e}")))?
    }

    async fn extract_entry(&self, path: &str, output: &Path) -> Result<()> {
        let bytes = self.read_entry(path).await?;
        if let Some(parent) = output.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(output, &bytes).await?;
        Ok(())
    }

    async fn extract_all(&self, output: &Path) -> Result<u64> {
        let path = self.path.clone();
        let target = output.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let f = File::open(&path)?;
            extract_all_tar(f, &target)
        })
        .await
        .map_err(|e| FsError::CorruptArchive(format!("join: {e}")))?
    }
}

#[async_trait]
impl ArchiveReader for TarGzReader {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::TarGz
    }

    async fn list(&self) -> Result<Vec<ArchiveEntry>> {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            let f = File::open(&path)?;
            let gz = flate2::read::GzDecoder::new(f);
            list_tar(gz)
        })
        .await
        .map_err(|e| FsError::CorruptArchive(format!("join: {e}")))?
    }

    async fn read_entry(&self, inner: &str) -> Result<Vec<u8>> {
        let path = self.path.clone();
        let target = inner.to_string();
        tokio::task::spawn_blocking(move || {
            let f = File::open(&path)?;
            let gz = flate2::read::GzDecoder::new(f);
            read_tar_entry(gz, &target)
        })
        .await
        .map_err(|e| FsError::CorruptArchive(format!("join: {e}")))?
    }

    async fn extract_entry(&self, path: &str, output: &Path) -> Result<()> {
        let bytes = self.read_entry(path).await?;
        if let Some(parent) = output.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(output, &bytes).await?;
        Ok(())
    }

    async fn extract_all(&self, output: &Path) -> Result<u64> {
        let path = self.path.clone();
        let target = output.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let f = File::open(&path)?;
            let gz = flate2::read::GzDecoder::new(f);
            extract_all_tar(gz, &target)
        })
        .await
        .map_err(|e| FsError::CorruptArchive(format!("join: {e}")))?
    }
}

fn list_tar<R: Read>(reader: R) -> Result<Vec<ArchiveEntry>> {
    let mut tar = tar::Archive::new(reader);
    let mut out = Vec::new();
    for entry in tar
        .entries()
        .map_err(|e| FsError::CorruptArchive(e.to_string()))?
    {
        let entry = entry.map_err(|e| FsError::CorruptArchive(e.to_string()))?;
        let raw = entry
            .path()
            .map_err(|e| FsError::CorruptArchive(e.to_string()))?
            .to_string_lossy()
            .into_owned();
        let Some(safe) = sanitise_entry_path(&raw) else {
            continue;
        };
        let header = entry.header();
        let size = header.size().unwrap_or(0);
        let is_dir = header.entry_type().is_dir();
        let modified = header
            .mtime()
            .ok()
            .and_then(|s| DateTime::<Utc>::from_timestamp(s as i64, 0));
        out.push(ArchiveEntry {
            path: safe,
            size,
            compressed_size: None,
            modified,
            is_dir,
            crc32: None,
        });
    }
    Ok(out)
}

fn read_tar_entry<R: Read>(reader: R, target: &str) -> Result<Vec<u8>> {
    let mut tar = tar::Archive::new(reader);
    for entry in tar
        .entries()
        .map_err(|e| FsError::CorruptArchive(e.to_string()))?
    {
        let mut entry = entry.map_err(|e| FsError::CorruptArchive(e.to_string()))?;
        let raw = entry
            .path()
            .map_err(|e| FsError::CorruptArchive(e.to_string()))?
            .to_string_lossy()
            .into_owned();
        let Some(safe) = sanitise_entry_path(&raw) else {
            continue;
        };
        if safe == target {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            return Ok(buf);
        }
    }
    Err(FsError::ArchiveEntryNotFound(target.to_string()))
}

fn extract_all_tar<R: Read>(reader: R, target: &Path) -> Result<u64> {
    let mut tar = tar::Archive::new(reader);
    let mut count = 0_u64;
    for entry in tar
        .entries()
        .map_err(|e| FsError::CorruptArchive(e.to_string()))?
    {
        let mut entry = entry.map_err(|e| FsError::CorruptArchive(e.to_string()))?;
        let raw = entry
            .path()
            .map_err(|e| FsError::CorruptArchive(e.to_string()))?
            .to_string_lossy()
            .into_owned();
        let Some(safe) = sanitise_entry_path(&raw) else {
            continue;
        };
        let dest = target.join(&safe);
        if entry.header().entry_type().is_dir() {
            std::fs::create_dir_all(&dest)?;
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;
        std::fs::write(&dest, &buf)?;
        count += 1;
    }
    Ok(count)
}
