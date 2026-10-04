//! Payload for the Windows Optimize widget.

#![allow(missing_docs)]

/// One setting row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizeRow {
    pub id: String,
    pub title_key: String,
    pub detail_key: String,
    pub option_keys: Vec<String>,
    /// Selected plan. For a switch, `1` is on.
    pub selected: u8,
    pub needs_admin: bool,
}

/// Render payload for the Optimize widget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizePayload {
    pub tab: i32,
    pub rows: Vec<OptimizeRow>,
    /// Fluent key for the status line. Empty when there is nothing to say.
    pub status_key: String,
    /// This build cannot change Windows settings.
    pub unsupported: bool,
}
