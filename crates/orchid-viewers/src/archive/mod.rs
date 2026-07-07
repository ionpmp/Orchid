//! Archive viewer — browses ZIP / 7z / TAR / TAR.GZ via `orchid-fs`.

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::{Mutex, RwLock};

use crate::error::{Result, ViewerError};
use crate::snapshot::{
    ArchiveEntryView, ArchivePreview, ArchiveSnapshot, ViewerSnapshot,
};
use crate::viewer_trait::Viewer;

/// Viewer wrapping [`orchid_fs::ArchiveReader`].
pub struct ArchiveViewer {
    path: RwLock<Option<orchid_fs::FsPath>>,
    reader: Mutex<Option<Box<dyn orchid_fs::ArchiveReader>>>,
    format: RwLock<Option<orchid_fs::ArchiveFormat>>,
    entries: RwLock<Vec<orchid_fs::ArchiveEntry>>,
    current_inner_path: RwLock<String>,
    selected_entry: RwLock<Option<String>>,
    preview_cache: Mutex<Option<(String, Vec<u8>)>>,
    last_status: RwLock<Option<String>>,
}

impl std::fmt::Debug for ArchiveViewer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArchiveViewer")
            .field("format", &*self.format.read())
            .field("entries", &self.entries.read().len())
            .finish_non_exhaustive()
    }
}

impl Default for ArchiveViewer {
    fn default() -> Self {
        Self::new()
    }
}

impl ArchiveViewer {
    /// Build an empty archive viewer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            path: RwLock::new(None),
            reader: Mutex::new(None),
            format: RwLock::new(None),
            entries: RwLock::new(Vec::new()),
            current_inner_path: RwLock::new(String::new()),
            selected_entry: RwLock::new(None),
            preview_cache: Mutex::new(None),
            last_status: RwLock::new(None),
        }
    }

    fn clear_status(&self) {
        *self.last_status.write() = None;
    }

    fn local_path(&self) -> Result<std::path::PathBuf> {
        self.path
            .read()
            .as_ref()
            .ok_or_else(|| ViewerError::ArchiveEntryNotFound("no archive open".into()))?
            .to_local()
            .map_err(ViewerError::Fs)
    }

    /// Descend into a subfolder. `folder_path` is an inner-archive path
    /// ending in `/`.
    ///
    /// # Errors
    ///
    /// Never fails in the MVP — always accepts the new prefix.
    pub async fn navigate_into(&self, folder_path: &str) -> Result<()> {
        let mut cur = self.current_inner_path.write();
        let trimmed = folder_path.trim_end_matches('/');
        if trimmed.is_empty() {
            cur.clear();
        } else {
            *cur = format!("{trimmed}/");
        }
        *self.selected_entry.write() = None;
        self.clear_status();
        Ok(())
    }

    /// Go up one folder in the current inner path.
    ///
    /// # Errors
    ///
    /// Never fails in the MVP.
    pub async fn navigate_up(&self) -> Result<()> {
        let mut cur = self.current_inner_path.write();
        if cur.is_empty() {
            return Ok(());
        }
        let trimmed = cur.trim_end_matches('/');
        match trimmed.rsplit_once('/') {
            Some((parent, _)) => *cur = format!("{parent}/"),
            None => cur.clear(),
        }
        *self.selected_entry.write() = None;
        self.clear_status();
        Ok(())
    }

    /// Select an entry by its full archive path. Also pre-reads the entry
    /// bytes (up to 256 KiB) so the next snapshot can surface a text
    /// preview without re-entering the async runtime.
    ///
    /// # Errors
    ///
    /// Returns [`ViewerError::ArchiveEntryNotFound`] if the path does not
    /// exist in the archive.
    pub async fn select(&self, entry_path: &str) -> Result<()> {
        let entry = self
            .entries
            .read()
            .iter()
            .find(|e| e.path == entry_path)
            .cloned()
            .ok_or_else(|| ViewerError::ArchiveEntryNotFound(entry_path.into()))?;
        *self.selected_entry.write() = Some(entry_path.to_string());
        *self.preview_cache.lock() = None;
        self.clear_status();
        if !entry.is_dir && entry.size <= 256 * 1024 {
            // Best-effort preview fetch. We re-open the archive from the
            // path because our reader trait currently lacks cheap cloning.
            let archive_path = self.path.read().as_ref().and_then(|p| p.to_local().ok());
            if let Some(archive_path) = archive_path {
                if let Ok(reader) = orchid_fs::open_archive(&archive_path) {
                    if let Ok(bytes) = reader.read_entry(entry_path).await {
                        *self.preview_cache.lock() = Some((entry_path.to_string(), bytes));
                    }
                }
            }
        }
        Ok(())
    }

    /// Extract a single entry to `output`.
    ///
    /// # Errors
    ///
    /// Propagates [`orchid_fs::FsError`].
    pub async fn extract_entry(
        &self,
        entry_path: &str,
        output: &std::path::Path,
    ) -> Result<()> {
        let local = self
            .path
            .read()
            .as_ref()
            .ok_or_else(|| ViewerError::ArchiveEntryNotFound(entry_path.into()))?
            .to_local()?;
        let reader = orchid_fs::open_archive(&local)?;
        reader.extract_entry(entry_path, output).await?;
        Ok(())
    }

    /// Extract every entry to `output`. Returns the file count.
    ///
    /// # Errors
    ///
    /// Propagates [`orchid_fs::FsError`].
    pub async fn extract_all(&self, output: &std::path::Path) -> Result<u64> {
        let Some(local) = self.path.read().as_ref().and_then(|p| p.to_local().ok()) else {
            return Ok(0);
        };
        let reader = orchid_fs::open_archive(&local)?;
        Ok(reader.extract_all(output).await?)
    }

    /// Extract the selected file next to the archive on disk.
    pub async fn extract_selected_to_sibling(&self) -> Result<std::path::PathBuf> {
        let selected = self
            .selected_entry
            .read()
            .clone()
            .ok_or_else(|| ViewerError::ArchiveEntryNotFound("nothing selected".into()))?;
        let entry = self
            .entries
            .read()
            .iter()
            .find(|e| e.path == selected)
            .cloned()
            .ok_or_else(|| ViewerError::ArchiveEntryNotFound(selected.clone()))?;
        if entry.is_dir {
            return Err(ViewerError::ArchiveEntryNotFound(
                "cannot extract a folder".into(),
            ));
        }
        let local = self.local_path()?;
        let file_name = selected.rsplit('/').next().unwrap_or(selected.as_str());
        let output = local
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(file_name);
        self.extract_entry(&selected, &output).await?;
        *self.last_status.write() = Some(format!("Extracted to {}", output.display()));
        Ok(output)
    }

    /// Extract all entries into `{archive_stem}_extracted/` beside the archive.
    pub async fn extract_all_to_sibling(&self) -> Result<(std::path::PathBuf, u64)> {
        let local = self.local_path()?;
        let stem = local
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("archive");
        let output = local
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(format!("{stem}_extracted"));
        let count = self.extract_all(&output).await?;
        *self.last_status.write() = Some(format!(
            "Extracted {count} entries to {}",
            output.display()
        ));
        Ok((output, count))
    }

    fn compute_preview(&self) -> Option<ArchivePreview> {
        let selected = self.selected_entry.read().clone()?;
        let entry = self
            .entries
            .read()
            .iter()
            .find(|e| e.path == selected)?
            .clone();
        if entry.is_dir {
            return None;
        }
        const SMALL_TEXT: u64 = 256 * 1024;
        if entry.size > SMALL_TEXT {
            return Some(ArchivePreview::Binary { size: entry.size });
        }
        let cached = self
            .preview_cache
            .lock()
            .as_ref()
            .filter(|(p, _)| *p == selected)
            .map(|(_, b)| b.clone());
        let Some(bytes) = cached else {
            return Some(ArchivePreview::Binary { size: entry.size });
        };
        if looks_like_text(&bytes) {
            Some(ArchivePreview::Text(
                String::from_utf8_lossy(&bytes).into_owned(),
            ))
        } else {
            Some(ArchivePreview::Binary { size: entry.size })
        }
    }
}

fn looks_like_text(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    // Treat <= 2% non-ASCII / null bytes as "text".
    let threshold = bytes.len() / 50;
    let mut suspicious = 0usize;
    for &b in bytes {
        if b == 0 || (b < 0x09 && b != b'\t') || b == 0x7F {
            suspicious += 1;
            if suspicious > threshold {
                return false;
            }
        }
    }
    true
}

#[async_trait]
impl Viewer for ArchiveViewer {
    fn type_id(&self) -> &'static str {
        "archive"
    }

    async fn open(
        &mut self,
        path: orchid_fs::FsPath,
        _registry: Arc<orchid_fs::FsProviderRegistry>,
    ) -> Result<()> {
        let local = path.to_local().map_err(ViewerError::Fs)?;
        let reader = orchid_fs::open_archive(&local)?;
        let entries = reader.list().await?;
        *self.format.write() = Some(reader.format());
        *self.reader.lock() = Some(reader);
        *self.entries.write() = entries;
        *self.current_inner_path.write() = String::new();
        *self.selected_entry.write() = None;
        *self.preview_cache.lock() = None;
        *self.last_status.write() = None;
        *self.path.write() = Some(path);
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        *self.reader.lock() = None;
        *self.format.write() = None;
        *self.entries.write() = Vec::new();
        *self.current_inner_path.write() = String::new();
        *self.selected_entry.write() = None;
        *self.preview_cache.lock() = None;
        *self.last_status.write() = None;
        *self.path.write() = None;
        Ok(())
    }

    fn snapshot(&self) -> ViewerSnapshot {
        let path_guard = self.path.read();
        let path_display = path_guard
            .as_ref()
            .map(|p| p.as_str().to_string())
            .unwrap_or_default();
        let format = self
            .format
            .read()
            .map(format_label)
            .unwrap_or("archive")
            .to_string();
        let entries_guard = self.entries.read();
        let cur = self.current_inner_path.read().clone();
        let rows = direct_children(&entries_guard, &cur);
        let selected_path = self.selected_entry.read().clone();
        let mut rendered = Vec::with_capacity(rows.len());
        for entry in &rows {
            rendered.push(entry_to_view(entry));
        }
        // Preview is best-effort — only valid when a reader is held.
        let preview = self.compute_preview();
        let total = entries_guard.len() as u32;
        let info = self
            .last_status
            .read()
            .clone()
            .unwrap_or_else(|| format!("{format}, {total} entries"));
        ViewerSnapshot::Archive(ArchiveSnapshot {
            path_display,
            format,
            total_entries: total,
            current_inner_path: cur,
            selected_path: selected_path.unwrap_or_default(),
            entries: rendered,
            preview,
            info_text: info,
        })
    }

    fn current_path(&self) -> Option<&orchid_fs::FsPath> {
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn direct_children<'a>(
    entries: &'a [orchid_fs::ArchiveEntry],
    prefix: &str,
) -> Vec<&'a orchid_fs::ArchiveEntry> {
    entries
        .iter()
        .filter(|e| {
            if !e.path.starts_with(prefix) {
                return false;
            }
            let rest = &e.path[prefix.len()..];
            let trimmed = rest.trim_end_matches('/');
            !trimmed.is_empty() && !trimmed.contains('/')
        })
        .collect()
}

fn entry_to_view(entry: &orchid_fs::ArchiveEntry) -> ArchiveEntryView {
    let name = entry
        .path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(&entry.path)
        .to_string();
    let modified_text = entry
        .modified
        .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default();
    ArchiveEntryView {
        path_in_archive: entry.path.clone(),
        name,
        is_dir: entry.is_dir,
        size: entry.size,
        modified_text,
        icon: if entry.is_dir { "folder" } else { "file" },
    }
}

fn format_label(fmt: orchid_fs::ArchiveFormat) -> &'static str {
    match fmt {
        orchid_fs::ArchiveFormat::Zip => "ZIP",
        orchid_fs::ArchiveFormat::SevenZ => "7z",
        orchid_fs::ArchiveFormat::Tar => "TAR",
        orchid_fs::ArchiveFormat::TarGz => "TAR.GZ",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entries() -> Vec<orchid_fs::ArchiveEntry> {
        vec![
            orchid_fs::ArchiveEntry {
                path: "a.txt".into(),
                size: 1,
                compressed_size: None,
                modified: None,
                is_dir: false,
                crc32: None,
            },
            orchid_fs::ArchiveEntry {
                path: "docs/".into(),
                size: 0,
                compressed_size: None,
                modified: None,
                is_dir: true,
                crc32: None,
            },
            orchid_fs::ArchiveEntry {
                path: "docs/b.txt".into(),
                size: 2,
                compressed_size: None,
                modified: None,
                is_dir: false,
                crc32: None,
            },
            orchid_fs::ArchiveEntry {
                path: "docs/sub/".into(),
                size: 0,
                compressed_size: None,
                modified: None,
                is_dir: true,
                crc32: None,
            },
            orchid_fs::ArchiveEntry {
                path: "docs/sub/c.txt".into(),
                size: 3,
                compressed_size: None,
                modified: None,
                is_dir: false,
                crc32: None,
            },
        ]
    }

    #[test]
    fn direct_children_root() {
        let entries = make_entries();
        let rows = direct_children(&entries, "");
        let names: Vec<&str> = rows.iter().map(|e| e.path.as_str()).collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"docs/"));
        assert!(!names.contains(&"docs/b.txt"));
    }

    #[test]
    fn direct_children_nested() {
        let entries = make_entries();
        let rows = direct_children(&entries, "docs/");
        let names: Vec<&str> = rows.iter().map(|e| e.path.as_str()).collect();
        assert!(names.contains(&"docs/b.txt"));
        assert!(names.contains(&"docs/sub/"));
        assert!(!names.contains(&"docs/sub/c.txt"));
    }
}
