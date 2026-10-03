//! Payload for the shared agent conversation.

/// One transcript line for the Agent panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentLine {
    /// Localized role label.
    pub role: String,
    /// Turn text. Tool results are clipped for the panel.
    pub text: String,
}

/// Render payload for the Agent widget.
///
/// The transcript is process-wide. Every Agent instance shows the same lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPayload {
    /// Oldest first.
    pub lines: Vec<AgentLine>,
    /// Absolute path waiting for confirmation. Empty when there is none.
    pub pending_path: String,
    /// Start of the proposed file text.
    pub pending_preview: String,
    /// Empty, a localized status, or a short error.
    pub status: String,
}
