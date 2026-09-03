//! Text viewer / editor.

pub mod buffer;
pub mod display;
pub mod grammars;
pub mod save;
pub mod search;
pub mod syntax;
pub mod undo;

use std::any::Any;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::{Mutex, RwLock};
use tree_sitter::{InputEdit, Point, Tree};

use crate::error::{Result, ViewerError};
use crate::snapshot::{SelectionRange, TextSnapshot, ViewerSnapshot};
use crate::viewer_trait::Viewer;

pub use buffer::{LineEnding, TextBuffer};
pub use display::{TextDisplayMode, VIEWER_ENCODINGS};
pub use grammars::{detect_language, PLAINTEXT};
pub use save::save_text;
pub use search::FindOptions;
pub use syntax::SyntaxHighlighter;
pub use undo::{TextOp, TextOpKind, UndoStack};

/// Default maximum text-viewer file size (50 MiB).
pub const DEFAULT_SIZE_LIMIT: u64 = 50 * 1024 * 1024;

/// Read vs edit mode toggle.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextViewerMode {
    Read,
    Edit,
}

/// Cursor coordinates.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CursorPos {
    pub line: u32,
    pub column: u32,
}

/// Original file bytes: mmap for local files, heap for remote / after save.
enum RawBytes {
    Empty,
    Heap(Arc<[u8]>),
    Mapped(memmap2::Mmap),
}

impl RawBytes {
    fn from_vec(bytes: Vec<u8>) -> Self {
        if bytes.is_empty() {
            Self::Empty
        } else {
            Self::Heap(Arc::from(bytes))
        }
    }

    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Empty => &[],
            Self::Heap(bytes) => bytes,
            Self::Mapped(map) => map,
        }
    }

    fn is_empty(&self) -> bool {
        self.as_slice().is_empty()
    }
}

/// Cached tree-sitter parse of the full document.
struct ParseCache {
    generation: u64,
    language: String,
    tree: Tree,
    /// Full LF-normalised source used to build `tree` (needed for scopes).
    source: String,
}

/// Text viewer / editor.
pub struct TextViewer {
    path: RwLock<Option<orchid_fs::FsPath>>,
    buffer: RwLock<Option<TextBuffer>>,
    language: RwLock<String>,
    highlighter: Arc<SyntaxHighlighter>,
    /// Bumped on every buffer mutation so the parse cache can be invalidated.
    content_generation: AtomicU64,
    parse_cache: Mutex<Option<ParseCache>>,
    /// Generation-keyed full document text for snapshots (avoids rope→String
    /// on every ~30 Hz pump tick when content is unchanged).
    plain_cache: Mutex<Option<(u64, Arc<str>)>>,
    undo: Mutex<UndoStack>,
    cursor: RwLock<CursorPos>,
    selection: RwLock<Option<SelectionRange>>,
    first_visible_line: RwLock<u32>,
    visible_line_count: RwLock<u32>,
    mode: RwLock<TextViewerMode>,
    registry: RwLock<Option<Arc<orchid_fs::FsProviderRegistry>>>,
    size_limit: u64,
    raw_bytes: RwLock<RawBytes>,
    display_mode: RwLock<TextDisplayMode>,
    display_cache: Mutex<Option<(TextDisplayMode, usize, Arc<str>)>>,
    find_gen: Mutex<i32>,
    find_anchor: Mutex<i32>,
    find_cursor: Mutex<i32>,
    find_match_index: Mutex<i32>,
    find_match_count: Mutex<i32>,
}

impl std::fmt::Debug for TextViewer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextViewer")
            .field("language", &*self.language.read())
            .field("mode", &*self.mode.read())
            .finish_non_exhaustive()
    }
}

impl TextViewer {
    /// Build a text viewer that shares the given highlighter.
    #[must_use]
    pub fn new(highlighter: Arc<SyntaxHighlighter>) -> Self {
        Self {
            path: RwLock::new(None),
            buffer: RwLock::new(None),
            language: RwLock::new(PLAINTEXT.into()),
            highlighter,
            content_generation: AtomicU64::new(0),
            parse_cache: Mutex::new(None),
            plain_cache: Mutex::new(None),
            undo: Mutex::new(UndoStack::new(500)),
            cursor: RwLock::new(CursorPos::default()),
            selection: RwLock::new(None),
            first_visible_line: RwLock::new(0),
            visible_line_count: RwLock::new(64),
            mode: RwLock::new(TextViewerMode::Read),
            registry: RwLock::new(None),
            size_limit: DEFAULT_SIZE_LIMIT,
            raw_bytes: RwLock::new(RawBytes::Empty),
            display_mode: RwLock::new(TextDisplayMode::Text),
            display_cache: Mutex::new(None),
            find_gen: Mutex::new(0),
            find_anchor: Mutex::new(0),
            find_cursor: Mutex::new(0),
            find_match_index: Mutex::new(0),
            find_match_count: Mutex::new(0),
        }
    }

    fn bump_content_generation(&self) -> u64 {
        self.content_generation.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn remember_plain_text(&self, generation: u64, text: Arc<str>) {
        *self.plain_cache.lock() = Some((generation, text));
    }

    /// Shared LF-normalised document text for the current content generation.
    fn cached_plain_text(&self, buffer: &TextBuffer) -> Arc<str> {
        let generation = self.content_generation.load(Ordering::Relaxed);
        if let Some((gen, text)) = self.plain_cache.lock().as_ref() {
            if *gen == generation {
                return Arc::clone(text);
            }
        }
        // Reuse parse-cache source when it matches this generation.
        if let Some(cached) = self.parse_cache.lock().as_ref() {
            if cached.generation == generation {
                let text: Arc<str> = Arc::from(cached.source.as_str());
                self.remember_plain_text(generation, Arc::clone(&text));
                return text;
            }
        }
        let text: Arc<str> = Arc::from(buffer.plain_text());
        self.remember_plain_text(generation, Arc::clone(&text));
        text
    }

    /// Apply a tree-sitter edit to the cached AST, or drop the cache on failure.
    fn sync_parse_cache_after_edit(&self, edit: InputEdit, new_source: String) {
        let generation = self.bump_content_generation();
        let plain: Arc<str> = Arc::from(new_source.as_str());
        self.remember_plain_text(generation, Arc::clone(&plain));
        let language = self.language.read().clone();
        let mut cache = self.parse_cache.lock();
        let Some(cached) = cache.as_mut() else {
            return;
        };
        if cached.language != language {
            *cache = None;
            return;
        }
        cached.tree.edit(&edit);
        match self
            .highlighter
            .parse(&language, &new_source, Some(&cached.tree))
        {
            Some(tree) => {
                cached.tree = tree;
                cached.source = new_source;
                cached.generation = generation;
            }
            None => {
                *cache = None;
            }
        }
    }

    fn invalidate_parse_cache(&self) {
        self.bump_content_generation();
        *self.parse_cache.lock() = None;
        *self.plain_cache.lock() = None;
    }

    /// Ensure a fresh tree-sitter parse for the current buffer generation.
    fn highlight_visible(
        &self,
        buffer: &TextBuffer,
        first: u32,
        count: u32,
    ) -> Vec<crate::snapshot::SyntaxLine> {
        let language = self.language.read().clone();
        let generation = self.content_generation.load(Ordering::Relaxed);
        let mut cache = self.parse_cache.lock();

        let needs_reparse = match cache.as_ref() {
            Some(c) => c.generation != generation || c.language != language,
            None => true,
        };

        if needs_reparse {
            let source = buffer.plain_text();
            let tree = self.highlighter.parse(&language, &source, None);
            match tree {
                Some(tree) => {
                    *cache = Some(ParseCache {
                        generation,
                        language: language.clone(),
                        tree,
                        source,
                    });
                }
                None => {
                    *cache = None;
                    return self.highlighter.highlight_lines(
                        &language,
                        &source,
                        first,
                        count.min(buffer.line_count()),
                    );
                }
            }
        }

        let cached = cache.as_ref().expect("parse cache populated above");
        self.highlighter.highlight_from_tree(
            &cached.language,
            &cached.source,
            &cached.tree,
            first,
            count.min(buffer.line_count()),
        )
    }

    fn insert_edit(buffer: &TextBuffer, at: CursorPos, text: &str) -> Option<InputEdit> {
        let start_byte = buffer.byte_index(at.line, at.column)?;
        let start_position = buffer.tree_sitter_point(at.line, at.column)?;
        let new_end_position = point_after_insert(start_position, text);
        Some(InputEdit {
            start_byte,
            old_end_byte: start_byte,
            new_end_byte: start_byte + text.len(),
            start_position,
            old_end_position: start_position,
            new_end_position,
        })
    }

    fn delete_edit(buffer: &TextBuffer, start: CursorPos, end: CursorPos) -> Option<InputEdit> {
        let start_byte = buffer.byte_index(start.line, start.column)?;
        let old_end_byte = buffer.byte_index(end.line, end.column)?;
        let start_position = buffer.tree_sitter_point(start.line, start.column)?;
        let old_end_position = buffer.tree_sitter_point(end.line, end.column)?;
        Some(InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte: start_byte,
            start_position,
            old_end_position,
            new_end_position: start_position,
        })
    }

    /// Switch read / edit mode.
    pub fn set_mode(&self, mode: TextViewerMode) {
        *self.mode.write() = mode;
    }

    /// Current read / edit mode.
    #[must_use]
    pub fn mode(&self) -> TextViewerMode {
        *self.mode.read()
    }

    /// Set the visible window for snapshots.
    pub fn set_visible_range(&self, first_line: u32, count: u32) {
        *self.first_visible_line.write() = first_line;
        *self.visible_line_count.write() = count.max(1);
    }

    /// First line currently shown in the snapshot window.
    #[must_use]
    pub fn first_visible_line(&self) -> u32 {
        *self.first_visible_line.read()
    }

    /// Number of lines requested for the snapshot window.
    #[must_use]
    pub fn visible_line_count(&self) -> u32 {
        *self.visible_line_count.read()
    }

    /// Scroll the visible window by whole lines (positive = down).
    pub fn scroll_lines(&self, delta: i32) {
        let buffer = self.buffer.read();
        let Some(buf) = buffer.as_ref() else {
            return;
        };
        let total = buf.line_count();
        drop(buffer);
        let count = *self.visible_line_count.read();
        let max_first = total.saturating_sub(count);
        let mut first = *self.first_visible_line.read() as i64 + i64::from(delta);
        first = first.clamp(0, i64::from(max_first));
        *self.first_visible_line.write() = first as u32;
    }

    /// Move the cursor.
    pub fn move_cursor(&self, pos: CursorPos) {
        *self.cursor.write() = pos;
    }

    /// Update the current selection.
    pub fn set_selection(&self, sel: Option<SelectionRange>) {
        *self.selection.write() = sel;
    }

    /// Insert `text` at the cursor.
    ///
    /// # Errors
    ///
    /// [`ViewerError::EditOutOfBounds`] for invalid cursor positions.
    pub fn insert(&self, text: &str) -> Result<()> {
        if *self.mode.read() != TextViewerMode::Edit {
            return Err(ViewerError::EditOutOfBounds);
        }
        let cursor = *self.cursor.read();
        let (edit, new_source, end) = {
            let mut guard = self.buffer.write();
            let buffer = guard.as_mut().ok_or(ViewerError::EditOutOfBounds)?;
            let edit =
                Self::insert_edit(buffer, cursor, text).ok_or(ViewerError::EditOutOfBounds)?;
            buffer.insert(cursor.line, cursor.column, text)?;
            let new_source = buffer.plain_text();
            let end = cursor_after_insert(cursor, text);
            (edit, new_source, end)
        };
        self.sync_parse_cache_after_edit(edit, new_source);
        *self.cursor.write() = end;
        self.undo.lock().push(TextOp {
            kind: TextOpKind::Insert,
            start_line: cursor.line,
            start_column: cursor.column,
            end_line: end.line,
            end_column: end.column,
            text: text.into(),
            timestamp: std::time::Instant::now(),
        });
        Ok(())
    }

    /// Delete the range `[start, end)`.
    ///
    /// # Errors
    ///
    /// [`ViewerError::EditOutOfBounds`] when not in edit mode or the range is invalid.
    pub fn delete(&self, start: CursorPos, end: CursorPos) -> Result<()> {
        if *self.mode.read() != TextViewerMode::Edit {
            return Err(ViewerError::EditOutOfBounds);
        }
        let (edit, deleted, new_source) = {
            let mut guard = self.buffer.write();
            let buffer = guard.as_mut().ok_or(ViewerError::EditOutOfBounds)?;
            let edit = Self::delete_edit(buffer, start, end).ok_or(ViewerError::EditOutOfBounds)?;
            let deleted = buffer
                .text_range(start.line, start.column, end.line, end.column)?
                .to_string();
            buffer.delete(start.line, start.column, end.line, end.column)?;
            let new_source = buffer.plain_text();
            (edit, deleted, new_source)
        };
        self.sync_parse_cache_after_edit(edit, new_source);
        *self.cursor.write() = start;
        self.undo.lock().push(TextOp {
            kind: TextOpKind::Delete,
            start_line: start.line,
            start_column: start.column,
            end_line: end.line,
            end_column: end.column,
            text: deleted,
            timestamp: std::time::Instant::now(),
        });
        Ok(())
    }

    /// Replace buffer contents with `text` (plain edit-mode push from the UI).
    ///
    /// Prefers a single contiguous delete or insert (common typing path) so
    /// tree-sitter and undo stay incremental. Selection-replace (delete+insert)
    /// updates the rope in place but invalidates the parse cache.
    ///
    /// # Errors
    ///
    /// [`ViewerError::EditOutOfBounds`] when not in edit mode or no buffer is open.
    pub fn replace_content(&self, text: &str) -> Result<()> {
        if *self.mode.read() != TextViewerMode::Edit {
            return Err(ViewerError::EditOutOfBounds);
        }
        let normalized = text.replace("\r\n", "\n");

        enum Outcome {
            Unchanged,
            Incremental {
                edit: InputEdit,
                new_source: String,
                op: TextOp,
                cursor: CursorPos,
            },
            /// Rope already updated; drop parse cache / undo (mixed middle replace).
            InPlaceInvalidate {
                cursor: CursorPos,
            },
        }

        let outcome = {
            let mut guard = self.buffer.write();
            let buffer = guard.as_mut().ok_or(ViewerError::EditOutOfBounds)?;
            if buffer.content_eq(&normalized) {
                Outcome::Unchanged
            } else {
                match buffer.single_span_diff(&normalized) {
                    None => {
                        buffer.replace_content(&normalized);
                        Outcome::InPlaceInvalidate {
                            cursor: CursorPos::default(),
                        }
                    }
                    Some((start_b, old_end_b, new_end_b)) => {
                        let start_char = buffer.byte_to_char(start_b);
                        let old_end_char = buffer.byte_to_char(old_end_b);
                        let (start_line, start_column) = buffer.line_col_at_char(start_char);
                        let (end_line, end_column) = buffer.line_col_at_char(old_end_char);
                        let start = CursorPos {
                            line: start_line,
                            column: start_column,
                        };
                        let end = CursorPos {
                            line: end_line,
                            column: end_column,
                        };
                        let inserted = &normalized[start_b..new_end_b];
                        let deleted_empty = start_b == old_end_b;
                        let inserted_empty = inserted.is_empty();

                        if !deleted_empty && inserted_empty {
                            let edit = Self::delete_edit(buffer, start, end)
                                .ok_or(ViewerError::EditOutOfBounds)?;
                            let deleted = buffer
                                .text_range(start_line, start_column, end_line, end_column)?
                                .to_string();
                            buffer.delete(start_line, start_column, end_line, end_column)?;
                            let new_source = buffer.plain_text();
                            Outcome::Incremental {
                                edit,
                                new_source,
                                op: TextOp {
                                    kind: TextOpKind::Delete,
                                    start_line,
                                    start_column,
                                    end_line,
                                    end_column,
                                    text: deleted,
                                    timestamp: std::time::Instant::now(),
                                },
                                cursor: start,
                            }
                        } else if deleted_empty && !inserted_empty {
                            let edit = Self::insert_edit(buffer, start, inserted)
                                .ok_or(ViewerError::EditOutOfBounds)?;
                            buffer.insert(start_line, start_column, inserted)?;
                            let new_source = buffer.plain_text();
                            let cursor = cursor_after_insert(start, inserted);
                            Outcome::Incremental {
                                edit,
                                new_source,
                                op: TextOp {
                                    kind: TextOpKind::Insert,
                                    start_line,
                                    start_column,
                                    end_line: cursor.line,
                                    end_column: cursor.column,
                                    text: inserted.to_string(),
                                    timestamp: std::time::Instant::now(),
                                },
                                cursor,
                            }
                        } else {
                            if !deleted_empty {
                                buffer.delete(start_line, start_column, end_line, end_column)?;
                            }
                            if !inserted_empty {
                                buffer.insert(start_line, start_column, inserted)?;
                            }
                            let cursor = cursor_after_insert(start, inserted);
                            Outcome::InPlaceInvalidate { cursor }
                        }
                    }
                }
            }
        };

        match outcome {
            Outcome::Unchanged => Ok(()),
            Outcome::InPlaceInvalidate { cursor } => {
                self.invalidate_parse_cache();
                self.undo.lock().clear();
                *self.cursor.write() = cursor;
                Ok(())
            }
            Outcome::Incremental {
                edit,
                new_source,
                op,
                cursor,
            } => {
                self.sync_parse_cache_after_edit(edit, new_source);
                *self.cursor.write() = cursor;
                self.undo.lock().push(op);
                Ok(())
            }
        }
    }

    /// Undo the last edit op.
    ///
    /// # Errors
    ///
    /// [`ViewerError::EditOutOfBounds`] when not in edit mode or no buffer is open.
    pub fn undo(&self) -> Result<()> {
        if *self.mode.read() != TextViewerMode::Edit {
            return Err(ViewerError::EditOutOfBounds);
        }
        let op = {
            let mut undo = self.undo.lock();
            undo.undo()
        };
        let Some(op) = op else {
            return Ok(());
        };
        match op.kind {
            TextOpKind::Insert => {
                let start = CursorPos {
                    line: op.start_line,
                    column: op.start_column,
                };
                let end = CursorPos {
                    line: op.end_line,
                    column: op.end_column,
                };
                let (edit, new_source) = {
                    let mut guard = self.buffer.write();
                    let buffer = guard.as_mut().ok_or(ViewerError::EditOutOfBounds)?;
                    let edit = Self::delete_edit(buffer, start, end)
                        .ok_or(ViewerError::EditOutOfBounds)?;
                    buffer.delete(op.start_line, op.start_column, op.end_line, op.end_column)?;
                    (edit, buffer.plain_text())
                };
                self.sync_parse_cache_after_edit(edit, new_source);
                *self.cursor.write() = start;
            }
            TextOpKind::Delete => {
                let at = CursorPos {
                    line: op.start_line,
                    column: op.start_column,
                };
                let (edit, new_source) = {
                    let mut guard = self.buffer.write();
                    let buffer = guard.as_mut().ok_or(ViewerError::EditOutOfBounds)?;
                    let edit = Self::insert_edit(buffer, at, &op.text)
                        .ok_or(ViewerError::EditOutOfBounds)?;
                    buffer.insert(op.start_line, op.start_column, &op.text)?;
                    (edit, buffer.plain_text())
                };
                self.sync_parse_cache_after_edit(edit, new_source);
                *self.cursor.write() = CursorPos {
                    line: op.end_line,
                    column: op.end_column,
                };
            }
        }
        Ok(())
    }

    /// Redo the last undone edit op.
    ///
    /// # Errors
    ///
    /// [`ViewerError::EditOutOfBounds`] when not in edit mode or no buffer is open.
    pub fn redo(&self) -> Result<()> {
        if *self.mode.read() != TextViewerMode::Edit {
            return Err(ViewerError::EditOutOfBounds);
        }
        let op = {
            let mut undo = self.undo.lock();
            undo.redo()
        };
        let Some(op) = op else {
            return Ok(());
        };
        match op.kind {
            TextOpKind::Insert => {
                let at = CursorPos {
                    line: op.start_line,
                    column: op.start_column,
                };
                let (edit, new_source) = {
                    let mut guard = self.buffer.write();
                    let buffer = guard.as_mut().ok_or(ViewerError::EditOutOfBounds)?;
                    let edit = Self::insert_edit(buffer, at, &op.text)
                        .ok_or(ViewerError::EditOutOfBounds)?;
                    buffer.insert(op.start_line, op.start_column, &op.text)?;
                    (edit, buffer.plain_text())
                };
                self.sync_parse_cache_after_edit(edit, new_source);
                *self.cursor.write() = CursorPos {
                    line: op.end_line,
                    column: op.end_column,
                };
            }
            TextOpKind::Delete => {
                let start = CursorPos {
                    line: op.start_line,
                    column: op.start_column,
                };
                let end = CursorPos {
                    line: op.end_line,
                    column: op.end_column,
                };
                let (edit, new_source) = {
                    let mut guard = self.buffer.write();
                    let buffer = guard.as_mut().ok_or(ViewerError::EditOutOfBounds)?;
                    let edit = Self::delete_edit(buffer, start, end)
                        .ok_or(ViewerError::EditOutOfBounds)?;
                    buffer.delete(op.start_line, op.start_column, op.end_line, op.end_column)?;
                    (edit, buffer.plain_text())
                };
                self.sync_parse_cache_after_edit(edit, new_source);
                *self.cursor.write() = start;
            }
        }
        Ok(())
    }

    /// Scroll by `delta` lines (negative = up).
    pub fn scroll_by_lines(&self, delta: i32) {
        let first = *self.first_visible_line.read();
        let new = if delta < 0 {
            first.saturating_sub(delta.unsigned_abs())
        } else {
            first.saturating_add(delta as u32)
        };
        *self.first_visible_line.write() = new;
    }

    /// Switch text / hex / binary presentation.
    pub fn set_display_mode(&self, mode: TextDisplayMode) {
        if mode != TextDisplayMode::Text {
            if let Some(buf) = self.buffer.read().as_ref() {
                if let Ok(bytes) = buf.to_bytes() {
                    *self.raw_bytes.write() = RawBytes::from_vec(bytes);
                }
            }
        }
        *self.display_mode.write() = mode;
        *self.display_cache.lock() = None;
    }

    /// Current presentation mode.
    #[must_use]
    pub fn display_mode(&self) -> TextDisplayMode {
        *self.display_mode.read()
    }

    /// Re-decode the original bytes with `label` (`UTF-8`, `windows-1251`, …).
    ///
    /// # Errors
    ///
    /// Unknown encoding label.
    pub fn set_encoding(&self, label: &str) -> Result<()> {
        let bytes = self.raw_bytes.read();
        if bytes.is_empty() && self.buffer.read().is_none() {
            return Err(ViewerError::EditOutOfBounds);
        }
        let mut buffer = TextBuffer::from_bytes_with_encoding(bytes.as_slice(), label)?;
        buffer.mark_dirty();
        *self.buffer.write() = Some(buffer);
        self.invalidate_parse_cache();
        self.undo.lock().clear();
        *self.display_cache.lock() = None;
        *self.cursor.write() = CursorPos::default();
        Ok(())
    }

    fn displayed_text(&self) -> Arc<str> {
        let mode = *self.display_mode.read();
        let raw = self.raw_bytes.read();
        if mode != TextDisplayMode::Text {
            if let Some((cached_mode, len, text)) = self.display_cache.lock().as_ref() {
                if *cached_mode == mode && *len == raw.as_slice().len() {
                    return Arc::clone(text);
                }
            }
            let dump: Arc<str> = Arc::from(match mode {
                TextDisplayMode::Hex => display::format_hex_dump(raw.as_slice()),
                TextDisplayMode::Binary => display::format_binary_hex(raw.as_slice()),
                TextDisplayMode::Text => String::new(),
            });
            *self.display_cache.lock() = Some((mode, raw.as_slice().len(), Arc::clone(&dump)));
            return dump;
        }
        drop(raw);
        let guard = self.buffer.read();
        guard
            .as_ref()
            .map(|b| self.cached_plain_text(b))
            .unwrap_or_default()
    }

    fn record_find(&self, start: usize, end: usize, index: i32, count: i32) {
        *self.find_anchor.lock() = start as i32;
        *self.find_cursor.lock() = end as i32;
        *self.find_match_index.lock() = index;
        *self.find_match_count.lock() = count;
        *self.find_gen.lock() += 1;
    }

    fn clear_find(&self) {
        *self.find_match_index.lock() = 0;
        *self.find_match_count.lock() = 0;
        *self.find_gen.lock() += 1;
    }

    /// Find the next / previous match in the current view (text or hex dump).
    ///
    /// # Errors
    ///
    /// Invalid regular expressions when `opts.regex` is set.
    pub fn find(&self, query: &str, forward: bool, opts: FindOptions) -> Result<bool> {
        if query.is_empty() {
            return Ok(false);
        }
        let haystack = self.displayed_text();
        let matches = search::collect_matches(&haystack, query, opts)?;
        if matches.is_empty() {
            self.clear_find();
            return Ok(false);
        }
        let sel_lo = (*self.find_anchor.lock()).max(0) as usize;
        let sel_hi = (*self.find_cursor.lock()).max(0) as usize;
        let Some((start, end)) = search::pick_match(&matches, sel_lo, sel_hi, forward) else {
            self.clear_find();
            return Ok(false);
        };
        let index = matches
            .iter()
            .position(|&(s, _)| s == start)
            .map(|i| (i + 1) as i32)
            .unwrap_or(0);
        self.record_find(start, end, index, matches.len() as i32);
        Ok(true)
    }

    /// Replace the current match (or the next one) then advance.
    ///
    /// Hex / binary views are read-only. Switches to edit mode for text.
    ///
    /// # Errors
    ///
    /// Invalid regex, or the buffer cannot be edited.
    pub fn replace_current(
        &self,
        query: &str,
        replacement: &str,
        opts: FindOptions,
    ) -> Result<bool> {
        if *self.display_mode.read() != TextDisplayMode::Text {
            return Ok(false);
        }
        if query.is_empty() {
            return Ok(false);
        }
        self.set_mode(TextViewerMode::Edit);
        let haystack = self.displayed_text();
        let lo = (*self.find_anchor.lock()).max(0) as usize;
        let hi = (*self.find_cursor.lock()).max(0) as usize;
        let selected = haystack.get(lo..hi).unwrap_or("");
        let re = search::compile_query(query, opts)?;
        let already = !selected.is_empty() && re.is_match(selected) && selected.len() == hi - lo;
        if !already && !self.find(query, true, opts)? {
            return Ok(false);
        }
        let lo = (*self.find_anchor.lock()).max(0) as usize;
        let hi = (*self.find_cursor.lock()).max(0) as usize;
        let start = self.cursor_at_byte(lo)?;
        let end = self.cursor_at_byte(hi)?;
        if start != end {
            self.delete(start, end)?;
        }
        if !replacement.is_empty() {
            *self.cursor.write() = start;
            self.insert(replacement)?;
        }
        let _ = self.find(query, true, opts)?;
        Ok(true)
    }

    /// Replace every match in the text view.
    ///
    /// # Errors
    ///
    /// Invalid regex, or the buffer cannot be edited.
    pub fn replace_all(&self, query: &str, replacement: &str, opts: FindOptions) -> Result<usize> {
        if *self.display_mode.read() != TextDisplayMode::Text {
            return Ok(0);
        }
        if query.is_empty() {
            return Ok(0);
        }
        self.set_mode(TextViewerMode::Edit);
        let haystack = self.displayed_text();
        let matches = search::collect_matches(&haystack, query, opts)?;
        if matches.is_empty() {
            self.clear_find();
            return Ok(0);
        }
        let mut out = String::with_capacity(haystack.len());
        let mut last = 0usize;
        for (s, e) in &matches {
            out.push_str(&haystack[last..*s]);
            out.push_str(replacement);
            last = *e;
        }
        out.push_str(&haystack[last..]);
        self.replace_content(&out)?;
        self.clear_find();
        Ok(matches.len())
    }

    fn cursor_at_byte(&self, byte: usize) -> Result<CursorPos> {
        let guard = self.buffer.read();
        let buffer = guard.as_ref().ok_or(ViewerError::EditOutOfBounds)?;
        let char_idx = buffer.byte_to_char(byte);
        let (line, column) = buffer.line_col_at_char(char_idx);
        Ok(CursorPos { line, column })
    }

    /// Whether undo has a frame.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.undo.lock().can_undo()
    }

    /// Whether redo has a frame.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.undo.lock().can_redo()
    }
}

async fn load_text_source(
    path: &orchid_fs::FsPath,
    provider: &dyn orchid_fs::FsProvider,
    size_limit: u64,
) -> Result<(TextBuffer, RawBytes)> {
    if let Ok(os) = path.to_local() {
        return tokio::task::spawn_blocking(move || load_local_text(&os, size_limit))
            .await
            .map_err(|e| ViewerError::TextDecode(format!("join: {e}")))?;
    }
    let bytes = provider.read(path).await?;
    if bytes.len() as u64 > size_limit {
        return Err(ViewerError::FileTooLarge {
            size: bytes.len() as u64,
            limit: size_limit,
        });
    }
    let buffer = TextBuffer::from_bytes(&bytes)?;
    Ok((buffer, RawBytes::from_vec(bytes)))
}

fn load_local_text(os: &Path, size_limit: u64) -> Result<(TextBuffer, RawBytes)> {
    let meta = std::fs::metadata(os)?;
    if meta.len() > size_limit {
        return Err(ViewerError::FileTooLarge {
            size: meta.len(),
            limit: size_limit,
        });
    }
    if meta.len() == 0 {
        return Ok((TextBuffer::from_bytes(b"")?, RawBytes::Empty));
    }
    let file = std::fs::File::open(os)?;
    match unsafe { memmap2::Mmap::map(&file) } {
        Ok(map) => {
            let buffer = TextBuffer::from_bytes(&map)?;
            Ok((buffer, RawBytes::Mapped(map)))
        }
        Err(_) => {
            let bytes = std::fs::read(os)?;
            let buffer = TextBuffer::from_bytes(&bytes)?;
            Ok((buffer, RawBytes::from_vec(bytes)))
        }
    }
}

#[async_trait]
impl Viewer for TextViewer {
    fn type_id(&self) -> &'static str {
        "text"
    }

    async fn open(
        &mut self,
        path: orchid_fs::FsPath,
        registry: Arc<orchid_fs::FsProviderRegistry>,
    ) -> Result<()> {
        let provider = registry
            .for_path(&path)
            .ok_or_else(|| orchid_fs::FsError::ProviderNotFound(path.scheme().to_string()))?;
        let (buffer, raw) = load_text_source(&path, provider.as_ref(), self.size_limit).await?;
        let sample = raw.as_slice();
        let language = detect_language(&path, &sample[..sample.len().min(512)]);
        *self.language.write() = language.to_string();
        *self.buffer.write() = Some(buffer);
        *self.raw_bytes.write() = raw;
        *self.path.write() = Some(path);
        *self.registry.write() = Some(registry);
        *self.cursor.write() = CursorPos::default();
        *self.first_visible_line.write() = 0;
        *self.display_mode.write() = TextDisplayMode::Text;
        *self.display_cache.lock() = None;
        *self.find_gen.lock() = 0;
        *self.find_anchor.lock() = 0;
        *self.find_cursor.lock() = 0;
        *self.find_match_index.lock() = 0;
        *self.find_match_count.lock() = 0;
        self.content_generation.store(0, Ordering::Relaxed);
        *self.parse_cache.lock() = None;
        *self.plain_cache.lock() = None;
        self.undo.lock().clear();
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        *self.buffer.write() = None;
        *self.path.write() = None;
        *self.registry.write() = None;
        *self.raw_bytes.write() = RawBytes::Empty;
        *self.display_mode.write() = TextDisplayMode::Text;
        *self.display_cache.lock() = None;
        self.content_generation.store(0, Ordering::Relaxed);
        *self.parse_cache.lock() = None;
        *self.plain_cache.lock() = None;
        self.undo.lock().clear();
        Ok(())
    }

    fn snapshot(&self) -> ViewerSnapshot {
        let path_guard = self.path.read();
        let path_display = path_guard
            .as_ref()
            .map(|p| p.as_str().to_string())
            .unwrap_or_default();
        let buffer_guard = self.buffer.read();
        let Some(buffer) = buffer_guard.as_ref() else {
            return ViewerSnapshot::Loading { path_display };
        };
        let first = *self.first_visible_line.read();
        let count = *self.visible_line_count.read();
        let display_mode = *self.display_mode.read();
        let hex_like = display_mode != TextDisplayMode::Text;
        let language = if hex_like {
            match display_mode {
                TextDisplayMode::Hex => "hex".into(),
                TextDisplayMode::Binary => "binary".into(),
                TextDisplayMode::Text => self.language.read().clone(),
            }
        } else {
            self.language.read().clone()
        };
        let highlighted = if hex_like {
            Vec::new()
        } else {
            self.highlight_visible(buffer, first, count)
        };
        let encoding = buffer.encoding().name().to_string();
        let line_ending = buffer.line_ending().label().to_string();
        let total = buffer.line_count();
        let dirty = buffer.is_dirty();
        let cursor = *self.cursor.read();
        drop(buffer_guard);
        let undo = self.undo.lock();
        let can_undo = undo.can_undo();
        let can_redo = undo.can_redo();
        drop(undo);
        let plain_text = self.displayed_text();
        ViewerSnapshot::Text(TextSnapshot {
            path_display,
            language,
            encoding,
            line_ending,
            dirty,
            read_only: hex_like || *self.mode.read() == TextViewerMode::Read,
            total_lines: total,
            visible_lines: highlighted,
            first_visible_line: first,
            cursor_line: cursor.line,
            cursor_column: cursor.column,
            selection: *self.selection.read(),
            info_text: String::new(),
            plain_text,
            display_mode: display_mode.as_u8(),
            find_gen: *self.find_gen.lock(),
            find_anchor: *self.find_anchor.lock(),
            find_cursor: *self.find_cursor.lock(),
            find_match_index: *self.find_match_index.lock(),
            find_match_count: *self.find_match_count.lock(),
            can_undo,
            can_redo,
        })
    }

    fn is_dirty(&self) -> bool {
        self.buffer.read().as_ref().is_some_and(|b| b.is_dirty())
    }

    async fn save(&mut self) -> Result<()> {
        let path = self.path.read().clone();
        let registry = self.registry.read().clone();
        let (Some(path), Some(registry)) = (path, registry) else {
            return Err(ViewerError::EditOutOfBounds);
        };
        let bytes = {
            let guard = self.buffer.read();
            let Some(buffer) = guard.as_ref() else {
                return Err(ViewerError::EditOutOfBounds);
            };
            buffer.to_bytes()?
        };
        let provider = registry
            .for_path(&path)
            .ok_or_else(|| orchid_fs::FsError::ProviderNotFound(path.scheme().to_string()))?;
        let tmp_raw = format!("{}.orchid-save", path.as_str());
        let tmp = orchid_fs::FsPath::new(tmp_raw)?;
        provider.write(&tmp, &bytes).await?;
        provider.rename(&tmp, &path).await?;
        if let Some(buffer) = self.buffer.write().as_mut() {
            buffer.mark_clean();
        }
        *self.raw_bytes.write() = RawBytes::from_vec(bytes);
        *self.display_cache.lock() = None;
        Ok(())
    }

    fn current_path(&self) -> Option<&orchid_fs::FsPath> {
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn cursor_after_insert(start: CursorPos, text: &str) -> CursorPos {
    if !text.contains('\n') {
        return CursorPos {
            line: start.line,
            column: start.column + text.chars().count() as u32,
        };
    }
    let last = text.rsplit('\n').next().unwrap_or("");
    let extra_lines = text.bytes().filter(|&b| b == b'\n').count() as u32;
    CursorPos {
        line: start.line + extra_lines,
        column: last.chars().count() as u32,
    }
}

fn point_after_insert(start: Point, text: &str) -> Point {
    if !text.contains('\n') {
        return Point {
            row: start.row,
            column: start.column + text.len(),
        };
    }
    let last = text.rsplit('\n').next().unwrap_or("");
    let extra_lines = text.bytes().filter(|&b| b == b'\n').count();
    Point {
        row: start.row + extra_lines,
        column: last.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::SyntaxScope;
    use crate::text::syntax::SyntaxHighlighter;
    use std::sync::Arc;

    fn viewer() -> TextViewer {
        TextViewer::new(Arc::new(SyntaxHighlighter::new()))
    }

    #[test]
    fn local_mmap_round_trips_utf8() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.txt");
        std::fs::write(&path, "hello\nворлд\n").unwrap();
        let (buffer, raw) = load_local_text(&path, DEFAULT_SIZE_LIMIT).unwrap();
        assert!(matches!(raw, RawBytes::Mapped(_)));
        assert_eq!(buffer.plain_text(), "hello\nворлд\n");
        assert_eq!(
            raw.as_slice(),
            b"hello\n\xd0\xb2\xd0\xbe\xd1\x80\xd0\xbb\xd0\xb4\n"
        );
    }

    #[test]
    fn set_mode_round_trips() {
        let tv = viewer();
        assert_eq!(*tv.mode.read(), TextViewerMode::Read);
        tv.set_mode(TextViewerMode::Edit);
        assert_eq!(*tv.mode.read(), TextViewerMode::Edit);
    }

    #[test]
    fn insert_requires_edit_mode() {
        let tv = viewer();
        *tv.buffer.write() = Some(TextBuffer::from_bytes(b"hi").unwrap());
        assert!(tv.insert("x").is_err());
        tv.set_mode(TextViewerMode::Edit);
        assert!(tv.insert("x").is_ok());
        assert_eq!(
            tv.buffer.read().as_ref().unwrap().line(0).as_deref(),
            Some("xhi")
        );
        assert!(tv.is_dirty());
    }

    #[test]
    fn delete_undo_redo() {
        let tv = viewer();
        *tv.buffer.write() = Some(TextBuffer::from_bytes(b"abcd").unwrap());
        tv.set_mode(TextViewerMode::Edit);
        tv.delete(
            CursorPos { line: 0, column: 1 },
            CursorPos { line: 0, column: 3 },
        )
        .unwrap();
        assert_eq!(
            tv.buffer.read().as_ref().unwrap().line(0).as_deref(),
            Some("ad")
        );
        tv.undo().unwrap();
        assert_eq!(
            tv.buffer.read().as_ref().unwrap().line(0).as_deref(),
            Some("abcd")
        );
        tv.redo().unwrap();
        assert_eq!(
            tv.buffer.read().as_ref().unwrap().line(0).as_deref(),
            Some("ad")
        );
    }

    #[test]
    fn replace_content_incremental_insert_keeps_undo() {
        let tv = viewer();
        *tv.buffer.write() = Some(TextBuffer::from_bytes(b"abc").unwrap());
        tv.set_mode(TextViewerMode::Edit);
        tv.replace_content("abXc").unwrap();
        assert_eq!(tv.buffer.read().as_ref().unwrap().plain_text(), "abXc");
        tv.undo().unwrap();
        assert_eq!(tv.buffer.read().as_ref().unwrap().plain_text(), "abc");
    }

    #[test]
    fn replace_content_marks_dirty() {
        let tv = viewer();
        *tv.buffer.write() = Some(TextBuffer::from_bytes(b"old").unwrap());
        tv.set_mode(TextViewerMode::Edit);
        tv.replace_content("new text").unwrap();
        assert_eq!(tv.buffer.read().as_ref().unwrap().plain_text(), "new text");
        assert!(tv.is_dirty());
        let snap = tv.snapshot();
        let ViewerSnapshot::Text(t) = snap else {
            panic!("expected text snapshot");
        };
        assert!(t.dirty);
        assert!(!t.read_only);
        assert_eq!(t.plain_text.as_ref(), "new text");
    }

    #[test]
    fn incremental_edit_keeps_highlight_cache_in_sync() {
        let tv = viewer();
        *tv.language.write() = "rust".into();
        *tv.buffer.write() =
            Some(TextBuffer::from_bytes(b"fn main() {\n    let x = 1;\n}\n").unwrap());
        tv.set_mode(TextViewerMode::Edit);
        tv.set_visible_range(0, 3);

        // Prime the parse cache via snapshot.
        let first = tv.snapshot();
        let ViewerSnapshot::Text(t0) = first else {
            panic!("expected text");
        };
        assert!(
            t0.visible_lines
                .iter()
                .any(|l| l.segments.iter().any(|s| s.scope != SyntaxScope::Plain)),
            "expected initial highlight"
        );
        let gen_before = tv.content_generation.load(Ordering::Relaxed);
        assert!(tv.parse_cache.lock().is_some());

        // Insert inside the function body.
        *tv.cursor.write() = CursorPos { line: 1, column: 4 };
        tv.insert("let y = 2;\n    ").unwrap();

        let gen_after = tv.content_generation.load(Ordering::Relaxed);
        assert!(gen_after > gen_before);
        let cache = tv.parse_cache.lock();
        let cached = cache
            .as_ref()
            .expect("cache should survive incremental edit");
        assert_eq!(cached.generation, gen_after);
        assert!(cached.source.contains("let y = 2;"));
        drop(cache);

        let second = tv.snapshot();
        let ViewerSnapshot::Text(t1) = second else {
            panic!("expected text");
        };
        assert!(t1.plain_text.contains("let y = 2;"));
        assert!(
            t1.visible_lines
                .iter()
                .any(|l| l.segments.iter().any(|s| s.scope != SyntaxScope::Plain)),
            "expected highlight after incremental edit"
        );
    }

    #[test]
    fn cursor_after_multiline_insert() {
        let start = CursorPos { line: 2, column: 3 };
        assert_eq!(
            cursor_after_insert(start, "abc"),
            CursorPos { line: 2, column: 6 }
        );
        assert_eq!(
            cursor_after_insert(start, "a\nb\nc"),
            CursorPos { line: 4, column: 1 }
        );
    }

    #[test]
    fn hex_mode_exposes_dump_and_forces_read_only() {
        let tv = viewer();
        *tv.buffer.write() = Some(TextBuffer::from_bytes(b"Hi").unwrap());
        *tv.raw_bytes.write() = RawBytes::from_vec(b"Hi".to_vec());
        tv.set_mode(TextViewerMode::Edit);
        tv.set_display_mode(TextDisplayMode::Hex);
        let ViewerSnapshot::Text(t) = tv.snapshot() else {
            panic!("expected text");
        };
        assert!(t.read_only);
        assert_eq!(t.display_mode, 1);
        assert!(t.plain_text.contains("48 69"));
    }

    #[test]
    fn encoding_switch_redecodes_bytes() {
        let tv = viewer();
        // "Привет" in windows-1251
        let bytes = [0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2];
        *tv.buffer.write() =
            Some(TextBuffer::from_bytes_with_encoding(&bytes, "windows-1251").unwrap());
        *tv.raw_bytes.write() = RawBytes::from_vec(bytes.to_vec());
        tv.set_encoding("windows-1251").unwrap();
        assert!(tv
            .buffer
            .read()
            .as_ref()
            .unwrap()
            .plain_text()
            .contains("Привет"));
        tv.set_encoding("UTF-8").unwrap();
        assert!(tv.buffer.read().as_ref().unwrap().is_dirty());
    }

    #[test]
    fn regex_replace_all_multiline() {
        let tv = viewer();
        *tv.buffer.write() = Some(TextBuffer::from_bytes(b"foo\nbar\nfoo").unwrap());
        *tv.raw_bytes.write() = RawBytes::from_vec(b"foo\nbar\nfoo".to_vec());
        let n = tv
            .replace_all(
                "foo",
                "x",
                FindOptions {
                    regex: false,
                    multiline: true,
                },
            )
            .unwrap();
        assert_eq!(n, 2);
        assert_eq!(tv.buffer.read().as_ref().unwrap().plain_text(), "x\nbar\nx");
    }
}
