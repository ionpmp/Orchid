//! Cached list thumbnails for the audio library.

#![allow(missing_docs)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use orchid_viewers::{discover_cover_sidecars, load_cover_art_sized, load_cover_file, FrameBuf};

const THUMB_EDGE: u32 = 48;
/// Max distinct thumbs kept (paths + folders share entries).
const CACHE_CAP: usize = 256;
/// How many uncached tracks to decode per payload build.
const LOAD_BUDGET: usize = 48;

#[derive(Clone)]
pub struct Thumb {
    pub rgba: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}

impl From<FrameBuf> for Thumb {
    fn from(f: FrameBuf) -> Self {
        Self {
            rgba: f.rgba,
            width: f.width,
            height: f.height,
        }
    }
}

/// Folder-first cover cache for browse lists.
#[derive(Default)]
pub struct CoverThumbCache {
    by_folder: HashMap<String, Option<Thumb>>,
    by_path: HashMap<String, Option<Thumb>>,
    /// Insertion order keys for crude eviction (`folder:` / `path:` prefixes).
    order: Vec<String>,
}

impl CoverThumbCache {
    /// Resolve a thumb for `path`, loading at most remaining `budget` new decodes.
    pub fn get(&mut self, path: &str, budget: &mut usize) -> Option<Thumb> {
        if let Some(hit) = self.by_path.get(path) {
            return hit.clone();
        }
        let folder = parent_key(path);
        if let Some(hit) = self.by_folder.get(&folder) {
            let cloned = hit.clone();
            self.remember_path(path, cloned.clone());
            return cloned;
        }
        if *budget == 0 {
            return None;
        }
        *budget = budget.saturating_sub(1);

        // Prefer shared folder sidecars (one decode per album directory).
        if let Some(thumb) = load_folder_sidecar_thumb(&folder) {
            self.remember_folder(&folder, Some(thumb.clone()));
            self.remember_path(path, Some(thumb.clone()));
            return Some(thumb);
        }
        self.remember_folder(&folder, None);

        let thumb = load_cover_art_sized(Path::new(path), THUMB_EDGE).map(Thumb::from);
        self.remember_path(path, thumb.clone());
        thumb
    }
}

fn parent_key(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn load_folder_sidecar_thumb(folder: &str) -> Option<Thumb> {
    if folder.is_empty() {
        return None;
    }
    // Synthetic media path so discover_cover_sidecars checks COVER_NAMES in the folder.
    let dummy = PathBuf::from(folder).join("__.mp3");
    for candidate in discover_cover_sidecars(&dummy) {
        // Only shared album covers, not per-stem `track.jpg`.
        let name = candidate.file_name()?.to_string_lossy();
        let lower = name.to_ascii_lowercase();
        if !(lower.starts_with("cover.")
            || lower.starts_with("folder.")
            || lower.starts_with("front."))
        {
            continue;
        }
        if let Some(buf) = load_cover_file(&candidate, THUMB_EDGE) {
            return Some(Thumb::from(buf));
        }
    }
    None
}

impl CoverThumbCache {
    fn remember_folder(&mut self, folder: &str, thumb: Option<Thumb>) {
        let key = format!("folder:{folder}");
        if self.by_folder.insert(folder.to_string(), thumb).is_none() {
            self.order.push(key);
            self.evict();
        }
    }

    fn remember_path(&mut self, path: &str, thumb: Option<Thumb>) {
        let key = format!("path:{path}");
        if self.by_path.insert(path.to_string(), thumb).is_none() {
            self.order.push(key);
            self.evict();
        }
    }

    fn evict(&mut self) {
        while self.by_folder.len() + self.by_path.len() > CACHE_CAP {
            let Some(key) = self.order.first().cloned() else {
                break;
            };
            self.order.remove(0);
            if let Some(rest) = key.strip_prefix("folder:") {
                self.by_folder.remove(rest);
            } else if let Some(rest) = key.strip_prefix("path:") {
                self.by_path.remove(rest);
            }
        }
    }
}

/// Budget for a single payload build.
#[must_use]
pub fn load_budget() -> usize {
    LOAD_BUDGET
}
