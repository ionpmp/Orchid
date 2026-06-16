//! Renderer-agnostic widget snapshot consumed by the UI layer.

use uuid::Uuid;

/// Live-render status of a widget.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetStatus {
    Loading,
    Ready,
    Stale,
    Error,
}

/// Widget snapshot pushed through the renderer.
#[derive(Debug, Clone)]
pub struct WidgetSnapshot {
    /// Widget instance id.
    pub instance_id: Uuid,
    /// Widget type identifier (`"terminal"`, `"weather"`, ...).
    pub widget_type: &'static str,
    /// Short title shown in the widget frame header.
    pub title: String,
    /// Current render status.
    pub status: WidgetStatus,
    /// Type-specific renderable payload.
    pub payload: WidgetPayload,
}

/// Type-erased payload. Every built-in widget variant has its own branch so
/// UI dispatch stays exhaustive without runtime downcasting.
#[derive(Debug, Clone)]
pub enum WidgetPayload {
    /// Placeholder used while the widget is still loading.
    Empty,
    /// A vertical list of text rows.
    Text {
        /// Rows top-to-bottom.
        lines: Vec<String>,
    },
    /// Generic key-value list.
    KeyValueList {
        /// Entries in display order.
        entries: Vec<(String, String)>,
    },
    /// Terminal grid and cursor.
    Terminal(TerminalPayload),
    /// Weather widget.
    Weather(crate::widget::payloads::WeatherPayload),
    /// Moon widget.
    Moon(crate::widget::payloads::MoonPayload),
    /// System indicators widget.
    SystemIndicators(crate::widget::payloads::SystemPayload),
    /// RSS feed widget.
    RssFeed(crate::widget::payloads::RssPayload),
    /// Universal search widget.
    UniversalSearch(crate::widget::payloads::UniversalSearchPayload),
    /// Media player widget.
    MediaPlayer(crate::widget::payloads::MediaPlayerPayload),
    /// Password manager widget.
    PasswordManager(crate::widget::payloads::PasswordManagerPayload),
    /// Content viewer widget.
    Viewer(crate::widget::payloads::ViewerPayload),
    /// File manager widget.
    FileManager(crate::widget::payloads::FileManagerPayload),
}

/// Terminal-specific payload carried inside [`WidgetPayload::Terminal`].
#[derive(Debug, Clone)]
pub struct TerminalPayload {
    /// Number of columns in the grid.
    pub cols: u16,
    /// Number of rows in the grid.
    pub rows: u16,
    /// Cells in row-major order (`cells[row * cols + col]`).
    pub cells: Vec<TerminalPayloadCell>,
    /// Zero-based cursor column.
    pub cursor_col: u16,
    /// Zero-based cursor row.
    pub cursor_row: u16,
    /// Whether the cursor should be drawn.
    pub cursor_visible: bool,
    /// Tab strip entries for the Slint terminal chrome.
    pub tabs: Vec<TerminalTabPayload>,
    /// Active tab index in [`Self::tabs`].
    pub active_tab: u32,
}

/// One tab in the terminal widget tab strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalTabPayload {
    /// Stable tab id (UUID string).
    pub tab_id: String,
    /// Display title (shell name or OSC title).
    pub title: String,
    /// Whether this tab is currently selected.
    pub is_active: bool,
}

/// Single terminal cell with resolved colours, ready to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalPayloadCell {
    /// Visible character.
    pub ch: char,
    /// RGBA foreground.
    pub fg_rgba: [u8; 4],
    /// RGBA background.
    pub bg_rgba: [u8; 4],
    /// Bold flag.
    pub bold: bool,
    /// Italic flag.
    pub italic: bool,
    /// Underline flag.
    pub underline: bool,
}
