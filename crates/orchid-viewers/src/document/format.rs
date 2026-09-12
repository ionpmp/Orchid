//! Character / paragraph / page formatting commands.

use super::edit_helpers::*;
use super::*;
use crate::error::{Result, ViewerError};

/// Maximum Word-compatible list indent level (`w:ilvl` 0..=8).
const MAX_LIST_LEVEL: u8 = 8;

impl DocumentViewer {
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

    /// Insert a named bookmark at the start of the current selection (or caret).
    ///
    /// Returns the generated bookmark name (e.g. `_OrchidBm1`).
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn insert_bookmark_at_selection(&self) -> Result<String> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let (start, _) = sel.normalized();
        let plain_offset = plain_offset_from_cursor(doc, start);
        let name = next_bookmark_name(doc);
        let bookmark = Bookmark {
            name: name.clone(),
            plain_offset,
        };
        self.undo
            .lock()
            .push(doc, EditCommand::AddBookmark { bookmark })?;
        self.invalidate_preview();
        Ok(name)
    }

    /// Insert a comment on the current selection (caret → zero-width range).
    ///
    /// Body text defaults to the selected plain text (trimmed, ≤200 chars) or
    /// `"Comment"` when the selection is empty. Author is `"Orchid"`.
    ///
    /// Returns the new comment id.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn insert_comment_at_selection(&self) -> Result<u32> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let (a, b) = sel.normalized();
        let start = plain_offset_from_cursor(doc, a);
        let end = plain_offset_from_cursor(doc, b);
        let start_plain = start.min(end);
        let end_plain = start.max(end);
        let id = next_comment_id(doc);
        let text = if start_plain < end_plain {
            let plain = doc.plain_text();
            let slice = plain.get(start_plain..end_plain).unwrap_or("").trim();
            let truncated: String = slice.chars().take(200).collect();
            if truncated.is_empty() {
                "Comment".into()
            } else {
                truncated
            }
        } else {
            "Comment".into()
        };
        let date = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let comment = DocComment {
            id,
            author: "Orchid".into(),
            initials: "Or".into(),
            date,
            text,
        };
        let range = CommentRange {
            id,
            start_plain,
            end_plain,
        };
        self.undo
            .lock()
            .push(doc, EditCommand::AddComment { comment, range })?;
        self.invalidate_preview();
        Ok(id)
    }

    /// Delete the comment covering the caret (or overlapping the selection).
    ///
    /// Prefers the narrowest overlapping range. Returns the removed id, or `None`
    /// when no comment touches the selection.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn delete_comment_at_selection(&self) -> Result<Option<u32>> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let (a, b) = sel.normalized();
        let lo = plain_offset_from_cursor(doc, a).min(plain_offset_from_cursor(doc, b));
        let hi = plain_offset_from_cursor(doc, a).max(plain_offset_from_cursor(doc, b));
        let Some(id) = comment_id_overlapping(doc, lo, hi) else {
            return Ok(None);
        };
        self.undo
            .lock()
            .push(doc, EditCommand::RemoveComment { id })?;
        self.invalidate_preview();
        Ok(Some(id))
    }

    /// Set the body text of the comment covering the caret / selection.
    ///
    /// Prefers the narrowest overlapping range. Empty / whitespace-only `text`
    /// is a no-op (`Ok(None)`). Returns the updated comment id, or `None` when
    /// no comment touches the selection.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn set_comment_text_at_selection(&self, text: &str) -> Result<Option<u32>> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let (a, b) = sel.normalized();
        let lo = plain_offset_from_cursor(doc, a).min(plain_offset_from_cursor(doc, b));
        let hi = plain_offset_from_cursor(doc, a).max(plain_offset_from_cursor(doc, b));
        let Some(id) = comment_id_overlapping(doc, lo, hi) else {
            return Ok(None);
        };
        let truncated: String = trimmed.chars().take(4000).collect();
        let current = doc
            .comments
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.text.as_str())
            .unwrap_or("");
        if current == truncated {
            return Ok(Some(id));
        }
        self.undo.lock().push(
            doc,
            EditCommand::UpdateCommentText {
                id,
                text: truncated,
            },
        )?;
        self.invalidate_preview();
        Ok(Some(id))
    }

    /// Move the selection to the next (`forward`) or previous comment range.
    ///
    /// Wraps at the ends. Selects the full plain-text span of the target
    /// comment and scrolls Preview to its start. Returns `false` when the
    /// document has no comments.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn goto_comment(&self, forward: bool) -> Result<bool> {
        let doc_guard = self.document.read();
        let doc = doc_guard.as_ref().ok_or(ViewerError::DocumentNotOpen)?;
        let mut ranges: Vec<(usize, usize, u32)> = doc
            .comment_ranges
            .iter()
            .map(|r| {
                let a = r.start_plain.min(r.end_plain);
                let b = r.start_plain.max(r.end_plain);
                (a, b, r.id)
            })
            .collect();
        ranges.sort_by_key(|&(a, b, id)| (a, b, id));
        if ranges.is_empty() {
            return Ok(false);
        }
        let sel = *self.selection.lock();
        let (na, nb) = sel.normalized();
        let lo = plain_offset_from_cursor(doc, na);
        let hi = plain_offset_from_cursor(doc, nb);
        let current_idx = comment_id_overlapping(doc, lo, hi)
            .and_then(|id| ranges.iter().position(|&(_, _, rid)| rid == id));
        let target = if let Some(i) = current_idx {
            if forward {
                ranges
                    .get(i + 1)
                    .copied()
                    .or_else(|| ranges.first().copied())
            } else if i == 0 {
                ranges.last().copied()
            } else {
                ranges.get(i - 1).copied()
            }
        } else if forward {
            ranges
                .iter()
                .copied()
                .find(|(a, _, _)| *a >= hi)
                .or_else(|| ranges.first().copied())
        } else {
            ranges
                .iter()
                .rev()
                .copied()
                .find(|(a, _, _)| *a < lo)
                .or_else(|| ranges.last().copied())
        };
        let Some((start, end, _)) = target else {
            return Ok(false);
        };
        let width = self.preview.lock().width;
        let scroll_y = self
            .layout
            .lock()
            .y_for_plain_offset(doc, width, start)
            .round() as i32;
        *self.selection.lock() = selection_from_plain_offsets(doc, start, end);
        *self.find_scroll_y_px.lock() = scroll_y;
        *self.find_gen.lock() += 1;
        self.invalidate_preview();
        Ok(true)
    }

    /// Toggle `w:keepNext` on selected paragraphs.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn toggle_keep_next_selection(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let cursors = paragraph_cursors_in_selection(doc, sel);
        if cursors.is_empty() {
            return Ok(());
        }
        let clear = cursors
            .iter()
            .any(|c| paragraph_ref(doc, *c).is_some_and(|p| p.keep_next));
        let new_keep = !clear;
        let mut next = doc.blocks.clone();
        let mut changed = false;
        for cursor in cursors {
            if let Some(p) = paragraph_mut_in_blocks(&mut next, cursor) {
                if p.keep_next != new_keep {
                    p.keep_next = new_keep;
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

    /// Toggle `w:keepLines` on selected paragraphs.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn toggle_keep_lines_selection(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let cursors = paragraph_cursors_in_selection(doc, sel);
        if cursors.is_empty() {
            return Ok(());
        }
        let clear = cursors
            .iter()
            .any(|c| paragraph_ref(doc, *c).is_some_and(|p| p.keep_lines));
        let new_keep = !clear;
        let mut next = doc.blocks.clone();
        let mut changed = false;
        for cursor in cursors {
            if let Some(p) = paragraph_mut_in_blocks(&mut next, cursor) {
                if p.keep_lines != new_keep {
                    p.keep_lines = new_keep;
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

    /// Toggle `w:widowControl` on selected paragraphs.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn toggle_widow_control_selection(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let cursors = paragraph_cursors_in_selection(doc, sel);
        if cursors.is_empty() {
            return Ok(());
        }
        let clear = cursors
            .iter()
            .any(|c| paragraph_ref(doc, *c).is_some_and(|p| p.widow_control));
        let new_keep = !clear;
        let mut next = doc.blocks.clone();
        let mut changed = false;
        for cursor in cursors {
            if let Some(p) = paragraph_mut_in_blocks(&mut next, cursor) {
                if p.widow_control != new_keep {
                    p.widow_control = new_keep;
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

    /// Toggle `w:contextualSpacing` on selected paragraphs.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn toggle_contextual_spacing_selection(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let cursors = paragraph_cursors_in_selection(doc, sel);
        if cursors.is_empty() {
            return Ok(());
        }
        let clear = cursors
            .iter()
            .any(|c| paragraph_ref(doc, *c).is_some_and(|p| p.contextual_spacing));
        let new_keep = !clear;
        let mut next = doc.blocks.clone();
        let mut changed = false;
        for cursor in cursors {
            if let Some(p) = paragraph_mut_in_blocks(&mut next, cursor) {
                if p.contextual_spacing != new_keep {
                    p.contextual_spacing = new_keep;
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

    /// Toggle `w:bidi` on selected paragraphs.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn toggle_bidi_selection(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let cursors = paragraph_cursors_in_selection(doc, sel);
        if cursors.is_empty() {
            return Ok(());
        }
        let clear = cursors
            .iter()
            .any(|c| paragraph_ref(doc, *c).is_some_and(|p| p.bidi));
        let new_keep = !clear;
        let mut next = doc.blocks.clone();
        let mut changed = false;
        for cursor in cursors {
            if let Some(p) = paragraph_mut_in_blocks(&mut next, cursor) {
                if p.bidi != new_keep {
                    p.bidi = new_keep;
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

    /// Toggle `w:suppressAutoHyphens` on selected paragraphs.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn toggle_suppress_auto_hyphens_selection(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let cursors = paragraph_cursors_in_selection(doc, sel);
        if cursors.is_empty() {
            return Ok(());
        }
        let clear = cursors
            .iter()
            .any(|c| paragraph_ref(doc, *c).is_some_and(|p| p.suppress_auto_hyphens));
        let new_keep = !clear;
        let mut next = doc.blocks.clone();
        let mut changed = false;
        for cursor in cursors {
            if let Some(p) = paragraph_mut_in_blocks(&mut next, cursor) {
                if p.suppress_auto_hyphens != new_keep {
                    p.suppress_auto_hyphens = new_keep;
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

    /// Cycle `w:outlineLvl` on selected paragraphs: body → H1…H9 → body.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn cycle_outline_level_selection(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let cursors = paragraph_cursors_in_selection(doc, sel);
        if cursors.is_empty() {
            return Ok(());
        }
        let current = cursors
            .iter()
            .find_map(|c| paragraph_ref(doc, *c).map(|p| p.outline_level));
        let next = match current.flatten() {
            None => Some(0),
            Some(lvl) if lvl < 8 => Some(lvl + 1),
            Some(_) => None,
        };
        let mut blocks = doc.blocks.clone();
        let mut changed = false;
        let next_style = Document::heading_style_id(next);
        for cursor in cursors {
            if let Some(p) = paragraph_mut_in_blocks(&mut blocks, cursor) {
                if p.outline_level != next || p.style_id != next_style {
                    p.outline_level = next;
                    p.style_id = next_style.clone();
                    changed = true;
                }
            }
        }
        if !changed {
            return Ok(());
        }
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks })?;
        self.invalidate_preview();
        Ok(())
    }

    /// Cycle named character style (`w:rStyle`) on the selection.
    ///
    /// Order: none → each `Document::character_styles` id (sorted) → none.
    /// No-op when the document defines no character styles.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn cycle_character_style_selection(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let mut ids: Vec<String> = doc.character_styles.keys().cloned().collect();
        ids.sort();
        if ids.is_empty() {
            return Ok(());
        }
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let (a, _) = sel.normalized();
        let current = run_style_id_at_cursor(doc, a);
        let next = match current.as_deref() {
            None => Some(ids[0].clone()),
            Some(cur) => match ids.iter().position(|id| id == cur) {
                Some(i) if i + 1 < ids.len() => Some(ids[i + 1].clone()),
                _ => None,
            },
        };
        let patch = RunStylePatch {
            style_id: Some(next),
            ..Default::default()
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
    pub(super) fn delete_selection_if_needed(
        &self,
        doc: &mut Document,
        sel: Selection,
    ) -> Result<Cursor> {
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
                'd' => s.double_strikethrough,
                'h' => s.highlight,
                '^' => s.superscript,
                '_' => s.subscript,
                'a' => s.all_caps,
                'm' => s.small_caps,
                'v' => s.vanish,
                'w' => s.shadow,
                'e' => s.emboss,
                'n' => s.imprint,
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
                double_strikethrough: if currently_on { None } else { Some(false) },
                ..Default::default()
            },
            'd' => RunStylePatch {
                double_strikethrough: Some(!currently_on),
                strikethrough: if currently_on { None } else { Some(false) },
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
            'a' => RunStylePatch {
                all_caps: Some(!currently_on),
                ..Default::default()
            },
            'm' => RunStylePatch {
                small_caps: Some(!currently_on),
                ..Default::default()
            },
            'v' => RunStylePatch {
                vanish: Some(!currently_on),
                ..Default::default()
            },
            'w' => RunStylePatch {
                shadow: Some(!currently_on),
                emboss: if currently_on { None } else { Some(false) },
                imprint: if currently_on { None } else { Some(false) },
                ..Default::default()
            },
            'e' => RunStylePatch {
                emboss: Some(!currently_on),
                imprint: if currently_on { None } else { Some(false) },
                shadow: if currently_on { None } else { Some(false) },
                ..Default::default()
            },
            'n' => RunStylePatch {
                imprint: Some(!currently_on),
                emboss: if currently_on { None } else { Some(false) },
                shadow: if currently_on { None } else { Some(false) },
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

    /// Toggle paragraph box border (`w:pBdr`) or table cell box border
    /// (`w:tcBorders`) on the current selection.
    ///
    /// When the caret is in a table cell, toggles all four cell edges; otherwise
    /// toggles a four-side paragraph border on selected paragraphs.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn toggle_paragraph_border_bottom_selection(&self) -> Result<()> {
        use crate::document::model::CELL_BORDER_ALL;

        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = effective_style_selection(doc, *self.selection.lock(), *self.source_mode.read());
        let cursors = paragraph_cursors_in_selection(doc, sel);
        if cursors.is_empty() {
            return Ok(());
        }

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
                        if t.rows
                            .get(row)
                            .and_then(|r| r.cells.get(col))
                            .is_some_and(|c| c.border_sides != 0)
                )
            });
            let new_sides = if clear { 0 } else { CELL_BORDER_ALL };
            for (bi, row, col) in cell_keys {
                if let Some(Block::Table(t)) = next.get_mut(bi) {
                    if let Some(cell) = t.rows.get_mut(row).and_then(|r| r.cells.get_mut(col)) {
                        if cell.border_sides != new_sides {
                            cell.border_sides = new_sides;
                            changed = true;
                        }
                    }
                }
            }
        } else {
            let clear = cursors
                .iter()
                .any(|c| paragraph_ref(doc, *c).is_some_and(|p| p.border_sides != 0));
            let new_sides = if clear { 0 } else { CELL_BORDER_ALL };
            for cursor in cursors {
                if let Some(p) = paragraph_mut_in_blocks(&mut next, cursor) {
                    if p.border_sides != new_sides {
                        p.border_sides = new_sides;
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
                let bumped = (p.indent_first_line_twips + delta_twips)
                    .clamp(INDENT_FIRST_LINE_MIN, INDENT_FIRST_LINE_MAX);
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

    /// Apply `mutate` to the page setup of the section that owns the caret.
    ///
    /// Mid-body sections update that paragraph's `section_properties`; the last
    /// section updates trailing [`Document::page_setup`].
    pub(super) fn mutate_caret_page_setup(
        &self,
        doc: &mut Document,
        mutate: impl FnOnce(&mut PageSetup),
    ) -> Result<bool> {
        let caret = self.selection.lock().head;
        let target = section_page_setup_target(doc, caret_section_index(doc, caret.block_idx));
        match target {
            SectionPageSetupTarget::Trailing => {
                let mut next = doc.page_setup.clone();
                mutate(&mut next);
                if next == doc.page_setup {
                    return Ok(false);
                }
                self.undo
                    .lock()
                    .push(doc, EditCommand::SetPageSetup { setup: next })?;
            }
            SectionPageSetupTarget::MidBody { end_block_idx } => {
                let Block::Paragraph(p) = doc
                    .blocks
                    .get(end_block_idx)
                    .ok_or(ViewerError::EditOutOfBounds)?
                else {
                    return Err(ViewerError::EditOutOfBounds);
                };
                let Some(current) = p.section_properties.as_ref() else {
                    return Err(ViewerError::EditOutOfBounds);
                };
                let mut next = current.clone();
                mutate(&mut next);
                if next == *current {
                    return Ok(false);
                }
                self.undo.lock().push(
                    doc,
                    EditCommand::SetSectionPageSetup {
                        end_block_idx,
                        setup: next,
                    },
                )?;
            }
        }
        Ok(true)
    }

    /// Bump all page margins by `delta_twips` (clamped to 0.25″–3″).
    ///
    /// Typical steps: `±180` (1/8″) or `±360` (1/4″). Applies to the caret
    /// section; Preview insets update immediately; undo restores the previous
    /// [`PageSetup`].
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn bump_page_margins(&self, delta_twips: i32) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let changed = self.mutate_caret_page_setup(doc, |next| {
            next.margin_top_twips = clamp_margin_twips(next.margin_top_twips as i32 + delta_twips);
            next.margin_bottom_twips =
                clamp_margin_twips(next.margin_bottom_twips as i32 + delta_twips);
            next.margin_left_twips =
                clamp_margin_twips(next.margin_left_twips as i32 + delta_twips);
            next.margin_right_twips =
                clamp_margin_twips(next.margin_right_twips as i32 + delta_twips);
        })?;
        if changed {
            self.sync_preview_width_after_margin_change(doc);
        }
        Ok(())
    }

    /// Nudge header and/or footer edge distances (`w:pgMar` `@w:header` / `@w:footer`).
    ///
    /// Positive `delta_twips` moves the story farther from the page edge.
    /// Applies to the caret section.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn bump_header_footer_distances(
        &self,
        header_delta_twips: i32,
        footer_delta_twips: i32,
    ) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let changed = self.mutate_caret_page_setup(doc, |next| {
            if header_delta_twips != 0 {
                next.header_distance_twips = clamp_header_footer_distance_twips(
                    next.header_distance_twips as i32 + header_delta_twips,
                );
            }
            if footer_delta_twips != 0 {
                next.footer_distance_twips = clamp_header_footer_distance_twips(
                    next.footer_distance_twips as i32 + footer_delta_twips,
                );
            }
        })?;
        if changed {
            self.invalidate_preview();
        }
        Ok(())
    }

    /// Plain text of the default header story (`\n`-joined paragraphs).
    #[must_use]
    pub fn header_plain_text(&self) -> String {
        self.document
            .read()
            .as_ref()
            .map(|d| story_plain_text(&d.header))
            .unwrap_or_default()
    }

    /// Plain text of the default footer story (`\n`-joined paragraphs).
    #[must_use]
    pub fn footer_plain_text(&self) -> String {
        self.document
            .read()
            .as_ref()
            .map(|d| story_plain_text(&d.footer))
            .unwrap_or_default()
    }

    /// Replace the default header story from plain text (empty clears).
    ///
    /// Preserves `PAGE`/`DATE`/… field runs and paragraph/run props when the
    /// overlay still contains each field's cached display text.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn set_header_plain_text(&self, text: &str) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let paragraphs = paragraphs_from_plain_preserving(&doc.header, text);
        if paragraphs == doc.header {
            return Ok(());
        }
        self.undo
            .lock()
            .push(doc, EditCommand::SetHeader { paragraphs })?;
        self.invalidate_preview();
        Ok(())
    }

    /// Replace the default footer story from plain text (empty clears).
    ///
    /// Preserves field runs and story formatting like [`Self::set_header_plain_text`].
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn set_footer_plain_text(&self, text: &str) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let paragraphs = paragraphs_from_plain_preserving(&doc.footer, text);
        if paragraphs == doc.footer {
            return Ok(());
        }
        self.undo
            .lock()
            .push(doc, EditCommand::SetFooter { paragraphs })?;
        self.invalidate_preview();
        Ok(())
    }

    /// Replace the first-page header story from plain text (empty clears).
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn set_header_first_plain_text(&self, text: &str) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let paragraphs = paragraphs_from_plain_preserving(&doc.header_first, text);
        if paragraphs == doc.header_first {
            return Ok(());
        }
        self.undo
            .lock()
            .push(doc, EditCommand::SetHeaderFirst { paragraphs })?;
        self.invalidate_preview();
        Ok(())
    }

    /// Replace the first-page footer story from plain text (empty clears).
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn set_footer_first_plain_text(&self, text: &str) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let paragraphs = paragraphs_from_plain_preserving(&doc.footer_first, text);
        if paragraphs == doc.footer_first {
            return Ok(());
        }
        self.undo
            .lock()
            .push(doc, EditCommand::SetFooterFirst { paragraphs })?;
        self.invalidate_preview();
        Ok(())
    }

    /// Replace the even-page header story from plain text (empty clears).
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn set_header_even_plain_text(&self, text: &str) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let paragraphs = paragraphs_from_plain_preserving(&doc.header_even, text);
        if paragraphs == doc.header_even {
            return Ok(());
        }
        self.undo
            .lock()
            .push(doc, EditCommand::SetHeaderEven { paragraphs })?;
        self.invalidate_preview();
        Ok(())
    }

    /// Replace the even-page footer story from plain text (empty clears).
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn set_footer_even_plain_text(&self, text: &str) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let paragraphs = paragraphs_from_plain_preserving(&doc.footer_even, text);
        if paragraphs == doc.footer_even {
            return Ok(());
        }
        self.undo
            .lock()
            .push(doc, EditCommand::SetFooterEven { paragraphs })?;
        self.invalidate_preview();
        Ok(())
    }

    /// Append `PAGE` / `NUMPAGES` fields to the default footer (`Pg#` toolbar).
    ///
    /// Inserts `PAGE`, ` / `, and `NUMPAGES` runs (with a leading space when the
    /// footer already has text).
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    /// Insert a simple field (`DATE`, `FILENAME`, …) at the preview caret.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`] / edit bounds errors.
    pub fn insert_field_at_selection(&self, field: DocField) -> Result<()> {
        let file_name = self.path.read().as_ref().and_then(|p| {
            std::path::Path::new(p.as_str())
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_owned)
        });
        let text = field.display(1, 1, file_name.as_deref());
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let sel = *self.selection.lock();
        let at = self.delete_selection_if_needed(doc, sel)?;
        if at.cell.is_some() {
            drop(doc_guard);
            return self.preview_insert_text(&text);
        }
        let mut blocks = doc.blocks.clone();
        let Block::Paragraph(p) = blocks
            .get_mut(at.block_idx)
            .ok_or(ViewerError::EditOutOfBounds)?
        else {
            return Err(ViewerError::EditOutOfBounds);
        };
        let (left, right) = split_runs_at(p, at);
        let field_run_idx = left.len();
        let mut runs = left;
        runs.push(Run {
            text: text.clone(),
            field: Some(field),
            ..Default::default()
        });
        runs.extend(right);
        p.runs = runs;
        self.undo
            .lock()
            .push(doc, EditCommand::ReplaceBlocks { blocks })?;
        let caret = Cursor {
            block_idx: at.block_idx,
            cell: None,
            run_idx: field_run_idx,
            byte_offset: text.len(),
        };
        *self.selection.lock() = Selection {
            anchor: caret,
            head: caret,
        };
        self.invalidate_preview();
        Ok(())
    }

    /// Insert PAGE / NUMPAGES fields into the default footer.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn insert_page_number_fields_in_footer(&self) -> Result<()> {
        use crate::document::model::DocField;
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let mut paragraphs = doc.footer.clone();
        let mut runs = vec![
            Run {
                text: "1".into(),
                field: Some(DocField::Page),
                ..Default::default()
            },
            Run {
                text: " / ".into(),
                ..Default::default()
            },
            Run {
                text: "1".into(),
                field: Some(DocField::NumPages),
                ..Default::default()
            },
        ];
        if let Some(last) = paragraphs.last_mut() {
            if last
                .runs
                .last()
                .is_some_and(|r| !r.text.is_empty() || r.field.is_some())
            {
                last.runs.push(Run {
                    text: " ".into(),
                    ..Default::default()
                });
            }
            last.runs.append(&mut runs);
        } else {
            paragraphs.push(Paragraph {
                runs,
                ..Default::default()
            });
        }
        if paragraphs == doc.footer {
            return Ok(());
        }
        self.undo
            .lock()
            .push(doc, EditCommand::SetFooter { paragraphs })?;
        self.invalidate_preview();
        Ok(())
    }

    /// Toggle page size between US Letter and ISO A4 (margins unchanged).
    ///
    /// Applies to the caret section.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn cycle_page_size(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let changed = self.mutate_caret_page_setup(doc, |next| {
            let landscape = is_landscape_page(next);
            if is_a4_page(next) {
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
        })?;
        if changed {
            self.sync_preview_width_after_margin_change(doc);
        }
        Ok(())
    }

    /// Swap page width and height (portrait ↔ landscape).
    ///
    /// Applies to the caret section.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn toggle_page_orientation(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let changed = self.mutate_caret_page_setup(doc, |next| {
            std::mem::swap(&mut next.width_twips, &mut next.height_twips);
        })?;
        if changed {
            self.sync_preview_width_after_margin_change(doc);
        }
        Ok(())
    }

    /// Toggle different first page (`w:titlePg`).
    ///
    /// When enabled, Preview prefers first-page header/footer stories if present.
    /// Applies to the caret section.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn toggle_title_page(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let changed = self.mutate_caret_page_setup(doc, |next| {
            next.title_page = !next.title_page;
        })?;
        if changed {
            self.invalidate_preview();
        }
        Ok(())
    }

    /// Toggle different odd and even pages (`w:evenAndOddHeaders`).
    ///
    /// When enabled, Preview prefers even-page header/footer stories if present
    /// (unless a title-page story takes precedence). Applies to the caret section.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentNotOpen`].
    pub fn toggle_even_and_odd_headers(&self) -> Result<()> {
        let mut doc_guard = self.document.write();
        let doc = doc_guard.as_mut().ok_or(ViewerError::DocumentNotOpen)?;
        let changed = self.mutate_caret_page_setup(doc, |next| {
            next.even_and_odd_headers = !next.even_and_odd_headers;
        })?;
        if changed {
            self.invalidate_preview();
        }
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
