//! ZIP archive reader.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};

use crate::archive::types::{sanitise_entry_path, ArchiveEntry, ArchiveFormat};
use crate::archive::reader::ArchiveReader;
use crate::error::{FsError, Result};

/// Reader around a `.zip` archive on disk.
pub struct ZipReader {
    path: PathBuf,
}

impl ZipReader {
    /// Open a reader over `path` (the file is reopened per call so that
    /// multiple async operations can run in parallel).
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

#[async_trait]
impl ArchiveReader for ZipReader {
    fn format(&self) -> ArchiveFormat {
        ArchiveFormat::Zip
    }

    async fn list(&self) -> Result<Vec<ArchiveEntry>> {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || list_sync(&path))
            .await
            .map_err(|e| FsError::CorruptArchive(format!("join: {e}")))?
    }

    async fn read_entry(&self, path: &str) -> Result<Vec<u8>> {
        let archive = self.path.clone();
        let entry = path.to_string();
        tokio::task::spawn_blocking(move || read_entry_sync(&archive, &entry))
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
        let archive = self.path.clone();
        let target = output.to_path_buf();
        tokio::task::spawn_blocking(move || extract_all_sync(&archive, &target))
            .await
            .map_err(|e| FsError::CorruptArchive(format!("join: {e}")))?
    }
}

fn list_sync(archive_path: &Path) -> Result<Vec<ArchiveEntry>> {
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut out = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let Some(path) = sanitise_entry_path(entry.name()) else {
            continue;
        };
        let modified = zip_time_to_utc(entry.last_modified());
        out.push(ArchiveEntry {
            path,
            size: entry.size(),
            compressed_size: Some(entry.compressed_size()),
            modified,
            is_dir: entry.is_dir(),
            crc32: Some(entry.crc32()),
        });
    }
    Ok(out)
}

fn read_entry_sync(archive_path: &Path, inner: &str) -> Result<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut entry = archive
        .by_name(inner)
        .map_err(|_| FsError::ArchiveEntryNotFound(inner.to_string()))?;
    // Reject zip-slip at read time too, belt-and-braces.
    if sanitise_entry_path(entry.name()).is_none() {
        return Err(FsError::CorruptArchive(format!(
            "path traversal in archive entry: {}",
            entry.name()
        )));
    }
    let mut buf = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut buf)?;
    Ok(buf)
}

fn extract_all_sync(archive_path: &Path, target: &Path) -> Result<u64> {
    use std::io::{copy as io_copy, Write};
    let file = std::fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut count = 0_u64;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(safe) = sanitise_entry_path(entry.name()) else {
            continue;
        };
        let dest = target.join(&safe);
        if entry.is_dir() {
            std::fs::create_dir_all(&dest)?;
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create(&dest)?;
        io_copy(&mut entry, &mut out)?;
        out.flush()?;
        count += 1;
    }
    Ok(count)
}

fn zip_time_to_utc(t: Option<zip::DateTime>) -> Option<chrono::DateTime<Utc>> {
    let t = t?;
    Utc.with_ymd_and_hms(
        t.year() as i32,
        t.month() as u32,
        t.day() as u32,
        t.hour() as u32,
        t.minute() as u32,
        t.second() as u32,
    )
    .single()
}
