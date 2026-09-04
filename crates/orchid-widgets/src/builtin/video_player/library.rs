//! Library scan helpers for the video player.

#![allow(missing_docs)]

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use orchid_viewers::is_video_file_extension;

/// Whether `ext` (lowercase, no dot) is a video file.
#[must_use]
pub fn is_video_extension(ext: &str) -> bool {
    is_video_file_extension(ext)
}

/// Indexed video in the library.
#[derive(Debug, Clone)]
pub struct LibraryVideo {
    pub path: PathBuf,
    pub title: String,
    pub folder: String,
}

impl LibraryVideo {
    fn from_path(path: PathBuf) -> Self {
        let title = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let folder = path
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            path,
            title,
            folder,
        }
    }
}

/// Browse row for the UI list.
#[derive(Debug, Clone)]
pub struct VideoRow {
    pub path: String,
    pub title: String,
    pub subtitle: String,
    pub duration_label: String,
}

/// In-memory library index.
#[derive(Debug, Default, Clone)]
pub struct LibraryIndex {
    pub videos: Vec<LibraryVideo>,
    by_path: HashMap<String, usize>,
}

impl LibraryIndex {
    /// Scan `roots` recursively for video files (depth-capped).
    #[must_use]
    pub fn scan(roots: &[String]) -> Self {
        let mut videos = Vec::new();
        let mut seen = BTreeSet::new();
        for root in roots {
            let path = PathBuf::from(root);
            if !path.is_dir() {
                continue;
            }
            walk_dir(&path, 0, &mut videos, &mut seen);
        }
        videos.sort_by(|a, b| {
            a.title
                .to_lowercase()
                .cmp(&b.title.to_lowercase())
                .then_with(|| a.path.cmp(&b.path))
        });
        let by_path = videos
            .iter()
            .enumerate()
            .map(|(i, v)| (v.path.to_string_lossy().into_owned(), i))
            .collect();
        Self { videos, by_path }
    }

    #[must_use]
    pub fn find_by_path(&self, path: &str) -> Option<&LibraryVideo> {
        if let Some(&i) = self.by_path.get(path) {
            return self.videos.get(i);
        }
        self.videos
            .iter()
            .find(|v| v.path.to_string_lossy() == path)
    }

    /// Filtered library rows for the browse list.
    #[must_use]
    pub fn browse_rows(&self, search: &str) -> Vec<VideoRow> {
        let q = search.trim().to_lowercase();
        self.videos
            .iter()
            .filter(|v| {
                if q.is_empty() {
                    return true;
                }
                v.title.to_lowercase().contains(&q)
                    || v.path.to_string_lossy().to_lowercase().contains(&q)
                    || v.folder.to_lowercase().contains(&q)
            })
            .map(video_row)
            .collect()
    }
}

#[must_use]
pub fn video_row(v: &LibraryVideo) -> VideoRow {
    VideoRow {
        path: v.path.to_string_lossy().into_owned(),
        title: v.title.clone(),
        subtitle: v.folder.clone(),
        duration_label: String::new(),
    }
}

const MAX_DEPTH: u32 = 8;

fn walk_dir(dir: &Path, depth: u32, out: &mut Vec<LibraryVideo>, seen: &mut BTreeSet<PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_dir(&path, depth + 1, out, seen);
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if !is_video_extension(&ext.to_ascii_lowercase()) {
            continue;
        }
        let Ok(canon) = path.canonicalize() else {
            if seen.insert(path.clone()) {
                out.push(LibraryVideo::from_path(path));
            }
            continue;
        };
        if seen.insert(canon.clone()) {
            out.push(LibraryVideo::from_path(path));
        }
    }
}
