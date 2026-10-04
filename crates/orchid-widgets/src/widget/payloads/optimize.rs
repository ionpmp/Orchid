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
    /// The stored value is not the Windows fallback for this row.
    pub changed: bool,
    /// Explorer or the taskbar reads this after a restart.
    pub needs_restart: bool,
    /// When set, the UI shows this instead of translating [`Self::title_key`].
    pub title_text: String,
    /// When set, the UI shows this instead of translating [`Self::detail_key`].
    pub detail_text: String,
    /// `false` hides the switch. Startup rows that cannot be changed use this.
    pub can_toggle: bool,
}

/// Render payload for the Optimize widget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizePayload {
    pub tab: i32,
    pub rows: Vec<OptimizeRow>,
    /// Current search text. Filtering happens in the widget.
    pub query: String,
    /// Fluent key for the status line. Empty when there is nothing to say.
    pub status_key: String,
    /// This build cannot change Windows settings.
    pub unsupported: bool,
}
