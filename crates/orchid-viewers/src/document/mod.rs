//! DOCX-compatible document viewer/editor (Tier 1 rich text).

pub mod cursor;
pub mod layout;
pub mod model;
pub mod ooxml;
pub mod undo;

use std::any::Any;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::{Mutex, RwLock};

use crate::error::{Result, ViewerError};
use crate::snapshot::{DocumentSnapshot, ViewerSnapshot};
use crate::viewer_trait::Viewer;

pub use cursor::{
    cursor_from_plain_offset, paragraph_indices_in_selection, plain_offset_from_cursor,
    selection_from_plain_offsets, Cursor, Selection,
};
pub use layout::{DEFAULT_PREVIEW_WIDTH, DocumentLayout};
pub use model::{
    Alignment, Block, Document, ImageFormat, InlineImage, ListKind, OpaqueXmlNode, PageSetup,
    Paragraph, Run, RunStyle, Table, TableCell, TableRow,
};
pub use undo::{EditCommand, RunStylePatch, UndoStack};

/// Soft ceiling for DOCX payloads accepted by the viewer (128 MiB).
pub const DEFAULT_SIZE_LIMIT: u64 = 128 * 1024 * 1024;

struct PreviewState {
    width: f32,
    bytes: Arc<Vec<u8>>,
    width_px: u32,
    height_px: u32,
    valid: bool,
    /// Plain-text selection baked into `bytes` (`start == end` → caret only).
    sel_start: usize,
    sel_end: usize,
}

impl Default for PreviewState {
    fn default() -> Self {
        Self {
            width: DEFAULT_PREVIEW_WIDTH,
            bytes: Arc::new(Vec::new()),
            width_px: 0,
            height_px: 0,
            valid: false,
            sel_start: 0,
            sel_end: 0,
        }
    }
}

/// Document viewer / editor for `.docx` (Office Open XML).
pub struct DocumentViewer {
    path: RwLock<Option<orchid_fs::FsPath>>,
    document: RwLock<Option<Document>>,
    undo: Mutex<UndoStack>,
    warnings: RwLock<Vec<String>>,
    registry: RwLock<Option<Arc<orchid_fs::FsProviderRegistry>>>,
    size_limit: u64,
    layout: Mutex<DocumentLayout>,
    preview: Mutex<PreviewState>,
    source_mode: RwLock<bool>,
    selection: Mutex<Selection>,
    /// Plain-text offset captured on preview pointer-down (drag selection).
    preview_drag_anchor: Mutex<Option<usize>>,
}

impl std::fmt::Debug for DocumentViewer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocumentViewer")
            .field(
                "path",
                &self.path.read().as_ref().map(|p| p.as_str().to_string()),
            )
            .finish_non_exhaustive()
    }
}

impl Default for DocumentViewer {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentViewer {
    /// Build an empty document viewer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            path: RwLock::new(None),
            document: RwLock::new(None),
            undo: Mutex::new(UndoStack::new()),
            warnings: RwLock::new(Vec::new()),
            registry: RwLock::new(None),
            size_limit: DEFAULT_SIZE_LIMIT,
            layout: Mutex::new(DocumentLayout::new()),
            preview: Mutex::new(PreviewState::default()),
            source_mode: RwLock::new(false),
            selection: Mutex::new(Selection {
                anchor: Cursor::default(),
                head: Cursor::default(),
            }),
            preview_drag_anchor: Mutex::new(None),
        }
    }

    fn invalidate_preview(&self) {
        self.preview.lock().valid = false;
    }

    /// Toggle plain-text source editor vs rich preview.
    pub fn set_source_mode(&self, source: bool) {
        *self.source_mode.write() = source;
    }

    /// Whether the UI should show the plain-text editor.
    #[must_use]
    pub fn source_mode(&self) -> bool {
        *self.source_mode.read()
    }

    /// Set the content width used for preview layout (CSS pixels).
    pub fn set_preview_width(&self, width: f32) {
        let mut prev = self.preview.lock();
        if (prev.width - width).abs() > 0.5 {
            prev.width = width.max(120.0);
            prev.valid = false;
        }
    }

    /// Update the active selection from plain-text UTF-8 byte offsets.
    pub fn set_selection_plain_offsets(&self, anchor: usize, head: usize) {
        let doc_guard = self.document.read();
        let Some(doc) = doc_guard.as_ref() else {
            return;
        };
        *self.selection.lock() = selection_from_plain_offsets(doc, anchor, head);
    }

    /// Handle a pointer event on the preview canvas (`0`=down, `1`=move, `2`=up).
    ///
    /// Coordinates are CSS pixels in the rendered preview image.
    pub fn preview_pointer(&self, phase: u8, x: f32, y: f32) {
        let doc_guard = self.document.read();
        let Some(doc) = doc_guard.as_ref() else {
            return;
        };
        let width = self.preview.lock().width;
        let Some(offset) = self
            .layout
            .lock()
            .hit_test_plain_offset(doc, width, x, y)
        else {
            return;
        };
        match phase {
            0 => {
                *self.preview_drag_anchor.lock() = Some(offset);
                *self.selection.lock() = selection_from_plain_offsets(doc, offset, offset);
            }
            1 | 2 => {
                let anchor = self
                    .preview_drag_anchor
                    .lock()
                    .unwrap_or(offset);
                *self.selection.lock() = selection_from_plain_offsets(doc, anchor, offset);
                if phase == 2 {
                    *self.preview_drag_anchor.lock() = None;
                }
            }
            _ => {}
        }
    }

    /// Insert `text` at the preview caret (replacing a non-empty selection).
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`] / edit bounds errors.
    pub fn preview_insert_text(&self, text: &str) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        let mut doc_guard = self.document.write();
        let doc = doc_guard
            .as_mut()
            .ok_or(ViewerError::DocumentNotOpen)?;
        let sel = *self.selection.lock();
        let at = self.delete_selection_if_needed(doc, sel)?;
        self.undo.lock().push(
            doc,
            EditCommand::InsertText {
                at,
                text: text.to_string(),
            },
        )?;
        let caret = Cursor {
            block_idx: at.block_idx,
            run_idx: at.run_idx,
            byte_offset: at.byte_offset + text.len(),
        };
        *self.selection.lock() = Selection {
            anchor: caret,
            head: caret,
        };
        self.invalidate_preview();
        Ok(())
    }

    /// Insert a paragraph break at the preview caret.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`] / edit bounds errors.
    pub fn preview_insert_paragraph_break(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard
            .as_mut()
            .ok_or(ViewerError::DocumentNotOpen)?;
        let sel = *self.selection.lock();
        let at = self.delete_selection_if_needed(doc, sel)?;
        let next = split_paragraph_blocks(doc, at)?;
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks: next })?;
        let caret = Cursor {
            block_idx: at.block_idx + 1,
            run_idx: 0,
            byte_offset: 0,
        };
        *self.selection.lock() = Selection {
            anchor: caret,
            head: caret,
        };
        self.invalidate_preview();
        Ok(())
    }

    /// Delete the selection, or one grapheme/cluster backward if collapsed.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn preview_delete_backward(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard
            .as_mut()
            .ok_or(ViewerError::DocumentNotOpen)?;
        let sel = *self.selection.lock();
        if !sel.is_collapsed() {
            let at = self.delete_selection_if_needed(doc, sel)?;
            *self.selection.lock() = Selection {
                anchor: at,
                head: at,
            };
            self.invalidate_preview();
            return Ok(());
        }
        let head_off = plain_offset_from_cursor(doc, sel.head);
        if head_off == 0 {
            return Ok(());
        }
        let plain = doc.plain_text();
        let prev = prev_char_boundary(&plain, head_off);
        let range = selection_from_plain_offsets(doc, prev, head_off);
        let at = self.delete_selection_if_needed(doc, range)?;
        *self.selection.lock() = Selection {
            anchor: at,
            head: at,
        };
        self.invalidate_preview();
        Ok(())
    }

    /// Delete the selection, or one character forward if collapsed.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn preview_delete_forward(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard
            .as_mut()
            .ok_or(ViewerError::DocumentNotOpen)?;
        let sel = *self.selection.lock();
        if !sel.is_collapsed() {
            let at = self.delete_selection_if_needed(doc, sel)?;
            *self.selection.lock() = Selection {
                anchor: at,
                head: at,
            };
            self.invalidate_preview();
            return Ok(());
        }
        let head_off = plain_offset_from_cursor(doc, sel.head);
        let plain = doc.plain_text();
        if head_off >= plain.len() {
            return Ok(());
        }
        let next = next_char_boundary(&plain, head_off);
        let range = selection_from_plain_offsets(doc, head_off, next);
        let at = self.delete_selection_if_needed(doc, range)?;
        *self.selection.lock() = Selection {
            anchor: at,
            head: at,
        };
        self.invalidate_preview();
        Ok(())
    }

    /// Move the caret by `delta` Unicode scalars (`extend` keeps the anchor).
    pub fn preview_move_by_chars(&self, delta: i32, extend: bool) {
        let doc_guard = self.document.read();
        let Some(doc) = doc_guard.as_ref() else {
            return;
        };
        let sel = *self.selection.lock();
        let plain = doc.plain_text();
        let mut off = plain_offset_from_cursor(doc, sel.head);
        if delta < 0 {
            for _ in 0..(-delta as usize) {
                if off == 0 {
                    break;
                }
                off = prev_char_boundary(&plain, off);
            }
        } else {
            for _ in 0..(delta as usize) {
                if off >= plain.len() {
                    break;
                }
                off = next_char_boundary(&plain, off);
            }
        }
        let head = cursor_from_plain_offset(doc, off);
        let anchor = if extend { sel.anchor } else { head };
        *self.selection.lock() = Selection { anchor, head };
        // Caret paint updates via snapshot selection cache.
    }

    /// Delete `sel` when non-empty; returns the caret where typing should continue.
    fn delete_selection_if_needed(&self, doc: &mut Document, sel: Selection) -> Result<Cursor> {
        if sel.is_collapsed() {
            return Ok(sel.head);
        }
        let (start, end) = sel.normalized();
        if start.block_idx == end.block_idx {
            self.undo.lock().push(
                doc,
                EditCommand::DeleteRange {
                    range: Selection {
                        anchor: start,
                        head: end,
                    },
                },
            )?;
            return Ok(start);
        }
        let next = delete_multi_paragraph(doc, start, end)?;
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks: next })?;
        Ok(start)
    }

    /// Current selection.
    #[must_use]
    pub fn selection(&self) -> Selection {
        *self.selection.lock()
    }

    /// Borrow the loaded document model (for tests / UI commands).
    #[must_use]
    pub fn document(&self) -> parking_lot::RwLockReadGuard<'_, Option<Document>> {
        self.document.read()
    }

    /// Mutable borrow of the loaded document.
    pub fn document_mut(&self) -> parking_lot::RwLockWriteGuard<'_, Option<Document>> {
        self.document.write()
    }

    /// Apply an edit command and push it onto the undo stack.
    ///
    /// # Errors
    ///
    /// Returns [`ViewerError::EditOutOfBounds`] when the command targets an
    /// invalid range, or [`ViewerError::DocumentNotOpen`] when nothing is loaded.
    pub fn apply(&self, cmd: EditCommand) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard
            .as_mut()
            .ok_or(ViewerError::DocumentNotOpen)?;
        self.undo.lock().push(doc, cmd)?;
        self.invalidate_preview();
        Ok(())
    }

    /// Undo the last edit.
    pub fn undo(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard
            .as_mut()
            .ok_or(ViewerError::DocumentNotOpen)?;
        self.undo.lock().undo(doc)?;
        self.invalidate_preview();
        Ok(())
    }

    /// Redo the last undone edit.
    pub fn redo(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard
            .as_mut()
            .ok_or(ViewerError::DocumentNotOpen)?;
        self.undo.lock().redo(doc)?;
        self.invalidate_preview();
        Ok(())
    }

    /// Whether undo is available.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.undo.lock().can_undo()
    }

    /// Whether redo is available.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.undo.lock().can_redo()
    }

    /// Replace body content from plain text (paragraphs separated by blank lines).
    ///
    /// Preserves [`PageSetup`] and retained package parts. Rich formatting of
    /// previous runs is not preserved across a full text push.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`] when nothing is loaded.
    pub fn replace_plain_text(&self, text: &str) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard
            .as_mut()
            .ok_or(ViewerError::DocumentNotOpen)?;
        // Skip no-op pushes so typing churn does not stack identical bodies.
        let next = plain_text_to_blocks_preserving(doc, text);
        if doc.blocks == next {
            return Ok(());
        }
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks: next })?;
        self.invalidate_preview();
        Ok(())
    }

    /// Toggle a boolean character style on the current selection.
    ///
    /// Collapsed selection styles the run under the caret. Non-empty selection
    /// styles the covered text (runs are split at boundaries).
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn toggle_style_all(&self, which: char) -> Result<()> {
        // Kept name for widget API compatibility; scope is selection-aware.
        self.toggle_style_selection(which)
    }

    /// Selection-scoped character style toggle.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn toggle_style_selection(&self, which: char) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard
            .as_mut()
            .ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let style_at = style_at_cursor(doc, sel.normalized().0);
        let currently_on = style_at
            .map(|s| match which {
                'b' => s.bold,
                'i' => s.italic,
                'u' => s.underline,
                _ => false,
            })
            .unwrap_or(false);
        let patch = match which {
            'b' => RunStylePatch {
                bold: Some(!currently_on),
                ..Default::default()
            },
            'i' => RunStylePatch {
                italic: Some(!currently_on),
                ..Default::default()
            },
            'u' => RunStylePatch {
                underline: Some(!currently_on),
                ..Default::default()
            },
            _ => return Ok(()),
        };
        self.undo.lock().push(
            doc,
            EditCommand::SetRunStyle {
                range: sel,
                style: patch,
            },
        )?;
        self.invalidate_preview();
        Ok(())
    }

    /// Set alignment on paragraphs touched by the selection.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn set_alignment_all(&self, alignment: Alignment) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard
            .as_mut()
            .ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let indices = paragraph_indices_in_selection(doc, sel);
        if indices.is_empty() {
            return Ok(());
        }
        let mut next = doc.blocks.clone();
        for bi in indices {
            if let Some(Block::Paragraph(p)) = next.get_mut(bi) {
                p.alignment = alignment;
            }
        }
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks: next })?;
        self.invalidate_preview();
        Ok(())
    }

    /// Set list kind on paragraphs touched by the selection.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn set_list_all(&self, kind: ListKind) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard
            .as_mut()
            .ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let indices = paragraph_indices_in_selection(doc, sel);
        if indices.is_empty() {
            return Ok(());
        }
        let mut next = doc.blocks.clone();
        for bi in indices {
            if let Some(Block::Paragraph(p)) = next.get_mut(bi) {
                p.list = kind;
                p.num_id = crate::document::ooxml::numbering::num_id_for_kind(kind);
            }
        }
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks: next })?;
        self.invalidate_preview();
        Ok(())
    }

    /// Toggle bullet or numbered list on selected paragraphs.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn toggle_list_all(&self, kind: ListKind) -> Result<()> {
        let current = {
            let doc_guard = self.document.read();
            let doc = doc_guard.as_ref().ok_or(ViewerError::DocumentNotOpen)?;
            let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
            let indices = paragraph_indices_in_selection(doc, sel);
            indices
                .first()
                .and_then(|bi| match doc.blocks.get(*bi) {
                    Some(Block::Paragraph(p)) => Some(p.list),
                    _ => None,
                })
                .unwrap_or(ListKind::None)
        };
        let next = if current == kind {
            ListKind::None
        } else {
            kind
        };
        self.set_list_all(next)
    }
}

fn plain_text_to_blocks_preserving(doc: &Document, text: &str) -> Vec<Block> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = if normalized.is_empty() {
        vec![""]
    } else {
        normalized.split('\n').collect()
    };
    lines
        .into_iter()
        .enumerate()
        .map(|(idx, line)| {
            if let Some(Block::Paragraph(prev)) = doc.blocks.get(idx) {
                let style = prev
                    .runs
                    .first()
                    .map(|r| r.style.clone())
                    .unwrap_or_default();
                Block::Paragraph(Paragraph {
                    runs: vec![Run {
                        text: line.to_string(),
                        style,
                    }],
                    alignment: prev.alignment,
                    list: prev.list,
                    list_level: prev.list_level,
                    num_id: prev.num_id,
                    unsupported: prev.unsupported.clone(),
                })
            } else {
                Block::Paragraph(Paragraph {
                    runs: vec![Run {
                        text: line.to_string(),
                        style: RunStyle::default(),
                    }],
                    ..Default::default()
                })
            }
        })
        .collect()
}

fn first_paragraph(doc: &Document) -> Option<&Paragraph> {
    doc.blocks.iter().find_map(|b| match b {
        Block::Paragraph(p) => Some(p),
        _ => None,
    })
}

fn style_at_cursor(doc: &Document, cursor: Cursor) -> Option<RunStyle> {
    let Block::Paragraph(p) = doc.blocks.get(cursor.block_idx)? else {
        return None;
    };
    p.runs
        .get(cursor.run_idx)
        .map(|r| r.style.clone())
        .or_else(|| p.runs.first().map(|r| r.style.clone()))
}

fn effective_style_selection(doc: &Document, sel: Selection, _source_mode: bool) -> Selection {
    if !sel.is_collapsed() {
        return sel;
    }
    // Collapsed caret (Source or Preview click) → style the whole paragraph so
    // toolbar B/I/U / align / list remain one-click useful.
    expand_selection_to_paragraph(doc, sel.head)
}

fn expand_selection_to_paragraph(doc: &Document, cursor: Cursor) -> Selection {
    let Some(Block::Paragraph(p)) = doc.blocks.get(cursor.block_idx) else {
        return Selection {
            anchor: cursor,
            head: cursor,
        };
    };
    if p.runs.is_empty() {
        return Selection {
            anchor: Cursor {
                block_idx: cursor.block_idx,
                run_idx: 0,
                byte_offset: 0,
            },
            head: Cursor {
                block_idx: cursor.block_idx,
                run_idx: 0,
                byte_offset: 0,
            },
        };
    }
    let last = p.runs.len() - 1;
    Selection {
        anchor: Cursor {
            block_idx: cursor.block_idx,
            run_idx: 0,
            byte_offset: 0,
        },
        head: Cursor {
            block_idx: cursor.block_idx,
            run_idx: last,
            byte_offset: p.runs[last].text.len(),
        },
    }
}

fn prev_char_boundary(text: &str, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }
    let mut i = offset.min(text.len()) - 1;
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn next_char_boundary(text: &str, offset: usize) -> usize {
    if offset >= text.len() {
        return text.len();
    }
    let mut i = offset + 1;
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

fn split_paragraph_blocks(doc: &Document, at: Cursor) -> Result<Vec<Block>> {
    let Block::Paragraph(p) = doc
        .blocks
        .get(at.block_idx)
        .ok_or(ViewerError::EditOutOfBounds)?
    else {
        return Err(ViewerError::EditOutOfBounds);
    };
    let mut left_runs = Vec::new();
    let mut right_runs = Vec::new();
    if p.runs.is_empty() {
        left_runs.push(Run {
            text: String::new(),
            style: RunStyle::default(),
        });
    } else {
        for (ri, run) in p.runs.iter().enumerate() {
            if ri < at.run_idx {
                left_runs.push(run.clone());
            } else if ri > at.run_idx {
                right_runs.push(run.clone());
            } else {
                let split = at.byte_offset.min(run.text.len());
                left_runs.push(Run {
                    text: run.text[..split].to_string(),
                    style: run.style.clone(),
                });
                right_runs.push(Run {
                    text: run.text[split..].to_string(),
                    style: run.style.clone(),
                });
            }
        }
    }
    if left_runs.is_empty() {
        left_runs.push(Run {
            text: String::new(),
            style: RunStyle::default(),
        });
    }
    if right_runs.is_empty() {
        right_runs.push(Run {
            text: String::new(),
            style: left_runs
                .last()
                .map(|r| r.style.clone())
                .unwrap_or_default(),
        });
    }
    let left = Paragraph {
        runs: left_runs,
        alignment: p.alignment,
        list: p.list,
        list_level: p.list_level,
        num_id: p.num_id,
        unsupported: p.unsupported.clone(),
    };
    let right = Paragraph {
        runs: right_runs,
        alignment: p.alignment,
        list: ListKind::None,
        list_level: 0,
        num_id: None,
        unsupported: Vec::new(),
    };
    let mut blocks = doc.blocks.clone();
    blocks[at.block_idx] = Block::Paragraph(left);
    blocks.insert(at.block_idx + 1, Block::Paragraph(right));
    Ok(blocks)
}

fn delete_multi_paragraph(doc: &Document, start: Cursor, end: Cursor) -> Result<Vec<Block>> {
    let Block::Paragraph(start_p) = doc
        .blocks
        .get(start.block_idx)
        .ok_or(ViewerError::EditOutOfBounds)?
    else {
        return Err(ViewerError::EditOutOfBounds);
    };
    let Block::Paragraph(end_p) = doc
        .blocks
        .get(end.block_idx)
        .ok_or(ViewerError::EditOutOfBounds)?
    else {
        return Err(ViewerError::EditOutOfBounds);
    };

    let mut merged_runs = Vec::new();
    for (ri, run) in start_p.runs.iter().enumerate() {
        if ri < start.run_idx {
            merged_runs.push(run.clone());
        } else if ri == start.run_idx {
            merged_runs.push(Run {
                text: run.text[..start.byte_offset.min(run.text.len())].to_string(),
                style: run.style.clone(),
            });
        }
    }
    for (ri, run) in end_p.runs.iter().enumerate() {
        if ri > end.run_idx {
            merged_runs.push(run.clone());
        } else if ri == end.run_idx {
            merged_runs.push(Run {
                text: run.text[end.byte_offset.min(run.text.len())..].to_string(),
                style: run.style.clone(),
            });
        }
    }
    merged_runs.retain(|r| !r.text.is_empty());
    if merged_runs.is_empty() {
        merged_runs.push(Run {
            text: String::new(),
            style: RunStyle::default(),
        });
    }

    let merged = Paragraph {
        runs: merged_runs,
        alignment: start_p.alignment,
        list: start_p.list,
        list_level: start_p.list_level,
        num_id: start_p.num_id,
        unsupported: start_p.unsupported.clone(),
    };
    let mut blocks = Vec::with_capacity(doc.blocks.len());
    for (bi, block) in doc.blocks.iter().enumerate() {
        if bi < start.block_idx || bi > end.block_idx {
            blocks.push(block.clone());
        } else if bi == start.block_idx {
            blocks.push(Block::Paragraph(merged.clone()));
        }
    }
    Ok(blocks)
}

#[async_trait]
impl Viewer for DocumentViewer {
    fn type_id(&self) -> &'static str {
        "document"
    }

    async fn open(
        &mut self,
        path: orchid_fs::FsPath,
        registry: Arc<orchid_fs::FsProviderRegistry>,
    ) -> Result<()> {
        let provider = registry
            .for_path(&path)
            .ok_or_else(|| orchid_fs::FsError::ProviderNotFound(path.scheme().to_string()))?;

        let os_path = if path.is_local() {
            path.to_local()?
        } else {
            // Remote: materialise into a temp file for zip/xml parsing.
            let bytes = provider.read(&path).await?;
            if bytes.len() as u64 > self.size_limit {
                return Err(ViewerError::FileTooLarge {
                    size: bytes.len() as u64,
                    limit: self.size_limit,
                });
            }
            let tmp = std::env::temp_dir().join(format!(
                "orchid-docx-{}.docx",
                uuid::Uuid::new_v4()
            ));
            tokio::fs::write(&tmp, &bytes).await?;
            let doc = Document::from_docx(&tmp).await?;
            let _ = tokio::fs::remove_file(&tmp).await;
            *self.document.write() = Some(doc);
            *self.path.write() = Some(path);
            *self.registry.write() = Some(registry);
            *self.undo.lock() = UndoStack::new();
            *self.warnings.write() = Vec::new();
            *self.preview.lock() = PreviewState::default();
            *self.source_mode.write() = false;
            *self.selection.lock() = Selection {
                anchor: Cursor::default(),
                head: Cursor::default(),
            };
            return Ok(());
        };

        let meta = tokio::fs::metadata(&os_path).await?;
        if meta.len() > self.size_limit {
            return Err(ViewerError::FileTooLarge {
                size: meta.len(),
                limit: self.size_limit,
            });
        }

        let doc = Document::from_docx(Path::new(&os_path)).await?;
        *self.document.write() = Some(doc);
        *self.path.write() = Some(path);
        *self.registry.write() = Some(registry);
        *self.undo.lock() = UndoStack::new();
        *self.warnings.write() = Vec::new();
        *self.preview.lock() = PreviewState::default();
        *self.source_mode.write() = false;
        *self.selection.lock() = Selection {
            anchor: Cursor::default(),
            head: Cursor::default(),
        };
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        *self.document.write() = None;
        *self.path.write() = None;
        *self.registry.write() = None;
        *self.undo.lock() = UndoStack::new();
        *self.warnings.write() = Vec::new();
        *self.preview.lock() = PreviewState::default();
        *self.source_mode.write() = false;
        *self.selection.lock() = Selection {
            anchor: Cursor::default(),
            head: Cursor::default(),
        };
        Ok(())
    }

    fn snapshot(&self) -> ViewerSnapshot {
        let path_display = self
            .path
            .read()
            .as_ref()
            .map(|p| p.as_str().to_string())
            .unwrap_or_default();
        let doc_guard = self.document.read();
        let Some(doc) = doc_guard.as_ref() else {
            return ViewerSnapshot::Loading { path_display };
        };
        let undo = self.undo.lock();
        let dirty = undo.is_dirty();
        let can_undo = undo.can_undo();
        let can_redo = undo.can_redo();
        drop(undo);
        let warnings = self.warnings.read().clone();
        let plain_text = doc.plain_text();
        let block_count = doc.blocks.len() as u32;
        let sel = *self.selection.lock();
        let caret = sel.normalized().0;
        let style = style_at_cursor(doc, caret);
        let para = match doc.blocks.get(caret.block_idx) {
            Some(Block::Paragraph(p)) => Some(p),
            _ => first_paragraph(doc),
        };
        let (bold, italic, underline, alignment, list_kind) = (
            style.as_ref().is_some_and(|s| s.bold),
            style.as_ref().is_some_and(|s| s.italic),
            style.as_ref().is_some_and(|s| s.underline),
            match para.map(|p| p.alignment).unwrap_or_default() {
                Alignment::Left => 0,
                Alignment::Center => 1,
                Alignment::Right => 2,
                Alignment::Justify => 3,
            },
            match para.map(|p| p.list).unwrap_or_default() {
                ListKind::None => 0,
                ListKind::Bullet => 1,
                ListKind::Numbered => 2,
            },
        );

        let source_mode = *self.source_mode.read();
        let (sel_start, sel_end) = {
            let (a, b) = sel.normalized();
            (
                plain_offset_from_cursor(doc, a),
                plain_offset_from_cursor(doc, b),
            )
        };
        let (preview_rgba, preview_width_px, preview_height_px) = {
            let mut prev = self.preview.lock();
            let sel_changed = prev.sel_start != sel_start || prev.sel_end != sel_end;
            if !prev.valid || sel_changed {
                let mut layout = self.layout.lock();
                let (bytes, w, h) = layout.render_document_with_selection(
                    doc,
                    prev.width,
                    Some((sel_start, sel_end)),
                );
                prev.bytes = bytes;
                prev.width_px = w;
                prev.height_px = h;
                prev.sel_start = sel_start;
                prev.sel_end = sel_end;
                prev.valid = true;
            }
            (
                Arc::clone(&prev.bytes),
                prev.width_px,
                prev.height_px,
            )
        };
        drop(doc_guard);
        ViewerSnapshot::Document(DocumentSnapshot {
            path_display,
            dirty,
            block_count,
            plain_text: Arc::from(plain_text.as_str()),
            warnings,
            info_text: String::new(),
            bold,
            italic,
            underline,
            alignment,
            list_kind,
            can_undo,
            can_redo,
            preview_rgba,
            preview_width_px,
            preview_height_px,
            source_mode,
        })
    }

    fn is_dirty(&self) -> bool {
        self.undo.lock().is_dirty()
    }

    async fn save(&mut self) -> Result<()> {
        let path = self
            .path
            .read()
            .clone()
            .ok_or(ViewerError::DocumentNotOpen)?;
        if !path.is_local() {
            return Err(ViewerError::DocumentSave(String::from(
                "saving remote documents is not supported yet",
            )));
        }
        let os_path = path.to_local()?;
        let doc = self
            .document
            .read()
            .clone()
            .ok_or(ViewerError::DocumentNotOpen)?;
        ooxml::container::save_document(&doc, Path::new(&os_path)).await?;
        self.undo.lock().mark_clean();
        Ok(())
    }

    fn current_path(&self) -> Option<&orchid_fs::FsPath> {
        // Safety: path is only replaced under exclusive access via open/close;
        // callers hold `&self` so the Option stays stable for the call duration.
        // We expose via a leaked-lifetime pattern used elsewhere — return None
        // and let UI use snapshot path_display when needed. Prefer owned clone
        // via snapshot; trait requires Option<&FsPath>.
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// Implement current_path properly with a stored path we can return.
// The RwLock prevents returning a reference — mirror TextViewer pattern.
impl DocumentViewer {
    /// Path of the open document, if any.
    #[must_use]
    pub fn path_clone(&self) -> Option<orchid_fs::FsPath> {
        self.path.read().clone()
    }
}
