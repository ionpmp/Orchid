//! Cell / line / snapshot types used by the emulator and the UI renderer.

use std::sync::Arc;

use bitflags::bitflags;

use crate::emulator::color::CellColor;
use crate::emulator::cursor::CursorState;

bitflags! {
    /// Style flags on a [`Cell`].
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct CellFlags: u16 {
        /// Bold weight.
        const BOLD              = 1 << 0;
        /// Italic.
        const ITALIC            = 1 << 1;
        /// Underline.
        const UNDERLINE         = 1 << 2;
        /// Strike-through.
        const STRIKETHROUGH     = 1 << 3;
        /// Swap fg / bg.
        const INVERSE           = 1 << 4;
        /// Hidden (space rendered with fg = bg).
        const HIDDEN            = 1 << 5;
        /// Dim.
        const DIM               = 1 << 6;
        /// Blink.
        const BLINK             = 1 << 7;
        /// This cell occupies two columns (wide char).
        const WIDE_CHAR         = 1 << 8;
        /// Spacer cell that follows a wide char.
        const WIDE_CHAR_SPACER  = 1 << 9;
    }
}

/// A single grid cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// Character. `' '` for empty cells.
    pub ch: char,
    /// Foreground colour.
    pub fg: CellColor,
    /// Background colour.
    pub bg: CellColor,
    /// Style flags.
    pub flags: CellFlags,
}

impl Cell {
    /// Empty space with default fg/bg, no flags.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            ch: ' ',
            fg: CellColor::Default,
            bg: CellColor::Default,
            flags: CellFlags::empty(),
        }
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::empty()
    }
}

/// One line in the grid snapshot.
///
/// `cells` is an [`Arc`] so unchanged rows are refcount-cloned instead of
/// deep-copied on every [`crate::TerminalEmulator::snapshot`].
#[derive(Debug, Clone)]
pub struct GridLine {
    /// Row (0 = top of visible area; negatives index into scrollback).
    pub line_number: i64,
    /// Cells from left to right.
    pub cells: Arc<[Cell]>,
}

/// Snapshot of the visible terminal grid at a point in time.
#[derive(Debug, Clone)]
pub struct GridSnapshot {
    /// Columns in the grid.
    pub cols: u16,
    /// Rows in the grid (visible area).
    pub rows: u16,
    /// Number of scrollback lines that precede the visible area.
    pub scrollback_offset: usize,
    /// Total scrollback lines currently retained.
    pub scrollback_total: usize,
    /// Lines (top-to-bottom).
    pub lines: Vec<GridLine>,
    /// Cursor state at snapshot time.
    pub cursor: CursorState,
    /// Monotonic generation bumped whenever grid contents / viewport change.
    pub content_generation: u64,
    /// Visible row indices that changed since the previous snapshot.
    ///
    /// Empty when [`Self::full_redraw`] is set (caller must repaint everything)
    /// or when nothing changed (generation still advances for cursor-only
    /// updates that also set dirty on the cursor row).
    pub dirty_lines: Vec<u16>,
    /// When true the entire visible grid must be re-rasterized (resize,
    /// viewport scroll, first frame after open).
    pub full_redraw: bool,
}

/// Where to position the viewport after a scroll operation.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollPosition {
    Top,
    Bottom,
    Line(i64),
}

/// Build a blank row of `cols` empty cells, shared via [`Arc`].
#[must_use]
pub fn empty_row(cols: usize) -> Arc<[Cell]> {
    Arc::from(vec![Cell::empty(); cols])
}
