//! Video library player payload.

#![allow(missing_docs)]

use std::sync::Arc;

/// One video row in the library or queue list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VideoPlayerItemRow {
    pub path: String,
    pub title: String,
    pub subtitle: String,
    pub duration_label: String,
    pub is_current: bool,
}

/// Configured library root folder.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VideoPlayerRootRow {
    pub path: String,
    pub label: String,
}

/// Snapshot for the Slint video-player surface.
#[derive(Debug, Clone, Default)]
pub struct VideoPlayerPayload {
    pub engine_available: bool,
    /// 0 = library, 1 = queue.
    pub browse_tab: u8,
    pub search_query: String,
    pub roots: Vec<VideoPlayerRootRow>,
    pub items: Vec<VideoPlayerItemRow>,
    pub queue: Vec<VideoPlayerItemRow>,
    pub queue_index: i32,
    pub queue_count: u32,
    pub has_track: bool,
    pub title: String,
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
    pub speed_label: String,
    pub library_count: u32,
    pub has_library_roots: bool,
    pub empty_hint: String,
    pub has_video: bool,
    pub frame_rgba: Arc<Vec<u8>>,
    pub frame_width: u32,
    pub frame_height: u32,
}
