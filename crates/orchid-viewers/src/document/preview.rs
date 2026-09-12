//! Preview interaction, find/replace, and caret movement.

use super::edit_helpers::*;
use super::*;
use crate::error::{Result, ViewerError};

impl DocumentViewer {
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

    pub(super) fn invalidate_preview(&self) {
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
                    let insets = PreviewInsets::from_page_setup(&union_section_page_setup(doc));
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
    pub(super) fn sync_preview_width_after_margin_change(&self, doc: &Document) {
        let mut prev = self.preview.lock();
        if prev.viewport_px > 0.0 {
            let insets = PreviewInsets::from_page_setup(&union_section_page_setup(doc));
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
        let _ = self
            .document
            .read()
            .as_ref()
            .ok_or(ViewerError::DocumentNotOpen)?;
        let mut z = self.preview_zoom.lock();
        let next = ((*z * 10.0).round() as i32 + steps).clamp(5, 30) as f32 / 10.0;
        *z = next;
        Ok(())
    }

    /// Reset preview display zoom to 100%.
    pub fn reset_preview_zoom(&self) -> Result<()> {
        let _ = self
            .document
            .read()
            .as_ref()
            .ok_or(ViewerError::DocumentNotOpen)?;
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

    pub(super) fn set_link_hover(&self, hovering: bool) -> bool {
        let mut state = self.link_hover.lock();
        if *state == hovering {
            return false;
        }
        *state = hovering;
        true
    }

    pub(super) fn clear_link_hover(&self) -> PreviewPointerOutcome {
        PreviewPointerOutcome {
            open_url: None,
            refresh: self.set_link_hover(false),
        }
    }

    pub(super) fn note_preview_click(&self, offset: usize) -> u8 {
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

    pub(super) fn sync_preview_click_count(&self, count: u8, offset: usize) {
        let mut state = self.preview_click.lock();
        state.count = count;
        state.last_at = Some(Instant::now());
        state.last_offset = offset;
    }

    pub(super) fn reset_preview_click(&self) {
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

    /// Insert a next-page section break, or cycle break type on an existing end.
    ///
    /// When the caret sits on a paragraph that already has mid-body
    /// [`Paragraph::section_properties`], toggles `w:type` between `nextPage` and
    /// `continuous` (Preview skips the page band for continuous). Otherwise splits
    /// like Enter, attaches a copy of the caret section's [`PageSetup`] (next-page)
    /// as `w:pPr/w:sectPr`, and starts the following content on a new page band.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`] / edit bounds errors.
    pub fn preview_insert_section_break(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = *self.selection.lock();
        if sel.head.cell.is_none() {
            if let Some(Block::Paragraph(p)) = doc.blocks.get(sel.head.block_idx) {
                if p.section_properties.is_some() {
                    let bi = sel.head.block_idx;
                    drop(doc_guard);
                    return self.cycle_section_break_type_at(bi);
                }
            }
        }
        let setup = {
            let mut s = page_setup_for_cursor(doc, sel.head).clone();
            s.section_break = SectionBreakType::NextPage;
            s
        };
        let at = self.delete_selection_if_needed(doc, sel)?;
        if at.cell.is_some() {
            // Section breaks are body-level in OOXML; fall back to a page break in cells.
            drop(doc_guard);
            return self.insert_break_at_caret(true);
        }
        let blocks = split_paragraph_blocks(doc, at)?;
        let caret = Cursor::at(at.block_idx + 1, 0, 0);
        let mut next = blocks;
        if let Some(Block::Paragraph(p)) = next.get_mut(at.block_idx) {
            p.section_properties = Some(setup);
        }
        // split_paragraph_blocks moves any prior section_properties to the right para;
        // clear so the new section does not immediately end.
        if let Some(Block::Paragraph(p)) = next.get_mut(caret.block_idx) {
            p.section_properties = None;
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

    /// Toggle `w:type` on the mid-body `sectPr` at `block_idx` (`nextPage` ↔ `continuous`).
    pub(super) fn cycle_section_break_type_at(&self, block_idx: usize) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let Some(Block::Paragraph(p)) = doc.blocks.get(block_idx) else {
            return Ok(());
        };
        let Some(current) = p.section_properties.as_ref() else {
            return Ok(());
        };
        let mut next = current.clone();
        next.section_break = match current.section_break {
            SectionBreakType::NextPage => SectionBreakType::Continuous,
            SectionBreakType::Continuous => SectionBreakType::NextPage,
        };
        if next == *current {
            return Ok(());
        }
        self.undo.lock().push(
            doc,
            EditCommand::SetSectionPageSetup {
                end_block_idx: block_idx,
                setup: next,
            },
        )?;
        self.invalidate_preview();
        Ok(())
    }

    pub(super) fn insert_break_at_caret(&self, page_break: bool) -> Result<()> {
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
    pub(super) fn delete_or_move_across_cells(
        &self,
        doc: &mut Document,
        range: Selection,
    ) -> Result<Cursor> {
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

    pub(super) fn delete_image_cursor(
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

    pub(super) fn table_cell_context(&self) -> Result<(usize, usize, usize)> {
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
}
