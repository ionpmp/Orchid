//! Audio library player payload.

#![allow(missing_docs)]

use std::sync::Arc;

/// One browse group (artist / album / folder / playlist).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AudioPlayerGroupRow {
    pub key: String,
    pub label: String,
    pub count: u32,
    /// True when `key` matches a configured library root (Folders tab remove).
    pub is_library_root: bool,
}

/// One track row in the list.
#[derive(Debug, Clone, Default)]
pub struct AudioPlayerTrackRow {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub subtitle: String,
    pub duration_label: String,
    pub is_current: bool,
    pub is_favorite: bool,
    pub has_cover: bool,
    pub cover_rgba: Arc<Vec<u8>>,
    pub cover_width: u32,
    pub cover_height: u32,
}

impl PartialEq for AudioPlayerTrackRow {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
            && self.title == other.title
            && self.artist == other.artist
            && self.album == other.album
            && self.subtitle == other.subtitle
            && self.duration_label == other.duration_label
            && self.is_current == other.is_current
            && self.is_favorite == other.is_favorite
            && self.has_cover == other.has_cover
            && self.cover_width == other.cover_width
            && self.cover_height == other.cover_height
            && (std::sync::Arc::ptr_eq(&self.cover_rgba, &other.cover_rgba)
                || self.cover_rgba.as_ref() == other.cover_rgba.as_ref())
    }
}

/// Playlist chip / row.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AudioPlayerPlaylistRow {
    pub id: String,
    pub name: String,
    pub count: u32,
    pub is_active: bool,
}

/// Configured library root folder.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AudioPlayerRootRow {
    pub path: String,
    pub label: String,
}

/// Snapshot for the Slint audio-player surface.
#[derive(Debug, Clone, Default)]
pub struct AudioPlayerPayload {
    pub engine_available: bool,
    pub browse_tab: u8,
    pub browse_filter: String,
    pub browse_filter_label: String,
    pub search_query: String,
    pub library_sort: u8,
    pub is_current_favorite: bool,
    pub renaming_playlist: bool,
    /// Prefill for the inline rename field (active playlist name).
    pub rename_playlist_draft: String,
    pub active_playlist_id: String,
    pub groups: Vec<AudioPlayerGroupRow>,
    pub tracks: Vec<AudioPlayerTrackRow>,
    pub playlists: Vec<AudioPlayerPlaylistRow>,
    pub roots: Vec<AudioPlayerRootRow>,
    pub queue: Vec<AudioPlayerTrackRow>,
    pub queue_index: i32,
    /// Index of the current track in `tracks` (−1 if not listed).
    pub current_track_index: i32,
    /// Number of tracks in the play queue.
    pub queue_count: u32,
    /// Sum of known ID3 durations for queued tracks (ms); 0 if none known.
    pub queue_duration_ms: u64,
    pub has_track: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub is_playing: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub progress: f32,
    pub position_label: String,
    pub duration_label: String,
    pub volume: u32,
    pub muted: bool,
    pub shuffle: bool,
    pub repeat: u8,
    pub sleep_label: String,
    pub eq_label: String,
    pub rg_label: String,
    pub speed_label: String,
    /// Soft crossfade window in seconds (0 = off).
    pub crossfade_secs: u8,
    pub lyrics_line: String,
    pub has_lyrics: bool,
    pub library_count: u32,
    pub library_roots_count: u32,
    pub has_library_roots: bool,
    pub roots_label: String,
    pub empty_hint: String,
    pub has_cover: bool,
    pub cover_rgba: Arc<Vec<u8>>,
    pub cover_width: u32,
    pub cover_height: u32,
}
