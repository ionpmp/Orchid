//! Payload wrapping [`orchid_viewers::ViewerSnapshot`].

/// Viewer-widget payload.
#[derive(Debug, Clone)]
pub struct ViewerPayload {
    /// Wrapped viewer snapshot.
    pub snapshot: orchid_viewers::ViewerSnapshot,
    /// Show the unlock dialog for an encrypted `.orchid`.
    pub passphrase_prompt: bool,
    /// Localized or raw unlock error shown under the dialog (`""` when none).
    pub passphrase_error: String,
}
