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
    /// Jyotish (Vedic panchanga) widget.
    Jyotish(crate::widget::payloads::JyotishPayload),
    /// Clock / world-clocks widget.
    Clock(crate::widget::payloads::ClockPayload),
    /// System indicators widget.
    SystemIndicators(crate::widget::payloads::SystemPayload),
    /// Processes (Task Manager) widget.
    Processes(crate::widget::payloads::ProcessesPayload),
    /// Quick calculator widget.
    Calculator(crate::widget::payloads::CalculatorPayload),
    /// Tabbed notes / scratchpad.
    Notes(crate::widget::payloads::NotesPayload),
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
    /// Recent files widget.
    RecentFiles(crate::widget::payloads::RecentFilesPayload),
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
    /// Panes in the active tab (fractional layout + per-pane grid).
    pub panes: Vec<TerminalPanePayload>,
    /// Draggable split dividers in the active tab.
    pub dividers: Vec<TerminalDividerPayload>,
}

/// One pane in the active tab's split tree.
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalPanePayload {
    /// Backing PTY session id (UUID string).
    pub session_id: String,
    /// Left edge as a fraction of the tab viewport (`0.0..=1.0`).
    pub left: f32,
    /// Top edge.
    pub top: f32,
    /// Right edge.
    pub right: f32,
    /// Bottom edge.
    pub bottom: f32,
    /// Whether this pane has keyboard focus.
    pub is_focused: bool,
    /// Whether the pane close affordance should be shown.
    pub show_close: bool,
    /// Grid columns for this pane.
    pub cols: u16,
    /// Grid rows.
    pub rows: u16,
    /// Cell data in row-major order.
    pub cells: Vec<TerminalPayloadCell>,
    /// Cursor column.
    pub cursor_col: u16,
    /// Cursor row.
    pub cursor_row: u16,
    /// Cursor visibility.
    pub cursor_visible: bool,
}

/// Draggable divider between two terminal panes.
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalDividerPayload {
    /// Session in the left / top half.
    pub first_session_id: String,
    /// Session in the right / bottom half.
    pub second_session_id: String,
    /// `true` when the split is horizontal (vertical bar divider).
    pub horizontal: bool,
    /// Hit-strip left edge (fraction of viewport).
    pub left: f32,
    /// Hit-strip top edge.
    pub top: f32,
    /// Hit-strip right edge.
    pub right: f32,
    /// Hit-strip bottom edge.
    pub bottom: f32,
    /// Parent split region left edge.
    pub parent_left: f32,
    /// Parent split region top edge.
    pub parent_top: f32,
    /// Parent split region right edge.
    pub parent_right: f32,
    /// Parent split region bottom edge.
    pub parent_bottom: f32,
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
