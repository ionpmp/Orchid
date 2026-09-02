//! Persistent config for the local video library player.

#![allow(missing_docs)]

use bincode_reloaded::{Decode, Encode};
use serde::{Deserialize, Serialize};

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
    Library = 0,
    Queue = 1,
}

impl BrowseTab {
    #[must_use]
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Queue,
            _ => Self::Library,
        }
    }
}

/// Persisted video-player state.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct VideoPlayerConfig {
    /// Folders to scan for video files.
    pub library_roots: Vec<String>,
    /// Last queue (paths) restored on activate.
    pub queue: Vec<String>,
    pub queue_index: u32,
    pub volume: f32,
    pub muted: bool,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    pub browse_tab: BrowseTab,
    /// Free-text library search (title / path).
    pub search_query: String,
    /// Playback speed ×100 (100 = 1.0×).
    pub speed_x100: u32,
}

impl Default for VideoPlayerConfig {
    fn default() -> Self {
        Self {
            library_roots: Vec::new(),
            queue: Vec::new(),
            queue_index: 0,
            volume: 100.0,
            muted: false,
            shuffle: false,
            repeat: RepeatMode::Off,
            browse_tab: BrowseTab::Library,
            search_query: String::new(),
            speed_x100: 100,
        }
    }
}

impl VideoPlayerConfig {
    pub fn normalize(&mut self) {
        self.library_roots.retain(|r| !r.trim().is_empty());
        self.queue.retain(|p| !p.trim().is_empty());
        if self.queue.is_empty() {
            self.queue_index = 0;
        } else {
            self.queue_index = self.queue_index.min(self.queue.len() as u32 - 1);
        }
        if self.volume.is_nan() || self.volume < 0.0 {
            self.volume = 0.0;
        } else if self.volume > 150.0 {
            self.volume = 150.0;
        }
        if self.speed_x100 == 0 {
            self.speed_x100 = 100;
        }
        self.speed_x100 = self.speed_x100.clamp(25, 400);
    }
}
