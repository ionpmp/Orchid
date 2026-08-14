//! Folder playlist for the image viewer (next/prev, loop, skip unreadable).

use std::collections::{HashSet, VecDeque};

use orchid_fs::{FsEntryKind, FsPath, FsProviderRegistry};
use orchid_viewers::is_image_file_extension;

/// Session playlist of image siblings in one folder.
#[derive(Debug, Clone)]
pub struct ImageFolderNav {
    /// Parent folder of the current image.
    pub folder: Option<FsPath>,
    /// Image files in [`Self::folder`], name-sorted.
    pub siblings: Vec<FsPath>,
    /// Index into [`Self::siblings`].
    pub index: usize,
    /// Wrap around at the ends.
    pub loop_playlist: bool,
    /// Recently opened images in this viewer (newest first).
    pub history: VecDeque<FsPath>,
    /// Paths that failed to decode; skipped on later steps.
    pub unreadable: HashSet<String>,
}

impl Default for ImageFolderNav {
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
pub enum NavStep {
    /// Next sibling.
    Next,
    /// Previous sibling.
    Prev,
    /// First readable sibling.
    First,
    /// Last readable sibling.
    Last,
    /// Random readable sibling (not the current one when possible).
    Random,
    /// 1-based index.
    Goto(usize),
}

impl ImageFolderNav {
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
    pub fn mark_unreadable(&mut self, path: &FsPath) {
        self.unreadable.insert(path.as_str().to_string());
    }

    /// Newest-first history paths for the UI.
    #[must_use]
    pub fn recent_paths(&self) -> Vec<String> {
        self.history
            .iter()
            .map(|p| p.as_str().to_string())
            .collect()
    }

    /// Next readable index for `step`, or `None` when the playlist is empty
    /// or exhausted without loop.
    #[must_use]
    pub fn pick(&self, step: NavStep) -> Option<usize> {
        let n = self.siblings.len();
        if n == 0 {
            return None;
        }
        match step {
            NavStep::First => self.first_readable_from(0, 1),
            NavStep::Last => self.first_readable_from(n - 1, -1),
            NavStep::Goto(one_based) => {
                let idx = one_based.saturating_sub(1);
                if idx < n && self.is_readable(idx) {
                    Some(idx)
                } else {
                    None
                }
            }
            NavStep::Random => self.pick_random(),
            NavStep::Next => self.step_from(self.index, 1),
            NavStep::Prev => self.step_from(self.index, -1),
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

    fn step_from(&self, from: usize, dir: i32) -> Option<usize> {
        let n = self.siblings.len();
        if n == 0 {
            return None;
        }
        let mut i = from;
        for _ in 0..n {
            i = wrap_index(i, dir, n, self.loop_playlist)?;
            if self.is_readable(i) {
                return Some(i);
            }
        }
        None
    }

    fn pick_random(&self) -> Option<usize> {
        let readable: Vec<usize> = (0..self.siblings.len())
            .filter(|i| self.is_readable(*i))
            .collect();
        if readable.is_empty() {
            return None;
        }
        if readable.len() == 1 {
            return Some(readable[0]);
        }
        let others: Vec<usize> = readable
            .iter()
            .copied()
            .filter(|i| *i != self.index)
            .collect();
        let pool = if others.is_empty() {
            &readable
        } else {
            &others
        };
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        let mut x = seed ^ (self.index as u64).wrapping_mul(0x9E37_79B9);
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        Some(pool[(x as usize) % pool.len()])
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

/// Image files in `folder`, sorted by file name (case-insensitive).
pub async fn list_image_siblings(
    registry: &FsProviderRegistry,
    folder: &FsPath,
) -> Option<Vec<FsPath>> {
    let provider = registry.for_path(folder)?;
    let entries = provider.list(folder).await.ok()?;
    let mut images: Vec<FsPath> = entries
        .into_iter()
        .filter(|e| {
            matches!(e.metadata.kind, FsEntryKind::File | FsEntryKind::Symlink)
                && e.path.extension().is_some_and(is_image_file_extension)
        })
        .map(|e| e.path)
        .collect();
    images.sort_by(|a, b| {
        let an = a.file_name().unwrap_or_default();
        let bn = b.file_name().unwrap_or_default();
        an.to_ascii_lowercase()
            .cmp(&bn.to_ascii_lowercase())
            .then_with(|| an.cmp(bn))
    });
    Some(images)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str) -> FsPath {
        FsPath::new(format!("local:c:/pics/{name}")).unwrap()
    }

    fn nav(names: &[&str], index: usize, loop_playlist: bool) -> ImageFolderNav {
        ImageFolderNav {
            folder: Some(FsPath::new("local:c:/pics").unwrap()),
            siblings: names.iter().map(|n| p(n)).collect(),
            index,
            loop_playlist,
            ..ImageFolderNav::default()
        }
    }

    #[test]
    fn next_wraps_when_looping() {
        let n = nav(&["a.png", "b.png", "c.png"], 2, true);
        assert_eq!(n.pick(NavStep::Next), Some(0));
    }

    #[test]
    fn next_stops_without_loop() {
        let n = nav(&["a.png", "b.png", "c.png"], 2, false);
        assert_eq!(n.pick(NavStep::Next), None);
    }

    #[test]
    fn skips_unreadable() {
        let mut n = nav(&["a.png", "b.png", "c.png"], 0, true);
        n.mark_unreadable(&p("b.png"));
        assert_eq!(n.pick(NavStep::Next), Some(2));
        n.index = 2;
        assert_eq!(n.pick(NavStep::Prev), Some(0));
    }

    #[test]
    fn goto_is_one_based() {
        let n = nav(&["a.png", "b.png", "c.png"], 0, true);
        assert_eq!(n.pick(NavStep::Goto(2)), Some(1));
        assert_eq!(n.pick(NavStep::Goto(9)), None);
    }

    #[test]
    fn first_and_last_skip_bad_ends() {
        let mut n = nav(&["a.png", "b.png", "c.png"], 1, true);
        n.mark_unreadable(&p("a.png"));
        n.mark_unreadable(&p("c.png"));
        assert_eq!(n.pick(NavStep::First), Some(1));
        assert_eq!(n.pick(NavStep::Last), Some(1));
    }

    #[test]
    fn history_is_mru() {
        let mut n = ImageFolderNav::default();
        n.push_history(&p("a.png"));
        n.push_history(&p("b.png"));
        n.push_history(&p("a.png"));
        assert_eq!(
            n.recent_paths(),
            vec![
                "local:c:/pics/a.png".to_string(),
                "local:c:/pics/b.png".to_string()
            ]
        );
    }
}
