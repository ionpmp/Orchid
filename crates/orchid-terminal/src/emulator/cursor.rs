//! Cursor state snapshot.

/// On-screen cursor state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorState {
    /// Column (`0..cols`).
    pub col: u16,
    /// Row (`0..rows`).
    pub row: u16,
    /// Visual style (block / underline / bar).
    pub style: CursorStyle,
    /// Whether the cursor should be drawn at all (DECTCEM).
    pub visible: bool,
    /// Whether the cursor should blink.
    pub blinking: bool,
}

impl Default for CursorState {
    fn default() -> Self {
        Self {
            col: 0,
            row: 0,
            style: CursorStyle::Block,
            visible: true,
            blinking: true,
        }
    }
}

/// Visual flavour of the cursor.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    Block,
    Underline,
    Bar,
}
