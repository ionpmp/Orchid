//! Optional grid backed by `alacritty_terminal`.
//!
//! Bytes go through Alacritty's parser. Visible cells are copied into
//! [`GridSnapshot`] so the existing raster stays unchanged. This grid does
//! not draw Sixel or Kitty images, does not answer OSC 52, and does not
//! report an OSC 7 directory.

use std::sync::Arc;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config as TermConfig, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, CursorShape, Processor};
use parking_lot::Mutex;

use crate::emulator::{
    Cell, CellColor, CellFlags, CursorState, CursorStyle, GridLine, GridSnapshot,
    DEFAULT_SCROLLBACK,
};
use crate::error::Result;

struct GridSize {
    columns: usize,
    screen_lines: usize,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.screen_lines
    }

    fn screen_lines(&self) -> usize {
        self.screen_lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

struct Bridge {
    title: Arc<Mutex<String>>,
    replies: Arc<Mutex<Vec<u8>>>,
    size: Arc<Mutex<WindowSize>>,
}

impl EventListener for Bridge {
    fn send_event(&self, event: Event) {
        match event {
            Event::Title(title) => *self.title.lock() = title,
            Event::ResetTitle => self.title.lock().clear(),
            Event::PtyWrite(text) => self.replies.lock().extend(text.into_bytes()),
            Event::TextAreaSizeRequest(formatter) => {
                let size = *self.size.lock();
                let text = formatter(size);
                self.replies.lock().extend(text.into_bytes());
            }
            Event::MouseCursorDirty
            | Event::ClipboardStore(_, _)
            | Event::ClipboardLoad(_, _)
            | Event::ColorRequest(_, _)
            | Event::CursorBlinkingChange
            | Event::Wakeup
            | Event::Bell
            | Event::Exit
            | Event::ChildExit(_) => {}
        }
    }
}

struct State {
    term: Term<Bridge>,
    parser: Processor,
    generation: u64,
}

/// Alacritty grid mapped onto Orchid's snapshot.
pub struct AlacrittyGrid {
    inner: Mutex<State>,
    title: Arc<Mutex<String>>,
    replies: Arc<Mutex<Vec<u8>>>,
    size: Arc<Mutex<WindowSize>>,
}

impl std::fmt::Debug for AlacrittyGrid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlacrittyGrid").finish_non_exhaustive()
    }
}

impl AlacrittyGrid {
    /// Blank grid. Columns and rows are at least 1.
    #[must_use]
    pub fn new(cols: u16, rows: u16) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        let title = Arc::new(Mutex::new(String::new()));
        let replies = Arc::new(Mutex::new(Vec::new()));
        let size = Arc::new(Mutex::new(WindowSize {
            num_lines: rows,
            num_cols: cols,
            cell_width: 1,
            cell_height: 1,
        }));
        let bridge = Bridge {
            title: Arc::clone(&title),
            replies: Arc::clone(&replies),
            size: Arc::clone(&size),
        };
        let mut config = TermConfig::default();
        config.scrolling_history = DEFAULT_SCROLLBACK;
        let dims = GridSize {
            columns: usize::from(cols),
            screen_lines: usize::from(rows),
        };
        let term = Term::new(config, &dims, bridge);
        Self {
            inner: Mutex::new(State {
                term,
                parser: Processor::new(),
                generation: 0,
            }),
            title,
            replies,
            size,
        }
    }

    /// Parse PTY bytes. Replies collected during the parse are returned.
    pub fn feed(&self, bytes: &[u8]) -> Vec<u8> {
        let mut state = self.inner.lock();
        let state = &mut *state;
        state.parser.advance(&mut state.term, bytes);
        state.generation = state.generation.wrapping_add(1);
        std::mem::take(&mut *self.replies.lock())
    }

    /// Resize the visible area. A zero axis becomes 1.
    ///
    /// # Errors
    ///
    /// Always `Ok`. The signature matches the built-in grid.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let cols = cols.max(1);
        let rows = rows.max(1);
        let mut state = self.inner.lock();
        state.term.resize(GridSize {
            columns: usize::from(cols),
            screen_lines: usize::from(rows),
        });
        state.generation = state.generation.wrapping_add(1);
        drop(state);
        let mut size = self.size.lock();
        size.num_cols = cols;
        size.num_lines = rows;
        Ok(())
    }

    /// Remember the cell size used by text-area size replies.
    pub fn set_cell_px(&self, width: u16, height: u16) {
        let mut size = self.size.lock();
        size.cell_width = width.max(1);
        size.cell_height = height.max(1);
    }

    /// Copy the visible Alacritty grid into a snapshot.
    ///
    /// Every snapshot asks for a full redraw. Inline images are empty.
    #[must_use]
    pub fn snapshot(&self) -> GridSnapshot {
        let state = self.inner.lock();
        let cols = state.term.columns();
        let rows = state.term.screen_lines();
        let history = state.term.grid().history_size();
        let show_cursor = state.term.mode().contains(TermMode::SHOW_CURSOR);
        let blinking = state.term.cursor_style().blinking;
        let content = state.term.renderable_content();
        let offset = content.display_offset as i32;
        let mut cells = vec![vec![Cell::empty(); cols]; rows];
        for indexed in content.display_iter {
            let row = indexed.point.line.0 + offset;
            let col = indexed.point.column.0;
            if row < 0 || col >= cols {
                continue;
            }
            let row = row as usize;
            if row >= rows {
                continue;
            }
            cells[row][col] = map_cell(&indexed.cell);
        }
        let point = content.cursor.point;
        let cursor_row = point.line.0 + offset;
        let on_screen = cursor_row >= 0 && (cursor_row as usize) < rows;
        let mut visible = on_screen && show_cursor;
        let style = match content.cursor.shape {
            CursorShape::Underline => CursorStyle::Underline,
            CursorShape::Beam => CursorStyle::Bar,
            CursorShape::Hidden => {
                visible = false;
                CursorStyle::Block
            }
            CursorShape::Block | CursorShape::HollowBlock => CursorStyle::Block,
        };
        let lines = cells
            .into_iter()
            .enumerate()
            .map(|(i, row)| GridLine {
                line_number: i as i64,
                cells: Arc::from(row),
            })
            .collect();
        GridSnapshot {
            cols: cols as u16,
            rows: rows as u16,
            scrollback_offset: content.display_offset,
            scrollback_total: history,
            lines,
            cursor: CursorState {
                col: point.column.0 as u16,
                row: if cursor_row < 0 { 0 } else { cursor_row as u16 },
                style,
                visible,
                blinking,
            },
            content_generation: state.generation,
            dirty_lines: Vec::new(),
            full_redraw: true,
            images: Vec::new(),
        }
    }

    /// Last OSC title. Empty until the shell sets one.
    #[must_use]
    pub fn title(&self) -> String {
        self.title.lock().clone()
    }
}

fn map_cell(cell: &alacritty_terminal::term::cell::Cell) -> Cell {
    Cell {
        ch: cell.c,
        fg: map_color(cell.fg),
        bg: map_color(cell.bg),
        flags: map_flags(cell.flags),
    }
}

fn map_color(color: Color) -> CellColor {
    match color {
        Color::Spec(rgb) => CellColor::Rgb(rgb.r, rgb.g, rgb.b),
        Color::Indexed(index) => CellColor::Indexed(index),
        Color::Named(named) => {
            let index = named as u16;
            if index < 16 {
                CellColor::Indexed(index as u8)
            } else {
                CellColor::Default
            }
        }
    }
}

fn map_flags(flags: Flags) -> CellFlags {
    let mut out = CellFlags::empty();
    if flags.contains(Flags::BOLD) {
        out.insert(CellFlags::BOLD);
    }
    if flags.contains(Flags::ITALIC) {
        out.insert(CellFlags::ITALIC);
    }
    if flags.intersects(Flags::ALL_UNDERLINES) {
        out.insert(CellFlags::UNDERLINE);
    }
    if flags.contains(Flags::STRIKEOUT) {
        out.insert(CellFlags::STRIKETHROUGH);
    }
    if flags.contains(Flags::INVERSE) {
        out.insert(CellFlags::INVERSE);
    }
    if flags.contains(Flags::HIDDEN) {
        out.insert(CellFlags::HIDDEN);
    }
    if flags.contains(Flags::DIM) {
        out.insert(CellFlags::DIM);
    }
    if flags.contains(Flags::WIDE_CHAR) {
        out.insert(CellFlags::WIDE_CHAR);
    }
    if flags.contains(Flags::WIDE_CHAR_SPACER) {
        out.insert(CellFlags::WIDE_CHAR_SPACER);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bold_and_title_come_from_alacritty() {
        let grid = AlacrittyGrid::new(20, 6);
        let _ = grid.feed(b"\x1b[1mB");
        let snap = grid.snapshot();
        assert_eq!(snap.lines[0].cells[0].ch, 'B');
        assert!(snap.lines[0].cells[0].flags.contains(CellFlags::BOLD));
        let _ = grid.feed(b"\x1b]0;Hello\x07");
        assert_eq!(grid.title(), "Hello");
        assert!(snap.images.is_empty());
    }
}
