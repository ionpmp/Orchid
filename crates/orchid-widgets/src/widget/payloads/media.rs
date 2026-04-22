//! Payload for the media-player widget.

/// Render-ready media-player payload.
#[derive(Debug, Clone, Default)]
#[allow(missing_docs)]
pub struct MediaPlayerPayload {
    pub has_session: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub source_app: String,
    pub position_text: String,
    pub duration_text: String,
    pub progress_fraction: f32,
    pub is_playing: bool,
    /// Base64-encoded thumbnail (`data:image/...` suffix omitted; the UI
    /// attaches the appropriate prefix). `None` when no art is available.
    pub thumbnail_base64: Option<String>,
}
