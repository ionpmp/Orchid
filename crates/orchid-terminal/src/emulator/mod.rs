//! VT / ANSI emulator.
//!
//! This crate intentionally does **not** depend on `alacritty_terminal` for
//! its emulator state: we maintain a minimal grid directly on top of the
//! [`vte`] parser. That keeps the dependency graph small, the API
//! focused on what Orchid needs, and the behaviour easy to audit. The
//! trade-off is that advanced features (vi mode, regex search over the
//! ANSI-parsed scrollback, and full xterm control-sequence coverage) are
//! deferred to a follow-up task — see the crate README.

pub mod color;
pub mod cursor;
pub mod grid;
pub mod selection;

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use uuid::Uuid;
use vte::{Params, Perform};

use crate::error::{Result, TerminalError};
use crate::events::{
    TerminalBell, TerminalClipboardWrite, TerminalCwdChanged, TerminalTitleChanged,
};
use crate::search::SearchMatch;

pub use color::{resolve_color, xterm_256_color, CellColor, ColorRole, Rgba, TerminalPalette};
pub use cursor::{CursorState, CursorStyle};
pub use grid::{empty_row, Cell, CellFlags, GridLine, GridSnapshot, ScrollPosition};
pub use selection::{GridPoint, Selection};

/// Default retained scrollback lines.
pub const DEFAULT_SCROLLBACK: usize = 5_000;

/// VT / ANSI emulator over an in-memory grid.
pub struct TerminalEmulator {
    inner: Arc<Mutex<EmulatorState>>,
    bus: Arc<orchid_core::EventBus>,
    session_id: Uuid,
}

impl std::fmt::Debug for TerminalEmulator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalEmulator")
            .field("session_id", &self.session_id)
            .finish_non_exhaustive()
    }
}

impl TerminalEmulator {
    /// Build a fresh emulator backed by a blank `cols × rows` grid.
    #[must_use]
    pub fn new(
        cols: u16,
        rows: u16,
        scrollback: usize,
        bus: Arc<orchid_core::EventBus>,
        session_id: Uuid,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(EmulatorState::new(
                cols.max(1),
                rows.max(1),
                scrollback,
            ))),
            bus,
            session_id,
        }
    }

    /// Feed bytes from the PTY into the emulator. Any response bytes the
    /// emulator wants to write back (DA1, DSR, ...) are returned; the caller
    /// must forward them to the PTY input.
    ///
    /// [`EmulatorState::content_generation`] is bumped only when a sequence
    /// mutates the visible grid, cursor, or viewport — not for pure queries
    /// (e.g. DSR cursor-position reports) or incomplete escape fragments.
    pub fn feed(&self, bytes: &[u8]) -> Vec<u8> {
        let mut state = self.inner.lock();
        let mut parser = state.parser.take().unwrap_or_default();
        let mut handler = Handler {
            state: &mut state,
            responses: Vec::new(),
            bus: &self.bus,
            session_id: self.session_id,
        };
        parser.advance(&mut handler, bytes);
        let responses = handler.responses;
        state.parser = Some(parser);
        responses
    }

    /// Resize the grid to `cols × rows`. No-op when the size is unchanged.
    ///
    /// # Errors
    ///
    /// [`TerminalError::InvalidResize`] on zero-valued dimensions.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        if cols == 0 || rows == 0 {
            return Err(TerminalError::InvalidResize { cols, rows });
        }
        self.inner.lock().resize(cols, rows);
        Ok(())
    }

    /// Point-in-time snapshot of the visible grid for rendering.
    #[must_use]
    pub fn snapshot(&self) -> GridSnapshot {
        self.inner.lock().snapshot()
    }

    /// Cheap cursor-only snapshot.
    #[must_use]
    pub fn cursor(&self) -> CursorState {
        self.inner.lock().cursor
    }

    /// Replace the active selection.
    pub fn set_selection(&self, sel: Selection) {
        self.inner.lock().selection = Some(sel);
    }

    /// Clear the active selection.
    pub fn clear_selection(&self) {
        self.inner.lock().selection = None;
    }

    /// Extract the text currently selected.
    #[must_use]
    pub fn selected_text(&self) -> String {
        self.inner.lock().selected_text()
    }

    /// Number of scrollback lines currently retained.
    #[must_use]
    pub fn scrollback_lines(&self) -> usize {
        self.inner.lock().scrollback.len()
    }

    /// Jump the viewport to the requested position.
    pub fn scroll_to(&self, line: ScrollPosition) {
        self.inner.lock().scroll_to(line);
    }

    /// Relative scroll. Positive delta scrolls down; negative scrolls up
    /// into scrollback.
    pub fn scroll_by(&self, lines: i32) {
        self.inner.lock().scroll_by(lines);
    }

    /// Most recent window title set via OSC 0 / 2.
    #[must_use]
    pub fn title(&self) -> String {
        self.inner.lock().title.clone()
    }

    /// Most recent working directory set via OSC 7.
    #[must_use]
    pub fn working_directory(&self) -> Option<PathBuf> {
        self.inner.lock().cwd.clone()
    }

    /// Substring search across visible + scrollback.
    #[must_use]
    pub fn search_in_scrollback(&self, query: &str, case_sensitive: bool) -> Vec<SearchMatch> {
        self.inner
            .lock()
            .search_in_scrollback(query, case_sensitive)
    }
}

// ---------------------------------------------------------------------------
// Emulator state — the thing the lock actually protects.
// ---------------------------------------------------------------------------

struct EmulatorState {
    cols: u16,
    rows: u16,
    scrollback_cap: usize,
    /// Visible rows as shared slices — snapshot clones are refcount bumps.
    /// Row-major flat storage is obtained by concatenating these; scroll uses
    /// `rotate_left` / `rotate_right` on the region instead of `Vec::remove`/`insert`.
    grid: Vec<Arc<[Cell]>>,
    /// Oldest-first scrollback (lines that have been pushed off the top).
    scrollback: std::collections::VecDeque<Arc<[Cell]>>,
    /// Viewport offset: 0 = live tail; positive values pin the view further
    /// into scrollback.
    viewport_offset: usize,
    cursor: CursorState,
    /// Current SGR state applied to newly emitted cells.
    current_fg: CellColor,
    current_bg: CellColor,
    current_flags: CellFlags,
    /// Reusable parser (owned so we can take / return to avoid borrow issues).
    parser: Option<vte::Parser>,
    title: String,
    cwd: Option<PathBuf>,
    selection: Option<Selection>,
    /// Scrollable region (DECSTBM). Zero-indexed, inclusive bounds.
    scroll_top: u16,
    scroll_bottom: u16,
    /// Bumped when grid / cursor / viewport change so UI equality can skip cell walks.
    content_generation: u64,
    /// Per-visible-row dirty flags accumulated since the last snapshot.
    dirty_lines: Vec<bool>,
    /// Force a full raster on the next snapshot (resize, viewport scroll, …).
    full_redraw: bool,
}

impl EmulatorState {
    fn new(cols: u16, rows: u16, scrollback_cap: usize) -> Self {
        let cols_usize = cols as usize;
        let grid = (0..rows).map(|_| empty_row(cols_usize)).collect();
        Self {
            cols,
            rows,
            scrollback_cap,
            grid,
            scrollback: std::collections::VecDeque::with_capacity(scrollback_cap.min(1024)),
            viewport_offset: 0,
            cursor: CursorState::default(),
            current_fg: CellColor::Default,
            current_bg: CellColor::Default,
            current_flags: CellFlags::empty(),
            parser: Some(vte::Parser::new()),
            title: String::new(),
            cwd: None,
            selection: None,
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
            content_generation: 1,
            dirty_lines: vec![true; rows as usize],
            full_redraw: true,
        }
    }

    fn bump_generation(&mut self) {
        self.content_generation = self.content_generation.wrapping_add(1).max(1);
    }

    fn mark_dirty_row(&mut self, row: u16) {
        if let Some(flag) = self.dirty_lines.get_mut(row as usize) {
            *flag = true;
        }
    }

    fn mark_dirty_range(&mut self, from: u16, to_inclusive: u16) {
        let end = (to_inclusive as usize + 1).min(self.dirty_lines.len());
        for flag in &mut self.dirty_lines[(from as usize).min(end)..end] {
            *flag = true;
        }
    }

    fn mark_full_redraw(&mut self) {
        self.full_redraw = true;
        self.dirty_lines.fill(true);
    }

    /// Copy-on-write mutate a single visible row.
    fn with_row_mut(&mut self, row: usize, f: impl FnOnce(&mut [Cell])) {
        let Some(existing) = self.grid.get(row).cloned() else {
            return;
        };
        let mut owned = existing.as_ref().to_vec();
        f(&mut owned);
        self.grid[row] = Arc::from(owned);
        if let Some(flag) = self.dirty_lines.get_mut(row) {
            *flag = true;
        }
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        if cols == self.cols && rows == self.rows {
            return;
        }
        self.bump_generation();
        let cols_usize = cols as usize;
        // Resize each existing row (COW).
        for line in &mut self.grid {
            let mut owned = line.as_ref().to_vec();
            if owned.len() < cols_usize {
                owned.resize(cols_usize, Cell::empty());
            } else if owned.len() > cols_usize {
                owned.truncate(cols_usize);
            }
            *line = Arc::from(owned);
        }
        // Add or drop lines.
        if rows as usize > self.grid.len() {
            while (self.grid.len() as u16) < rows {
                self.grid.push(empty_row(cols_usize));
            }
        } else {
            while (self.grid.len() as u16) > rows {
                let dropped = self.grid.remove(0);
                self.push_scrollback(dropped);
            }
        }
        self.cols = cols;
        self.rows = rows;
        self.scroll_top = 0;
        self.scroll_bottom = rows.saturating_sub(1);
        self.cursor.col = self.cursor.col.min(cols.saturating_sub(1));
        self.cursor.row = self.cursor.row.min(rows.saturating_sub(1));
        self.dirty_lines = vec![true; rows as usize];
        self.mark_full_redraw();
    }

    fn push_scrollback(&mut self, line: Arc<[Cell]>) {
        if self.scrollback_cap == 0 {
            return;
        }
        if self.scrollback.len() >= self.scrollback_cap {
            self.scrollback.pop_front();
        }
        self.scrollback.push_back(line);
    }

    fn snapshot(&mut self) -> GridSnapshot {
        let mut lines = Vec::with_capacity(self.rows as usize);
        // If viewport_offset > 0, show scrollback-ending-at-offset instead
        // of the live grid.
        if self.viewport_offset == 0 {
            for (i, row) in self.grid.iter().enumerate() {
                lines.push(GridLine {
                    line_number: i as i64,
                    cells: Arc::clone(row),
                });
            }
        } else {
            // Take the last `rows` lines of (scrollback + grid) ending at
            // viewport_offset from the tail.
            let total = self.scrollback.len() + self.grid.len();
            let bottom = total.saturating_sub(self.viewport_offset);
            let top = bottom.saturating_sub(self.rows as usize);
            for abs in top..bottom {
                let row = if abs < self.scrollback.len() {
                    Arc::clone(&self.scrollback[abs])
                } else {
                    Arc::clone(&self.grid[abs - self.scrollback.len()])
                };
                let line_number = (abs as i64) - (self.scrollback.len() as i64);
                lines.push(GridLine {
                    line_number,
                    cells: row,
                });
            }
        }

        let full_redraw = self.full_redraw || self.viewport_offset != 0;
        let dirty_lines = if full_redraw {
            (0..self.rows).collect()
        } else {
            self.dirty_lines
                .iter()
                .enumerate()
                .filter_map(|(i, d)| d.then_some(i as u16))
                .collect()
        };
        self.dirty_lines.fill(false);
        self.full_redraw = false;

        GridSnapshot {
            cols: self.cols,
            rows: self.rows,
            scrollback_offset: self.viewport_offset,
            scrollback_total: self.scrollback.len(),
            lines,
            cursor: self.cursor,
            content_generation: self.content_generation,
            dirty_lines,
            full_redraw,
        }
    }

    fn scroll_to(&mut self, pos: ScrollPosition) {
        self.bump_generation();
        self.mark_full_redraw();
        match pos {
            ScrollPosition::Top => {
                self.viewport_offset = self.scrollback.len();
            }
            ScrollPosition::Bottom => {
                self.viewport_offset = 0;
            }
            ScrollPosition::Line(_) => {
                // Placeholder: absolute line scrolling is pending alongside
                // a scrollback search UI.
            }
        }
    }

    fn scroll_by(&mut self, lines: i32) {
        if lines != 0 {
            self.bump_generation();
            self.mark_full_redraw();
        }
        if lines > 0 {
            self.viewport_offset = self.viewport_offset.saturating_sub(lines as usize);
        } else {
            let add = (-lines) as usize;
            let cap = self.scrollback.len();
            self.viewport_offset = (self.viewport_offset + add).min(cap);
        }
    }

    fn selected_text(&self) -> String {
        let Some(sel) = &self.selection else {
            return String::new();
        };
        match sel {
            Selection::Line { row } => self.row_text(*row),
            Selection::Linear { start, end } => {
                let (lo, hi) = if (start.row, start.col) <= (end.row, end.col) {
                    (*start, *end)
                } else {
                    (*end, *start)
                };
                let mut out = String::new();
                if lo.row == hi.row {
                    let text = self.row_text(lo.row);
                    let chars: Vec<char> = text.chars().collect();
                    let a = (lo.col as usize).min(chars.len());
                    let b = (hi.col as usize).min(chars.len());
                    out.extend(chars[a..b].iter());
                } else {
                    out.push_str(&self.row_text(lo.row));
                    out.push('\n');
                    for r in (lo.row + 1)..hi.row {
                        out.push_str(&self.row_text(r));
                        out.push('\n');
                    }
                    let text = self.row_text(hi.row);
                    let chars: Vec<char> = text.chars().collect();
                    let b = (hi.col as usize).min(chars.len());
                    out.extend(chars[..b].iter());
                }
                out
            }
            Selection::Block { start, end } => {
                let top = start.row.min(end.row);
                let bot = start.row.max(end.row);
                let left = start.col.min(end.col);
                let right = start.col.max(end.col);
                let mut out = String::new();
                for r in top..=bot {
                    let text = self.row_text(r);
                    let chars: Vec<char> = text.chars().collect();
                    let a = (left as usize).min(chars.len());
                    let b = (right as usize).min(chars.len());
                    out.extend(chars[a..b].iter());
                    out.push('\n');
                }
                out
            }
            Selection::Word { at } => {
                let text = self.row_text(at.row);
                let chars: Vec<char> = text.chars().collect();
                let pos = (at.col as usize).min(chars.len().saturating_sub(1));
                if chars.is_empty() {
                    return String::new();
                }
                let mut start = pos;
                while start > 0 && !chars[start - 1].is_whitespace() {
                    start -= 1;
                }
                let mut end = pos;
                while end < chars.len() && !chars[end].is_whitespace() {
                    end += 1;
                }
                chars[start..end].iter().collect()
            }
        }
    }

    fn row_text(&self, row: i64) -> String {
        if row < 0 {
            let idx = (self.scrollback.len() as i64 + row) as usize;
            let Some(line) = self.scrollback.get(idx) else {
                return String::new();
            };
            cells_to_string(line.as_ref())
        } else {
            let Some(line) = self.grid.get(row as usize) else {
                return String::new();
            };
            cells_to_string(line.as_ref())
        }
    }

    fn search_in_scrollback(&self, query: &str, case_sensitive: bool) -> Vec<SearchMatch> {
        if query.is_empty() {
            return Vec::new();
        }
        let needle_lower;
        let needle: &str = if case_sensitive {
            query
        } else {
            needle_lower = query.to_lowercase();
            needle_lower.as_str()
        };
        let mut out = Vec::new();
        // Scrollback first (negative line numbers).
        for (idx, line) in self.scrollback.iter().enumerate() {
            let hay = cells_to_string(line.as_ref());
            let hay = if case_sensitive {
                hay
            } else {
                hay.to_lowercase()
            };
            let line_no = (idx as i64) - (self.scrollback.len() as i64);
            push_matches(&mut out, &hay, needle, line_no);
        }
        for (idx, line) in self.grid.iter().enumerate() {
            let hay = cells_to_string(line.as_ref());
            let hay = if case_sensitive {
                hay
            } else {
                hay.to_lowercase()
            };
            push_matches(&mut out, &hay, needle, idx as i64);
        }
        out
    }

    // -----------------------------------------------------------------
    // VT helpers
    // -----------------------------------------------------------------

    fn write_char(&mut self, ch: char) {
        if self.cursor.col as usize >= self.cols as usize {
            self.carriage_return();
            self.line_feed();
        }
        let row = self.cursor.row as usize;
        let col = self.cursor.col as usize;
        let fg = self.current_fg;
        let bg = self.current_bg;
        let flags = self.current_flags;
        self.with_row_mut(row, |line| {
            if let Some(cell) = line.get_mut(col) {
                cell.ch = ch;
                cell.fg = fg;
                cell.bg = bg;
                cell.flags = flags;
            }
        });
        self.cursor.col = self.cursor.col.saturating_add(1);
        self.bump_generation();
    }

    fn line_feed(&mut self) {
        if self.cursor.row == self.scroll_bottom {
            // Scroll the region up by one line via rotate (O(rows) Arc moves,
            // not O(rows) heap reallocations from remove/insert).
            let top = self.scroll_top as usize;
            let bot = self.scroll_bottom as usize;
            if top == 0 {
                let scrolled = Arc::clone(&self.grid[0]);
                self.push_scrollback(scrolled);
            }
            if bot >= top && bot < self.grid.len() {
                self.grid[top..=bot].rotate_left(1);
                self.grid[bot] = empty_row(self.cols as usize);
                self.mark_dirty_range(self.scroll_top, self.scroll_bottom);
            }
        } else {
            let prev = self.cursor.row;
            self.cursor.row = self.cursor.row.saturating_add(1).min(self.rows - 1);
            self.mark_dirty_row(prev);
            self.mark_dirty_row(self.cursor.row);
        }
        self.bump_generation();
    }

    fn reverse_line_feed(&mut self) {
        if self.cursor.row == self.scroll_top {
            let top = self.scroll_top as usize;
            let bot = self.scroll_bottom as usize;
            if bot >= top && bot < self.grid.len() {
                self.grid[top..=bot].rotate_right(1);
                self.grid[top] = empty_row(self.cols as usize);
                self.mark_dirty_range(self.scroll_top, self.scroll_bottom);
            }
        } else {
            let prev = self.cursor.row;
            self.cursor.row = self.cursor.row.saturating_sub(1);
            self.mark_dirty_row(prev);
            self.mark_dirty_row(self.cursor.row);
        }
        self.bump_generation();
    }

    fn carriage_return(&mut self) {
        if self.cursor.col != 0 {
            self.mark_dirty_row(self.cursor.row);
            self.cursor.col = 0;
            self.bump_generation();
        }
    }

    fn backspace(&mut self) {
        let prev = self.cursor.col;
        self.cursor.col = self.cursor.col.saturating_sub(1);
        if self.cursor.col != prev {
            self.mark_dirty_row(self.cursor.row);
            self.bump_generation();
        }
    }

    fn tab(&mut self) {
        // Standard 8-column tab stops.
        let next = ((self.cursor.col / 8) + 1) * 8;
        let col = next.min(self.cols.saturating_sub(1));
        if col != self.cursor.col {
            self.cursor.col = col;
            self.mark_dirty_row(self.cursor.row);
            self.bump_generation();
        }
    }

    fn clear_row_range(&mut self, row: usize, from: usize, to: usize) {
        let cols = self.cols as usize;
        let end = to.min(cols);
        if from >= end {
            return;
        }
        self.with_row_mut(row, |line| {
            for cell in line.iter_mut().take(end).skip(from) {
                *cell = Cell::empty();
            }
        });
        self.bump_generation();
    }

    fn erase_in_line(&mut self, mode: u16) {
        let row = self.cursor.row as usize;
        let col = self.cursor.col as usize;
        let cols = self.cols as usize;
        match mode {
            0 => self.clear_row_range(row, col, cols),
            1 => self.clear_row_range(row, 0, col + 1),
            2 => self.clear_row_range(row, 0, cols),
            _ => {}
        }
    }

    fn erase_in_display(&mut self, mode: u16) {
        let cols = self.cols as usize;
        let cursor_row = self.cursor.row as usize;
        let cursor_col = self.cursor.col as usize;
        match mode {
            0 => {
                self.clear_row_range(cursor_row, cursor_col, cols);
                for r in cursor_row + 1..self.grid.len() {
                    self.clear_row_range(r, 0, cols);
                }
            }
            1 => {
                for r in 0..cursor_row {
                    self.clear_row_range(r, 0, cols);
                }
                self.clear_row_range(cursor_row, 0, cursor_col + 1);
            }
            2 | 3 => {
                for r in 0..self.grid.len() {
                    self.clear_row_range(r, 0, cols);
                }
            }
            _ => {}
        }
    }

    fn cursor_up(&mut self, n: u16) {
        let row = self.cursor.row.saturating_sub(n).max(self.scroll_top);
        if row != self.cursor.row {
            self.mark_dirty_row(self.cursor.row);
            self.cursor.row = row;
            self.mark_dirty_row(self.cursor.row);
            self.bump_generation();
        }
    }

    fn cursor_down(&mut self, n: u16) {
        let row = self
            .cursor
            .row
            .saturating_add(n)
            .min(self.scroll_bottom)
            .min(self.rows.saturating_sub(1));
        if row != self.cursor.row {
            self.mark_dirty_row(self.cursor.row);
            self.cursor.row = row;
            self.mark_dirty_row(self.cursor.row);
            self.bump_generation();
        }
    }

    fn cursor_right(&mut self, n: u16) {
        let col = self
            .cursor
            .col
            .saturating_add(n)
            .min(self.cols.saturating_sub(1));
        if col != self.cursor.col {
            self.mark_dirty_row(self.cursor.row);
            self.cursor.col = col;
            self.bump_generation();
        }
    }

    fn cursor_left(&mut self, n: u16) {
        let col = self.cursor.col.saturating_sub(n);
        if col != self.cursor.col {
            self.mark_dirty_row(self.cursor.row);
            self.cursor.col = col;
            self.bump_generation();
        }
    }

    fn cursor_position(&mut self, row: u16, col: u16) {
        let row = row.saturating_sub(1).min(self.rows.saturating_sub(1));
        let col = col.saturating_sub(1).min(self.cols.saturating_sub(1));
        if row != self.cursor.row || col != self.cursor.col {
            self.mark_dirty_row(self.cursor.row);
            self.cursor.row = row;
            self.cursor.col = col;
            self.mark_dirty_row(self.cursor.row);
            self.bump_generation();
        }
    }

    fn apply_sgr(&mut self, params: &Params) {
        let mut iter = params.iter();
        while let Some(group) = iter.next() {
            let n = *group.first().unwrap_or(&0);
            match n {
                0 => {
                    self.current_fg = CellColor::Default;
                    self.current_bg = CellColor::Default;
                    self.current_flags = CellFlags::empty();
                }
                1 => self.current_flags |= CellFlags::BOLD,
                2 => self.current_flags |= CellFlags::DIM,
                3 => self.current_flags |= CellFlags::ITALIC,
                4 => self.current_flags |= CellFlags::UNDERLINE,
                5 => self.current_flags |= CellFlags::BLINK,
                7 => self.current_flags |= CellFlags::INVERSE,
                8 => self.current_flags |= CellFlags::HIDDEN,
                9 => self.current_flags |= CellFlags::STRIKETHROUGH,
                22 => self.current_flags.remove(CellFlags::BOLD | CellFlags::DIM),
                23 => self.current_flags.remove(CellFlags::ITALIC),
                24 => self.current_flags.remove(CellFlags::UNDERLINE),
                25 => self.current_flags.remove(CellFlags::BLINK),
                27 => self.current_flags.remove(CellFlags::INVERSE),
                28 => self.current_flags.remove(CellFlags::HIDDEN),
                29 => self.current_flags.remove(CellFlags::STRIKETHROUGH),
                30..=37 => self.current_fg = CellColor::Indexed((n - 30) as u8),
                38 => {
                    let Some(next) = iter.next() else { break };
                    self.current_fg = parse_extended_color(*next.first().unwrap_or(&0), &mut iter)
                        .unwrap_or(self.current_fg);
                }
                39 => self.current_fg = CellColor::Default,
                40..=47 => self.current_bg = CellColor::Indexed((n - 40) as u8),
                48 => {
                    let Some(next) = iter.next() else { break };
                    self.current_bg = parse_extended_color(*next.first().unwrap_or(&0), &mut iter)
                        .unwrap_or(self.current_bg);
                }
                49 => self.current_bg = CellColor::Default,
                90..=97 => self.current_fg = CellColor::Indexed((n - 90 + 8) as u8),
                100..=107 => self.current_bg = CellColor::Indexed((n - 100 + 8) as u8),
                _ => {}
            }
        }
    }

    fn set_scroll_region(&mut self, top: u16, bottom: u16) {
        let top = top.saturating_sub(1).min(self.rows.saturating_sub(1));
        let bottom = bottom.saturating_sub(1).min(self.rows.saturating_sub(1));
        if top < bottom {
            self.scroll_top = top;
            self.scroll_bottom = bottom;
            self.cursor_position(1, 1);
        }
    }
}

fn parse_extended_color<'a>(mode: u16, iter: &mut vte::ParamsIter<'a>) -> Option<CellColor> {
    match mode {
        2 => {
            let r = iter.next()?.first().copied().unwrap_or(0) as u8;
            let g = iter.next()?.first().copied().unwrap_or(0) as u8;
            let b = iter.next()?.first().copied().unwrap_or(0) as u8;
            Some(CellColor::Rgb(r, g, b))
        }
        5 => {
            let idx = iter.next()?.first().copied().unwrap_or(0) as u8;
            Some(CellColor::Indexed(idx))
        }
        _ => None,
    }
}

fn cells_to_string(cells: &[Cell]) -> String {
    let mut s: String = cells.iter().map(|c| c.ch).collect();
    // Trim trailing spaces; most users expect that when selecting.
    let trimmed = s.trim_end_matches(' ');
    s.truncate(trimmed.len());
    s
}

fn push_matches(out: &mut Vec<SearchMatch>, hay: &str, needle: &str, line: i64) {
    let mut start = 0;
    let hay_bytes = hay.as_bytes();
    let needle_bytes = needle.as_bytes();
    if needle_bytes.is_empty() {
        return;
    }
    while start + needle_bytes.len() <= hay_bytes.len() {
        if &hay_bytes[start..start + needle_bytes.len()] == needle_bytes {
            out.push(SearchMatch {
                line,
                col_start: start as u16,
                col_end: (start + needle_bytes.len()) as u16,
            });
            start += needle_bytes.len();
        } else {
            start += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// VTE `Perform` glue
// ---------------------------------------------------------------------------

struct Handler<'a> {
    state: &'a mut EmulatorState,
    responses: Vec<u8>,
    bus: &'a orchid_core::EventBus,
    session_id: Uuid,
}

impl<'a> Perform for Handler<'a> {
    fn print(&mut self, c: char) {
        self.state.write_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => {
                // BEL
                self.bus.publish(
                    orchid_core::EventSource::Subsystem("terminal".into()),
                    TerminalBell {
                        session_id: self.session_id,
                    },
                );
            }
            0x08 => self.state.backspace(),
            0x09 => self.state.tab(),
            0x0A..=0x0C => self.state.line_feed(),
            0x0D => self.state.carriage_return(),
            _ => {}
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        let mut iter = params.iter();
        let p1 = iter.next().and_then(|g| g.first().copied()).unwrap_or(0);
        let p2 = iter.next().and_then(|g| g.first().copied()).unwrap_or(0);
        match action {
            'A' => self.state.cursor_up(p1.max(1)),
            'B' => self.state.cursor_down(p1.max(1)),
            'C' => self.state.cursor_right(p1.max(1)),
            'D' => self.state.cursor_left(p1.max(1)),
            'H' | 'f' => {
                let row = if p1 == 0 { 1 } else { p1 };
                let col = if p2 == 0 { 1 } else { p2 };
                self.state.cursor_position(row, col);
            }
            'J' => self.state.erase_in_display(p1),
            'K' => self.state.erase_in_line(p1),
            'm' => self.state.apply_sgr(params),
            'r' => {
                let top = if p1 == 0 { 1 } else { p1 };
                let bot = if p2 == 0 { self.state.rows } else { p2 };
                self.state.set_scroll_region(top, bot);
            }
            'n' if p1 == 6 => {
                // Device Status Report (CPR: cursor position).
                let row = self.state.cursor.row + 1;
                let col = self.state.cursor.col + 1;
                self.responses
                    .extend_from_slice(format!("\x1b[{row};{col}R").as_bytes());
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            b'c' => {
                // RIS — reset.
                self.state.current_fg = CellColor::Default;
                self.state.current_bg = CellColor::Default;
                self.state.current_flags = CellFlags::empty();
                self.state.cursor = CursorState::default();
                self.state.mark_full_redraw();
                self.state.bump_generation();
            }
            b'M' => self.state.reverse_line_feed(),
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        let Some((first, rest)) = params.split_first() else {
            return;
        };
        let Ok(code) = std::str::from_utf8(first) else {
            return;
        };
        match code {
            "0" | "1" | "2" => {
                if let Some(title) = rest.first() {
                    let title = String::from_utf8_lossy(title).to_string();
                    self.state.title = title.clone();
                    self.bus.publish(
                        orchid_core::EventSource::Subsystem("terminal".into()),
                        TerminalTitleChanged {
                            session_id: self.session_id,
                            title,
                        },
                    );
                }
            }
            "7" => {
                if let Some(uri) = rest.first() {
                    let uri_s = String::from_utf8_lossy(uri).to_string();
                    // `file://host/path/`
                    if let Some(rest) = uri_s
                        .strip_prefix("file://")
                        .and_then(|s| s.split_once('/').map(|(_, p)| p))
                    {
                        let path = percent_decode_lossy(rest);
                        let buf = PathBuf::from(path);
                        self.state.cwd = Some(buf.clone());
                        self.bus.publish(
                            orchid_core::EventSource::Subsystem("terminal".into()),
                            TerminalCwdChanged {
                                session_id: self.session_id,
                                cwd: buf,
                            },
                        );
                    }
                }
            }
            "52" => {
                if rest.first().is_some_and(|b| b == b"?") {
                    return;
                }
                let Some(b64) = rest
                    .iter()
                    .rev()
                    .find(|b| !b.is_empty())
                    .and_then(|b| std::str::from_utf8(b).ok())
                else {
                    return;
                };
                let Ok(bytes) = decode_osc52_base64(b64) else {
                    tracing::debug!(len = b64.len(), "OSC 52: invalid base64 payload");
                    return;
                };
                let Ok(text) = String::from_utf8(bytes) else {
                    tracing::debug!("OSC 52: clipboard payload is not UTF-8");
                    return;
                };
                self.bus.publish(
                    orchid_core::EventSource::Subsystem("terminal".into()),
                    TerminalClipboardWrite {
                        session_id: self.session_id,
                        text,
                    },
                );
            }
            _ => {}
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
}

fn decode_osc52_base64(input: &str) -> std::result::Result<Vec<u8>, ()> {
    const TABLE: [i8; 256] = {
        let mut t = [-1i8; 256];
        let mut i = 0u8;
        while i < 26 {
            t[(b'A' + i) as usize] = i as i8;
            t[(b'a' + i) as usize] = (i + 26) as i8;
            i += 1;
        }
        let mut d = 0u8;
        while d < 10 {
            t[(b'0' + d) as usize] = (d + 52) as i8;
            d += 1;
        }
        t[b'+' as usize] = 62;
        t[b'/' as usize] = 63;
        t
    };
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &ch in input.as_bytes() {
        if ch == b'=' {
            break;
        }
        let val = TABLE[ch as usize];
        if val < 0 {
            continue;
        }
        buf = (buf << 6) | val as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

fn percent_decode_lossy(s: &str) -> String {
    let mut out = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = hex_digit(bytes[i + 1]);
            let lo = hex_digit(bytes[i + 2]);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bus() -> Arc<orchid_core::EventBus> {
        Arc::new(orchid_core::EventBus::new(
            orchid_core::EventBusConfig::default(),
        ))
    }

    #[test]
    fn plain_text_fills_cells() {
        let e = TerminalEmulator::new(20, 3, 100, bus(), Uuid::nil());
        e.feed(b"hello");
        let s = e.snapshot();
        let line: String = s.lines[0].cells.iter().map(|c| c.ch).collect();
        assert!(line.starts_with("hello"));
    }

    #[test]
    fn newline_advances_cursor() {
        let e = TerminalEmulator::new(20, 3, 100, bus(), Uuid::nil());
        e.feed(b"a\r\nb");
        let s = e.snapshot();
        assert_eq!(s.lines[0].cells[0].ch, 'a');
        assert_eq!(s.lines[1].cells[0].ch, 'b');
    }

    #[test]
    fn csi_cursor_up_moves() {
        let e = TerminalEmulator::new(20, 5, 100, bus(), Uuid::nil());
        e.feed(b"abc\n\x1b[A");
        let cur = e.cursor();
        // After 'abc\n': row=1, col=3. After ESC[A: row=0.
        assert_eq!(cur.row, 0);
    }

    #[test]
    fn sgr_red_sets_fg() {
        let e = TerminalEmulator::new(20, 3, 100, bus(), Uuid::nil());
        e.feed(b"\x1b[31mX");
        let s = e.snapshot();
        assert!(matches!(s.lines[0].cells[0].fg, CellColor::Indexed(1)));
    }

    #[test]
    fn cursor_position_reporting() {
        let e = TerminalEmulator::new(20, 3, 100, bus(), Uuid::nil());
        e.feed(b"abcd");
        let reply = e.feed(b"\x1b[6n");
        let s = String::from_utf8_lossy(&reply).into_owned();
        assert!(s.starts_with("\x1b["));
        assert!(s.contains(';'));
        assert!(s.ends_with('R'));
    }

    #[test]
    fn osc0_updates_title() {
        let e = TerminalEmulator::new(20, 3, 100, bus(), Uuid::nil());
        e.feed(b"\x1b]0;hello world\x07");
        assert_eq!(e.title(), "hello world");
    }

    #[test]
    fn decode_osc52_base64_roundtrip() {
        assert_eq!(
            decode_osc52_base64("aGVsbG8=").unwrap(),
            b"hello".as_slice()
        );
        assert_eq!(decode_osc52_base64("").unwrap(), b"".as_slice());
    }

    #[test]
    fn osc52_publishes_clipboard_write() {
        use crate::events::TerminalClipboardWrite;
        use orchid_core::{Event, EventFilter, HandlerPriority};

        let bus = bus();
        let received = Arc::new(Mutex::new(None::<String>));
        let got = Arc::clone(&received);
        let _sub = bus
            .subscribe_sync(
                EventFilter::of_type(TerminalClipboardWrite::event_type()),
                HandlerPriority::Normal,
                move |env| {
                    if let Some(ev) = env.downcast::<TerminalClipboardWrite>() {
                        *got.lock() = Some(ev.text.clone());
                    }
                },
            )
            .unwrap();

        let e = TerminalEmulator::new(20, 3, 100, bus, Uuid::nil());
        e.feed(b"\x1b]52;c;aGVsbG8=\x07");
        assert_eq!(received.lock().as_deref(), Some("hello"));
    }

    #[test]
    fn search_in_scrollback_finds_substring() {
        let e = TerminalEmulator::new(20, 3, 100, bus(), Uuid::nil());
        e.feed(b"abc def ghi");
        let hits = e.search_in_scrollback("def", true);
        assert_eq!(hits.len(), 1);
    }
}
