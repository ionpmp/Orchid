//! 7z archive reader (read-only in MVP).

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sevenz_rust::{Password, SevenZReader};

use crate::archive::reader::ArchiveReader;
use crate::archive::types::{sanitise_entry_path, ArchiveEntry, ArchiveFormat};
use crate::error::{FsError, Result};

/// Reader around a `.7z` archive on disk.
pub struct SevenZArchiveReader {
    path: PathBuf,
}

impl SevenZArchiveReader {
    /// Open a reader over `path`.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

#[async_trait]
impl ArchiveReader for SevenZArchiveReader {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::SevenZ
    }

    async fn list(&self) -> Result<Vec<ArchiveEntry>> {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || list_sync(&path))
            .await
            .map_err(|e| FsError::CorruptArchive(format!("join: {e}")))?
    }

    async fn read_entry(&self, inner: &str) -> Result<Vec<u8>> {
        let path = self.path.clone();
        let target = inner.to_string();
        tokio::task::spawn_blocking(move || read_entry_sync(&path, &target))
            .await
            .map_err(|e| FsError::CorruptArchive(format!("join: {e}")))?
    }

    async fn extract_entry(&self, inner: &str, output: &Path) -> Result<()> {
        let bytes = self.read_entry(inner).await?;
        if let Some(parent) = output.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(output, &bytes).await?;
        Ok(())
    }

    async fn extract_all(&self, output: &Path) -> Result<u64> {
        let path = self.path.clone();
        let target = output.to_path_buf();
        tokio::task::spawn_blocking(move || extract_all_sync(&path, &target))
            .await
            .map_err(|e| FsError::CorruptArchive(format!("join: {e}")))?
    }
}

fn list_sync(path: &Path) -> Result<Vec<ArchiveEntry>> {
    let mut reader = SevenZReader::open(path, Password::empty())
        .map_err(|e| FsError::CorruptArchive(e.to_string()))?;
    let mut out: Vec<ArchiveEntry> = Vec::new();
    reader
        .for_each_entries(|entry, _r| {
            if let Some(safe) = sanitise_entry_path(entry.name()) {
                out.push(ArchiveEntry {
                    path: safe,
                    size: entry.size(),
                    compressed_size: None,
                    modified: filetime_to_utc(entry.last_modified_date().into()),
                    is_dir: entry.is_directory(),
                    crc32: None,
                });
            }
            Ok(true)
        })
        .map_err(|e| FsError::CorruptArchive(e.to_string()))?;
    Ok(out)
}

fn read_entry_sync(path: &Path, target: &str) -> Result<Vec<u8>> {
    let mut reader = SevenZReader::open(path, Password::empty())
        .map_err(|e| FsError::CorruptArchive(e.to_string()))?;
    let mut buf: Option<Vec<u8>> = None;
    reader
        .for_each_entries(|entry, r| {
            if buf.is_some() {
                return Ok(false);
            }
            if let Some(safe) = sanitise_entry_path(entry.name()) {
                if safe == target && !entry.is_directory() {
                    let mut data = Vec::with_capacity(entry.size() as usize);
                    r.read_to_end(&mut data).map_err(sevenz_rust::Error::io)?;
                    buf = Some(data);
                    return Ok(false);
                }
            }
            Ok(true)
        })
        .map_err(|e| FsError::CorruptArchive(e.to_string()))?;
    buf.ok_or_else(|| FsError::ArchiveEntryNotFound(target.to_string()))
}

fn extract_all_sync(path: &Path, output: &Path) -> Result<u64> {
    std::fs::create_dir_all(output)?;
    let mut reader = SevenZReader::open(path, Password::empty())
        .map_err(|e| FsError::CorruptArchive(e.to_string()))?;
    let mut count = 0_u64;
    reader
        .for_each_entries(|entry, r| {
            let Some(safe) = sanitise_entry_path(entry.name()) else {
                return Ok(true);
            };
            let dest = output.join(&safe);
            if entry.is_directory() {
                std::fs::create_dir_all(&dest).map_err(sevenz_rust::Error::io)?;
                return Ok(true);
            }
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(sevenz_rust::Error::io)?;
            }
            let mut data = Vec::with_capacity(entry.size() as usize);
            r.read_to_end(&mut data).map_err(sevenz_rust::Error::io)?;
            std::fs::write(&dest, &data).map_err(sevenz_rust::Error::io)?;
            count += 1;
            Ok(true)
        })
        .map_err(|e| FsError::CorruptArchive(e.to_string()))?;
    Ok(count)
}

fn filetime_to_utc(raw: u64) -> Option<DateTime<Utc>> {
    if raw == 0 {
        return None;
    }
    const EPOCH_DIFF_SECS: i64 = 11_644_473_600;
    let total_100ns = raw as i128;
    let secs = (total_100ns / 10_000_000) as i64 - EPOCH_DIFF_SECS;
    let nanos = ((total_100ns % 10_000_000) * 100) as u32;
    DateTime::<Utc>::from_timestamp(secs, nanos)
}
