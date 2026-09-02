//! Persistent config for the local audio library player.

#![allow(missing_docs)]

use bincode_reloaded::{Decode, Encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Repeat mode for the play queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Encode, Decode)]
#[repr(u8)]
pub enum RepeatMode {
    #[default]
    Off = 0,
    All = 1,
    One = 2,
}

impl RepeatMode {
    #[must_use]
    pub fn cycle(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }

    #[must_use]
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::All,
            2 => Self::One,
            _ => Self::Off,
        }
    }
}

/// Browse tab in the library UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Encode, Decode)]
#[repr(u8)]
pub enum BrowseTab {
    #[default]
    Songs = 0,
    Artists = 1,
    Albums = 2,
    Folders = 3,
    Genres = 4,
    Playlists = 5,
    NowPlaying = 6,
}

impl BrowseTab {
    #[must_use]
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Artists,
            2 => Self::Albums,
            3 => Self::Folders,
            4 => Self::Genres,
            5 => Self::Playlists,
            6 => Self::NowPlaying,
            _ => Self::Songs,
        }
    }

    #[must_use]
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Track list sort order for library browse views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Encode, Decode)]
#[repr(u8)]
pub enum LibrarySort {
    #[default]
    ArtistAlbum = 0,
    Title = 1,
    Album = 2,
    Year = 3,
    Genre = 4,
}

impl LibrarySort {
    #[must_use]
    pub fn cycle(self) -> Self {
        match self {
            Self::ArtistAlbum => Self::Title,
            Self::Title => Self::Album,
            Self::Album => Self::Year,
            Self::Year => Self::Genre,
            Self::Genre => Self::ArtistAlbum,
        }
    }

    #[must_use]
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Title,
            2 => Self::Album,
            3 => Self::Year,
            4 => Self::Genre,
            _ => Self::ArtistAlbum,
        }
    }
}

/// User-defined playlist.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct PlaylistEntry {
    pub id: String,
    pub name: String,
    /// Absolute OS paths of tracks.
    pub tracks: Vec<String>,
}

impl PlaylistEntry {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            tracks: Vec::new(),
        }
    }
}

/// Persisted audio-player state.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct AudioPlayerConfig {
    /// Folders to scan for audio files.
    pub library_roots: Vec<String>,
    pub playlists: Vec<PlaylistEntry>,
    /// Favorite track paths.
    pub favorites: Vec<String>,
    /// Last queue (paths) restored on activate.
    pub queue: Vec<String>,
    pub queue_index: u32,
    pub volume: f32,
    pub muted: bool,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub browse_tab: BrowseTab,
    /// Selected playlist id when on Playlists tab.
    pub active_playlist_id: String,
    /// Artist / album / folder drill-down key (empty = top level).
    pub browse_filter: String,
    /// Free-text library search (title / artist / album).
    pub search_query: String,
    /// Track list ordering in library browse views.
    pub library_sort: LibrarySort,
    /// Inline rename editor open for the active user playlist.
    pub renaming_playlist: bool,
    /// Last EQ preset index (session restore for audio-only engine).
    pub eq_index: u32,
    /// Last ReplayGain mode index.
    pub replaygain_index: u32,
    /// Playback speed ×100 (100 = 1.0×).
    pub speed_x100: u32,
    /// Recently played track paths (most recent first).
    pub recent_tracks: Vec<String>,
    /// Soft crossfade window in seconds (0 = off). Values: 0, 3, 5, 8, 12.
    #[serde(default)]
    pub crossfade_secs: u8,
}

impl Default for AudioPlayerConfig {
    fn default() -> Self {
        Self {
            library_roots: Vec::new(),
            playlists: Vec::new(),
            favorites: Vec::new(),
            queue: Vec::new(),
            queue_index: 0,
            volume: 100.0,
            muted: false,
            shuffle: false,
            repeat: RepeatMode::Off,
            browse_tab: BrowseTab::Songs,
            active_playlist_id: String::new(),
            browse_filter: String::new(),
            search_query: String::new(),
            library_sort: LibrarySort::default(),
            renaming_playlist: false,
            eq_index: 0,
            replaygain_index: 0,
            speed_x100: 100,
            recent_tracks: Vec::new(),
            crossfade_secs: 0,
        }
    }
}

impl AudioPlayerConfig {
    pub fn normalize(&mut self) {
        self.volume = self.volume.clamp(0.0, 150.0);
        if self.queue_index as usize >= self.queue.len() && !self.queue.is_empty() {
            self.queue_index = (self.queue.len() - 1) as u32;
        }
        if self.queue.is_empty() {
            self.queue_index = 0;
        }
        self.favorites.sort();
        self.favorites.dedup();
        self.recent_tracks.truncate(50);
        // Ensure a Favorites synthetic playlist is not required — favorites are separate.
        if !self
            .playlists
            .iter()
            .any(|p| p.id == self.active_playlist_id)
            && !self.active_playlist_id.is_empty()
            && self.active_playlist_id != "__recent__"
        {
            self.active_playlist_id.clear();
        }
        self.eq_index %= 4;
        self.replaygain_index %= 3;
        if self.speed_x100 == 0 {
            self.speed_x100 = 100;
        }
        self.crossfade_secs = match self.crossfade_secs {
            0 | 3 | 5 | 8 | 12 => self.crossfade_secs,
            _ => 0,
        };
    }

    pub fn cycle_crossfade(&mut self) {
        self.crossfade_secs = match self.crossfade_secs {
            0 => 3,
            3 => 5,
            5 => 8,
            8 => 12,
            _ => 0,
        };
    }
}
