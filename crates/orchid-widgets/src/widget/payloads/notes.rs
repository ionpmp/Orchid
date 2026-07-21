//! Payload for the notes / scratchpad widget.

#![allow(missing_docs)]

/// One tab row for the notes UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotesTabRow {
    pub id: String,
    pub title: String,
    pub is_active: bool,
}

/// Render payload for the notes widget.
#[derive(Debug, Clone)]
pub struct NotesPayload {
    pub tabs: Vec<NotesTabRow>,
    pub active_index: i32,
    pub title: String,
    pub body: String,
    pub font_size: i32,
    pub word_wrap: bool,
    pub mono_font: bool,
    pub show_status_bar: bool,
    pub char_count: i32,
    pub word_count: i32,
    pub line_count: i32,
    /// Bumped when find selects a match; UI applies cursor/anchor.
    pub find_gen: i32,
    pub find_cursor: i32,
    pub find_anchor: i32,
}
