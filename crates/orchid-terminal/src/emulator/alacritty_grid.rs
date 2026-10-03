//! Optional grid backed by `alacritty_terminal`.
//!
//! Bytes go through Alacritty's parser. Visible cells are copied into
//! [`GridSnapshot`] so the existing raster stays unchanged. Sixel and direct
//! Kitty images are decoded with the built-in graphics helpers and placed at
//! the cursor. OSC 52 copy publishes [`TerminalClipboardWrite`]. OSC 7 stores
//! the working directory. zlib Kitty payloads are skipped.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config as TermConfig, Term, TermDamage, TermMode};
use alacritty_terminal::vte::ansi::{Color, CursorShape, Processor};
use parking_lot::Mutex;
use uuid::Uuid;

use crate::emulator::graphics::{self, ApcSplitter, StreamEvent};
use crate::emulator::{
    Cell, CellColor, CellFlags, CursorState, CursorStyle, GridLine, GridSnapshot, InlineImage,
    DEFAULT_SCROLLBACK,
};
use crate::error::Result;
use crate::events::{TerminalClipboardWrite, TerminalCwdChanged};

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
    bus: Arc<orchid_core::EventBus>,
    session_id: Uuid,
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
            Event::ClipboardStore(_, text) => {
                self.bus.publish(
                    orchid_core::EventSource::Subsystem("terminal".into()),
                    TerminalClipboardWrite {
                        session_id: self.session_id,
                        text,
                    },
                );
            }
            Event::MouseCursorDirty
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

struct PlacedImage {
    col: u16,
    row: i32,
    width: u32,
    height: u32,
    rgba: Arc<Vec<u8>>,
}

struct State {
    term: Term<Bridge>,
    parser: Processor,
    generation: u64,
    apc: ApcSplitter,
    dcs: DcsSplitter,
    images: Vec<PlacedImage>,
    kitty_acc: HashMap<u32, Vec<u8>>,
    cwd: Option<PathBuf>,
    /// Last viewport, so a partial damage update does not copy every cell.
    frame: Vec<Arc<[Cell]>>,
    frame_cols: usize,
    frame_rows: usize,
}

/// Alacritty grid mapped onto Orchid's snapshot.
pub struct AlacrittyGrid {
    inner: Mutex<State>,
    title: Arc<Mutex<String>>,
    replies: Arc<Mutex<Vec<u8>>>,
    size: Arc<Mutex<WindowSize>>,
    bus: Arc<orchid_core::EventBus>,
    session_id: Uuid,
}

impl std::fmt::Debug for AlacrittyGrid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlacrittyGrid").finish_non_exhaustive()
    }
}

impl AlacrittyGrid {
    /// Blank grid. Columns and rows are at least 1.
    #[must_use]
    pub fn new(cols: u16, rows: u16, bus: Arc<orchid_core::EventBus>, session_id: Uuid) -> Self {
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
            bus: Arc::clone(&bus),
            session_id,
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
                apc: ApcSplitter::default(),
                dcs: DcsSplitter::default(),
                images: Vec::new(),
                kitty_acc: HashMap::new(),
                cwd: None,
                frame: Vec::new(),
                frame_cols: 0,
                frame_rows: 0,
            }),
            title,
            replies,
            size,
            bus,
            session_id,
        }
    }

    /// Parse PTY bytes. Replies collected during the parse are returned.
    pub fn feed(&self, bytes: &[u8]) -> Vec<u8> {
        let cell_h = self.size.lock().cell_height.max(1);
        let mut state = self.inner.lock();
        let previous = state.cwd.clone();
        let events = state.apc.push(bytes);
        for event in events {
            match event {
                StreamEvent::Vt(clean) => state.feed_vt(&clean, cell_h),
                StreamEvent::Apc(payload) => state.handle_apc(&payload, cell_h),
            }
        }
        let cwd_changed = state.cwd.as_ref() != previous.as_ref();
        let cwd = state.cwd.clone();
        state.generation = state.generation.wrapping_add(1);
        drop(state);
        if cwd_changed {
            if let Some(cwd) = cwd {
                self.bus.publish(
                    orchid_core::EventSource::Subsystem("terminal".into()),
                    TerminalCwdChanged {
                        session_id: self.session_id,
                        cwd,
                    },
                );
            }
        }
        std::mem::take(&mut *self.replies.lock())
    }

    /// Directory from the latest OSC 7 `file://` sequence.
    #[must_use]
    pub fn working_directory(&self) -> Option<PathBuf> {
        self.inner.lock().cwd.clone()
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
    /// The first frame and any scroll copy the whole viewport. Later frames
    /// rewrite only the lines Alacritty marked damaged. Images placed since
    /// the last clear stay in the list, shifted by the scrollback offset.
    #[must_use]
    pub fn snapshot(&self) -> GridSnapshot {
        let mut state = self.inner.lock();
        let cols = state.term.columns();
        let rows = state.term.screen_lines();
        let history = state.term.grid().history_size();
        let offset = state.term.grid().display_offset();
        let show_cursor = state.term.mode().contains(TermMode::SHOW_CURSOR);
        let blinking = state.term.cursor_style().blinking;
        let damaged = damaged_viewport_rows(&mut state.term, offset, rows);
        state.term.reset_damage();
        let rows_hit = damaged.filter(|_| {
            state.frame_cols == cols && state.frame_rows == rows && state.frame.len() == rows
        });
        let (dirty_lines, full_redraw) = if let Some(rows_hit) = rows_hit {
            for row in &rows_hit {
                state.frame[*row] = Arc::from(fill_viewport_row(&state.term, offset, *row, cols));
            }
            (rows_hit.into_iter().map(|row| row as u16).collect(), false)
        } else {
            state.frame = (0..rows)
                .map(|row| Arc::from(fill_viewport_row(&state.term, offset, row, cols)))
                .collect();
            state.frame_cols = cols;
            state.frame_rows = rows;
            (Vec::new(), true)
        };
        let content = state.term.renderable_content();
        let point = content.cursor.point;
        let cursor_row = point.line.0 + offset as i32;
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
        let lines = state
            .frame
            .iter()
            .enumerate()
            .map(|(i, row)| GridLine {
                line_number: i as i64,
                cells: Arc::clone(row),
            })
            .collect();
        GridSnapshot {
            cols: cols as u16,
            rows: rows as u16,
            scrollback_offset: offset,
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
            dirty_lines,
            full_redraw,
            images: state.visible_images(offset),
        }
    }

    /// Last OSC title. Empty until the shell sets one.
    #[must_use]
    pub fn title(&self) -> String {
        self.title.lock().clone()
    }
}

impl State {
    fn feed_vt(&mut self, bytes: &[u8], cell_h: u16) {
        self.note_osc7(bytes);
        let pieces = self.dcs.push(bytes);
        for piece in pieces {
            match piece {
                DcsPiece::Vt(clean) => self.parser.advance(&mut self.term, &clean),
                DcsPiece::Sixel(body) => {
                    if let Some((w, h, rgba)) = graphics::decode_sixel(&body) {
                        self.place_image(w, h, Arc::new(rgba), true, cell_h);
                    }
                }
            }
        }
    }

    fn handle_apc(&mut self, payload: &[u8], cell_h: u16) {
        if payload.first() != Some(&b'G') {
            return;
        }
        let body = &payload[1..];
        let split = body.iter().position(|b| *b == b';').unwrap_or(body.len());
        let header = String::from_utf8_lossy(&body[..split]);
        let data = if split < body.len() {
            &body[split + 1..]
        } else {
            &[]
        };
        let cmd = graphics::parse_kitty_command(&header);
        if cmd.skip {
            self.kitty_acc.remove(&cmd.id);
            return;
        }
        if cmd.action == b'd' {
            self.kitty_acc.remove(&cmd.id);
            return;
        }
        if cmd.action != b't' && cmd.action != b'T' {
            return;
        }
        let chunk = graphics::decode_base64(data);
        let too_big = {
            let acc = self.kitty_acc.entry(cmd.id).or_default();
            let too_big = acc.len().saturating_add(chunk.len()) > graphics::KITTY_CAP;
            if !too_big {
                acc.extend(chunk);
            }
            too_big
        };
        if too_big {
            self.kitty_acc.remove(&cmd.id);
            return;
        }
        if cmd.more {
            return;
        }
        let bytes = self.kitty_acc.remove(&cmd.id).unwrap_or_default();
        let Some((w, h, rgba)) =
            graphics::decode_kitty_bytes(cmd.format, cmd.width, cmd.height, &bytes)
        else {
            return;
        };
        self.place_image(w, h, Arc::new(rgba), cmd.move_cursor, cell_h);
    }

    fn place_image(
        &mut self,
        width: u32,
        height: u32,
        rgba: Arc<Vec<u8>>,
        move_cursor: bool,
        cell_h: u16,
    ) {
        if width == 0 || height == 0 {
            return;
        }
        let (col, row) = cursor_cell(&self.term);
        self.images.push(PlacedImage {
            col,
            row: i32::from(row),
            width,
            height,
            rgba,
        });
        if self.images.len() > 32 {
            self.images.remove(0);
        }
        if move_cursor {
            let span = height.div_ceil(u32::from(cell_h.max(1))).max(1);
            let mut motion = vec![b'\r'];
            for _ in 0..span {
                motion.push(b'\n');
            }
            self.parser.advance(&mut self.term, &motion);
        }
    }

    fn visible_images(&self, display_offset: usize) -> Vec<InlineImage> {
        let shift = display_offset as i32;
        self.images
            .iter()
            .map(|img| InlineImage {
                col: img.col,
                row: img.row - shift,
                width: img.width,
                height: img.height,
                rgba: Arc::clone(&img.rgba),
            })
            .collect()
    }

    fn note_osc7(&mut self, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        let mut rest = text.as_ref();
        while let Some(start) = rest.find("\u{1b}]7;") {
            rest = &rest[start + 4..];
            let end = rest.find(['\u{7}', '\u{1b}']).unwrap_or(rest.len());
            let uri = &rest[..end];
            if let Some(path) = file_uri_path(uri) {
                self.cwd = Some(path);
            }
            rest = &rest[end..];
        }
    }
}

fn cursor_cell(term: &Term<Bridge>) -> (u16, u16) {
    let content = term.renderable_content();
    let offset = content.display_offset as i32;
    let row = content.cursor.point.line.0 + offset;
    let col = content.cursor.point.column.0 as u16;
    let row = if row < 0 { 0 } else { row as u16 };
    (col, row)
}

fn file_uri_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let path = rest.split_once('/')?.1;
    let decoded = percent_decode_lossy(path);
    if decoded.is_empty() {
        None
    } else {
        Some(PathBuf::from(decoded))
    }
}

fn percent_decode_lossy(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
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

#[derive(Debug, Default)]
struct DcsSplitter {
    held_esc: bool,
    in_dcs: bool,
    dcs_esc: bool,
    buf: Vec<u8>,
}

enum DcsPiece {
    Vt(Vec<u8>),
    Sixel(Vec<u8>),
}

impl DcsSplitter {
    fn push(&mut self, input: &[u8]) -> Vec<DcsPiece> {
        let mut out = Vec::new();
        let mut vt = Vec::new();
        for &b in input {
            if self.in_dcs {
                if self.dcs_esc {
                    self.dcs_esc = false;
                    if b == b'\\' {
                        self.finish(&mut out);
                        continue;
                    }
                    self.buf.push(0x1b);
                    self.buf.push(b);
                } else if b == 0x1b {
                    self.dcs_esc = true;
                } else {
                    self.buf.push(b);
                }
            } else if self.held_esc {
                self.held_esc = false;
                if b == b'P' {
                    if !vt.is_empty() {
                        out.push(DcsPiece::Vt(std::mem::take(&mut vt)));
                    }
                    self.in_dcs = true;
                    self.buf.clear();
                } else {
                    vt.push(0x1b);
                    vt.push(b);
                }
            } else if b == 0x1b {
                self.held_esc = true;
            } else {
                vt.push(b);
            }
        }
        if !vt.is_empty() {
            out.push(DcsPiece::Vt(vt));
        }
        out
    }

    fn finish(&mut self, out: &mut Vec<DcsPiece>) {
        self.in_dcs = false;
        self.dcs_esc = false;
        let buf = std::mem::take(&mut self.buf);
        if let Some(q) = buf.iter().position(|b| *b == b'q') {
            out.push(DcsPiece::Sixel(buf[q + 1..].to_vec()));
        }
    }
}

fn damaged_viewport_rows(
    term: &mut Term<Bridge>,
    offset: usize,
    rows: usize,
) -> Option<Vec<usize>> {
    let TermDamage::Partial(lines) = term.damage() else {
        return None;
    };
    let mut hit = Vec::new();
    for bounds in lines {
        let Some(row) = viewport_row(bounds.line, offset, rows) else {
            return None;
        };
        if !hit.contains(&row) {
            hit.push(row);
        }
    }
    Some(hit)
}

fn viewport_row(line: usize, offset: usize, rows: usize) -> Option<usize> {
    let row = line as i32 - offset as i32;
    if row < 0 {
        return None;
    }
    let row = row as usize;
    (row < rows).then_some(row)
}

fn fill_viewport_row(term: &Term<Bridge>, offset: usize, row: usize, cols: usize) -> Vec<Cell> {
    let line = Line(row as i32 - offset as i32);
    let src = &term.grid()[line];
    (0..cols).map(|col| map_cell(&src[Column(col)])).collect()
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
    use orchid_core::{Event, EventBus, EventBusConfig};

    fn grid() -> AlacrittyGrid {
        AlacrittyGrid::new(
            20,
            6,
            Arc::new(EventBus::new(EventBusConfig::default())),
            Uuid::nil(),
        )
    }

    #[test]
    fn second_snapshot_rewrites_only_the_damaged_line() {
        let grid = grid();
        let _ = grid.feed(b"A");
        let first = grid.snapshot();
        assert!(first.full_redraw);
        assert_eq!(first.lines[0].cells[0].ch, 'A');
        let _ = grid.feed(b"B");
        let second = grid.snapshot();
        assert!(!second.full_redraw);
        assert_eq!(second.dirty_lines, vec![0]);
        assert_eq!(second.lines[0].cells[0].ch, 'A');
        assert_eq!(second.lines[0].cells[1].ch, 'B');
    }

    #[test]
    fn bold_and_title_come_from_alacritty() {
        let grid = grid();
        let _ = grid.feed(b"\x1b[1mB");
        let snap = grid.snapshot();
        assert_eq!(snap.lines[0].cells[0].ch, 'B');
        assert!(snap.lines[0].cells[0].flags.contains(CellFlags::BOLD));
        let _ = grid.feed(b"\x1b]0;Hello\x07");
        assert_eq!(grid.title(), "Hello");
        assert!(snap.images.is_empty());
    }

    #[test]
    fn sixel_and_kitty_land_on_the_snapshot() {
        let grid = grid();
        let _ = grid.feed(b"\x1bP0;0;0q#1;2;100;0;0@\x1b\\");
        let snap = grid.snapshot();
        assert_eq!(snap.images.len(), 1);
        assert_eq!(snap.images[0].width, 1);
        assert_eq!(&snap.images[0].rgba[..4], &[255, 0, 0, 255]);
        let _ = grid.feed(b"\x1b_Ga=T,f=32,s=1,v=1,C=1;/wAA/w==\x1b\\");
        let snap = grid.snapshot();
        assert_eq!(snap.images.len(), 2);
    }

    #[test]
    fn osc7_sets_the_working_directory() {
        let grid = grid();
        let _ = grid.feed(b"\x1b]7;file://localhost/tmp/orchid\x07");
        assert_eq!(
            grid.working_directory().unwrap(),
            PathBuf::from("tmp/orchid")
        );
    }

    #[test]
    fn osc52_publishes_clipboard_text() {
        use orchid_core::{EventFilter, HandlerPriority};

        let bus = Arc::new(EventBus::new(EventBusConfig::default()));
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
        let grid = AlacrittyGrid::new(20, 6, bus, Uuid::nil());
        let _ = grid.feed(b"\x1b]52;c;aGVsbG8=\x07");
        assert_eq!(received.lock().as_deref(), Some("hello"));
    }
}
