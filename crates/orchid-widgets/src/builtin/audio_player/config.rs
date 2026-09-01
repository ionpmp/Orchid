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
    Playlists = 4,
    NowPlaying = 5,
}

impl BrowseTab {
    #[must_use]
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Artists,
            2 => Self::Albums,
            3 => Self::Folders,
            4 => Self::Playlists,
            5 => Self::NowPlaying,
            _ => Self::Songs,
        }
    }

    #[must_use]
    pub fn as_u8(self) -> u8 {
        self as u8
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
    /// Inline rename editor open for the active user playlist.
    pub renaming_playlist: bool,
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
            renaming_playlist: false,
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
        // Ensure a Favorites synthetic playlist is not required — favorites are separate.
        if !self
            .playlists
            .iter()
            .any(|p| p.id == self.active_playlist_id)
            && !self.active_playlist_id.is_empty()
        {
            self.active_playlist_id.clear();
        }
    }
}
