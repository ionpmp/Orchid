//! DOCX-compatible document viewer/editor (Tier 1 rich text).

pub mod cursor;
pub mod layout;
pub mod model;
pub mod ooxml;
pub mod sample;
pub mod table_edit;
pub mod undo;

use std::any::Any;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use parking_lot::{Mutex, RwLock};

use crate::error::{Result, ViewerError};
use crate::snapshot::{DocumentSnapshot, ViewerSnapshot};
use crate::viewer_trait::Viewer;

pub use cursor::{
    adjacent_cell_cursor, adjacent_in_cell, cursor_from_plain_offset,
    expand_selection_to_hyperlink_span, hyperlink_at_cursor, is_image_cursor, is_safe_external_url,
    normalize_external_link_url, normalize_internal_bookmark, paragraph_cursors_in_selection,
    paragraph_indices_in_selection, paragraph_mut, paragraph_mut_in_blocks, paragraph_ref,
    plain_offset_from_cursor, selection_from_plain_offsets, CellPath, Cursor, Selection,
};
pub use layout::{DocumentLayout, PreviewInsets, DEFAULT_PREVIEW_WIDTH};
pub use model::{
    Alignment, Block, Bookmark, CellImage, Document, Hyperlink, ImageFormat, InlineImage,
    LineSpacingRule, ListKind, OpaqueXmlNode, PageSetup, Paragraph, Run, RunStyle, Table, TableCell,
    TableRow, VMerge,
};
pub use sample::{create_sample_docx, sample_document};
pub use undo::{EditCommand, RunStylePatch, UndoStack};

/// Soft ceiling for DOCX payloads accepted by the viewer (128 MiB).
pub const DEFAULT_SIZE_LIMIT: u64 = 128 * 1024 * 1024;

struct PreviewState {
    /// Content column width (CSS px), excluding page margins.
    width: f32,
    /// Last Slint viewport width used to derive [`Self::width`] (`0` = unknown).
    viewport_px: f32,
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
            viewport_px: 0.0,
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
    /// Multi-click tracking for word (2×) / paragraph (3×) select.
    preview_click: Mutex<PreviewClickState>,
    /// Bumped when [`Self::preview_find`] selects a match (or reports no match).
    find_gen: Mutex<i32>,
    find_anchor: Mutex<i32>,
    find_cursor: Mutex<i32>,
    /// 1-based index of the current find match (`0` when none).
    find_match_index: Mutex<i32>,
    /// Total non-overlapping matches for the last query (`0` when none).
    find_match_count: Mutex<i32>,
    /// Preview image Y (CSS px) to scroll to for the current find match (`-1` = none).
    find_scroll_y_px: Mutex<i32>,
    /// Preview display zoom factor (`1.0` = 100%; layout width unchanged).
    preview_zoom: Mutex<f32>,
    /// Preview pointer is over an external hyperlink.
    link_hover: Mutex<bool>,
}

/// Result of [`DocumentViewer::preview_pointer`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PreviewPointerOutcome {
    /// Safe external URL to open (`http`/`https`/`mailto`); `None` if none.
    pub open_url: Option<String>,
    /// Whether the UI should refresh the document snapshot.
    pub refresh: bool,
}

#[derive(Default)]
struct PreviewClickState {
    count: u8,
    last_at: Option<Instant>,
    last_offset: usize,
}

const MULTI_CLICK_GAP: Duration = Duration::from_millis(500);

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
            preview_click: Mutex::new(PreviewClickState::default()),
            find_gen: Mutex::new(0),
            find_anchor: Mutex::new(0),
            find_cursor: Mutex::new(0),
            find_match_index: Mutex::new(0),
            find_match_count: Mutex::new(0),
            find_scroll_y_px: Mutex::new(-1),
            preview_zoom: Mutex::new(1.0),
            link_hover: Mutex::new(false),
        }
    }

    /// Find the next / previous match in plain text.
    ///
    /// When `match_case` is false, comparison is case-insensitive. Selects the
    /// match (preview highlight / source offsets) and returns `true` when found.
    /// Empty queries are ignored. On a miss, clears the match status (`0/0`) and
    /// still bumps [`Self::find_gen`] so the UI can show "no match".
    pub fn preview_find(&self, query: &str, forward: bool, match_case: bool) -> bool {
        let q = query.trim();
        if q.is_empty() {
            return false;
        }
        let doc_guard = self.document.read();
        let Some(doc) = doc_guard.as_ref() else {
            return false;
        };
        let plain = doc.plain_text();
        let (haystack, needle) = if match_case {
            (plain.clone(), q.to_string())
        } else {
            (plain.to_lowercase(), q.to_lowercase())
        };
        let q_bytes = needle.len();
        if q_bytes == 0 || haystack.is_empty() {
            *self.find_match_index.lock() = 0;
            *self.find_match_count.lock() = 0;
            *self.find_scroll_y_px.lock() = -1;
            *self.find_gen.lock() += 1;
            return false;
        }

        let starts = non_overlapping_match_starts(&haystack, &needle);
        let match_count = starts.len() as i32;
        if starts.is_empty() {
            *self.find_match_index.lock() = 0;
            *self.find_match_count.lock() = 0;
            *self.find_scroll_y_px.lock() = -1;
            *self.find_gen.lock() += 1;
            return false;
        }

        let sel = *self.selection.lock();
        let (norm_a, norm_b) = sel.normalized();
        let sel_lo = plain_offset_from_cursor(doc, norm_a);
        let sel_hi = plain_offset_from_cursor(doc, norm_b);

        let found = if forward {
            let start = sel_hi.min(haystack.len());
            starts
                .iter()
                .copied()
                .find(|&s| s >= start)
                .or_else(|| starts.first().copied())
        } else {
            let end = sel_lo.min(haystack.len());
            starts
                .iter()
                .rev()
                .copied()
                .find(|&s| s + q_bytes <= end)
                .or_else(|| starts.last().copied())
        };

        let Some(byte_start) = found else {
            *self.find_match_index.lock() = 0;
            *self.find_match_count.lock() = 0;
            *self.find_scroll_y_px.lock() = -1;
            *self.find_gen.lock() += 1;
            return false;
        };
        let byte_end = (byte_start + q_bytes).min(plain.len());
        let match_index = starts
            .iter()
            .position(|&s| s == byte_start)
            .map(|i| (i + 1) as i32)
            .unwrap_or(0);
        let width = self.preview.lock().width;
        let scroll_y = self
            .layout
            .lock()
            .y_for_plain_offset(doc, width, byte_start)
            .round() as i32;
        *self.selection.lock() = selection_from_plain_offsets(doc, byte_start, byte_end);
        *self.find_anchor.lock() = byte_start as i32;
        *self.find_cursor.lock() = byte_end as i32;
        *self.find_match_index.lock() = match_index;
        *self.find_match_count.lock() = match_count;
        *self.find_scroll_y_px.lock() = scroll_y;
        *self.find_gen.lock() += 1;
        self.invalidate_preview();
        true
    }

    /// Current find status: `(1-based index, total)` — `(0, 0)` when no match.
    #[must_use]
    pub fn find_match_status(&self) -> (i32, i32) {
        (*self.find_match_index.lock(), *self.find_match_count.lock())
    }

    /// Replace the current find match with `replacement`, then advance to the next.
    ///
    /// When the selection is not already a match for `query` (respecting
    /// `match_case`), finds the next match first. Empty queries are ignored.
    ///
    /// # Errors
    ///
    /// Propagates edit errors from insert/delete.
    pub fn preview_replace_current(
        &self,
        query: &str,
        replacement: &str,
        match_case: bool,
    ) -> Result<bool> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(false);
        }
        let selected = self.selected_plain_text();
        let selected_ok = if match_case {
            selected == q
        } else {
            selected.to_lowercase() == q.to_lowercase()
        };
        if !selected_ok && !self.preview_find(q, true, match_case) {
            return Ok(false);
        }
        self.preview_insert_text(replacement)?;
        let _ = self.preview_find(q, true, match_case);
        Ok(true)
    }

    /// Replace every non-overlapping match of `query`.
    ///
    /// Uses a single undo step. Returns the number of replacements performed.
    ///
    /// # Errors
    ///
    /// Propagates edit errors.
    pub fn preview_replace_all(
        &self,
        query: &str,
        replacement: &str,
        match_case: bool,
    ) -> Result<usize> {
        use crate::document::undo::{apply_command, EditCommand};

        let q = query.trim();
        if q.is_empty() {
            return Ok(0);
        }
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let plain = doc.plain_text();
        let (haystack, needle) = if match_case {
            (plain, q.to_string())
        } else {
            (plain.to_lowercase(), q.to_lowercase())
        };
        let q_bytes = needle.len();
        if q_bytes == 0 {
            return Ok(0);
        }
        let starts = non_overlapping_match_starts(&haystack, &needle);
        if starts.is_empty() {
            drop(doc_guard);
            *self.find_match_index.lock() = 0;
            *self.find_match_count.lock() = 0;
            *self.find_gen.lock() += 1;
            self.invalidate_preview();
            return Ok(0);
        }

        let count = starts.len();
        let previous = doc.blocks.clone();
        for &start in starts.iter().rev() {
            let end = start + q_bytes;
            let range = selection_from_plain_offsets(doc, start, end);
            apply_command(doc, &EditCommand::DeleteRange { range })?;
            let at = selection_from_plain_offsets(doc, start, start).head;
            if !replacement.is_empty() {
                apply_command(
                    doc,
                    &EditCommand::InsertText {
                        at,
                        text: replacement.to_string(),
                    },
                )?;
            }
        }
        // Snapshot the mutated body, restore the original, then push one undoable swap.
        let new_blocks = std::mem::replace(&mut doc.blocks, previous);
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks: new_blocks })?;
        drop(doc_guard);

        let caret_off = {
            let guard = self.document.read();
            guard.as_ref().map(|d| d.plain_text().len()).unwrap_or(0)
        };
        self.set_selection_plain_offsets(caret_off, caret_off);
        let _ = self.preview_find(q, true, match_case);
        self.invalidate_preview();
        Ok(count)
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

    /// Set preview content width from the Slint viewport width (CSS pixels).
    ///
    /// Subtracts left/right [`PageSetup`] margins so the rendered page fits the
    /// viewport the way Word margins would.
    pub fn set_preview_viewport_width(&self, viewport_px: f32) {
        let viewport_px = viewport_px.max(200.0);
        let (left, right) = {
            let guard = self.document.read();
            match guard.as_ref() {
                Some(doc) => {
                    let insets = PreviewInsets::from_page_setup(&doc.page_setup);
                    (insets.left, insets.right)
                }
                None => {
                    let insets = PreviewInsets::default_letter();
                    (insets.left, insets.right)
                }
            }
        };
        let content = (viewport_px - left - right).max(160.0);
        let mut prev = self.preview.lock();
        prev.viewport_px = viewport_px;
        if (prev.width - content).abs() > 0.5 {
            prev.width = content;
            prev.valid = false;
        }
    }

    /// Re-derive content width after page margins change (keeps image ≈ viewport).
    fn sync_preview_width_after_margin_change(&self, doc: &Document) {
        let mut prev = self.preview.lock();
        if prev.viewport_px > 0.0 {
            let insets = PreviewInsets::from_page_setup(&doc.page_setup);
            let content = (prev.viewport_px - insets.left - insets.right).max(160.0);
            prev.width = content;
        }
        prev.valid = false;
    }

    /// Current preview display zoom (`1.0` = 100%).
    #[must_use]
    pub fn preview_zoom(&self) -> f32 {
        *self.preview_zoom.lock()
    }

    /// Bump preview display zoom by `steps` tenths (`+1` → +10%). Clamped to 50%–300%.
    pub fn bump_preview_zoom(&self, steps: i32) -> Result<()> {
        let _ = self.document.read().as_ref().ok_or(ViewerError::DocumentNotOpen)?;
        let mut z = self.preview_zoom.lock();
        let next = ((*z * 10.0).round() as i32 + steps).clamp(5, 30) as f32 / 10.0;
        *z = next;
        Ok(())
    }

    /// Reset preview display zoom to 100%.
    pub fn reset_preview_zoom(&self) -> Result<()> {
        let _ = self.document.read().as_ref().ok_or(ViewerError::DocumentNotOpen)?;
        *self.preview_zoom.lock() = 1.0;
        Ok(())
    }

    /// Update the active selection from plain-text UTF-8 byte offsets.
    pub fn set_selection_plain_offsets(&self, anchor: usize, head: usize) {
        let doc_guard = self.document.read();
        let Some(doc) = doc_guard.as_ref() else {
            return;
        };
        *self.selection.lock() = selection_from_plain_offsets(doc, anchor, head);
    }

    /// Handle a pointer event on the preview canvas
    /// (`0`=down, `1`=move, `2`=up, `3`=double-click word select,
    /// `4`=triple-click paragraph select, `5`=hover hit-test).
    ///
    /// Coordinates are CSS pixels in the rendered preview image.
    /// Rapid successive downs also advance multi-click (2→word, 3→paragraph).
    ///
    /// When `ctrl` is true on phase `0` over a safe external hyperlink, returns
    /// [`PreviewPointerOutcome::open_url`] and does not change the selection.
    pub fn preview_pointer(&self, phase: u8, x: f32, y: f32, ctrl: bool) -> PreviewPointerOutcome {
        let doc_guard = self.document.read();
        let Some(doc) = doc_guard.as_ref() else {
            return self.clear_link_hover();
        };
        // Hover leave / invalid coords clear the pointer affordance.
        if phase == 5 && !(x.is_finite() && y.is_finite() && x >= 0.0 && y >= 0.0) {
            return self.clear_link_hover();
        }
        let width = self.preview.lock().width;
        let Some(cursor) = self.layout.lock().hit_test_cursor(doc, width, x, y) else {
            return self.clear_link_hover();
        };
        let link_url = hyperlink_at_cursor(doc, cursor).and_then(|hl| {
            if hl.is_internal() {
                Some(hl.display_target())
            } else if is_safe_external_url(&hl.url) {
                Some(hl.url.clone())
            } else {
                None
            }
        });
        let hover_changed = self.set_link_hover(link_url.is_some());

        if phase == 5 {
            return PreviewPointerOutcome {
                open_url: None,
                refresh: hover_changed,
            };
        }

        // Ctrl+click: open external URL, or jump to an internal bookmark.
        if phase == 0 && ctrl {
            if let Some(hl) = hyperlink_at_cursor(doc, cursor) {
                if hl.is_internal() {
                    if let Some(name) = hl.bookmark.as_deref() {
                        if let Some(offset) = doc.bookmark_offset(name) {
                            let at = cursor_from_plain_offset(doc, offset);
                            *self.selection.lock() = Selection {
                                anchor: at,
                                head: at,
                            };
                            return PreviewPointerOutcome {
                                open_url: None,
                                refresh: true,
                            };
                        }
                    }
                } else if is_safe_external_url(&hl.url) {
                    return PreviewPointerOutcome {
                        open_url: Some(hl.url.clone()),
                        refresh: hover_changed,
                    };
                }
            }
        }

        let offset = plain_offset_from_cursor(doc, cursor);
        let on_image = is_image_cursor(doc, cursor);
        match phase {
            0 => {
                let count = self.note_preview_click(offset);
                match count {
                    1 => {
                        *self.preview_drag_anchor.lock() = Some(offset);
                        *self.selection.lock() = if on_image {
                            Selection {
                                anchor: cursor,
                                head: cursor,
                            }
                        } else {
                            selection_from_plain_offsets(doc, offset, offset)
                        };
                    }
                    2 if on_image => {
                        *self.preview_drag_anchor.lock() = None;
                        *self.selection.lock() = Selection {
                            anchor: cursor,
                            head: cursor,
                        };
                    }
                    2 => {
                        *self.preview_drag_anchor.lock() = None;
                        let plain = doc.plain_text();
                        let (start, end) = word_range_at(&plain, offset);
                        *self.selection.lock() = selection_from_plain_offsets(doc, start, end);
                    }
                    _ if on_image => {
                        *self.preview_drag_anchor.lock() = None;
                        *self.selection.lock() = Selection {
                            anchor: cursor,
                            head: cursor,
                        };
                        self.reset_preview_click();
                    }
                    _ => {
                        *self.preview_drag_anchor.lock() = None;
                        *self.selection.lock() = expand_selection_to_paragraph(doc, cursor);
                        // Next down starts a new multi-click sequence.
                        self.reset_preview_click();
                    }
                }
            }
            1 | 2 => {
                let anchor = *self.preview_drag_anchor.lock();
                let Some(anchor) = anchor else {
                    // Multi-click selection: ignore drag/up so word/paragraph stays.
                    return PreviewPointerOutcome {
                        open_url: None,
                        refresh: hover_changed,
                    };
                };
                *self.selection.lock() = selection_from_plain_offsets(doc, anchor, offset);
                if phase == 2 {
                    *self.preview_drag_anchor.lock() = None;
                }
            }
            3 => {
                *self.preview_drag_anchor.lock() = None;
                self.sync_preview_click_count(2, offset);
                let plain = doc.plain_text();
                let (start, end) = word_range_at(&plain, offset);
                *self.selection.lock() = selection_from_plain_offsets(doc, start, end);
            }
            4 => {
                *self.preview_drag_anchor.lock() = None;
                let cursor = cursor_from_plain_offset(doc, offset);
                *self.selection.lock() = expand_selection_to_paragraph(doc, cursor);
                self.reset_preview_click();
            }
            _ => {
                return PreviewPointerOutcome {
                    open_url: None,
                    refresh: hover_changed,
                };
            }
        }
        PreviewPointerOutcome {
            open_url: None,
            refresh: true,
        }
    }

    fn set_link_hover(&self, hovering: bool) -> bool {
        let mut state = self.link_hover.lock();
        if *state == hovering {
            return false;
        }
        *state = hovering;
        true
    }

    fn clear_link_hover(&self) -> PreviewPointerOutcome {
        PreviewPointerOutcome {
            open_url: None,
            refresh: self.set_link_hover(false),
        }
    }

    fn note_preview_click(&self, offset: usize) -> u8 {
        let now = Instant::now();
        let mut state = self.preview_click.lock();
        let contiguous = state
            .last_at
            .is_some_and(|t| now.duration_since(t) <= MULTI_CLICK_GAP)
            && offset.abs_diff(state.last_offset) <= 2;
        state.count = if contiguous {
            state.count.saturating_add(1).max(1)
        } else {
            1
        };
        state.last_at = Some(now);
        state.last_offset = offset;
        state.count
    }

    fn sync_preview_click_count(&self, count: u8, offset: usize) {
        let mut state = self.preview_click.lock();
        state.count = count;
        state.last_at = Some(Instant::now());
        state.last_offset = offset;
    }

    fn reset_preview_click(&self) {
        *self.preview_click.lock() = PreviewClickState::default();
    }

    /// Select the word (or whitespace run) at a plain-text byte offset.
    pub fn preview_select_word_at_offset(&self, offset: usize) {
        let doc_guard = self.document.read();
        let Some(doc) = doc_guard.as_ref() else {
            return;
        };
        let plain = doc.plain_text();
        let (start, end) = word_range_at(&plain, offset);
        *self.preview_drag_anchor.lock() = None;
        *self.selection.lock() = selection_from_plain_offsets(doc, start, end);
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
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
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
            cell: at.cell,
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

    /// Insert a soft line break (`w:br` / `\n` within the current paragraph).
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`] / edit bounds errors.
    pub fn preview_insert_soft_break(&self) -> Result<()> {
        self.preview_insert_text("\n")
    }

    /// Insert a paragraph break at the preview caret.
    ///
    /// Inside a table cell this splits that cell's paragraph list (not body blocks).
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`] / edit bounds errors.
    pub fn preview_insert_paragraph_break(&self) -> Result<()> {
        self.insert_break_at_caret(false)
    }

    /// Insert a page break at the preview caret (Ctrl+Enter).
    ///
    /// Splits the paragraph like Enter, then marks the new paragraph with
    /// [`Paragraph::page_break_before`] for OOXML `w:pageBreakBefore`.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`] / edit bounds errors.
    pub fn preview_insert_page_break(&self) -> Result<()> {
        self.insert_break_at_caret(true)
    }

    fn insert_break_at_caret(&self, page_break: bool) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = *self.selection.lock();
        let at = self.delete_selection_if_needed(doc, sel)?;
        let (mut next, caret) = if at.cell.is_some() {
            split_cell_paragraph(doc, at)?
        } else {
            let blocks = split_paragraph_blocks(doc, at)?;
            (blocks, Cursor::at(at.block_idx + 1, 0, 0))
        };
        if page_break {
            match next.get_mut(caret.block_idx) {
                Some(Block::Paragraph(p)) => p.page_break_before = true,
                Some(Block::Table(t)) => {
                    if let Some(path) = caret.cell {
                        if let Some(p) = t
                            .rows
                            .get_mut(path.row)
                            .and_then(|r| r.cells.get_mut(path.col))
                            .and_then(|c| c.paragraphs.get_mut(path.para_idx))
                        {
                            p.page_break_before = true;
                        }
                    }
                }
                _ => {}
            }
        }
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks: next })?;
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
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
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
        if is_image_cursor(doc, sel.head) {
            let caret = self.delete_image_cursor(doc, sel.head, false)?;
            *self.selection.lock() = Selection {
                anchor: caret,
                head: caret,
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
        let at = self.delete_or_move_across_cells(doc, range)?;
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
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
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
        if is_image_cursor(doc, sel.head) {
            let caret = self.delete_image_cursor(doc, sel.head, true)?;
            *self.selection.lock() = Selection {
                anchor: caret,
                head: caret,
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
        let at = self.delete_or_move_across_cells(doc, range)?;
        *self.selection.lock() = Selection {
            anchor: at,
            head: at,
        };
        self.invalidate_preview();
        Ok(())
    }

    /// Delete `range`, or when it would cross cells just move the caret (no merge).
    fn delete_or_move_across_cells(&self, doc: &mut Document, range: Selection) -> Result<Cursor> {
        let (start, end) = range.normalized();
        if !start.same_paragraph(end)
            && (start.cell.is_some() || end.cell.is_some())
            && !start.same_cell(end)
        {
            return Ok(start);
        }
        self.delete_selection_if_needed(doc, range)
    }

    /// Move the caret to the next (`forward`) or previous table cell.
    ///
    /// Returns `true` when the caret moved. Outside a table (or at the edge)
    /// returns `false` without changing selection.
    pub fn preview_move_table_cell(&self, forward: bool) -> bool {
        let doc_guard = self.document.read();
        let Some(doc) = doc_guard.as_ref() else {
            return false;
        };
        let head = self.selection.lock().head;
        let Some(next) = adjacent_cell_cursor(doc, head, forward) else {
            return false;
        };
        *self.selection.lock() = Selection {
            anchor: next,
            head: next,
        };
        self.invalidate_preview();
        true
    }

    /// Insert an inline image block after the caret's block.
    ///
    /// Codec is sniffed from `bytes` (defaults to PNG). Pixel size is decoded
    /// when `width_px`/`height_px` are zero.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`] / edit bounds errors.
    pub fn preview_insert_image(
        &self,
        bytes: Vec<u8>,
        width_px: u32,
        height_px: u32,
    ) -> Result<()> {
        use crate::document::model::{CellImage, ImageFormat};

        let head = self.selection.lock().head;
        let format = infer::get(&bytes)
            .map(|k| ImageFormat::from_extension(k.extension()))
            .unwrap_or(ImageFormat::Png);
        let (width_px, height_px) = if width_px == 0 || height_px == 0 {
            image::load_from_memory(&bytes)
                .map(|img| (img.width(), img.height()))
                .unwrap_or((width_px.max(1), height_px.max(1)))
        } else {
            (width_px, height_px)
        };
        let inline = InlineImage {
            bytes,
            format,
            width_px,
            height_px,
            r_id: None,
            part_path: None,
        };

        if let Some(path) = head.cell {
            let after_paragraph = path.para_idx;
            let image_idx = {
                let guard = self.document.read();
                let doc = guard.as_ref().ok_or(ViewerError::DocumentNotOpen)?;
                let Block::Table(t) = doc
                    .blocks
                    .get(head.block_idx)
                    .ok_or(ViewerError::EditOutOfBounds)?
                else {
                    return Err(ViewerError::EditOutOfBounds);
                };
                t.rows
                    .get(path.row)
                    .and_then(|r| r.cells.get(path.col))
                    .map(|c| c.images.len())
                    .ok_or(ViewerError::EditOutOfBounds)?
            };
            self.apply(EditCommand::InsertCellImage {
                table_idx: head.block_idx,
                row: path.row,
                col: path.col,
                image_idx,
                image: CellImage {
                    after_paragraph,
                    image: inline,
                },
            })?;
            let cursor = Cursor::on_cell_image(
                head.block_idx,
                path.row,
                path.col,
                after_paragraph,
                image_idx,
            );
            *self.selection.lock() = Selection {
                anchor: cursor,
                head: cursor,
            };
            return Ok(());
        }

        let at_block = head.block_idx.saturating_add(1);
        self.apply(EditCommand::InsertImage {
            at: Cursor::at(at_block, 0, 0),
            image: inline,
        })?;
        let image_idx = {
            let guard = self.document.read();
            let doc = guard.as_ref().ok_or(ViewerError::DocumentNotOpen)?;
            let idx = at_block.min(doc.blocks.len().saturating_sub(1));
            match doc.blocks.get(idx) {
                Some(Block::Image(_)) => idx,
                _ => return Err(ViewerError::EditOutOfBounds),
            }
        };
        let cursor = Cursor::at(image_idx, 0, 0);
        *self.selection.lock() = Selection {
            anchor: cursor,
            head: cursor,
        };
        Ok(())
    }

    /// Insert an empty `rows`×`cols` table after the caret's block (clamped 1..=20).
    ///
    /// Places the caret in the new table's top-left cell.
    pub fn preview_insert_table(&self, rows: usize, cols: usize) -> Result<()> {
        let head = self.selection.lock().head;
        let at_block = head.block_idx.saturating_add(1);
        self.apply(EditCommand::InsertTable {
            at_block,
            table: crate::document::model::Table::empty(rows, cols),
        })?;
        // Insert clamps to `min(at_block, len)` before push; after apply that index holds the table.
        let table_idx = {
            let guard = self.document.read();
            let doc = guard.as_ref().ok_or(ViewerError::EditOutOfBounds)?;
            let idx = at_block.min(doc.blocks.len().saturating_sub(1));
            match doc.blocks.get(idx) {
                Some(Block::Table(_)) => idx,
                _ => return Err(ViewerError::EditOutOfBounds),
            }
        };
        let cursor = Cursor {
            block_idx: table_idx,
            cell: Some(CellPath::new(0, 0, 0)),
            run_idx: 0,
            byte_offset: 0,
        };
        *self.selection.lock() = Selection {
            anchor: cursor,
            head: cursor,
        };
        Ok(())
    }

    /// Insert an empty row below the caret's table cell.
    pub fn preview_insert_table_row(&self) -> Result<()> {
        let (table_idx, row, col) = self.table_cell_context()?;
        self.apply(EditCommand::InsertTableRow {
            table_idx,
            at_row: row + 1,
        })?;
        let cursor = Cursor {
            block_idx: table_idx,
            cell: Some(CellPath::new(row + 1, col, 0)),
            run_idx: 0,
            byte_offset: 0,
        };
        *self.selection.lock() = Selection {
            anchor: cursor,
            head: cursor,
        };
        Ok(())
    }

    /// Delete the caret's table row (refuses when it is the only row).
    pub fn preview_delete_table_row(&self) -> Result<()> {
        let (table_idx, row, col) = self.table_cell_context()?;
        self.apply(EditCommand::DeleteTableRow {
            table_idx,
            row_idx: row,
        })?;
        let new_row = self
            .document
            .read()
            .as_ref()
            .and_then(|d| match d.blocks.get(table_idx) {
                Some(Block::Table(t)) if !t.rows.is_empty() => Some(row.min(t.rows.len() - 1)),
                _ => None,
            })
            .unwrap_or(0);
        let cursor = Cursor {
            block_idx: table_idx,
            cell: Some(CellPath::new(new_row, col, 0)),
            run_idx: 0,
            byte_offset: 0,
        };
        *self.selection.lock() = Selection {
            anchor: cursor,
            head: cursor,
        };
        Ok(())
    }

    /// Insert an empty column to the right of the caret's table cell.
    pub fn preview_insert_table_column(&self) -> Result<()> {
        let (table_idx, row, col) = self.table_cell_context()?;
        self.apply(EditCommand::InsertTableColumn {
            table_idx,
            at_col: col + 1,
        })?;
        let cursor = Cursor {
            block_idx: table_idx,
            cell: Some(CellPath::new(row, col + 1, 0)),
            run_idx: 0,
            byte_offset: 0,
        };
        *self.selection.lock() = Selection {
            anchor: cursor,
            head: cursor,
        };
        Ok(())
    }

    /// Delete the caret's table column (refuses when it is the only column).
    pub fn preview_delete_table_column(&self) -> Result<()> {
        let (table_idx, row, col) = self.table_cell_context()?;
        self.apply(EditCommand::DeleteTableColumn {
            table_idx,
            col_idx: col,
        })?;
        let new_col = col.min(
            self.document
                .read()
                .as_ref()
                .and_then(|d| match d.blocks.get(table_idx) {
                    Some(Block::Table(t)) => {
                        t.rows.first().map(|r| r.cells.len().saturating_sub(1))
                    }
                    _ => None,
                })
                .unwrap_or(0),
        );
        let cursor = Cursor {
            block_idx: table_idx,
            cell: Some(CellPath::new(row, new_col, 0)),
            run_idx: 0,
            byte_offset: 0,
        };
        *self.selection.lock() = Selection {
            anchor: cursor,
            head: cursor,
        };
        Ok(())
    }

    /// Merge the caret cell with its right neighbor, or with the cell below.
    pub fn preview_merge_table_cells(&self) -> Result<()> {
        let (table_idx, row, col) = self.table_cell_context()?;
        let mut next = {
            let guard = self.document.read();
            let doc = guard.as_ref().ok_or(ViewerError::DocumentNotOpen)?;
            match doc.blocks.get(table_idx) {
                Some(Block::Table(t)) => t.clone(),
                _ => return Err(ViewerError::EditOutOfBounds),
            }
        };
        table_edit::merge_at(&mut next, row, col)?;
        self.apply(EditCommand::ReplaceBlock {
            block_idx: table_idx,
            previous: Block::Table(next),
        })?;
        let cursor = Cursor {
            block_idx: table_idx,
            cell: Some(CellPath::new(row, col, 0)),
            run_idx: 0,
            byte_offset: 0,
        };
        *self.selection.lock() = Selection {
            anchor: cursor,
            head: cursor,
        };
        Ok(())
    }

    /// Unmerge the caret cell (horizontal and/or vertical span).
    pub fn preview_unmerge_table_cells(&self) -> Result<()> {
        let (table_idx, row, col) = self.table_cell_context()?;
        let mut next = {
            let guard = self.document.read();
            let doc = guard.as_ref().ok_or(ViewerError::DocumentNotOpen)?;
            match doc.blocks.get(table_idx) {
                Some(Block::Table(t)) => t.clone(),
                _ => return Err(ViewerError::EditOutOfBounds),
            }
        };
        table_edit::unmerge_at(&mut next, row, col)?;
        let safe_col = next
            .rows
            .get(row)
            .map(|r| col.min(r.cells.len().saturating_sub(1)))
            .unwrap_or(0);
        self.apply(EditCommand::ReplaceBlock {
            block_idx: table_idx,
            previous: Block::Table(next),
        })?;
        let cursor = Cursor {
            block_idx: table_idx,
            cell: Some(CellPath::new(row, safe_col, 0)),
            run_idx: 0,
            byte_offset: 0,
        };
        *self.selection.lock() = Selection {
            anchor: cursor,
            head: cursor,
        };
        Ok(())
    }

    fn delete_image_cursor(
        &self,
        doc: &mut Document,
        cursor: Cursor,
        forward: bool,
    ) -> Result<Cursor> {
        if let Some(path) = cursor.cell {
            if let Some(image_idx) = path.image_idx {
                self.undo.lock().push(
                    doc,
                    EditCommand::RemoveCellImage {
                        table_idx: cursor.block_idx,
                        row: path.row,
                        col: path.col,
                        image_idx,
                    },
                )?;
                return Ok(adjacent_in_cell(doc, cursor, forward)
                    .unwrap_or_else(|| fallback_cell_caret(doc, cursor, !forward)));
            }
        }
        self.undo.lock().push(
            doc,
            EditCommand::RemoveBlock {
                block_idx: cursor.block_idx,
            },
        )?;
        Ok(Cursor::at(
            cursor.block_idx.min(doc.blocks.len().saturating_sub(1)),
            0,
            0,
        ))
    }

    fn table_cell_context(&self) -> Result<(usize, usize, usize)> {
        let head = self.selection.lock().head;
        let Some(cell) = head.cell else {
            return Err(ViewerError::EditOutOfBounds);
        };
        let doc_guard = self.document.read();
        let doc = doc_guard.as_ref().ok_or(ViewerError::DocumentNotOpen)?;
        match doc.blocks.get(head.block_idx) {
            Some(Block::Table(t))
                if cell.row < t.rows.len() && cell.col < t.rows[cell.row].cells.len() =>
            {
                Ok((head.block_idx, cell.row, cell.col))
            }
            _ => Err(ViewerError::EditOutOfBounds),
        }
    }

    /// Move the caret by `delta` Unicode scalars (`extend` keeps the anchor).
    pub fn preview_move_by_chars(&self, delta: i32, extend: bool) {
        let doc_guard = self.document.read();
        let Some(doc) = doc_guard.as_ref() else {
            return;
        };
        let sel = *self.selection.lock();
        if delta.abs() == 1 && !extend {
            let forward = delta > 0;
            if is_image_cursor(doc, sel.head) {
                if let Some(next) = step_image_aware(doc, sel.head, forward) {
                    *self.selection.lock() = Selection {
                        anchor: next,
                        head: next,
                    };
                    return;
                }
            } else if let Some(next) = step_image_aware(doc, sel.head, forward) {
                if next != sel.head {
                    *self.selection.lock() = Selection {
                        anchor: next,
                        head: next,
                    };
                    return;
                }
            }
        }
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

    /// Move the caret by whole words (`extend` keeps the anchor).
    pub fn preview_move_by_words(&self, delta: i32, extend: bool) {
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
                off = prev_word_boundary(&plain, off);
            }
        } else {
            for _ in 0..(delta as usize) {
                if off >= plain.len() {
                    break;
                }
                off = next_word_boundary(&plain, off);
            }
        }
        let head = cursor_from_plain_offset(doc, off);
        let anchor = if extend { sel.anchor } else { head };
        *self.selection.lock() = Selection { anchor, head };
    }

    /// Delete the selection, or the word before the caret if collapsed.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn preview_delete_word_backward(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
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
        let prev = prev_word_boundary(&plain, head_off);
        let range = selection_from_plain_offsets(doc, prev, head_off);
        let at = self.delete_selection_if_needed(doc, range)?;
        *self.selection.lock() = Selection {
            anchor: at,
            head: at,
        };
        self.invalidate_preview();
        Ok(())
    }

    /// Delete the selection, or the word after the caret if collapsed.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn preview_delete_word_forward(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
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
        let next = next_word_boundary(&plain, head_off);
        let range = selection_from_plain_offsets(doc, head_off, next);
        let at = self.delete_selection_if_needed(doc, range)?;
        *self.selection.lock() = Selection {
            anchor: at,
            head: at,
        };
        self.invalidate_preview();
        Ok(())
    }

    /// Move the caret by whole plain-text lines (`extend` keeps the anchor).
    pub fn preview_move_vertical(&self, delta_lines: i32, extend: bool) {
        let doc_guard = self.document.read();
        let Some(doc) = doc_guard.as_ref() else {
            return;
        };
        let sel = *self.selection.lock();
        let plain = doc.plain_text();
        let off = plain_offset_from_cursor(doc, sel.head);
        let line_start = plain[..off].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let col_chars = plain[line_start..off].chars().count();

        let mut target_start = line_start;
        if delta_lines < 0 {
            for _ in 0..(-delta_lines as usize) {
                if target_start == 0 {
                    break;
                }
                let prev_nl = target_start - 1;
                target_start = plain[..prev_nl].rfind('\n').map(|i| i + 1).unwrap_or(0);
            }
        } else {
            for _ in 0..(delta_lines as usize) {
                match plain[target_start..].find('\n') {
                    Some(rel) => target_start += rel + 1,
                    None => break,
                }
            }
        }

        let line_end = plain[target_start..]
            .find('\n')
            .map(|i| target_start + i)
            .unwrap_or(plain.len());
        let mut new_off = line_end;
        for (i, (byte_idx, _)) in plain[target_start..line_end].char_indices().enumerate() {
            if i == col_chars {
                new_off = target_start + byte_idx;
                break;
            }
        }
        let head = cursor_from_plain_offset(doc, new_off);
        let anchor = if extend { sel.anchor } else { head };
        *self.selection.lock() = Selection { anchor, head };
    }

    /// Plain text covered by the current selection (empty when collapsed).
    #[must_use]
    pub fn selected_plain_text(&self) -> String {
        let doc_guard = self.document.read();
        let Some(doc) = doc_guard.as_ref() else {
            return String::new();
        };
        let sel = *self.selection.lock();
        if sel.is_collapsed() {
            return String::new();
        }
        let (start, end) = sel.normalized();
        let a = plain_offset_from_cursor(doc, start);
        let b = plain_offset_from_cursor(doc, end);
        let plain = doc.plain_text();
        plain.get(a..b).unwrap_or("").to_string()
    }

    /// Paste plain text at the caret, turning `\n` into paragraph breaks.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`] / edit bounds errors.
    pub fn preview_paste_plain(&self, text: &str) -> Result<()> {
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        if normalized.is_empty() {
            return Ok(());
        }
        let mut lines = normalized.split('\n');
        let Some(first) = lines.next() else {
            return Ok(());
        };
        if !first.is_empty() {
            self.preview_insert_text(first)?;
        } else if normalized.starts_with('\n') {
            // Leading newline — still create a break below after empty first.
        }
        for line in lines {
            self.preview_insert_paragraph_break()?;
            if !line.is_empty() {
                self.preview_insert_text(line)?;
            }
        }
        Ok(())
    }

    /// Cut: return selected text and delete the selection.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn preview_cut_selection(&self) -> Result<String> {
        let text = self.selected_plain_text();
        if text.is_empty() {
            return Ok(text);
        }
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = *self.selection.lock();
        let at = self.delete_selection_if_needed(doc, sel)?;
        *self.selection.lock() = Selection {
            anchor: at,
            head: at,
        };
        self.invalidate_preview();
        Ok(text)
    }

    /// Select the entire document body.
    pub fn preview_select_all(&self) {
        let doc_guard = self.document.read();
        let Some(doc) = doc_guard.as_ref() else {
            return;
        };
        let len = doc.plain_text().len();
        *self.selection.lock() = selection_from_plain_offsets(doc, 0, len);
    }

    /// Move to the start (`to_end = false`) or end of the current plain-text line.
    pub fn preview_move_line_boundary(&self, to_end: bool, extend: bool) {
        let doc_guard = self.document.read();
        let Some(doc) = doc_guard.as_ref() else {
            return;
        };
        let sel = *self.selection.lock();
        let plain = doc.plain_text();
        let off = plain_offset_from_cursor(doc, sel.head);
        let line_start = plain[..off].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let line_end = plain[line_start..]
            .find('\n')
            .map(|i| line_start + i)
            .unwrap_or(plain.len());
        let new_off = if to_end { line_end } else { line_start };
        let head = cursor_from_plain_offset(doc, new_off);
        let anchor = if extend { sel.anchor } else { head };
        *self.selection.lock() = Selection { anchor, head };
    }

    /// Move to the start (`to_end = false`) or end of the document.
    pub fn preview_move_document_boundary(&self, to_end: bool, extend: bool) {
        let doc_guard = self.document.read();
        let Some(doc) = doc_guard.as_ref() else {
            return;
        };
        let sel = *self.selection.lock();
        let new_off = if to_end { doc.plain_text().len() } else { 0 };
        let head = cursor_from_plain_offset(doc, new_off);
        let anchor = if extend { sel.anchor } else { head };
        *self.selection.lock() = Selection { anchor, head };
    }

    /// Apply a style patch to the effective selection (collapsed → whole paragraph).
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn apply_style_patch_selection(&self, patch: RunStylePatch) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
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

    /// Clear character formatting on the effective selection (Ctrl+Space).
    ///
    /// Resets bold/italic/underline/strike/highlight/super/sub, colour, font family, size,
    /// and external hyperlink. Paragraph alignment and list markers are left unchanged.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn clear_formatting_selection(&self) -> Result<()> {
        self.apply_style_patch_selection(RunStylePatch::clear_character())
    }

    /// Apply (or replace) a hyperlink on the selection.
    ///
    /// External targets: `http`/`https`/`mailto` or bare hosts (normalized to `https://`).
    /// Internal targets: `#bookmarkName` (creates the bookmark at document start if missing).
    ///
    /// Collapsed caret on an existing link updates that link span. Collapsed caret
    /// elsewhere inserts the display text and links it. Non-empty selection
    /// links the selected text.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`] or invalid/unsafe URL
    /// ([`ViewerError::EditOutOfBounds`]).
    pub fn set_hyperlink_selection(&self, url: &str) -> Result<()> {
        let hl = if let Some(name) = normalize_internal_bookmark(url) {
            Hyperlink {
                url: String::new(),
                r_id: None,
                bookmark: Some(name),
            }
        } else if let Some(url) = normalize_external_link_url(url) {
            Hyperlink {
                url,
                r_id: None,
                bookmark: None,
            }
        } else {
            return Err(ViewerError::EditOutOfBounds);
        };

        if let Some(name) = hl.bookmark.clone() {
            let mut doc_guard = self.document.write();
            let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
            if doc.bookmark_offset(&name).is_none() {
                doc.bookmarks.push(crate::document::model::Bookmark {
                    name,
                    plain_offset: 0,
                });
            }
        }

        let range = {
            let doc_guard = self.document.read();
            let doc = doc_guard.as_ref().ok_or(ViewerError::DocumentNotOpen)?;
            let sel = *self.selection.lock();
            if !sel.is_collapsed() {
                let (a, b) = sel.normalized();
                Some(Selection { anchor: a, head: b })
            } else {
                expand_selection_to_hyperlink_span(doc, sel.head)
            }
        };

        let insert_label = hl.display_target();
        let range = if let Some(range) = range {
            range
        } else {
            let start_off = {
                let doc_guard = self.document.read();
                let doc = doc_guard.as_ref().ok_or(ViewerError::DocumentNotOpen)?;
                plain_offset_from_cursor(doc, self.selection.lock().head)
            };
            self.preview_insert_text(&insert_label)?;
            let doc_guard = self.document.read();
            let doc = doc_guard.as_ref().ok_or(ViewerError::DocumentNotOpen)?;
            let range =
                selection_from_plain_offsets(doc, start_off, start_off + insert_label.len());
            *self.selection.lock() = range;
            range
        };

        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        self.undo.lock().push(
            doc,
            EditCommand::SetRunStyle {
                range,
                style: RunStylePatch {
                    hyperlink: Some(Some(hl)),
                    ..Default::default()
                },
            },
        )?;
        self.invalidate_preview();
        Ok(())
    }

    /// Remove external hyperlinks from the selection (or the link span under the caret).
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn remove_hyperlink_selection(&self) -> Result<()> {
        let range = {
            let doc_guard = self.document.read();
            let doc = doc_guard.as_ref().ok_or(ViewerError::DocumentNotOpen)?;
            let sel = *self.selection.lock();
            if sel.is_collapsed() {
                expand_selection_to_hyperlink_span(doc, sel.head).unwrap_or(sel)
            } else {
                let (a, b) = sel.normalized();
                Selection { anchor: a, head: b }
            }
        };
        if range.is_collapsed() {
            return Ok(());
        }
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        self.undo.lock().push(
            doc,
            EditCommand::SetRunStyle {
                range,
                style: RunStylePatch {
                    hyperlink: Some(None),
                    ..Default::default()
                },
            },
        )?;
        self.invalidate_preview();
        Ok(())
    }

    /// Step font size up (`direction > 0`) or down on the selection.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn bump_font_size_selection(&self, direction: i32) -> Result<()> {
        let current = {
            let doc_guard = self.document.read();
            let doc = doc_guard.as_ref().ok_or(ViewerError::DocumentNotOpen)?;
            let sel = *self.selection.lock();
            style_at_cursor(doc, sel.normalized().0)
                .and_then(|s| s.font_size_pt)
                .unwrap_or(DEFAULT_FONT_SIZE_PT)
        };
        let next = next_font_size(current, direction);
        self.apply_style_patch_selection(RunStylePatch {
            font_size_pt: Some(Some(next)),
            ..Default::default()
        })
    }

    /// Set run colour on the selection.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn set_color_selection(&self, rgb: [u8; 3]) -> Result<()> {
        self.apply_style_patch_selection(RunStylePatch {
            color: Some(Some(rgb)),
            ..Default::default()
        })
    }

    /// Set font family on the selection.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn set_font_family_selection(&self, family: &str) -> Result<()> {
        let family = family.trim();
        if family.is_empty() {
            return Ok(());
        }
        self.apply_style_patch_selection(RunStylePatch {
            font_family: Some(Some(family.to_string())),
            ..Default::default()
        })
    }

    /// Cycle font family presets on the selection (`direction > 0` → next).
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn bump_font_family_selection(&self, direction: i32) -> Result<()> {
        let current = {
            let doc_guard = self.document.read();
            let doc = doc_guard.as_ref().ok_or(ViewerError::DocumentNotOpen)?;
            let sel = *self.selection.lock();
            style_at_cursor(doc, sel.normalized().0).and_then(|s| s.font_family)
        };
        let next = next_font_family(current.as_deref(), direction);
        self.set_font_family_selection(next)
    }

    /// Delete `sel` when non-empty; returns the caret where typing should continue.
    fn delete_selection_if_needed(&self, doc: &mut Document, sel: Selection) -> Result<Cursor> {
        if sel.is_collapsed() {
            return Ok(sel.head);
        }
        let (start, end) = sel.normalized();
        if start.same_paragraph(end) {
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
        if start.same_cell(end) {
            let next = delete_multi_cell_paragraph(doc, start, end)?;
            self.undo
                .lock()
                .push(doc, EditCommand::ReplaceBlocks { blocks: next })?;
            return Ok(start);
        }
        // Cross-cell / body↔table multi-delete is not supported in Tier-1.
        if start.cell.is_some() || end.cell.is_some() {
            return Err(ViewerError::EditOutOfBounds);
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

    /// Active selection as UTF-8 byte offsets in [`Document::plain_text`].
    #[must_use]
    pub fn selection_plain_offsets(&self) -> (usize, usize) {
        let doc_guard = self.document.read();
        let Some(doc) = doc_guard.as_ref() else {
            return (0, 0);
        };
        let sel = *self.selection.lock();
        let (a, b) = sel.normalized();
        (
            plain_offset_from_cursor(doc, a),
            plain_offset_from_cursor(doc, b),
        )
    }

    /// Borrow the loaded document model (for tests / UI commands).
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
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        self.undo.lock().push(doc, cmd)?;
        self.invalidate_preview();
        Ok(())
    }

    /// Undo the last edit.
    pub fn undo(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        self.undo.lock().undo(doc)?;
        self.sync_preview_width_after_margin_change(doc);
        Ok(())
    }

    /// Redo the last undone edit.
    pub fn redo(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        self.undo.lock().redo(doc)?;
        self.sync_preview_width_after_margin_change(doc);
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
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
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
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let style_at = style_at_cursor(doc, sel.normalized().0);
        let currently_on = style_at
            .map(|s| match which {
                'b' => s.bold,
                'i' => s.italic,
                'u' => s.underline,
                's' => s.strikethrough,
                'h' => s.highlight,
                '^' => s.superscript,
                '_' => s.subscript,
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
            's' => RunStylePatch {
                strikethrough: Some(!currently_on),
                ..Default::default()
            },
            'h' => RunStylePatch {
                highlight: Some(!currently_on),
                ..Default::default()
            },
            '^' => RunStylePatch {
                superscript: Some(!currently_on),
                subscript: if currently_on { None } else { Some(false) },
                ..Default::default()
            },
            '_' => RunStylePatch {
                subscript: Some(!currently_on),
                superscript: if currently_on { None } else { Some(false) },
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

    /// Set alignment on paragraphs touched by the selection (body or same cell).
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn set_alignment_all(&self, alignment: Alignment) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let cursors = paragraph_cursors_in_selection(doc, sel);
        if cursors.is_empty() {
            return Ok(());
        }
        let mut next = doc.blocks.clone();
        for cursor in cursors {
            if let Some(p) = paragraph_mut_in_blocks(&mut next, cursor) {
                p.alignment = alignment;
            }
        }
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks: next })?;
        self.invalidate_preview();
        Ok(())
    }

    /// Bump space-before / space-after (twips) on selected paragraphs.
    ///
    /// Steps of ±120 twips (6 pt). Values clamp to `0..=2880` (0–2").
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn bump_paragraph_spacing_selection(
        &self,
        before_delta: i32,
        after_delta: i32,
    ) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let cursors = paragraph_cursors_in_selection(doc, sel);
        if cursors.is_empty() {
            return Ok(());
        }
        let mut next = doc.blocks.clone();
        for cursor in cursors {
            if let Some(p) = paragraph_mut_in_blocks(&mut next, cursor) {
                if before_delta != 0 {
                    p.space_before_twips =
                        clamp_spacing_twips(p.space_before_twips as i32 + before_delta);
                }
                if after_delta != 0 {
                    p.space_after_twips =
                        clamp_spacing_twips(p.space_after_twips as i32 + after_delta);
                }
            }
        }
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks: next })?;
        self.invalidate_preview();
        Ok(())
    }

    /// Toggle paragraph or table-cell shading on the selection.
    ///
    /// When the caret/selection is inside a table cell, toggles `w:tcPr/w:shd`
    /// on those cells; otherwise toggles paragraph `w:shd`. If any target
    /// already has a fill, clears shading; otherwise applies `#D9E2F3`.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn toggle_paragraph_shade_selection(&self) -> Result<()> {
        const FILL: [u8; 3] = [0xD9, 0xE2, 0xF3];
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let cursors = paragraph_cursors_in_selection(doc, sel);
        if cursors.is_empty() {
            return Ok(());
        }

        // Prefer cell shading when any selected cursor is in a table cell.
        let cell_keys: Vec<(usize, usize, usize)> = {
            let mut keys = Vec::new();
            for c in &cursors {
                if let Some(path) = c.cell {
                    let key = (c.block_idx, path.row, path.col);
                    if !keys.contains(&key) {
                        keys.push(key);
                    }
                }
            }
            keys
        };

        let mut next = doc.blocks.clone();
        let mut changed = false;

        if !cell_keys.is_empty() {
            let clear = cell_keys.iter().any(|&(bi, row, col)| {
                matches!(
                    next.get(bi),
                    Some(Block::Table(t))
                        if t.rows.get(row).and_then(|r| r.cells.get(col)).is_some_and(|c| c.shade_fill.is_some())
                )
            });
            let new_fill = if clear { None } else { Some(FILL) };
            for (bi, row, col) in cell_keys {
                if let Some(Block::Table(t)) = next.get_mut(bi) {
                    if let Some(cell) = t.rows.get_mut(row).and_then(|r| r.cells.get_mut(col)) {
                        if cell.shade_fill != new_fill {
                            cell.shade_fill = new_fill;
                            changed = true;
                        }
                    }
                }
            }
        } else {
            let clear = cursors
                .iter()
                .any(|c| paragraph_ref(doc, *c).is_some_and(|p| p.shade_fill.is_some()));
            for cursor in cursors {
                if let Some(p) = paragraph_mut_in_blocks(&mut next, cursor) {
                    let new_fill = if clear { None } else { Some(FILL) };
                    if p.shade_fill != new_fill {
                        p.shade_fill = new_fill;
                        changed = true;
                    }
                }
            }
        }

        if !changed {
            return Ok(());
        }
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks: next })?;
        self.invalidate_preview();
        Ok(())
    }

    /// Toggle bottom paragraph border (`w:pBdr/w:bottom`) on selected paragraphs.
    ///
    /// If any selected paragraph already has a bottom border, clears borders;
    /// otherwise applies a single bottom rule.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn toggle_paragraph_border_bottom_selection(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let cursors = paragraph_cursors_in_selection(doc, sel);
        if cursors.is_empty() {
            return Ok(());
        }

        let clear = cursors
            .iter()
            .any(|c| paragraph_ref(doc, *c).is_some_and(|p| p.border_bottom));
        let new_border = !clear;

        let mut next = doc.blocks.clone();
        let mut changed = false;
        for cursor in cursors {
            if let Some(p) = paragraph_mut_in_blocks(&mut next, cursor) {
                if p.border_bottom != new_border {
                    p.border_bottom = new_border;
                    changed = true;
                }
            }
        }

        if !changed {
            return Ok(());
        }
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks: next })?;
        self.invalidate_preview();
        Ok(())
    }

    /// Bump left indent (`w:ind/@w:left`) on selected paragraphs.
    ///
    /// Typical steps: `±720` (0.5″). Values clamp to `0..=2880` (0–2″).
    /// First-line / hanging indent is left unchanged.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn bump_indent_left_selection(&self, delta_twips: i32) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let cursors = paragraph_cursors_in_selection(doc, sel);
        if cursors.is_empty() {
            return Ok(());
        }
        let mut next = doc.blocks.clone();
        let mut changed = false;
        for cursor in cursors {
            if let Some(p) = paragraph_mut_in_blocks(&mut next, cursor) {
                let bumped = clamp_spacing_twips(p.indent_left_twips as i32 + delta_twips);
                if bumped != p.indent_left_twips {
                    p.indent_left_twips = bumped;
                    changed = true;
                }
            }
        }
        if !changed {
            return Ok(());
        }
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks: next })?;
        self.invalidate_preview();
        Ok(())
    }

    /// Bump right indent (`w:ind/@w:right`) on selected paragraphs.
    ///
    /// Typical steps: `±720` (0.5″). Values clamp to `0..=2880` (0–2″).
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn bump_indent_right_selection(&self, delta_twips: i32) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let cursors = paragraph_cursors_in_selection(doc, sel);
        if cursors.is_empty() {
            return Ok(());
        }
        let mut next = doc.blocks.clone();
        let mut changed = false;
        for cursor in cursors {
            if let Some(p) = paragraph_mut_in_blocks(&mut next, cursor) {
                let bumped = clamp_spacing_twips(p.indent_right_twips as i32 + delta_twips);
                if bumped != p.indent_right_twips {
                    p.indent_right_twips = bumped;
                    changed = true;
                }
            }
        }
        if !changed {
            return Ok(());
        }
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks: next })?;
        self.invalidate_preview();
        Ok(())
    }

    /// Bump first-line / hanging indent (`w:firstLine` / `w:hanging`).
    ///
    /// Positive values are first-line indent; negative are hanging.
    /// Typical steps: `±360` (0.25″). Clamped to `-1440..=1440` (±1″).
    /// Left indent is left unchanged.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn bump_indent_first_line_selection(&self, delta_twips: i32) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let cursors = paragraph_cursors_in_selection(doc, sel);
        if cursors.is_empty() {
            return Ok(());
        }
        let mut next = doc.blocks.clone();
        let mut changed = false;
        for cursor in cursors {
            if let Some(p) = paragraph_mut_in_blocks(&mut next, cursor) {
                let bumped =
                    (p.indent_first_line_twips + delta_twips).clamp(INDENT_FIRST_LINE_MIN, INDENT_FIRST_LINE_MAX);
                if bumped != p.indent_first_line_twips {
                    p.indent_first_line_twips = bumped;
                    changed = true;
                }
            }
        }
        if !changed {
            return Ok(());
        }
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks: next })?;
        self.invalidate_preview();
        Ok(())
    }

    /// Step line spacing (`w:line` auto) on selected paragraphs through presets.
    ///
    /// Presets: single (240), 1.15 (276), 1.5 (360), double (480). `delta` is
    /// typically `-1` / `+1`.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn bump_line_spacing_selection(&self, delta: i32) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let cursors = paragraph_cursors_in_selection(doc, sel);
        if cursors.is_empty() {
            return Ok(());
        }
        let mut next = doc.blocks.clone();
        for cursor in cursors {
            if let Some(p) = paragraph_mut_in_blocks(&mut next, cursor) {
                let current = if p.line_spacing_rule == LineSpacingRule::Auto {
                    p.line_spacing
                } else {
                    0
                };
                p.line_spacing_rule = LineSpacingRule::Auto;
                p.line_spacing = bump_line_spacing(current, delta);
            }
        }
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks: next })?;
        self.invalidate_preview();
        Ok(())
    }

    /// Bump all page margins by `delta_twips` (clamped to 0.25″–3″).
    ///
    /// Typical steps: `±180` (1/8″) or `±360` (1/4″). Preview insets update
    /// immediately; undo restores the previous [`PageSetup`].
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn bump_page_margins(&self, delta_twips: i32) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let mut next = doc.page_setup.clone();
        next.margin_top_twips = clamp_margin_twips(next.margin_top_twips as i32 + delta_twips);
        next.margin_bottom_twips = clamp_margin_twips(next.margin_bottom_twips as i32 + delta_twips);
        next.margin_left_twips = clamp_margin_twips(next.margin_left_twips as i32 + delta_twips);
        next.margin_right_twips = clamp_margin_twips(next.margin_right_twips as i32 + delta_twips);
        if next == doc.page_setup {
            return Ok(());
        }
        self.undo
            .lock()
            .push(doc, EditCommand::SetPageSetup { setup: next })?;
        self.sync_preview_width_after_margin_change(doc);
        Ok(())
    }

    /// Toggle page size between US Letter and ISO A4 (margins unchanged).
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn cycle_page_size(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let mut next = doc.page_setup.clone();
        let landscape = is_landscape_page(&next);
        if is_a4_page(&next) {
            if landscape {
                next.width_twips = PAGE_LETTER_HEIGHT_TWIPS;
                next.height_twips = PAGE_LETTER_WIDTH_TWIPS;
            } else {
                next.width_twips = PAGE_LETTER_WIDTH_TWIPS;
                next.height_twips = PAGE_LETTER_HEIGHT_TWIPS;
            }
        } else if landscape {
            next.width_twips = PAGE_A4_HEIGHT_TWIPS;
            next.height_twips = PAGE_A4_WIDTH_TWIPS;
        } else {
            next.width_twips = PAGE_A4_WIDTH_TWIPS;
            next.height_twips = PAGE_A4_HEIGHT_TWIPS;
        }
        if next == doc.page_setup {
            return Ok(());
        }
        self.undo
            .lock()
            .push(doc, EditCommand::SetPageSetup { setup: next })?;
        self.sync_preview_width_after_margin_change(doc);
        Ok(())
    }

    /// Swap page width and height (portrait ↔ landscape).
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn toggle_page_orientation(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let mut next = doc.page_setup.clone();
        std::mem::swap(&mut next.width_twips, &mut next.height_twips);
        if next == doc.page_setup {
            return Ok(());
        }
        self.undo
            .lock()
            .push(doc, EditCommand::SetPageSetup { setup: next })?;
        self.sync_preview_width_after_margin_change(doc);
        Ok(())
    }

    /// Set list kind on paragraphs touched by the selection (body or same cell).
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn set_list_all(&self, kind: ListKind) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let cursors = paragraph_cursors_in_selection(doc, sel);
        if cursors.is_empty() {
            return Ok(());
        }
        let mut next = doc.blocks.clone();
        for cursor in cursors {
            if let Some(p) = paragraph_mut_in_blocks(&mut next, cursor) {
                p.list = kind;
                p.num_id = crate::document::ooxml::numbering::num_id_for_kind(kind);
                if kind == ListKind::None {
                    p.list_level = 0;
                }
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
            let sel =
                effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
            let cursors = paragraph_cursors_in_selection(doc, sel);
            cursors
                .first()
                .and_then(|c| paragraph_ref(doc, *c))
                .map(|p| p.list)
                .unwrap_or(ListKind::None)
        };
        let next = if current == kind {
            ListKind::None
        } else {
            kind
        };
        self.set_list_all(next)
    }

    /// Indent (`delta > 0`) or outdent list paragraphs in the selection.
    ///
    /// Outdenting past level 0 clears the list. Non-list paragraphs are skipped.
    /// Works for body paragraphs and paragraphs inside one table cell.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn bump_list_level_selection(&self, delta: i32) -> Result<()> {
        if delta == 0 {
            return Ok(());
        }
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let cursors = paragraph_cursors_in_selection(doc, sel);
        if cursors.is_empty() {
            return Ok(());
        }
        let mut next = doc.blocks.clone();
        let mut changed = false;
        for cursor in cursors {
            let Some(p) = paragraph_mut_in_blocks(&mut next, cursor) else {
                continue;
            };
            if p.list == ListKind::None {
                continue;
            }
            let level = i32::from(p.list_level);
            if delta > 0 {
                let new_level = (level + delta).clamp(0, i32::from(MAX_LIST_LEVEL)) as u8;
                if new_level != p.list_level {
                    p.list_level = new_level;
                    changed = true;
                }
            } else {
                let new_level = level + delta;
                if new_level < 0 {
                    p.list = ListKind::None;
                    p.list_level = 0;
                    p.num_id = crate::document::ooxml::numbering::num_id_for_kind(ListKind::None);
                    changed = true;
                } else {
                    let new_level = new_level as u8;
                    if new_level != p.list_level {
                        p.list_level = new_level;
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            return Ok(());
        }
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks: next })?;
        self.invalidate_preview();
        Ok(())
    }
}

/// Maximum Word-compatible list indent level (`w:ilvl` 0..=8).
const MAX_LIST_LEVEL: u8 = 8;

fn step_image_aware(doc: &Document, cursor: Cursor, forward: bool) -> Option<Cursor> {
    if is_image_cursor(doc, cursor) {
        if cursor.cell.is_some() {
            return adjacent_in_cell(doc, cursor, forward);
        }
        let bi = cursor.block_idx;
        if forward && bi + 1 < doc.blocks.len() {
            return Some(Cursor::at(bi + 1, 0, 0));
        }
        if !forward && bi > 0 {
            return end_of_block_cursor(doc, bi - 1);
        }
        return None;
    }
    let path = cursor.cell?;
    if path.image_idx.is_some() {
        return None;
    }
    let p = paragraph_ref(doc, cursor)?;
    let at_end = if p.runs.is_empty() {
        true
    } else {
        let last = p.runs.len() - 1;
        cursor.run_idx == last && cursor.byte_offset >= p.runs[last].text.len()
    };
    let at_start = cursor.run_idx == 0 && cursor.byte_offset == 0;
    if forward && at_end {
        return adjacent_in_cell(doc, cursor, true);
    }
    if !forward && at_start {
        return adjacent_in_cell(doc, cursor, false);
    }
    None
}

fn end_of_block_cursor(doc: &Document, block_idx: usize) -> Option<Cursor> {
    match doc.blocks.get(block_idx)? {
        Block::Paragraph(p) => Some(end_of_paragraph_cursor(
            Cursor {
                block_idx,
                cell: None,
                run_idx: 0,
                byte_offset: 0,
            },
            p,
        )),
        Block::Table(_) => Some(Cursor {
            block_idx,
            cell: Some(CellPath::new(0, 0, 0)),
            run_idx: 0,
            byte_offset: 0,
        }),
        Block::Image(_) => Some(Cursor::at(block_idx, 0, 0)),
    }
}

fn end_of_paragraph_cursor(cursor: Cursor, p: &Paragraph) -> Cursor {
    if p.runs.is_empty() {
        return Cursor {
            block_idx: cursor.block_idx,
            cell: cursor.cell,
            run_idx: 0,
            byte_offset: 0,
        };
    }
    let last = p.runs.len() - 1;
    Cursor {
        block_idx: cursor.block_idx,
        cell: cursor.cell,
        run_idx: last,
        byte_offset: p.runs[last].text.len(),
    }
}

fn fallback_cell_caret(doc: &Document, cursor: Cursor, backward: bool) -> Cursor {
    let Some(path) = cursor.cell else {
        return cursor;
    };
    if backward {
        let para_cursor = Cursor {
            block_idx: cursor.block_idx,
            cell: Some(CellPath::new(path.row, path.col, path.para_idx)),
            run_idx: 0,
            byte_offset: 0,
        };
        if let Some(p) = paragraph_ref(doc, para_cursor) {
            return end_of_paragraph_cursor(para_cursor, p);
        }
    }
    Cursor {
        block_idx: cursor.block_idx,
        cell: Some(CellPath::new(path.row, path.col, 0)),
        run_idx: 0,
        byte_offset: 0,
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
                        ..Default::default()
                    }],
                    alignment: prev.alignment,
                    list: prev.list,
                    list_level: prev.list_level,
                    num_id: prev.num_id,
                    page_break_before: prev.page_break_before,
                    space_before_twips: prev.space_before_twips,
                    space_after_twips: prev.space_after_twips,
                    line_spacing: prev.line_spacing,
                    line_spacing_rule: prev.line_spacing_rule,
                    indent_left_twips: prev.indent_left_twips,
                    indent_first_line_twips: prev.indent_first_line_twips,
                    indent_right_twips: prev.indent_right_twips,
                    shade_fill: prev.shade_fill,
                    border_bottom: prev.border_bottom,
                    unsupported: prev.unsupported.clone(),
                })
            } else {
                Block::Paragraph(Paragraph {
                    runs: vec![Run {
                        text: line.to_string(),
                        style: RunStyle::default(),
                        ..Default::default()
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
    let p = paragraph_ref(doc, cursor)?;
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
    let Some(p) = paragraph_ref(doc, cursor) else {
        return Selection {
            anchor: cursor,
            head: cursor,
        };
    };
    if p.runs.is_empty() {
        let c = Cursor {
            block_idx: cursor.block_idx,
            cell: cursor.cell,
            run_idx: 0,
            byte_offset: 0,
        };
        return Selection { anchor: c, head: c };
    }
    let last = p.runs.len() - 1;
    Selection {
        anchor: Cursor {
            block_idx: cursor.block_idx,
            cell: cursor.cell,
            run_idx: 0,
            byte_offset: 0,
        },
        head: Cursor {
            block_idx: cursor.block_idx,
            cell: cursor.cell,
            run_idx: last,
            byte_offset: p.runs[last].text.len(),
        },
    }
}

const DEFAULT_FONT_SIZE_PT: f32 = 14.0;
const SPACING_TWIPS_MAX: i32 = 2880;
/// First-line / hanging clamp (±1″).
const INDENT_FIRST_LINE_MIN: i32 = -1440;
const INDENT_FIRST_LINE_MAX: i32 = 1440;
/// Auto line-spacing presets in 240ths of a line (single, 1.15, 1.5, double).
const LINE_SPACING_PRESETS: &[u32] = &[240, 276, 360, 480];
/// Page margin clamp: 0.25″ … 3″.
const MARGIN_TWIPS_MIN: i32 = 360;
const MARGIN_TWIPS_MAX: i32 = 4320;
/// US Letter page size (twips).
const PAGE_LETTER_WIDTH_TWIPS: u32 = 12240;
const PAGE_LETTER_HEIGHT_TWIPS: u32 = 15840;
/// ISO A4 page size (twips).
const PAGE_A4_WIDTH_TWIPS: u32 = 11906;
const PAGE_A4_HEIGHT_TWIPS: u32 = 16838;

fn clamp_spacing_twips(v: i32) -> u32 {
    v.clamp(0, SPACING_TWIPS_MAX) as u32
}

fn clamp_margin_twips(v: i32) -> u32 {
    v.clamp(MARGIN_TWIPS_MIN, MARGIN_TWIPS_MAX) as u32
}

fn is_landscape_page(ps: &PageSetup) -> bool {
    ps.width_twips > ps.height_twips
}

fn page_portrait_dims(ps: &PageSetup) -> (u32, u32) {
    if is_landscape_page(ps) {
        (ps.height_twips, ps.width_twips)
    } else {
        (ps.width_twips, ps.height_twips)
    }
}

fn is_a4_page(ps: &PageSetup) -> bool {
    let (w, h) = page_portrait_dims(ps);
    w == PAGE_A4_WIDTH_TWIPS && h == PAGE_A4_HEIGHT_TWIPS
}

fn bump_line_spacing(current: u32, delta: i32) -> u32 {
    let effective = if current == 0 { 276 } else { current };
    let idx = LINE_SPACING_PRESETS
        .iter()
        .enumerate()
        .min_by_key(|(_, &p)| (p as i32 - effective as i32).unsigned_abs())
        .map(|(i, _)| i)
        .unwrap_or(1);
    let next = (idx as i32 + delta).clamp(0, LINE_SPACING_PRESETS.len() as i32 - 1) as usize;
    LINE_SPACING_PRESETS[next]
}

const FONT_SIZE_STEPS: &[f32] = &[
    9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 36.0,
];

/// Common Windows-friendly families exposed by the document toolbar.
const FONT_FAMILY_PRESETS: &[&str] = &[
    "Segoe UI",
    "Calibri",
    "Arial",
    "Times New Roman",
    "Consolas",
];

fn next_font_size(current: f32, direction: i32) -> f32 {
    if direction < 0 {
        FONT_SIZE_STEPS
            .iter()
            .rev()
            .find(|&&s| s < current - 0.01)
            .copied()
            .unwrap_or(FONT_SIZE_STEPS[0])
    } else {
        FONT_SIZE_STEPS
            .iter()
            .find(|&&s| s > current + 0.01)
            .copied()
            .unwrap_or(*FONT_SIZE_STEPS.last().unwrap_or(&DEFAULT_FONT_SIZE_PT))
    }
}

fn next_font_family(current: Option<&str>, direction: i32) -> &'static str {
    let n = FONT_FAMILY_PRESETS.len() as i32;
    if n == 0 {
        return "Segoe UI";
    }
    let idx = current
        .and_then(|c| {
            FONT_FAMILY_PRESETS
                .iter()
                .position(|p| p.eq_ignore_ascii_case(c))
        })
        .map(|i| i as i32)
        .unwrap_or(if direction < 0 { 0 } else { -1 });
    let next = if direction < 0 {
        (idx - 1 + n) % n
    } else {
        (idx + 1) % n
    };
    FONT_FAMILY_PRESETS[next as usize]
}

/// Map a toolbar slug (`segoe-ui`) to a preset family name.
pub fn resolve_font_family_slug(slug: &str) -> Option<&'static str> {
    let key = slug.trim().to_ascii_lowercase().replace(' ', "-");
    match key.as_str() {
        "segoe-ui" | "segoe" => Some("Segoe UI"),
        "calibri" => Some("Calibri"),
        "arial" => Some("Arial"),
        "times-new-roman" | "times" | "tnr" => Some("Times New Roman"),
        "consolas" | "mono" => Some("Consolas"),
        _ => FONT_FAMILY_PRESETS
            .iter()
            .copied()
            .find(|p| p.eq_ignore_ascii_case(slug.trim())),
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

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Byte offset of the previous word boundary (Windows-style Ctrl+Left).
fn prev_word_boundary(text: &str, offset: usize) -> usize {
    let mut off = offset.min(text.len());
    while off > 0 {
        let prev = prev_char_boundary(text, off);
        let Some(c) = text[prev..off].chars().next() else {
            break;
        };
        if !c.is_whitespace() {
            break;
        }
        off = prev;
    }
    if off == 0 {
        return 0;
    }
    let prev = prev_char_boundary(text, off);
    let Some(c) = text[prev..off].chars().next() else {
        return 0;
    };
    let word = is_word_char(c);
    while off > 0 {
        let prev = prev_char_boundary(text, off);
        let Some(c) = text[prev..off].chars().next() else {
            break;
        };
        if c.is_whitespace() || is_word_char(c) != word {
            break;
        }
        off = prev;
    }
    off
}

/// Inclusive-exclusive byte range of the word (or whitespace run) at `offset`.
fn word_range_at(text: &str, offset: usize) -> (usize, usize) {
    if text.is_empty() {
        return (0, 0);
    }
    let mut probe = offset.min(text.len());
    if probe == text.len() {
        probe = prev_char_boundary(text, probe);
    } else if let Some(c) = text[probe..].chars().next() {
        if c.is_whitespace() && probe > 0 {
            // Prefer the preceding token when the click lands on trailing space.
            let prev = prev_char_boundary(text, probe);
            if let Some(pc) = text[prev..probe].chars().next() {
                if !pc.is_whitespace() {
                    probe = prev;
                }
            }
        }
    }

    let Some(c) = text[probe..].chars().next() else {
        return (text.len(), text.len());
    };
    let class_word = !c.is_whitespace() && is_word_char(c);
    let class_ws = c.is_whitespace();

    let mut start = probe;
    while start > 0 {
        let prev = prev_char_boundary(text, start);
        let Some(pc) = text[prev..start].chars().next() else {
            break;
        };
        let same = if class_ws {
            pc.is_whitespace()
        } else if class_word {
            is_word_char(pc)
        } else {
            !pc.is_whitespace() && !is_word_char(pc)
        };
        if !same {
            break;
        }
        start = prev;
    }

    let mut end = next_char_boundary(text, probe);
    while end < text.len() {
        let Some(nc) = text[end..].chars().next() else {
            break;
        };
        let same = if class_ws {
            nc.is_whitespace()
        } else if class_word {
            is_word_char(nc)
        } else {
            !nc.is_whitespace() && !is_word_char(nc)
        };
        if !same {
            break;
        }
        end = next_char_boundary(text, end);
    }
    (start, end)
}

/// Byte offset of the next word boundary (Windows-style Ctrl+Right).
fn next_word_boundary(text: &str, offset: usize) -> usize {
    let mut off = offset.min(text.len());
    if off >= text.len() {
        return text.len();
    }
    if let Some(c) = text[off..].chars().next() {
        if !c.is_whitespace() {
            let word = is_word_char(c);
            while off < text.len() {
                let Some(c) = text[off..].chars().next() else {
                    break;
                };
                if c.is_whitespace() || is_word_char(c) != word {
                    break;
                }
                off = next_char_boundary(text, off);
            }
        }
    }
    while off < text.len() {
        let Some(c) = text[off..].chars().next() else {
            break;
        };
        if !c.is_whitespace() {
            break;
        }
        off = next_char_boundary(text, off);
    }
    off
}

fn split_runs_at(p: &Paragraph, at: Cursor) -> (Vec<Run>, Vec<Run>) {
    let mut left_runs = Vec::new();
    let mut right_runs = Vec::new();
    if p.runs.is_empty() {
        left_runs.push(Run::default());
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
                    hyperlink: run.hyperlink.clone(),
                });
                right_runs.push(Run {
                    text: run.text[split..].to_string(),
                    style: run.style.clone(),
                    hyperlink: run.hyperlink.clone(),
                });
            }
        }
    }
    if left_runs.is_empty() {
        left_runs.push(Run::default());
    }
    if right_runs.is_empty() {
        let style = left_runs
            .last()
            .map(|r| r.style.clone())
            .unwrap_or_default();
        let hyperlink = left_runs.last().and_then(|r| r.hyperlink.clone());
        right_runs.push(Run {
            text: String::new(),
            style,
            hyperlink,
        });
    }
    (left_runs, right_runs)
}

fn split_paragraph_blocks(doc: &Document, at: Cursor) -> Result<Vec<Block>> {
    let Block::Paragraph(p) = doc
        .blocks
        .get(at.block_idx)
        .ok_or(ViewerError::EditOutOfBounds)?
    else {
        return Err(ViewerError::EditOutOfBounds);
    };
    let (left_runs, right_runs) = split_runs_at(p, at);
    let left = Paragraph {
        runs: left_runs,
        alignment: p.alignment,
        list: p.list,
        list_level: p.list_level,
        num_id: p.num_id,
        page_break_before: p.page_break_before,
        space_before_twips: p.space_before_twips,
        space_after_twips: 0,
        line_spacing: p.line_spacing,
        line_spacing_rule: p.line_spacing_rule,
        indent_left_twips: p.indent_left_twips,
        indent_first_line_twips: p.indent_first_line_twips,
        indent_right_twips: p.indent_right_twips,
        shade_fill: p.shade_fill,
        border_bottom: p.border_bottom,
        unsupported: p.unsupported.clone(),
    };
    let right = Paragraph {
        runs: right_runs,
        alignment: p.alignment,
        list: ListKind::None,
        list_level: 0,
        num_id: None,
        page_break_before: false,
        space_before_twips: 0,
        space_after_twips: p.space_after_twips,
        line_spacing: p.line_spacing,
        line_spacing_rule: p.line_spacing_rule,
        indent_left_twips: p.indent_left_twips,
        indent_first_line_twips: p.indent_first_line_twips,
        indent_right_twips: p.indent_right_twips,
        shade_fill: p.shade_fill,
        border_bottom: p.border_bottom,
        unsupported: Vec::new(),
    };
    let mut blocks = doc.blocks.clone();
    blocks[at.block_idx] = Block::Paragraph(left);
    blocks.insert(at.block_idx + 1, Block::Paragraph(right));
    Ok(blocks)
}

fn split_cell_paragraph(doc: &Document, at: Cursor) -> Result<(Vec<Block>, Cursor)> {
    let path = at.cell.ok_or(ViewerError::EditOutOfBounds)?;
    let Block::Table(t) = doc
        .blocks
        .get(at.block_idx)
        .ok_or(ViewerError::EditOutOfBounds)?
    else {
        return Err(ViewerError::EditOutOfBounds);
    };
    let p = t
        .rows
        .get(path.row)
        .ok_or(ViewerError::EditOutOfBounds)?
        .cells
        .get(path.col)
        .ok_or(ViewerError::EditOutOfBounds)?
        .paragraphs
        .get(path.para_idx)
        .ok_or(ViewerError::EditOutOfBounds)?;
    let (left_runs, right_runs) = split_runs_at(p, at);
    let left = Paragraph {
        runs: left_runs,
        alignment: p.alignment,
        list: p.list,
        list_level: p.list_level,
        num_id: p.num_id,
        page_break_before: p.page_break_before,
        space_before_twips: p.space_before_twips,
        space_after_twips: 0,
        line_spacing: p.line_spacing,
        line_spacing_rule: p.line_spacing_rule,
        indent_left_twips: p.indent_left_twips,
        indent_first_line_twips: p.indent_first_line_twips,
        indent_right_twips: p.indent_right_twips,
        shade_fill: p.shade_fill,
        border_bottom: p.border_bottom,
        unsupported: p.unsupported.clone(),
    };
    let right = Paragraph {
        runs: right_runs,
        alignment: p.alignment,
        list: ListKind::None,
        list_level: 0,
        num_id: None,
        page_break_before: false,
        space_before_twips: 0,
        space_after_twips: p.space_after_twips,
        line_spacing: p.line_spacing,
        line_spacing_rule: p.line_spacing_rule,
        indent_left_twips: p.indent_left_twips,
        indent_first_line_twips: p.indent_first_line_twips,
        indent_right_twips: p.indent_right_twips,
        shade_fill: p.shade_fill,
        border_bottom: p.border_bottom,
        unsupported: Vec::new(),
    };
    let mut blocks = doc.blocks.clone();
    let Block::Table(t) = blocks
        .get_mut(at.block_idx)
        .ok_or(ViewerError::EditOutOfBounds)?
    else {
        return Err(ViewerError::EditOutOfBounds);
    };
    let cell = t
        .rows
        .get_mut(path.row)
        .ok_or(ViewerError::EditOutOfBounds)?
        .cells
        .get_mut(path.col)
        .ok_or(ViewerError::EditOutOfBounds)?;
    cell.paragraphs[path.para_idx] = left;
    cell.paragraphs.insert(path.para_idx + 1, right);
    let caret = Cursor {
        block_idx: at.block_idx,
        cell: Some(CellPath::new(path.row, path.col, path.para_idx + 1)),
        run_idx: 0,
        byte_offset: 0,
    };
    Ok((blocks, caret))
}

fn delete_multi_cell_paragraph(doc: &Document, start: Cursor, end: Cursor) -> Result<Vec<Block>> {
    if !start.same_cell(end) {
        return Err(ViewerError::EditOutOfBounds);
    }
    let path = start.cell.ok_or(ViewerError::EditOutOfBounds)?;
    let end_path = end.cell.ok_or(ViewerError::EditOutOfBounds)?;
    let Block::Table(t) = doc
        .blocks
        .get(start.block_idx)
        .ok_or(ViewerError::EditOutOfBounds)?
    else {
        return Err(ViewerError::EditOutOfBounds);
    };
    let paras = &t
        .rows
        .get(path.row)
        .ok_or(ViewerError::EditOutOfBounds)?
        .cells
        .get(path.col)
        .ok_or(ViewerError::EditOutOfBounds)?
        .paragraphs;
    if path.para_idx >= paras.len() || end_path.para_idx >= paras.len() {
        return Err(ViewerError::EditOutOfBounds);
    }
    let start_p = &paras[path.para_idx];
    let end_p = &paras[end_path.para_idx];

    let mut merged_runs = Vec::new();
    for (ri, run) in start_p.runs.iter().enumerate() {
        if ri < start.run_idx {
            merged_runs.push(run.clone());
        } else if ri == start.run_idx {
            merged_runs.push(Run {
                text: run.text[..start.byte_offset.min(run.text.len())].to_string(),
                style: run.style.clone(),
                hyperlink: run.hyperlink.clone(),
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
                hyperlink: run.hyperlink.clone(),
            });
        }
    }
    merged_runs.retain(|r| !r.text.is_empty());
    if merged_runs.is_empty() {
        merged_runs.push(Run::default());
    }
    let merged = Paragraph {
        runs: merged_runs,
        alignment: start_p.alignment,
        list: start_p.list,
        list_level: start_p.list_level,
        num_id: start_p.num_id,
        page_break_before: start_p.page_break_before,
        space_before_twips: start_p.space_before_twips,
        space_after_twips: start_p.space_after_twips,
        line_spacing: start_p.line_spacing,
        line_spacing_rule: start_p.line_spacing_rule,
        indent_left_twips: start_p.indent_left_twips,
        indent_first_line_twips: start_p.indent_first_line_twips,
        indent_right_twips: start_p.indent_right_twips,
        shade_fill: start_p.shade_fill,
        border_bottom: start_p.border_bottom,
        unsupported: start_p.unsupported.clone(),
    };
    let mut new_paras = Vec::with_capacity(paras.len());
    for (pi, p) in paras.iter().enumerate() {
        if pi < path.para_idx || pi > end_path.para_idx {
            new_paras.push((*p).clone());
        } else if pi == path.para_idx {
            new_paras.push(merged.clone());
        }
    }

    let mut blocks = doc.blocks.clone();
    let Block::Table(t) = blocks
        .get_mut(start.block_idx)
        .ok_or(ViewerError::EditOutOfBounds)?
    else {
        return Err(ViewerError::EditOutOfBounds);
    };
    let cell = t
        .rows
        .get_mut(path.row)
        .ok_or(ViewerError::EditOutOfBounds)?
        .cells
        .get_mut(path.col)
        .ok_or(ViewerError::EditOutOfBounds)?;
    cell.paragraphs = new_paras;
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
                hyperlink: run.hyperlink.clone(),
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
                hyperlink: run.hyperlink.clone(),
            });
        }
    }
    merged_runs.retain(|r| !r.text.is_empty());
    if merged_runs.is_empty() {
        merged_runs.push(Run::default());
    }

    let merged = Paragraph {
        runs: merged_runs,
        alignment: start_p.alignment,
        list: start_p.list,
        list_level: start_p.list_level,
        num_id: start_p.num_id,
        page_break_before: start_p.page_break_before,
        space_before_twips: start_p.space_before_twips,
        space_after_twips: start_p.space_after_twips,
        line_spacing: start_p.line_spacing,
        line_spacing_rule: start_p.line_spacing_rule,
        indent_left_twips: start_p.indent_left_twips,
        indent_first_line_twips: start_p.indent_first_line_twips,
        indent_right_twips: start_p.indent_right_twips,
        shade_fill: start_p.shade_fill,
        border_bottom: start_p.border_bottom,
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
            let tmp =
                std::env::temp_dir().join(format!("orchid-docx-{}.docx", uuid::Uuid::new_v4()));
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
        let (word_count, char_count) = crate::document::model::text_stats(&plain_text);
        let sel = *self.selection.lock();
        let caret = sel.normalized().0;
        let style = style_at_cursor(doc, caret);
        let link_url = hyperlink_at_cursor(doc, sel.head)
            .map(|hl| hl.display_target())
            .unwrap_or_default();
        let para = paragraph_ref(doc, caret).or_else(|| first_paragraph(doc));
        let (
            bold,
            italic,
            underline,
            strikethrough,
            highlight,
            superscript,
            subscript,
            font_size_pt,
            font_family,
            color_rgb,
            alignment,
            list_kind,
        ) = (
            style.as_ref().is_some_and(|s| s.bold),
            style.as_ref().is_some_and(|s| s.italic),
            style.as_ref().is_some_and(|s| s.underline),
            style.as_ref().is_some_and(|s| s.strikethrough),
            style.as_ref().is_some_and(|s| s.highlight),
            style.as_ref().is_some_and(|s| s.superscript),
            style.as_ref().is_some_and(|s| s.subscript),
            style.as_ref().and_then(|s| s.font_size_pt).unwrap_or(0.0),
            style
                .as_ref()
                .and_then(|s| s.font_family.clone())
                .unwrap_or_default(),
            style
                .as_ref()
                .and_then(|s| s.color)
                .map(|[r, g, b]| (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b))
                .unwrap_or(0),
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
        let shade = if let Some(path) = caret.cell {
            matches!(
                doc.blocks.get(caret.block_idx),
                Some(Block::Table(t))
                    if t.rows
                        .get(path.row)
                        .and_then(|r| r.cells.get(path.col))
                        .is_some_and(|c| c.shade_fill.is_some())
            )
        } else {
            para.is_some_and(|p| p.shade_fill.is_some())
        };
        let border_bottom = para.is_some_and(|p| p.border_bottom);

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
            (Arc::clone(&prev.bytes), prev.width_px, prev.height_px)
        };
        let page_is_a4 = is_a4_page(&doc.page_setup);
        let page_landscape = is_landscape_page(&doc.page_setup);
        drop(doc_guard);
        ViewerSnapshot::Document(DocumentSnapshot {
            path_display,
            dirty,
            block_count,
            word_count,
            char_count,
            plain_text: Arc::from(plain_text.as_str()),
            warnings,
            info_text: String::new(),
            bold,
            italic,
            underline,
            strikethrough,
            highlight,
            shade,
            border_bottom,
            superscript,
            subscript,
            font_size_pt,
            font_family,
            color_rgb,
            alignment,
            list_kind,
            can_undo,
            can_redo,
            preview_rgba,
            preview_width_px,
            preview_height_px,
            preview_render_scale: crate::document::layout::PREVIEW_RENDER_SCALE.round() as i32,
            source_mode,
            find_gen: *self.find_gen.lock(),
            find_anchor: *self.find_anchor.lock(),
            find_cursor: *self.find_cursor.lock(),
            find_match_index: *self.find_match_index.lock(),
            find_match_count: *self.find_match_count.lock(),
            find_scroll_y_px: *self.find_scroll_y_px.lock(),
            preview_zoom_percent: ((*self.preview_zoom.lock() * 100.0).round() as i32).clamp(50, 300),
            link_hover: *self.link_hover.lock(),
            link_url,
            page_is_a4,
            page_landscape,
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

/// Non-overlapping UTF-8 byte starts of `needle` inside `haystack`.
fn non_overlapping_match_starts(haystack: &str, needle: &str) -> Vec<usize> {
    if needle.is_empty() || haystack.is_empty() {
        return Vec::new();
    }
    let mut starts = Vec::new();
    let mut pos = 0;
    while pos <= haystack.len() {
        match haystack[pos..].find(needle) {
            Some(rel) => {
                let abs = pos + rel;
                starts.push(abs);
                pos = abs + needle.len();
            }
            None => break,
        }
    }
    starts
}
