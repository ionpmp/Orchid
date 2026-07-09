//! Text viewer / editor.

pub mod buffer;
pub mod grammars;
pub mod save;
pub mod syntax;
pub mod undo;

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::{Mutex, RwLock};

use crate::error::{Result, ViewerError};
use crate::snapshot::{SelectionRange, TextSnapshot, ViewerSnapshot};
use crate::viewer_trait::Viewer;

pub use buffer::{LineEnding, TextBuffer};
pub use grammars::{detect_language, PLAINTEXT};
pub use save::save_text;
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

/// Text viewer / editor.
pub struct TextViewer {
    path: RwLock<Option<orchid_fs::FsPath>>,
    buffer: RwLock<Option<TextBuffer>>,
    language: RwLock<String>,
    highlighter: Arc<SyntaxHighlighter>,
    undo: Mutex<UndoStack>,
    cursor: RwLock<CursorPos>,
    selection: RwLock<Option<SelectionRange>>,
    first_visible_line: RwLock<u32>,
    visible_line_count: RwLock<u32>,
    mode: RwLock<TextViewerMode>,
    registry: RwLock<Option<Arc<orchid_fs::FsProviderRegistry>>>,
    size_limit: u64,
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
            undo: Mutex::new(UndoStack::new(500)),
            cursor: RwLock::new(CursorPos::default()),
            selection: RwLock::new(None),
            first_visible_line: RwLock::new(0),
            visible_line_count: RwLock::new(64),
            mode: RwLock::new(TextViewerMode::Read),
            registry: RwLock::new(None),
            size_limit: DEFAULT_SIZE_LIMIT,
        }
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
        let mut buffer = self.buffer.write();
        let Some(buffer) = buffer.as_mut() else {
            return Err(ViewerError::EditOutOfBounds);
        };
        buffer.insert(cursor.line, cursor.column, text)?;
        let advance_cols = text.chars().count() as u32;
        *self.cursor.write() = CursorPos {
            line: cursor.line,
            column: cursor.column + advance_cols,
        };
        self.undo.lock().push(TextOp {
            kind: TextOpKind::Insert,
            start_line: cursor.line,
            start_column: cursor.column,
            end_line: cursor.line,
            end_column: cursor.column + advance_cols,
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
        let mut buffer = self.buffer.write();
        let Some(buffer) = buffer.as_mut() else {
            return Err(ViewerError::EditOutOfBounds);
        };
        let deleted = buffer
            .text_range(start.line, start.column, end.line, end.column)?
            .to_string();
        buffer.delete(start.line, start.column, end.line, end.column)?;
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

    /// Replace the entire buffer with `text` (plain edit-mode push from the UI).
    ///
    /// Full-document replaces clear the undo stack (MVP; typing undo uses insert/delete).
    ///
    /// # Errors
    ///
    /// [`ViewerError::EditOutOfBounds`] when not in edit mode or no buffer is open.
    pub fn replace_content(&self, text: &str) -> Result<()> {
        if *self.mode.read() != TextViewerMode::Edit {
            return Err(ViewerError::EditOutOfBounds);
        }
        let mut buffer = self.buffer.write();
        let Some(buffer) = buffer.as_mut() else {
            return Err(ViewerError::EditOutOfBounds);
        };
        if buffer.plain_text() == text.replace("\r\n", "\n") {
            return Ok(());
        }
        buffer.replace_content(text);
        self.undo.lock().clear();
        Ok(())
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
        let mut buffer = self.buffer.write();
        let Some(buffer) = buffer.as_mut() else {
            return Err(ViewerError::EditOutOfBounds);
        };
        match op.kind {
            TextOpKind::Insert => {
                buffer.delete(op.start_line, op.start_column, op.end_line, op.end_column)?;
                *self.cursor.write() = CursorPos {
                    line: op.start_line,
                    column: op.start_column,
                };
            }
            TextOpKind::Delete => {
                buffer.insert(op.start_line, op.start_column, &op.text)?;
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
        let mut buffer = self.buffer.write();
        let Some(buffer) = buffer.as_mut() else {
            return Err(ViewerError::EditOutOfBounds);
        };
        match op.kind {
            TextOpKind::Insert => {
                buffer.insert(op.start_line, op.start_column, &op.text)?;
                *self.cursor.write() = CursorPos {
                    line: op.end_line,
                    column: op.end_column,
                };
            }
            TextOpKind::Delete => {
                buffer.delete(op.start_line, op.start_column, op.end_line, op.end_column)?;
                *self.cursor.write() = CursorPos {
                    line: op.start_line,
                    column: op.start_column,
                };
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
        let bytes = provider.read(&path).await?;
        if bytes.len() as u64 > self.size_limit {
            return Err(ViewerError::FileTooLarge {
                size: bytes.len() as u64,
                limit: self.size_limit,
            });
        }
        let language = detect_language(&path, &bytes[..bytes.len().min(512)]);
        let buffer = TextBuffer::from_bytes(&bytes)?;
        *self.language.write() = language.to_string();
        *self.buffer.write() = Some(buffer);
        *self.path.write() = Some(path);
        *self.registry.write() = Some(registry);
        *self.cursor.write() = CursorPos::default();
        *self.first_visible_line.write() = 0;
        self.undo.lock().clear();
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        *self.buffer.write() = None;
        *self.path.write() = None;
        *self.registry.write() = None;
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
        let slice = buffer.visible_slice(first, count);
        let source = slice.join("\n");
        let language = self.language.read().clone();
        let highlighted =
            self.highlighter
                .highlight_lines(&language, &source, first, count.min(buffer.line_count()));
        let encoding = buffer.encoding().name().to_string();
        let line_ending = buffer.line_ending().label().to_string();
        let total = buffer.line_count();
        let cursor = *self.cursor.read();
        ViewerSnapshot::Text(TextSnapshot {
            path_display,
            language,
            encoding,
            line_ending,
            dirty: buffer.is_dirty(),
            read_only: *self.mode.read() == TextViewerMode::Read,
            total_lines: total,
            visible_lines: highlighted,
            first_visible_line: first,
            cursor_line: cursor.line,
            cursor_column: cursor.column,
            selection: *self.selection.read(),
            info_text: String::new(),
            plain_text: buffer.plain_text(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::syntax::SyntaxHighlighter;
    use std::sync::Arc;

    fn viewer() -> TextViewer {
        TextViewer::new(Arc::new(SyntaxHighlighter::new()))
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
        assert_eq!(tv.buffer.read().as_ref().unwrap().line(0).as_deref(), Some("xhi"));
        assert!(tv.is_dirty());
    }

    #[test]
    fn delete_undo_redo() {
        let tv = viewer();
        *tv.buffer.write() = Some(TextBuffer::from_bytes(b"abcd").unwrap());
        tv.set_mode(TextViewerMode::Edit);
        tv.delete(CursorPos { line: 0, column: 1 }, CursorPos { line: 0, column: 3 })
            .unwrap();
        assert_eq!(tv.buffer.read().as_ref().unwrap().line(0).as_deref(), Some("ad"));
        tv.undo().unwrap();
        assert_eq!(tv.buffer.read().as_ref().unwrap().line(0).as_deref(), Some("abcd"));
        tv.redo().unwrap();
        assert_eq!(tv.buffer.read().as_ref().unwrap().line(0).as_deref(), Some("ad"));
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
        assert_eq!(t.plain_text, "new text");
    }
}
