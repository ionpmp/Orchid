//! Library scan and browse helpers for the audio player.

#![allow(missing_docs)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use orchid_viewers::{is_media_file_extension, load_track_meta, TrackMeta};

use super::config::BrowseTab;

/// Audio-only extensions (subset of media extensions).
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "wav", "flac", "ogg", "aac", "m4a", "wma", "opus", "aiff",
];

/// Whether `ext` (lowercase, no dot) is an audio file.
#[must_use]
pub fn is_audio_extension(ext: &str) -> bool {
    AUDIO_EXTENSIONS.contains(&ext) || {
        // Fall back to media classifier for audio kinds only.
        is_media_file_extension(ext)
            && matches!(
                ext,
                "mp3" | "wav" | "flac" | "ogg" | "aac" | "m4a" | "wma" | "opus" | "aiff"
            )
    }
}

/// Indexed track in the library.
#[derive(Debug, Clone)]
pub struct LibraryTrack {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub track: Option<u32>,
    pub year: Option<i32>,
    pub folder: String,
}

impl LibraryTrack {
    fn from_path(path: PathBuf) -> Self {
        let meta: TrackMeta = load_track_meta(&path);
        let folder = path
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            path,
            title: meta.title,
            artist: if meta.artist.is_empty() {
                "Unknown Artist".into()
            } else {
                meta.artist
            },
            album: if meta.album.is_empty() {
                "Unknown Album".into()
            } else {
                meta.album
            },
            genre: meta.genre,
            track: meta.track,
            year: meta.year,
            folder,
        }
    }
}

/// In-memory library index.
#[derive(Debug, Default, Clone)]
pub struct LibraryIndex {
    pub tracks: Vec<LibraryTrack>,
}

impl LibraryIndex {
    /// Scan `roots` recursively for audio files (depth-capped).
    #[must_use]
    pub fn scan(roots: &[String]) -> Self {
        let mut tracks = Vec::new();
        let mut seen = BTreeSet::new();
        for root in roots {
            let path = PathBuf::from(root);
            if !path.is_dir() {
                continue;
            }
            walk_dir(&path, 0, &mut tracks, &mut seen);
        }
        tracks.sort_by(|a, b| {
            a.artist
                .to_lowercase()
                .cmp(&b.artist.to_lowercase())
                .then_with(|| a.album.to_lowercase().cmp(&b.album.to_lowercase()))
                .then_with(|| a.track.cmp(&b.track))
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        Self { tracks }
    }

    #[must_use]
    pub fn find_by_path(&self, path: &str) -> Option<&LibraryTrack> {
        self.tracks.iter().find(|t| t.path.to_string_lossy() == path)
    }

    /// Flat list for the current browse tab / filter / search.
    #[must_use]
    pub fn browse_rows(
        &self,
        tab: BrowseTab,
        filter: &str,
        search: &str,
        playlist_tracks: Option<&[String]>,
        favorites: &[String],
    ) -> BrowseResult {
        let q = search.trim().to_lowercase();
        let matches = |t: &LibraryTrack| track_matches(t, &q);
        match tab {
            BrowseTab::Songs => BrowseResult {
                groups: Vec::new(),
                tracks: self
                    .tracks
                    .iter()
                    .filter(|t| matches(t))
                    .map(track_row)
                    .collect(),
            },
            BrowseTab::Artists => {
                if filter.is_empty() {
                    let mut artists: BTreeMap<String, usize> = BTreeMap::new();
                    for t in self.tracks.iter().filter(|t| matches(t)) {
                        *artists.entry(t.artist.clone()).or_default() += 1;
                    }
                    BrowseResult {
                        groups: artists
                            .into_iter()
                            .map(|(name, count)| BrowseGroup {
                                key: name.clone(),
                                label: name,
                                count: count as u32,
                            })
                            .collect(),
                        tracks: Vec::new(),
                    }
                } else {
                    BrowseResult {
                        groups: Vec::new(),
                        tracks: self
                            .tracks
                            .iter()
                            .filter(|t| t.artist == filter && matches(t))
                            .map(track_row)
                            .collect(),
                    }
                }
            }
            BrowseTab::Albums => {
                if filter.is_empty() {
                    let mut albums: BTreeMap<(String, String), usize> = BTreeMap::new();
                    for t in self.tracks.iter().filter(|t| matches(t)) {
                        *albums
                            .entry((t.album.clone(), t.artist.clone()))
                            .or_default() += 1;
                    }
                    BrowseResult {
                        groups: albums
                            .into_iter()
                            .map(|((album, artist), count)| BrowseGroup {
                                key: format!("{album}\u{1f}{artist}"),
                                label: if artist.is_empty() {
                                    album
                                } else {
                                    format!("{album} — {artist}")
                                },
                                count: count as u32,
                            })
                            .collect(),
                        tracks: Vec::new(),
                    }
                } else {
                    let (album, artist) = split_album_key(filter);
                    BrowseResult {
                        groups: Vec::new(),
                        tracks: self
                            .tracks
                            .iter()
                            .filter(|t| {
                                t.album == album
                                    && (artist.is_empty() || t.artist == artist)
                                    && matches(t)
                            })
                            .map(track_row)
                            .collect(),
                    }
                }
            }
            BrowseTab::Folders => {
                if filter.is_empty() {
                    let mut folders: BTreeMap<String, usize> = BTreeMap::new();
                    for t in self.tracks.iter().filter(|t| matches(t)) {
                        *folders.entry(t.folder.clone()).or_default() += 1;
                    }
                    BrowseResult {
                        groups: folders
                            .into_iter()
                            .map(|(name, count)| BrowseGroup {
                                key: name.clone(),
                                label: name,
                                count: count as u32,
                            })
                            .collect(),
                        tracks: Vec::new(),
                    }
                } else {
                    BrowseResult {
                        groups: Vec::new(),
                        tracks: self
                            .tracks
                            .iter()
                            .filter(|t| t.folder == filter && matches(t))
                            .map(track_row)
                            .collect(),
                    }
                }
            }
            BrowseTab::Playlists => {
                if let Some(paths) = playlist_tracks {
                    let set: BTreeSet<&str> = paths.iter().map(String::as_str).collect();
                    BrowseResult {
                        groups: Vec::new(),
                        tracks: self
                            .tracks
                            .iter()
                            .filter(|t| {
                                set.contains(t.path.to_string_lossy().as_ref()) && matches(t)
                            })
                            .map(track_row)
                            .collect(),
                    }
                } else {
                    BrowseResult {
                        groups: Vec::new(),
                        tracks: favorites
                            .iter()
                            .filter_map(|p| self.find_by_path(p))
                            .filter(|t| matches(t))
                            .map(track_row)
                            .collect(),
                    }
                }
            }
            BrowseTab::NowPlaying => BrowseResult {
                groups: Vec::new(),
                tracks: Vec::new(),
            },
        }
    }
}

/// Group row (artist / album / folder).
#[derive(Debug, Clone)]
pub struct BrowseGroup {
    pub key: String,
    pub label: String,
    pub count: u32,
}

/// Track row for the list UI.
#[derive(Debug, Clone)]
pub struct TrackRow {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub subtitle: String,
}

/// Result of a browse query.
#[derive(Debug, Clone, Default)]
pub struct BrowseResult {
    pub groups: Vec<BrowseGroup>,
    pub tracks: Vec<TrackRow>,
}

fn track_row(t: &LibraryTrack) -> TrackRow {
    TrackRow {
        path: t.path.to_string_lossy().into_owned(),
        title: t.title.clone(),
        artist: t.artist.clone(),
        album: t.album.clone(),
        subtitle: format!("{} — {}", t.artist, t.album),
    }
}

fn track_matches(t: &LibraryTrack, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    t.title.to_lowercase().contains(q)
        || t.artist.to_lowercase().contains(q)
        || t.album.to_lowercase().contains(q)
        || t.path.to_string_lossy().to_lowercase().contains(q)
}

/// Whether a browse track row matches a lowercase search query.
#[must_use]
pub(crate) fn track_row_matches(row: &TrackRow, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    row.title.to_lowercase().contains(q)
        || row.artist.to_lowercase().contains(q)
        || row.album.to_lowercase().contains(q)
        || row.path.to_lowercase().contains(q)
        || row.subtitle.to_lowercase().contains(q)
}

fn split_album_key(filter: &str) -> (String, String) {
    if let Some((album, artist)) = filter.split_once('\u{1f}') {
        (album.to_string(), artist.to_string())
    } else {
        (filter.to_string(), String::new())
    }
}

fn walk_dir(dir: &Path, depth: u32, out: &mut Vec<LibraryTrack>, seen: &mut BTreeSet<PathBuf>) {
    const MAX_DEPTH: u32 = 8;
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
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let ext = ext.to_ascii_lowercase();
        if !is_audio_extension(&ext) {
            continue;
        }
        let Ok(canon) = path.canonicalize() else {
            if seen.insert(path.clone()) {
                out.push(LibraryTrack::from_path(path));
            }
            continue;
        };
        if seen.insert(canon.clone()) {
            out.push(LibraryTrack::from_path(canon));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scans_audio_files_only() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.mp3"), b"").unwrap();
        fs::write(dir.path().join("b.flac"), b"").unwrap();
        fs::write(dir.path().join("c.mp4"), b"").unwrap();
        fs::write(dir.path().join("d.txt"), b"").unwrap();
        let idx = LibraryIndex::scan(&[dir.path().to_string_lossy().into_owned()]);
        assert_eq!(idx.tracks.len(), 2);
        let artists = idx.browse_rows(BrowseTab::Artists, "", "", None, &[]);
        assert!(!artists.groups.is_empty());
    }

    #[test]
    fn search_filters_songs_by_title_artist_album() {
        let mut idx = LibraryIndex::default();
        idx.tracks.push(LibraryTrack {
            path: PathBuf::from("/music/a.mp3"),
            title: "Neon Lights".into(),
            artist: "Synth Wave".into(),
            album: "Night Drive".into(),
            genre: String::new(),
            track: Some(1),
            year: None,
            folder: "/music".into(),
        });
        idx.tracks.push(LibraryTrack {
            path: PathBuf::from("/music/b.mp3"),
            title: "Acoustic Morning".into(),
            artist: "Folk Duo".into(),
            album: "Sunrise".into(),
            genre: String::new(),
            track: Some(2),
            year: None,
            folder: "/music".into(),
        });
        let neon = idx.browse_rows(BrowseTab::Songs, "", "neon", None, &[]);
        assert_eq!(neon.tracks.len(), 1);
        assert_eq!(neon.tracks[0].title, "Neon Lights");
        let folk = idx.browse_rows(BrowseTab::Artists, "", "folk", None, &[]);
        assert_eq!(folk.groups.len(), 1);
        assert_eq!(folk.groups[0].label, "Folk Duo");
        let miss = idx.browse_rows(BrowseTab::Songs, "", "zzzz", None, &[]);
        assert!(miss.tracks.is_empty());
    }
}
