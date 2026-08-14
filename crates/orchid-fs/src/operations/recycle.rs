//! Browse, restore, purge, and empty the OS Recycle Bin / Trash.
//!
//! Listing and restore are available on Windows and Freedesktop Linux via
//! `trash::os_limited`. Other platforms can still send files to trash; the
//! virtual folder stays empty.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};

use crate::entry::{ExtendedAttributes, FsEntry, FsEntryKind, FsMetadata};
use crate::error::{FsError, Result};
use crate::path::FsPath;

/// Virtual-folder path that lists Recycle Bin items.
pub const RECYCLE_PATH: &str = "virtual:recycle";

/// One item currently in the Recycle Bin.
#[derive(Debug, Clone)]
pub struct RecycleItem {
    /// Platform trash id (lossy UTF-8).
    pub id: String,
    /// Original file name.
    pub name: String,
    /// Full original path before deletion.
    pub original_path: PathBuf,
    /// When the item was deleted, if the OS reports it.
    pub deleted_at: Option<DateTime<Utc>>,
}

impl RecycleItem {
    /// Canonical `virtual:recycle/…` path used in the file-manager listing.
    ///
    /// # Errors
    ///
    /// Invalid scheme construction (should not happen for well-formed ids).
    pub fn virtual_path(&self) -> Result<FsPath> {
        recycle_item_path(&self.id, &self.original_path)
    }
}

/// `true` when `raw` is the Recycle Bin virtual folder.
#[must_use]
pub fn is_recycle_listing(raw: &str) -> bool {
    raw == RECYCLE_PATH
}

/// `true` when `raw` is a Recycle Bin item (`virtual:recycle/<id>`).
#[must_use]
pub fn is_recycle_item(raw: &str) -> bool {
    raw.starts_with("virtual:recycle/")
}

/// `true` when `raw` is the Recycle Bin folder or an item inside it.
#[must_use]
pub fn is_recycle_virtual(raw: &str) -> bool {
    is_recycle_listing(raw) || is_recycle_item(raw)
}

/// Decode the original OS path stored on a Recycle Bin item path.
#[must_use]
pub fn recycle_original_path(raw: &str) -> Option<String> {
    let rest = raw.strip_prefix("virtual:recycle/")?;
    let encoded = rest.split_once('#')?.1;
    decode_token(encoded)
}

/// Decode the platform trash id from a Recycle Bin item path.
#[must_use]
pub fn recycle_item_id(raw: &str) -> Option<String> {
    let rest = raw.strip_prefix("virtual:recycle/")?;
    let id_part = rest.split_once('#').map_or(rest, |(id, _)| id);
    decode_token(id_part)
}

/// Build `virtual:recycle/<id>#<original>` for a trash item.
///
/// # Errors
///
/// [`FsError::InvalidPath`] if the encoded body cannot be parsed.
pub fn recycle_item_path(id: &str, original: &Path) -> Result<FsPath> {
    let id_enc = encode_token(id);
    let orig_enc = encode_token(&original.to_string_lossy());
    FsPath::new(format!("virtual:recycle/{id_enc}#{orig_enc}"))
}

/// Whether this OS can list / restore Recycle Bin items.
#[must_use]
pub fn recycle_listing_supported() -> bool {
    cfg!(any(
        target_os = "windows",
        all(
            unix,
            not(target_os = "macos"),
            not(target_os = "ios"),
            not(target_os = "android")
        )
    ))
}

/// List items currently in the Recycle Bin.
///
/// # Errors
///
/// OS / COM failures while enumerating trash.
pub async fn list_recycle() -> Result<Vec<RecycleItem>> {
    if !recycle_listing_supported() {
        return Ok(Vec::new());
    }
    tokio::task::spawn_blocking(list_recycle_blocking)
        .await
        .map_err(|e| FsError::Io(std::io::Error::other(e)))?
}

/// Restore Recycle Bin items identified by their `virtual:recycle/…` paths.
///
/// # Errors
///
/// Missing items, restore collisions, or OS failures.
pub async fn restore_recycle(paths: &[String]) -> Result<()> {
    let ids = ids_from_paths(paths);
    if ids.is_empty() {
        return Ok(());
    }
    apply_recycle_ids(ids, RecycleOp::Restore).await
}

/// Permanently delete Recycle Bin items identified by `virtual:recycle/…` paths.
///
/// # Errors
///
/// OS failures while purging.
pub async fn purge_recycle(paths: &[String]) -> Result<()> {
    let ids = ids_from_paths(paths);
    if ids.is_empty() {
        return Ok(());
    }
    apply_recycle_ids(ids, RecycleOp::Purge).await
}

/// Permanently delete every item in the Recycle Bin.
///
/// # Errors
///
/// OS failures while emptying.
pub async fn empty_recycle() -> Result<()> {
    if !recycle_listing_supported() {
        return Err(unsupported());
    }
    tokio::task::spawn_blocking(|| {
        let items = list_trash_items()?;
        purge_trash_items(items)
    })
    .await
    .map_err(|e| FsError::Io(std::io::Error::other(e)))?
}

/// Convert listed items into [`FsEntry`] rows for the file manager.
#[must_use]
pub fn recycle_entries(items: &[RecycleItem]) -> Vec<FsEntry> {
    items
        .iter()
        .filter_map(|item| {
            let path = item.virtual_path().ok()?;
            Some(FsEntry {
                name: item.name.clone(),
                path,
                metadata: FsMetadata {
                    kind: FsEntryKind::File,
                    size: 0,
                    created: None,
                    modified: item.deleted_at,
                    accessed: None,
                    readonly: true,
                    hidden: false,
                    system: false,
                    mime: None,
                    extended: ExtendedAttributes::default(),
                },
            })
        })
        .collect()
}

#[derive(Clone, Copy)]
enum RecycleOp {
    Restore,
    Purge,
}

fn ids_from_paths(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .filter_map(|p| recycle_item_id(p))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
}

async fn apply_recycle_ids(ids: Vec<String>, op: RecycleOp) -> Result<()> {
    if !recycle_listing_supported() {
        return Err(unsupported());
    }
    tokio::task::spawn_blocking(move || {
        let items = list_trash_items()?;
        let wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
        let selected: Vec<_> = items
            .into_iter()
            .filter(|item| wanted.contains(item.id.as_str()))
            .collect();
        if selected.is_empty() {
            return Err(FsError::NotFound("recycle-bin items".into()));
        }
        match op {
            RecycleOp::Restore => restore_trash_items(selected),
            RecycleOp::Purge => purge_trash_items(selected),
        }
    })
    .await
    .map_err(|e| FsError::Io(std::io::Error::other(e)))?
}

fn encode_token(value: &str) -> String {
    URL_SAFE_NO_PAD.encode(value.as_bytes())
}

fn decode_token(value: &str) -> Option<String> {
    let bytes = URL_SAFE_NO_PAD.decode(value.as_bytes()).ok()?;
    String::from_utf8(bytes).ok()
}

fn unsupported() -> FsError {
    FsError::Io(std::io::Error::other("fm-error-recycle-unsupported"))
}

fn map_trash_error(err: trash::Error) -> FsError {
    match err {
        trash::Error::RestoreCollision { .. } => {
            FsError::Io(std::io::Error::other("fm-error-recycle-collision"))
        }
        trash::Error::RestoreTwins { .. } => {
            FsError::Io(std::io::Error::other("fm-error-recycle-twins"))
        }
        other => FsError::Io(std::io::Error::other(other)),
    }
}

fn deleted_at(unix_secs: i64) -> Option<DateTime<Utc>> {
    if unix_secs <= 0 {
        return None;
    }
    DateTime::<Utc>::from_timestamp(unix_secs, 0)
}

#[cfg(any(
    target_os = "windows",
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
))]
fn list_recycle_blocking() -> Result<Vec<RecycleItem>> {
    let items = trash::os_limited::list().map_err(map_trash_error)?;
    Ok(items
        .into_iter()
        .map(|item| RecycleItem {
            id: item.id.to_string_lossy().into_owned(),
            name: item.name.to_string_lossy().into_owned(),
            original_path: item.original_path(),
            deleted_at: deleted_at(item.time_deleted),
        })
        .collect())
}

#[cfg(not(any(
    target_os = "windows",
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
)))]
fn list_recycle_blocking() -> Result<Vec<RecycleItem>> {
    Ok(Vec::new())
}

struct ListedTrash {
    id: String,
    item: trash::TrashItem,
}

#[cfg(any(
    target_os = "windows",
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
))]
fn list_trash_items() -> Result<Vec<ListedTrash>> {
    let items = trash::os_limited::list().map_err(map_trash_error)?;
    Ok(items
        .into_iter()
        .map(|item| ListedTrash {
            id: item.id.to_string_lossy().into_owned(),
            item,
        })
        .collect())
}

#[cfg(not(any(
    target_os = "windows",
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
)))]
fn list_trash_items() -> Result<Vec<ListedTrash>> {
    Err(unsupported())
}

#[cfg(any(
    target_os = "windows",
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
))]
fn restore_trash_items(items: Vec<ListedTrash>) -> Result<()> {
    let raw: Vec<trash::TrashItem> = items.into_iter().map(|i| i.item).collect();
    trash::os_limited::restore_all(raw).map_err(map_trash_error)
}

#[cfg(not(any(
    target_os = "windows",
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
)))]
fn restore_trash_items(_items: Vec<ListedTrash>) -> Result<()> {
    Err(unsupported())
}

#[cfg(any(
    target_os = "windows",
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
))]
fn purge_trash_items(items: Vec<ListedTrash>) -> Result<()> {
    let raw: Vec<trash::TrashItem> = items.into_iter().map(|i| i.item).collect();
    trash::os_limited::purge_all(raw).map_err(map_trash_error)
}

#[cfg(not(any(
    target_os = "windows",
    all(
        unix,
        not(target_os = "macos"),
        not(target_os = "ios"),
        not(target_os = "android")
    )
)))]
fn purge_trash_items(_items: Vec<ListedTrash>) -> Result<()> {
    Err(unsupported())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_and_item_predicates() {
        assert!(is_recycle_listing("virtual:recycle"));
        assert!(!is_recycle_item("virtual:recycle"));
        assert!(is_recycle_virtual("virtual:recycle"));
        let path = recycle_item_path("id-1", Path::new(r"c:\Users\Ada\notes.txt")).unwrap();
        assert!(is_recycle_item(path.as_str()));
        assert_eq!(recycle_item_id(path.as_str()).as_deref(), Some("id-1"));
        assert_eq!(
            recycle_original_path(path.as_str()).as_deref(),
            Some(r"c:\Users\Ada\notes.txt")
        );
    }

    #[test]
    fn virtual_path_roundtrip_survives_normalisation() {
        let original = Path::new(r"C:\Temp\file with spaces.txt");
        let path = recycle_item_path("::{GUID}\\item", original).unwrap();
        assert!(path.as_str().starts_with("virtual:recycle/"));
        assert_eq!(
            recycle_item_id(path.as_str()).as_deref(),
            Some("::{GUID}\\item")
        );
        assert_eq!(
            recycle_original_path(path.as_str()).as_deref(),
            Some(r"C:\Temp\file with spaces.txt")
        );
    }
}
