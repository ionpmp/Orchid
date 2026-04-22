//! Payload wrapping [`orchid_viewers::ViewerSnapshot`].

/// Viewer-widget payload.
#[derive(Debug, Clone)]
pub struct ViewerPayload {
    /// Wrapped viewer snapshot.
    pub snapshot: orchid_viewers::ViewerSnapshot,
}
