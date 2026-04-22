//! Text viewer / editor.

pub mod buffer;
pub mod grammars;
pub mod save;
pub mod syntax;
pub mod undo;

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

    /// Set the visible window for snapshots.
    pub fn set_visible_range(&self, first_line: u32, count: u32) {
        *self.first_visible_line.write() = first_line;
        *self.visible_line_count.write() = count.max(1);
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
        let info = format!(
            "{encoding}, {line_ending}, {language}, {total} lines"
        );
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
            info_text: info,
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
}
