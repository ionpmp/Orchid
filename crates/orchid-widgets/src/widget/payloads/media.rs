//! Payload for the media-player widget.

use std::sync::Arc;

/// Render-ready media-player payload.
#[derive(Debug, Clone, Default)]
#[allow(missing_docs)]
pub struct MediaPlayerPayload {
    pub has_session: bool,
    pub is_loading: bool,
    pub is_unsupported: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub source_app: String,
    pub position_secs: u64,
    pub duration_secs: u64,
    pub progress_fraction: f32,
    pub is_playing: bool,
    /// Encoded thumbnail image bytes (JPEG/PNG/…), shared via [`Arc`] so the
    /// UI can cache a decoded Slint `Image` by pointer identity without a
    /// base64 round-trip on every poll.
    pub thumbnail_bytes: Option<Arc<[u8]>>,
}
