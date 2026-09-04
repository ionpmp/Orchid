//! Library scan and browse helpers for the audio player.

#![allow(missing_docs)]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use orchid_viewers::{is_media_file_extension, load_track_meta, TrackMeta};

use super::config::{BrowseTab, LibrarySort};

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
    /// ID3 TLEN duration when known (milliseconds).
    pub duration_ms: Option<u32>,
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
            duration_ms: meta.duration_ms,
        }
    }
}

/// In-memory library index.
#[derive(Debug, Default, Clone)]
pub struct LibraryIndex {
    pub tracks: Vec<LibraryTrack>,
    by_path: HashMap<String, usize>,
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
        let by_path = tracks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.path.to_string_lossy().into_owned(), i))
            .collect();
        Self { tracks, by_path }
    }

    #[must_use]
    pub fn find_by_path(&self, path: &str) -> Option<&LibraryTrack> {
        if let Some(&i) = self.by_path.get(path) {
            return self.tracks.get(i);
        }
        self.tracks
            .iter()
            .find(|t| t.path.to_string_lossy() == path)
    }

    /// Flat list for the current browse tab / filter / search.
    #[must_use]
    pub fn browse_rows(
        &self,
        tab: BrowseTab,
        filter: &str,
        search: &str,
        active_playlist_id: &str,
        playlist_tracks: Option<&[String]>,
        recent_tracks: &[String],
        favorites: &[String],
        sort: LibrarySort,
    ) -> BrowseResult {
        let q = search.trim().to_lowercase();
        let matches = |t: &LibraryTrack| track_matches(t, &q);
        let mut result = match tab {
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
            BrowseTab::Genres => {
                if filter.is_empty() {
                    let mut genres: BTreeMap<String, usize> = BTreeMap::new();
                    for t in self
                        .tracks
                        .iter()
                        .filter(|t| matches(t) && !t.genre.trim().is_empty())
                    {
                        *genres.entry(t.genre.clone()).or_default() += 1;
                    }
                    BrowseResult {
                        groups: genres
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
                            .filter(|t| t.genre == filter && matches(t))
                            .map(track_row)
                            .collect(),
                    }
                }
            }
            BrowseTab::Playlists => {
                if active_playlist_id == super::RECENT_PLAYLIST_ID {
                    BrowseResult {
                        groups: Vec::new(),
                        tracks: recent_tracks
                            .iter()
                            .filter_map(|p| self.find_by_path(p))
                            .filter(|t| matches(t))
                            .map(track_row)
                            .collect(),
                    }
                } else if let Some(paths) = playlist_tracks {
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
        };
        let preserve_order =
            tab == BrowseTab::Playlists && active_playlist_id == super::RECENT_PLAYLIST_ID;
        if !preserve_order && tab != BrowseTab::NowPlaying && !result.tracks.is_empty() {
            sort_track_rows(&mut result.tracks, self, sort);
        }
        result
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
    pub duration_label: String,
}

/// Result of a browse query.
#[derive(Debug, Clone, Default)]
pub struct BrowseResult {
    pub groups: Vec<BrowseGroup>,
    pub tracks: Vec<TrackRow>,
}

pub(crate) fn track_row(t: &LibraryTrack) -> TrackRow {
    TrackRow {
        path: t.path.to_string_lossy().into_owned(),
        title: t.title.clone(),
        artist: t.artist.clone(),
        album: t.album.clone(),
        subtitle: format!("{} — {}", t.artist, t.album),
        duration_label: t
            .duration_ms
            .map(|ms| format_duration_ms(u64::from(ms)))
            .unwrap_or_default(),
    }
}

fn format_duration_ms(ms: u64) -> String {
    let total = ms / 1000;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn sort_track_rows(tracks: &mut [TrackRow], index: &LibraryIndex, sort: LibrarySort) {
    tracks.sort_by(|a, b| {
        let ta = index.find_by_path(&a.path);
        let tb = index.find_by_path(&b.path);
        match (ta, tb) {
            (Some(a), Some(b)) => compare_tracks(a, b, sort),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
        }
    });
}

fn compare_tracks(a: &LibraryTrack, b: &LibraryTrack, sort: LibrarySort) -> std::cmp::Ordering {
    match sort {
        LibrarySort::ArtistAlbum => a
            .artist
            .to_lowercase()
            .cmp(&b.artist.to_lowercase())
            .then_with(|| a.album.to_lowercase().cmp(&b.album.to_lowercase()))
            .then_with(|| a.track.cmp(&b.track))
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase())),
        LibrarySort::Title => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
        LibrarySort::Album => a
            .album
            .to_lowercase()
            .cmp(&b.album.to_lowercase())
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase())),
        LibrarySort::Year => match (a.year, b.year) {
            (None, None) => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (Some(ya), Some(yb)) => yb
                .cmp(&ya)
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase())),
        },
        LibrarySort::Genre => {
            let ga = if a.genre.is_empty() {
                "\u{ffff}"
            } else {
                a.genre.as_str()
            };
            let gb = if b.genre.is_empty() {
                "\u{ffff}"
            } else {
                b.genre.as_str()
            };
            ga.to_lowercase()
                .cmp(&gb.to_lowercase())
                .then_with(|| a.artist.to_lowercase().cmp(&b.artist.to_lowercase()))
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        }
    }
}

fn track_matches(t: &LibraryTrack, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    t.title.to_lowercase().contains(q)
        || t.artist.to_lowercase().contains(q)
        || t.album.to_lowercase().contains(q)
        || t.genre.to_lowercase().contains(q)
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
        let artists = idx.browse_rows(
            BrowseTab::Artists,
            "",
            "",
            "",
            None,
            &[],
            &[],
            LibrarySort::default(),
        );
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
            duration_ms: None,
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
            duration_ms: None,
        });
        let neon = idx.browse_rows(
            BrowseTab::Songs,
            "",
            "neon",
            "",
            None,
            &[],
            &[],
            LibrarySort::default(),
        );
        assert_eq!(neon.tracks.len(), 1);
        assert_eq!(neon.tracks[0].title, "Neon Lights");
        let folk = idx.browse_rows(
            BrowseTab::Artists,
            "",
            "folk",
            "",
            None,
            &[],
            &[],
            LibrarySort::default(),
        );
        assert_eq!(folk.groups.len(), 1);
        assert_eq!(folk.groups[0].label, "Folk Duo");
        let miss = idx.browse_rows(
            BrowseTab::Songs,
            "",
            "zzzz",
            "",
            None,
            &[],
            &[],
            LibrarySort::default(),
        );
        assert!(miss.tracks.is_empty());
    }

    #[test]
    fn sort_orders_songs_by_title() {
        let mut idx = LibraryIndex::default();
        idx.tracks.push(LibraryTrack {
            path: PathBuf::from("/music/z.mp3"),
            title: "Zulu".into(),
            artist: "Band".into(),
            album: "One".into(),
            genre: String::new(),
            track: None,
            year: None,
            folder: "/music".into(),
            duration_ms: None,
        });
        idx.tracks.push(LibraryTrack {
            path: PathBuf::from("/music/a.mp3"),
            title: "Alpha".into(),
            artist: "Band".into(),
            album: "One".into(),
            genre: String::new(),
            track: None,
            year: None,
            folder: "/music".into(),
            duration_ms: None,
        });
        let sorted = idx.browse_rows(
            BrowseTab::Songs,
            "",
            "",
            "",
            None,
            &[],
            &[],
            LibrarySort::Title,
        );
        assert_eq!(sorted.tracks[0].title, "Alpha");
        assert_eq!(sorted.tracks[1].title, "Zulu");
    }

    #[test]
    fn sort_orders_songs_by_genre() {
        let mut idx = LibraryIndex::default();
        idx.tracks.push(LibraryTrack {
            path: PathBuf::from("/music/rock.mp3"),
            title: "Riff".into(),
            artist: "A".into(),
            album: "One".into(),
            genre: "Rock".into(),
            track: None,
            year: None,
            folder: "/music".into(),
            duration_ms: None,
        });
        idx.tracks.push(LibraryTrack {
            path: PathBuf::from("/music/jazz.mp3"),
            title: "Swing".into(),
            artist: "B".into(),
            album: "Two".into(),
            genre: "Jazz".into(),
            track: None,
            year: None,
            folder: "/music".into(),
            duration_ms: None,
        });
        let sorted = idx.browse_rows(
            BrowseTab::Songs,
            "",
            "",
            "",
            None,
            &[],
            &[],
            LibrarySort::Genre,
        );
        assert_eq!(sorted.tracks[0].title, "Swing");
        assert_eq!(sorted.tracks[1].title, "Riff");
    }

    #[test]
    fn genres_tab_groups_and_filters() {
        let mut idx = LibraryIndex::default();
        for (path, title, genre) in [
            ("/a.mp3", "A", "Rock"),
            ("/b.mp3", "B", "Jazz"),
            ("/c.mp3", "C", "Rock"),
            ("/d.mp3", "D", ""),
        ] {
            idx.tracks.push(LibraryTrack {
                path: PathBuf::from(path),
                title: title.into(),
                artist: "X".into(),
                album: "Y".into(),
                genre: genre.into(),
                track: None,
                year: None,
                folder: "/music".into(),
                duration_ms: None,
            });
        }
        let groups = idx.browse_rows(
            BrowseTab::Genres,
            "",
            "",
            "",
            None,
            &[],
            &[],
            LibrarySort::Title,
        );
        assert_eq!(groups.groups.len(), 2);
        assert!(groups
            .groups
            .iter()
            .any(|g| g.key == "Rock" && g.count == 2));
        assert!(groups
            .groups
            .iter()
            .any(|g| g.key == "Jazz" && g.count == 1));
        let rock = idx.browse_rows(
            BrowseTab::Genres,
            "Rock",
            "",
            "",
            None,
            &[],
            &[],
            LibrarySort::Title,
        );
        assert_eq!(rock.tracks.len(), 2);
        assert_eq!(rock.tracks[0].title, "A");
        assert_eq!(rock.tracks[1].title, "C");
    }

    #[test]
    fn formats_duration_label_from_ms() {
        assert_eq!(format_duration_ms(125_000), "2:05");
        assert_eq!(format_duration_ms(3_661_000), "1:01:01");
        let t = LibraryTrack {
            path: PathBuf::from("/a.mp3"),
            title: "A".into(),
            artist: "X".into(),
            album: "Y".into(),
            genre: String::new(),
            track: None,
            year: None,
            folder: "/music".into(),
            duration_ms: Some(125_000),
        };
        assert_eq!(track_row(&t).duration_label, "2:05");
    }

    #[test]
    fn recent_playlist_preserves_play_order() {
        let mut idx = LibraryIndex::default();
        for (path, title) in [("/a.mp3", "A"), ("/b.mp3", "B"), ("/c.mp3", "C")] {
            idx.tracks.push(LibraryTrack {
                path: PathBuf::from(path),
                title: title.into(),
                artist: "X".into(),
                album: "Y".into(),
                genre: String::new(),
                track: None,
                year: None,
                folder: "/music".into(),
                duration_ms: None,
            });
        }
        let recent = vec!["/c.mp3".into(), "/a.mp3".into()];
        let rows = idx.browse_rows(
            BrowseTab::Playlists,
            "",
            "",
            crate::builtin::audio_player::RECENT_PLAYLIST_ID,
            None,
            &recent,
            &[],
            LibrarySort::Title,
        );
        assert_eq!(rows.tracks.len(), 2);
        assert_eq!(rows.tracks[0].path, "/c.mp3");
        assert_eq!(rows.tracks[1].path, "/a.mp3");
    }
}
