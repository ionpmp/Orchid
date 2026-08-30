//! Folder playlist for the media viewer (next/prev, loop).

use std::collections::{HashSet, VecDeque};

use orchid_fs::{FsEntryKind, FsPath, FsProviderRegistry};
use orchid_viewers::is_media_file_extension;

/// Session playlist of media siblings in one folder.
#[derive(Debug, Clone)]
pub struct MediaFolderNav {
    /// Parent folder of the current file.
    pub folder: Option<FsPath>,
    /// Media files in [`Self::folder`], name-sorted.
    pub siblings: Vec<FsPath>,
    /// Index into [`Self::siblings`].
    pub index: usize,
    /// Wrap around at the ends.
    pub loop_playlist: bool,
    /// Recently opened media in this viewer (newest first).
    pub history: VecDeque<FsPath>,
    /// Paths that failed to open; skipped on later steps.
    pub unreadable: HashSet<String>,
}

impl Default for MediaFolderNav {
    fn default() -> Self {
        Self {
            folder: None,
            siblings: Vec::new(),
            index: 0,
            loop_playlist: true,
            history: VecDeque::new(),
            unreadable: HashSet::new(),
        }
    }
}

/// Direction along the playlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaNavStep {
    /// Next sibling.
    Next,
    /// Previous sibling.
    Prev,
    /// First readable sibling.
    First,
    /// Last readable sibling.
    Last,
    /// 1-based index.
    Goto(usize),
}

impl MediaFolderNav {
    const HISTORY_CAP: usize = 24;

    /// Replace the folder listing and select `current` when present.
    pub fn set_folder(&mut self, folder: FsPath, siblings: Vec<FsPath>, current: &FsPath) {
        self.folder = Some(folder);
        self.siblings = siblings;
        self.index = self.siblings.iter().position(|p| p == current).unwrap_or(0);
    }

    /// Point at `current` if it is already in the listing.
    pub fn set_current(&mut self, current: &FsPath) {
        if let Some(i) = self.siblings.iter().position(|p| p == current) {
            self.index = i;
        }
    }

    /// Remember a successfully opened path.
    pub fn push_history(&mut self, path: &FsPath) {
        let key = path.as_str();
        self.history.retain(|p| p.as_str() != key);
        self.history.push_front(path.clone());
        while self.history.len() > Self::HISTORY_CAP {
            self.history.pop_back();
        }
    }

    /// Mark a path so later navigation skips it.
    #[allow(dead_code)]
    pub fn mark_unreadable(&mut self, path: &FsPath) {
        self.unreadable.insert(path.as_str().to_string());
    }

    /// Next readable index for `step`, or `None` when exhausted.
    #[must_use]
    pub fn pick(&self, step: MediaNavStep) -> Option<usize> {
        let n = self.siblings.len();
        if n == 0 {
            return None;
        }
        match step {
            MediaNavStep::First => self.first_readable_from(0, 1),
            MediaNavStep::Last => self.first_readable_from(n - 1, -1),
            MediaNavStep::Goto(one_based) => {
                let idx = one_based.saturating_sub(1);
                if idx < n && self.is_readable(idx) {
                    Some(idx)
                } else {
                    None
                }
            }
            MediaNavStep::Next => self.step_from(self.index, 1),
            MediaNavStep::Prev => self.step_from(self.index, -1),
        }
    }

    fn is_readable(&self, idx: usize) -> bool {
        self.siblings
            .get(idx)
            .is_some_and(|p| !self.unreadable.contains(p.as_str()))
    }

    fn first_readable_from(&self, start: usize, dir: i32) -> Option<usize> {
        let n = self.siblings.len();
        if n == 0 {
            return None;
        }
        let mut i = start;
        for _ in 0..n {
            if self.is_readable(i) {
                return Some(i);
            }
            i = wrap_index(i, dir, n, true)?;
        }
        None
    }

    fn step_from(&self, start: usize, dir: i32) -> Option<usize> {
        let n = self.siblings.len();
        let mut i = start;
        for _ in 0..n {
            i = wrap_index(i, dir, n, self.loop_playlist)?;
            if self.is_readable(i) {
                return Some(i);
            }
            if !self.loop_playlist && ((dir > 0 && i + 1 >= n) || (dir < 0 && i == 0)) {
                // keep scanning within bounds only
            }
        }
        None
    }
}

fn wrap_index(i: usize, dir: i32, n: usize, loop_playlist: bool) -> Option<usize> {
    if n == 0 {
        return None;
    }
    if dir > 0 {
        if i + 1 < n {
            Some(i + 1)
        } else if loop_playlist {
            Some(0)
        } else {
            None
        }
    } else if i > 0 {
        Some(i - 1)
    } else if loop_playlist {
        Some(n - 1)
    } else {
        None
    }
}

/// Media files in `folder`, sorted by file name (case-insensitive).
pub async fn list_media_siblings(
    registry: &FsProviderRegistry,
    folder: &FsPath,
) -> Option<Vec<FsPath>> {
    let provider = registry.for_path(folder)?;
    let entries = provider.list(folder).await.ok()?;
    let mut files: Vec<FsPath> = entries
        .into_iter()
        .filter(|e| {
            matches!(e.metadata.kind, FsEntryKind::File | FsEntryKind::Symlink)
                && e.path.extension().is_some_and(is_media_file_extension)
        })
        .map(|e| e.path)
        .collect();
    files.sort_by(|a, b| {
        let an = a.file_name().unwrap_or_default();
        let bn = b.file_name().unwrap_or_default();
        an.to_ascii_lowercase()
            .cmp(&bn.to_ascii_lowercase())
            .then_with(|| an.cmp(bn))
    });
    Some(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str) -> FsPath {
        FsPath::new(format!("local:c:/media/{name}")).unwrap()
    }

    fn nav(names: &[&str], index: usize, loop_playlist: bool) -> MediaFolderNav {
        MediaFolderNav {
            folder: Some(FsPath::new("local:c:/media").unwrap()),
            siblings: names.iter().map(|n| p(n)).collect(),
            index,
            loop_playlist,
            ..MediaFolderNav::default()
        }
    }

    #[test]
    fn next_wraps_when_looping() {
        let n = nav(&["a.mp4", "b.mp3", "c.mkv"], 2, true);
        assert_eq!(n.pick(MediaNavStep::Next), Some(0));
    }

    #[test]
    fn next_stops_without_loop() {
        let n = nav(&["a.mp4", "b.mp3", "c.mkv"], 2, false);
        assert_eq!(n.pick(MediaNavStep::Next), None);
    }

    #[test]
    fn skips_unreadable() {
        let mut n = nav(&["a.mp4", "b.mp3", "c.mkv"], 0, true);
        n.mark_unreadable(&p("b.mp3"));
        assert_eq!(n.pick(MediaNavStep::Next), Some(2));
    }
}
