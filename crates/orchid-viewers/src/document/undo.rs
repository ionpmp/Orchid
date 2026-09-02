//! Edit commands and undo/redo for the document model.

use crate::document::cursor::{paragraph_mut, paragraph_ref, Cursor, Selection};
use crate::document::model::{
    Alignment, Block, Bookmark, CellImage, Document, Hyperlink, InlineImage, ListKind, PageSetup,
    Paragraph, Run, RunStyle, Table, TableCell, TableRow,
};
use crate::error::{Result, ViewerError};

/// One reversible edit.
#[derive(Debug, Clone)]
pub enum EditCommand {
    /// Insert text at a cursor.
    InsertText {
        /// Insertion point.
        at: Cursor,
        /// Text to insert.
        text: String,
    },
    /// Delete the text covered by a selection.
    DeleteRange {
        /// Range to delete.
        range: Selection,
    },
    /// Overlay style fields onto runs intersecting `range`.
    SetRunStyle {
        /// Target range.
        range: Selection,
        /// Style fields to apply.
        style: RunStylePatch,
    },
    /// Set paragraph alignment.
    SetAlignment {
        /// Block index (must be a paragraph).
        paragraph_idx: usize,
        /// New alignment.
        alignment: Alignment,
    },
    /// Set list kind on a paragraph.
    ToggleList {
        /// Block index.
        paragraph_idx: usize,
        /// Desired list kind.
        kind: ListKind,
    },
    /// Insert a new table block at `at_block`.
    InsertTable {
        /// Block index to insert at (existing blocks shift right).
        at_block: usize,
        /// Table payload.
        table: Table,
    },
    /// Insert an empty row into a table.
    InsertTableRow {
        /// Block index of the table.
        table_idx: usize,
        /// Row index to insert at.
        at_row: usize,
    },
    /// Delete a table row.
    DeleteTableRow {
        /// Block index of the table.
        table_idx: usize,
        /// Row index to remove.
        row_idx: usize,
    },
    /// Insert an empty column into a table.
    InsertTableColumn {
        /// Block index of the table.
        table_idx: usize,
        /// Column index to insert at.
        at_col: usize,
    },
    /// Delete a table column.
    DeleteTableColumn {
        /// Block index of the table.
        table_idx: usize,
        /// Column index to remove.
        col_idx: usize,
    },
    /// Insert an image as a new block at `at.block_idx`.
    InsertImage {
        /// Insertion cursor (block index used).
        at: Cursor,
        /// Image payload.
        image: InlineImage,
    },
    /// Insert an image inside a table cell.
    InsertCellImage {
        /// Block index of the table.
        table_idx: usize,
        /// Row index.
        row: usize,
        /// Column index.
        col: usize,
        /// Index in [`TableCell::images`].
        image_idx: usize,
        /// Image payload.
        image: CellImage,
    },
    /// Remove a cell image (inverse stores the removed image).
    RemoveCellImage {
        /// Block index of the table.
        table_idx: usize,
        /// Row index.
        row: usize,
        /// Column index.
        col: usize,
        /// Index in [`TableCell::images`].
        image_idx: usize,
    },
    /// Restore a previous paragraph (inverse helper).
    ReplaceParagraph {
        /// Block index.
        paragraph_idx: usize,
        /// Paragraph to restore.
        previous: Paragraph,
    },
    /// Restore a previous block (inverse helper).
    ReplaceBlock {
        /// Block index.
        block_idx: usize,
        /// Block to restore.
        previous: Block,
    },
    /// Replace the entire body block list (plain-text push / bulk format).
    ReplaceBlocks {
        /// New body blocks.
        blocks: Vec<Block>,
    },
    /// Replace page size / margins (`w:sectPr` / `w:pgMar`).
    SetPageSetup {
        /// New page geometry.
        setup: PageSetup,
    },
    /// Insert a named bookmark at a plain-text offset.
    AddBookmark {
        /// Bookmark payload.
        bookmark: Bookmark,
    },
    /// Remove a bookmark by name.
    RemoveBookmark {
        /// Bookmark name (`w:name`).
        name: String,
    },
    /// Remove a block (inverse of insert).
    RemoveBlock {
        /// Block index to remove.
        block_idx: usize,
    },
    /// Re-insert a table row (inverse of delete).
    RestoreTableRow {
        /// Block index of the table.
        table_idx: usize,
        /// Row index.
        at_row: usize,
        /// Saved row.
        row: TableRow,
    },
    /// Re-insert a table column (inverse of delete).
    RestoreTableColumn {
        /// Block index of the table.
        table_idx: usize,
        /// Column index.
        at_col: usize,
        /// Saved cell per row (`None` when that row had no cell at the index).
        cells: Vec<Option<TableCell>>,
        /// Saved `column_widths_twips` entry when the table had explicit widths.
        width_twips: Option<u32>,
    },
}

/// Partial style update — only set fields are applied.
#[derive(Debug, Clone, Default)]
pub struct RunStylePatch {
    /// Set bold.
    pub bold: Option<bool>,
    /// Set italic.
    pub italic: Option<bool>,
    /// Set underline.
    pub underline: Option<bool>,
    /// Set strikethrough.
    pub strikethrough: Option<bool>,
    /// Set highlight.
    pub highlight: Option<bool>,
    /// Set superscript (clears subscript when enabled).
    pub superscript: Option<bool>,
    /// Set subscript (clears superscript when enabled).
    pub subscript: Option<bool>,
    /// Set colour (`Some(None)` clears).
    pub color: Option<Option<[u8; 3]>>,
    /// Set font family.
    pub font_family: Option<Option<String>>,
    /// Set font size.
    pub font_size_pt: Option<Option<f32>>,
    /// Set external hyperlink (`Some(None)` clears).
    pub hyperlink: Option<Option<Hyperlink>>,
}

impl RunStylePatch {
    /// Patch that resets every character-style field to the document default.
    #[must_use]
    pub fn clear_character() -> Self {
        Self {
            bold: Some(false),
            italic: Some(false),
            underline: Some(false),
            strikethrough: Some(false),
            highlight: Some(false),
            superscript: Some(false),
            subscript: Some(false),
            color: Some(None),
            font_family: Some(None),
            font_size_pt: Some(None),
            hyperlink: Some(None),
        }
    }

    /// Apply this patch onto `style`.
    pub fn apply_to(&self, style: &mut RunStyle) {
        if let Some(v) = self.bold {
            style.bold = v;
        }
        if let Some(v) = self.italic {
            style.italic = v;
        }
        if let Some(v) = self.underline {
            style.underline = v;
        }
        if let Some(v) = self.strikethrough {
            style.strikethrough = v;
        }
        if let Some(v) = self.highlight {
            style.highlight = v;
        }
        if let Some(v) = self.superscript {
            style.superscript = v;
            if v {
                style.subscript = false;
            }
        }
        if let Some(v) = self.subscript {
            style.subscript = v;
            if v {
                style.superscript = false;
            }
        }
        if let Some(ref c) = self.color {
            style.color = *c;
        }
        if let Some(ref f) = self.font_family {
            style.font_family = f.clone();
        }
        if let Some(s) = self.font_size_pt {
            style.font_size_pt = s;
        }
    }

    /// Apply character style and optional hyperlink onto `run`.
    pub fn apply_to_run(&self, run: &mut Run) {
        self.apply_to(&mut run.style);
        if let Some(ref hl) = self.hyperlink {
            run.hyperlink = hl.clone();
        }
    }
}

#[derive(Debug, Clone)]
struct StackEntry {
    forward: EditCommand,
    inverse: EditCommand,
}

/// Undo + redo stacks.
#[derive(Debug, Default)]
pub struct UndoStack {
    past: Vec<StackEntry>,
    future: Vec<EditCommand>,
    dirty: bool,
    saved_len: usize,
}

impl UndoStack {
    /// Empty stack.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether unsaved edits exist.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Whether undo is available.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    /// Whether redo is available.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    /// Mark the document as saved.
    pub fn mark_clean(&mut self) {
        self.saved_len = self.past.len();
        self.dirty = false;
    }

    /// Apply `cmd`, recording undo information.
    ///
    /// # Errors
    ///
    /// Propagates edit errors from [`apply_command`].
    pub fn push(&mut self, doc: &mut Document, cmd: EditCommand) -> Result<()> {
        let inverse = apply_command(doc, &cmd)?;
        self.past.push(StackEntry {
            forward: cmd,
            inverse,
        });
        self.future.clear();
        self.dirty = true;
        Ok(())
    }

    /// Undo last command.
    ///
    /// # Errors
    ///
    /// [`ViewerError::EditOutOfBounds`] when the stack is empty.
    pub fn undo(&mut self, doc: &mut Document) -> Result<()> {
        let Some(entry) = self.past.pop() else {
            return Err(ViewerError::EditOutOfBounds);
        };
        let _ = apply_command(doc, &entry.inverse)?;
        self.future.push(entry.forward);
        self.dirty = self.past.len() != self.saved_len;
        Ok(())
    }

    /// Redo last undone command.
    ///
    /// # Errors
    ///
    /// [`ViewerError::EditOutOfBounds`] when there is nothing to redo.
    pub fn redo(&mut self, doc: &mut Document) -> Result<()> {
        let Some(cmd) = self.future.pop() else {
            return Err(ViewerError::EditOutOfBounds);
        };
        let inverse = apply_command(doc, &cmd)?;
        self.past.push(StackEntry {
            forward: cmd,
            inverse,
        });
        self.dirty = true;
        Ok(())
    }
}

/// Apply a command; returns an inverse suitable for undo.
///
/// # Errors
///
/// [`ViewerError::EditOutOfBounds`] on invalid indices.
pub fn apply_command(doc: &mut Document, cmd: &EditCommand) -> Result<EditCommand> {
    match cmd {
        EditCommand::InsertText { at, text } => {
            apply_insert_text(doc, *at, text)?;
            let end = Cursor {
                block_idx: at.block_idx,
                cell: at.cell,
                run_idx: at.run_idx,
                byte_offset: at.byte_offset + text.len(),
            };
            Ok(EditCommand::DeleteRange {
                range: Selection {
                    anchor: *at,
                    head: end,
                },
            })
        }
        EditCommand::DeleteRange { range } => {
            let (start, end) = range.normalized();
            let text = extract_text(doc, start, end)?;
            apply_delete_range(doc, start, end)?;
            Ok(EditCommand::InsertText { at: start, text })
        }
        EditCommand::SetRunStyle { range, style } => {
            let previous = doc.blocks.clone();
            apply_set_run_style(doc, *range, style)?;
            Ok(EditCommand::ReplaceBlocks { blocks: previous })
        }
        EditCommand::SetAlignment {
            paragraph_idx,
            alignment,
        } => {
            let Block::Paragraph(p) = doc
                .blocks
                .get_mut(*paragraph_idx)
                .ok_or(ViewerError::EditOutOfBounds)?
            else {
                return Err(ViewerError::EditOutOfBounds);
            };
            let previous = p.clone();
            p.alignment = *alignment;
            Ok(EditCommand::ReplaceParagraph {
                paragraph_idx: *paragraph_idx,
                previous,
            })
        }
        EditCommand::ToggleList {
            paragraph_idx,
            kind,
        } => {
            let Block::Paragraph(p) = doc
                .blocks
                .get_mut(*paragraph_idx)
                .ok_or(ViewerError::EditOutOfBounds)?
            else {
                return Err(ViewerError::EditOutOfBounds);
            };
            let previous = p.clone();
            p.list = *kind;
            p.num_id = crate::document::ooxml::numbering::num_id_for_kind(*kind);
            Ok(EditCommand::ReplaceParagraph {
                paragraph_idx: *paragraph_idx,
                previous,
            })
        }
        EditCommand::InsertTable { at_block, table } => {
            let idx = (*at_block).min(doc.blocks.len());
            doc.blocks.insert(idx, Block::Table(table.clone()));
            Ok(EditCommand::RemoveBlock { block_idx: idx })
        }
        EditCommand::InsertTableRow { table_idx, at_row } => {
            let Block::Table(t) = doc
                .blocks
                .get_mut(*table_idx)
                .ok_or(ViewerError::EditOutOfBounds)?
            else {
                return Err(ViewerError::EditOutOfBounds);
            };
            let cols = t.rows.first().map(|r| r.cells.len()).unwrap_or(1).max(1);
            let row = TableRow {
                cells: (0..cols)
                    .map(|_| TableCell::from_paragraphs(vec![Paragraph::default()]))
                    .collect(),
            };
            let idx = (*at_row).min(t.rows.len());
            t.rows.insert(idx, row);
            Ok(EditCommand::DeleteTableRow {
                table_idx: *table_idx,
                row_idx: idx,
            })
        }
        EditCommand::DeleteTableRow { table_idx, row_idx } => {
            let Block::Table(t) = doc
                .blocks
                .get_mut(*table_idx)
                .ok_or(ViewerError::EditOutOfBounds)?
            else {
                return Err(ViewerError::EditOutOfBounds);
            };
            if t.rows.len() <= 1 || *row_idx >= t.rows.len() {
                return Err(ViewerError::EditOutOfBounds);
            }
            let row = t.rows.remove(*row_idx);
            Ok(EditCommand::RestoreTableRow {
                table_idx: *table_idx,
                at_row: *row_idx,
                row,
            })
        }
        EditCommand::RestoreTableRow {
            table_idx,
            at_row,
            row,
        } => {
            let Block::Table(t) = doc
                .blocks
                .get_mut(*table_idx)
                .ok_or(ViewerError::EditOutOfBounds)?
            else {
                return Err(ViewerError::EditOutOfBounds);
            };
            let idx = (*at_row).min(t.rows.len());
            t.rows.insert(idx, row.clone());
            Ok(EditCommand::DeleteTableRow {
                table_idx: *table_idx,
                row_idx: idx,
            })
        }
        EditCommand::InsertTableColumn { table_idx, at_col } => {
            let Block::Table(t) = doc
                .blocks
                .get_mut(*table_idx)
                .ok_or(ViewerError::EditOutOfBounds)?
            else {
                return Err(ViewerError::EditOutOfBounds);
            };
            if t.rows.is_empty() {
                return Err(ViewerError::EditOutOfBounds);
            }
            let at = *at_col;
            for row in &mut t.rows {
                let i = at.min(row.cells.len());
                row.cells
                    .insert(i, TableCell::from_paragraphs(vec![Paragraph::default()]));
            }
            if !t.column_widths_twips.is_empty() {
                let i = at.min(t.column_widths_twips.len());
                let neighbor = t
                    .column_widths_twips
                    .get(i.saturating_sub(1))
                    .or_else(|| t.column_widths_twips.first())
                    .copied()
                    .unwrap_or(2880);
                t.column_widths_twips.insert(i, neighbor);
            }
            Ok(EditCommand::DeleteTableColumn {
                table_idx: *table_idx,
                col_idx: at,
            })
        }
        EditCommand::DeleteTableColumn { table_idx, col_idx } => {
            let Block::Table(t) = doc
                .blocks
                .get_mut(*table_idx)
                .ok_or(ViewerError::EditOutOfBounds)?
            else {
                return Err(ViewerError::EditOutOfBounds);
            };
            let max_cols = t.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
            if max_cols <= 1 || *col_idx >= max_cols {
                return Err(ViewerError::EditOutOfBounds);
            }
            let mut cells = Vec::with_capacity(t.rows.len());
            for row in &mut t.rows {
                if *col_idx < row.cells.len() {
                    cells.push(Some(row.cells.remove(*col_idx)));
                } else {
                    cells.push(None);
                }
            }
            let width_twips = if *col_idx < t.column_widths_twips.len() {
                Some(t.column_widths_twips.remove(*col_idx))
            } else {
                None
            };
            Ok(EditCommand::RestoreTableColumn {
                table_idx: *table_idx,
                at_col: *col_idx,
                cells,
                width_twips,
            })
        }
        EditCommand::RestoreTableColumn {
            table_idx,
            at_col,
            cells,
            width_twips,
        } => {
            let Block::Table(t) = doc
                .blocks
                .get_mut(*table_idx)
                .ok_or(ViewerError::EditOutOfBounds)?
            else {
                return Err(ViewerError::EditOutOfBounds);
            };
            if cells.len() != t.rows.len() {
                return Err(ViewerError::EditOutOfBounds);
            }
            for (row, cell) in t.rows.iter_mut().zip(cells.iter()) {
                if let Some(cell) = cell {
                    let i = (*at_col).min(row.cells.len());
                    row.cells.insert(i, cell.clone());
                }
            }
            if let Some(w) = *width_twips {
                let i = (*at_col).min(t.column_widths_twips.len());
                t.column_widths_twips.insert(i, w);
            }
            Ok(EditCommand::DeleteTableColumn {
                table_idx: *table_idx,
                col_idx: *at_col,
            })
        }
        EditCommand::InsertImage { at, image } => {
            let idx = at.block_idx.min(doc.blocks.len());
            doc.blocks.insert(idx, Block::Image(image.clone()));
            Ok(EditCommand::RemoveBlock { block_idx: idx })
        }
        EditCommand::InsertCellImage {
            table_idx,
            row,
            col,
            image_idx,
            image,
        } => {
            let Block::Table(t) = doc
                .blocks
                .get_mut(*table_idx)
                .ok_or(ViewerError::EditOutOfBounds)?
            else {
                return Err(ViewerError::EditOutOfBounds);
            };
            let cell = t
                .rows
                .get_mut(*row)
                .ok_or(ViewerError::EditOutOfBounds)?
                .cells
                .get_mut(*col)
                .ok_or(ViewerError::EditOutOfBounds)?;
            let idx = (*image_idx).min(cell.images.len());
            cell.images.insert(idx, image.clone());
            Ok(EditCommand::RemoveCellImage {
                table_idx: *table_idx,
                row: *row,
                col: *col,
                image_idx: idx,
            })
        }
        EditCommand::RemoveCellImage {
            table_idx,
            row,
            col,
            image_idx,
        } => {
            let Block::Table(t) = doc
                .blocks
                .get_mut(*table_idx)
                .ok_or(ViewerError::EditOutOfBounds)?
            else {
                return Err(ViewerError::EditOutOfBounds);
            };
            let cell = t
                .rows
                .get_mut(*row)
                .ok_or(ViewerError::EditOutOfBounds)?
                .cells
                .get_mut(*col)
                .ok_or(ViewerError::EditOutOfBounds)?;
            if *image_idx >= cell.images.len() {
                return Err(ViewerError::EditOutOfBounds);
            }
            let removed = cell.images.remove(*image_idx);
            Ok(EditCommand::InsertCellImage {
                table_idx: *table_idx,
                row: *row,
                col: *col,
                image_idx: *image_idx,
                image: removed,
            })
        }
        EditCommand::ReplaceParagraph {
            paragraph_idx,
            previous,
        } => {
            let slot = doc
                .blocks
                .get_mut(*paragraph_idx)
                .ok_or(ViewerError::EditOutOfBounds)?;
            let current = match slot {
                Block::Paragraph(p) => p.clone(),
                _ => return Err(ViewerError::EditOutOfBounds),
            };
            *slot = Block::Paragraph(previous.clone());
            Ok(EditCommand::ReplaceParagraph {
                paragraph_idx: *paragraph_idx,
                previous: current,
            })
        }
        EditCommand::ReplaceBlock {
            block_idx,
            previous,
        } => {
            if *block_idx > doc.blocks.len() {
                return Err(ViewerError::EditOutOfBounds);
            }
            if *block_idx == doc.blocks.len() {
                doc.blocks.push(previous.clone());
                return Ok(EditCommand::RemoveBlock {
                    block_idx: *block_idx,
                });
            }
            let current = doc.blocks[*block_idx].clone();
            doc.blocks[*block_idx] = previous.clone();
            Ok(EditCommand::ReplaceBlock {
                block_idx: *block_idx,
                previous: current,
            })
        }
        EditCommand::ReplaceBlocks { blocks } => {
            let previous = std::mem::replace(&mut doc.blocks, blocks.clone());
            Ok(EditCommand::ReplaceBlocks { blocks: previous })
        }
        EditCommand::SetPageSetup { setup } => {
            let previous = std::mem::replace(&mut doc.page_setup, setup.clone());
            Ok(EditCommand::SetPageSetup { setup: previous })
        }
        EditCommand::AddBookmark { bookmark } => {
            if doc.bookmarks.iter().any(|b| b.name == bookmark.name) {
                return Err(ViewerError::EditOutOfBounds);
            }
            doc.bookmarks.push(bookmark.clone());
            Ok(EditCommand::RemoveBookmark {
                name: bookmark.name.clone(),
            })
        }
        EditCommand::RemoveBookmark { name } => {
            let idx = doc
                .bookmarks
                .iter()
                .position(|b| b.name == *name)
                .ok_or(ViewerError::EditOutOfBounds)?;
            let bookmark = doc.bookmarks.remove(idx);
            Ok(EditCommand::AddBookmark { bookmark })
        }
        EditCommand::RemoveBlock { block_idx } => {
            if *block_idx >= doc.blocks.len() {
                return Err(ViewerError::EditOutOfBounds);
            }
            let previous = doc.blocks.remove(*block_idx);
            Ok(EditCommand::ReplaceBlock {
                block_idx: *block_idx,
                previous,
            })
        }
    }
}

fn apply_insert_text(doc: &mut Document, at: Cursor, text: &str) -> Result<()> {
    let p = paragraph_mut(doc, at).ok_or(ViewerError::EditOutOfBounds)?;
    if p.runs.is_empty() {
        p.runs.push(Run {
            text: text.to_string(),
            style: RunStyle::default(),
            ..Default::default()
        });
        return Ok(());
    }
    let run = p
        .runs
        .get_mut(at.run_idx)
        .ok_or(ViewerError::EditOutOfBounds)?;
    if at.byte_offset > run.text.len() {
        return Err(ViewerError::EditOutOfBounds);
    }
    run.text.insert_str(at.byte_offset, text);
    Ok(())
}

fn apply_delete_range(doc: &mut Document, start: Cursor, end: Cursor) -> Result<()> {
    if !start.same_paragraph(end) {
        return Err(ViewerError::EditOutOfBounds);
    }
    let p = paragraph_mut(doc, start).ok_or(ViewerError::EditOutOfBounds)?;
    if start.run_idx == end.run_idx {
        let run = p
            .runs
            .get_mut(start.run_idx)
            .ok_or(ViewerError::EditOutOfBounds)?;
        if start.byte_offset > end.byte_offset || end.byte_offset > run.text.len() {
            return Err(ViewerError::EditOutOfBounds);
        }
        run.text.drain(start.byte_offset..end.byte_offset);
        return Ok(());
    }
    let first = p
        .runs
        .get_mut(start.run_idx)
        .ok_or(ViewerError::EditOutOfBounds)?;
    if start.byte_offset > first.text.len() {
        return Err(ViewerError::EditOutOfBounds);
    }
    first.text.truncate(start.byte_offset);
    let last = p
        .runs
        .get(end.run_idx)
        .ok_or(ViewerError::EditOutOfBounds)?;
    if end.byte_offset > last.text.len() {
        return Err(ViewerError::EditOutOfBounds);
    }
    let rest = last.text[end.byte_offset..].to_string();
    p.runs.drain((start.run_idx + 1)..=end.run_idx);
    if let Some(run) = p.runs.get_mut(start.run_idx) {
        run.text.push_str(&rest);
    }
    Ok(())
}

fn apply_set_run_style(doc: &mut Document, range: Selection, patch: &RunStylePatch) -> Result<()> {
    let (start, end) = range.normalized();
    if start.same_paragraph(end) {
        return apply_set_run_style_one_para(doc, start, end, patch);
    }
    if start.cell.is_some() || end.cell.is_some() {
        return Err(ViewerError::EditOutOfBounds);
    }
    if start.block_idx > end.block_idx || end.block_idx >= doc.blocks.len() {
        return Err(ViewerError::EditOutOfBounds);
    }
    for bi in start.block_idx..=end.block_idx {
        let Block::Paragraph(p) = doc.blocks.get_mut(bi).ok_or(ViewerError::EditOutOfBounds)?
        else {
            continue;
        };
        if p.runs.is_empty() {
            p.runs.push(Run {
                text: String::new(),
                style: RunStyle::default(),
                ..Default::default()
            });
        }
        let (run_start, byte_start) = if bi == start.block_idx {
            (start.run_idx, start.byte_offset)
        } else {
            (0, 0)
        };
        let (run_end, byte_end) = if bi == end.block_idx {
            (end.run_idx, end.byte_offset)
        } else {
            let last = p.runs.len().saturating_sub(1);
            let end_off = p.runs.get(last).map(|r| r.text.len()).unwrap_or(0);
            (last, end_off)
        };
        if bi == start.block_idx
            && bi == end.block_idx
            && run_start == run_end
            && byte_start == byte_end
        {
            let idx = run_start.min(p.runs.len().saturating_sub(1));
            if let Some(run) = p.runs.get_mut(idx) {
                patch.apply_to_run(run);
            }
            continue;
        }
        apply_style_to_paragraph_range(p, run_start, byte_start, run_end, byte_end, patch)?;
    }
    Ok(())
}

fn apply_set_run_style_one_para(
    doc: &mut Document,
    start: Cursor,
    end: Cursor,
    patch: &RunStylePatch,
) -> Result<()> {
    let p = paragraph_mut(doc, start).ok_or(ViewerError::EditOutOfBounds)?;
    if p.runs.is_empty() {
        p.runs.push(Run {
            text: String::new(),
            style: RunStyle::default(),
            ..Default::default()
        });
    }
    let run_start = start.run_idx;
    let byte_start = start.byte_offset;
    let run_end = end.run_idx;
    let byte_end = end.byte_offset;
    if run_start == run_end && byte_start == byte_end {
        let idx = run_start.min(p.runs.len().saturating_sub(1));
        if let Some(run) = p.runs.get_mut(idx) {
            patch.apply_to_run(run);
        }
        return Ok(());
    }
    apply_style_to_paragraph_range(p, run_start, byte_start, run_end, byte_end, patch)
}

fn apply_style_to_paragraph_range(
    p: &mut Paragraph,
    run_start: usize,
    byte_start: usize,
    run_end: usize,
    byte_end: usize,
    patch: &RunStylePatch,
) -> Result<()> {
    if run_start >= p.runs.len() || run_end >= p.runs.len() {
        return Err(ViewerError::EditOutOfBounds);
    }
    // Split end first so indices for the start split stay valid when in the same run.
    let mut end_run = run_end;
    let mut end_byte = byte_end;
    if end_byte > 0 && end_byte < p.runs[end_run].text.len() {
        split_run_at(p, end_run, end_byte)?;
        // Styled portion stays in `end_run`; the right half is end_run+1.
    } else if end_byte == 0 && end_run > run_start {
        // Selection ends at the start of this run — exclude it.
        end_run = end_run.saturating_sub(1);
        end_byte = p.runs[end_run].text.len();
    }

    let mut start_run = run_start;
    if byte_start > 0 && byte_start < p.runs[start_run].text.len() {
        start_run = split_run_at(p, start_run, byte_start)?;
        if end_run >= run_start {
            end_run += 1;
        }
    } else if byte_start >= p.runs[start_run].text.len() && byte_start > 0 {
        start_run = start_run.saturating_add(1);
    }

    if start_run >= p.runs.len() {
        return Ok(());
    }
    let last = end_run.min(p.runs.len().saturating_sub(1));
    for idx in start_run..=last {
        if let Some(run) = p.runs.get_mut(idx) {
            patch.apply_to_run(run);
        }
    }
    let _ = end_byte;
    Ok(())
}

/// Split `p.runs[run_idx]` at `byte_offset`; returns index of the right-hand run.
fn split_run_at(p: &mut Paragraph, run_idx: usize, byte_offset: usize) -> Result<usize> {
    let run = p
        .runs
        .get_mut(run_idx)
        .ok_or(ViewerError::EditOutOfBounds)?;
    if byte_offset == 0 {
        return Ok(run_idx);
    }
    if byte_offset >= run.text.len() {
        return Ok(run_idx + 1);
    }
    if !run.text.is_char_boundary(byte_offset) {
        return Err(ViewerError::EditOutOfBounds);
    }
    let right_text = run.text[byte_offset..].to_string();
    run.text.truncate(byte_offset);
    let style = run.style.clone();
    let hyperlink = run.hyperlink.clone();
    p.runs.insert(
        run_idx + 1,
        Run {
            text: right_text,
            style,
            hyperlink,
        },
    );
    Ok(run_idx + 1)
}

fn extract_text(doc: &Document, start: Cursor, end: Cursor) -> Result<String> {
    if !start.same_paragraph(end) {
        return Err(ViewerError::EditOutOfBounds);
    }
    let p = paragraph_ref(doc, start).ok_or(ViewerError::EditOutOfBounds)?;
    if start.run_idx == end.run_idx {
        let run = p
            .runs
            .get(start.run_idx)
            .ok_or(ViewerError::EditOutOfBounds)?;
        return Ok(run.text[start.byte_offset..end.byte_offset].to_string());
    }
    let mut out = String::new();
    for idx in start.run_idx..=end.run_idx {
        let run = p.runs.get(idx).ok_or(ViewerError::EditOutOfBounds)?;
        if idx == start.run_idx {
            out.push_str(&run.text[start.byte_offset..]);
        } else if idx == end.run_idx {
            out.push_str(&run.text[..end.byte_offset]);
        } else {
            out.push_str(&run.text);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::model::ImageFormat;

    fn doc_with_hello() -> Document {
        Document {
            blocks: vec![Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Hello".into(),
                    style: RunStyle::default(),
                    ..Default::default()
                }],
                ..Default::default()
            })],
            ..Default::default()
        }
    }

    fn plain(doc: &Document) -> String {
        match &doc.blocks[0] {
            Block::Paragraph(p) => p.plain_text(),
            _ => String::new(),
        }
    }

    #[test]
    fn insert_then_undo() {
        let mut doc = doc_with_hello();
        let mut stack = UndoStack::new();
        stack
            .push(
                &mut doc,
                EditCommand::InsertText {
                    at: Cursor::at(0, 0, 5),
                    text: "!".into(),
                },
            )
            .unwrap();
        assert_eq!(plain(&doc), "Hello!");
        stack.undo(&mut doc).unwrap();
        assert_eq!(plain(&doc), "Hello");
        stack.redo(&mut doc).unwrap();
        assert_eq!(plain(&doc), "Hello!");
    }

    #[test]
    fn set_bold_then_undo() {
        let mut doc = doc_with_hello();
        let mut stack = UndoStack::new();
        stack
            .push(
                &mut doc,
                EditCommand::SetRunStyle {
                    range: Selection {
                        anchor: Cursor::default(),
                        head: Cursor::at(0, 0, 5),
                    },
                    style: RunStylePatch {
                        bold: Some(true),
                        ..Default::default()
                    },
                },
            )
            .unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => assert!(p.runs[0].style.bold),
            _ => panic!("expected paragraph"),
        }
        stack.undo(&mut doc).unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => assert!(!p.runs[0].style.bold),
            _ => panic!("expected paragraph"),
        }
    }

    #[test]
    fn set_highlight_then_undo() {
        let mut doc = doc_with_hello();
        let mut stack = UndoStack::new();
        stack
            .push(
                &mut doc,
                EditCommand::SetRunStyle {
                    range: Selection {
                        anchor: Cursor::default(),
                        head: Cursor::at(0, 0, 5),
                    },
                    style: RunStylePatch {
                        highlight: Some(true),
                        ..Default::default()
                    },
                },
            )
            .unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => assert!(p.runs[0].style.highlight),
            _ => panic!("expected paragraph"),
        }
        stack.undo(&mut doc).unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => assert!(!p.runs[0].style.highlight),
            _ => panic!("expected paragraph"),
        }
    }

    #[test]
    fn set_page_setup_then_undo() {
        let mut doc = doc_with_hello();
        let original = doc.page_setup.clone();
        let mut next = original.clone();
        next.margin_left_twips = 720;
        next.margin_right_twips = 720;
        let mut stack = UndoStack::new();
        stack
            .push(&mut doc, EditCommand::SetPageSetup { setup: next.clone() })
            .unwrap();
        assert_eq!(doc.page_setup.margin_left_twips, 720);
        stack.undo(&mut doc).unwrap();
        assert_eq!(doc.page_setup, original);
        stack.redo(&mut doc).unwrap();
        assert_eq!(doc.page_setup, next);
    }

    #[test]
    fn cycle_page_size_letter_a4_round_trip() {
        let mut doc = doc_with_hello();
        assert_eq!(doc.page_setup.width_twips, 12240);
        assert_eq!(doc.page_setup.height_twips, 15840);
        let mut stack = UndoStack::new();
        let mut a4 = doc.page_setup.clone();
        a4.width_twips = 11906;
        a4.height_twips = 16838;
        stack
            .push(&mut doc, EditCommand::SetPageSetup { setup: a4.clone() })
            .unwrap();
        assert_eq!(doc.page_setup, a4);
        stack.undo(&mut doc).unwrap();
        assert_eq!(doc.page_setup.width_twips, 12240);
    }

    #[test]
    fn set_strikethrough_then_undo() {
        let mut doc = doc_with_hello();
        let mut stack = UndoStack::new();
        stack
            .push(
                &mut doc,
                EditCommand::SetRunStyle {
                    range: Selection {
                        anchor: Cursor::default(),
                        head: Cursor::at(0, 0, 5),
                    },
                    style: RunStylePatch {
                        strikethrough: Some(true),
                        ..Default::default()
                    },
                },
            )
            .unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => assert!(p.runs[0].style.strikethrough),
            _ => panic!("expected paragraph"),
        }
        stack.undo(&mut doc).unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => assert!(!p.runs[0].style.strikethrough),
            _ => panic!("expected paragraph"),
        }
    }

    #[test]
    fn clear_character_formatting_then_undo() {
        let mut doc = doc_with_hello();
        let mut stack = UndoStack::new();
        stack
            .push(
                &mut doc,
                EditCommand::SetRunStyle {
                    range: Selection {
                        anchor: Cursor::default(),
                        head: Cursor::at(0, 0, 5),
                    },
                    style: RunStylePatch {
                        bold: Some(true),
                        italic: Some(true),
                        highlight: Some(true),
                        color: Some(Some([255, 0, 0])),
                        font_size_pt: Some(Some(18.0)),
                        ..Default::default()
                    },
                },
            )
            .unwrap();
        stack
            .push(
                &mut doc,
                EditCommand::SetRunStyle {
                    range: Selection {
                        anchor: Cursor::default(),
                        head: Cursor::at(0, 0, 5),
                    },
                    style: RunStylePatch::clear_character(),
                },
            )
            .unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(p.runs[0].style, RunStyle::default());
            }
            _ => panic!("expected paragraph"),
        }
        stack.undo(&mut doc).unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert!(p.runs[0].style.bold);
                assert!(p.runs[0].style.italic);
                assert!(p.runs[0].style.highlight);
                assert_eq!(p.runs[0].style.color, Some([255, 0, 0]));
                assert_eq!(p.runs[0].style.font_size_pt, Some(18.0));
            }
            _ => panic!("expected paragraph"),
        }
    }

    #[test]
    fn set_superscript_then_undo_and_clears_subscript() {
        let mut doc = doc_with_hello();
        let mut stack = UndoStack::new();
        stack
            .push(
                &mut doc,
                EditCommand::SetRunStyle {
                    range: Selection {
                        anchor: Cursor::default(),
                        head: Cursor::at(0, 0, 5),
                    },
                    style: RunStylePatch {
                        subscript: Some(true),
                        ..Default::default()
                    },
                },
            )
            .unwrap();
        stack
            .push(
                &mut doc,
                EditCommand::SetRunStyle {
                    range: Selection {
                        anchor: Cursor::default(),
                        head: Cursor::at(0, 0, 5),
                    },
                    style: RunStylePatch {
                        superscript: Some(true),
                        ..Default::default()
                    },
                },
            )
            .unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert!(p.runs[0].style.superscript);
                assert!(!p.runs[0].style.subscript);
            }
            _ => panic!("expected paragraph"),
        }
        stack.undo(&mut doc).unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert!(!p.runs[0].style.superscript);
                assert!(p.runs[0].style.subscript);
            }
            _ => panic!("expected paragraph"),
        }
    }

    #[test]
    fn partial_run_bold_splits() {
        let mut doc = doc_with_hello();
        let mut stack = UndoStack::new();
        stack
            .push(
                &mut doc,
                EditCommand::SetRunStyle {
                    range: Selection {
                        anchor: Cursor::at(0, 0, 1),
                        head: Cursor::at(0, 0, 4),
                    },
                    style: RunStylePatch {
                        bold: Some(true),
                        ..Default::default()
                    },
                },
            )
            .unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert!(p.runs.len() >= 2);
                assert!(!p.runs[0].style.bold);
                assert_eq!(p.runs[0].text, "H");
                assert!(p.runs[1].style.bold);
                assert_eq!(p.runs[1].text, "ell");
                if p.runs.len() > 2 {
                    assert!(!p.runs[2].style.bold);
                    assert_eq!(p.runs[2].text, "o");
                }
            }
            _ => panic!("paragraph"),
        }
        stack.undo(&mut doc).unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(p.plain_text(), "Hello");
                assert!(p.runs.iter().all(|r| !r.style.bold));
            }
            _ => panic!("paragraph"),
        }
    }

    #[test]
    fn insert_text_in_table_cell_undo() {
        use crate::document::cursor::CellPath;
        use crate::document::model::Table;

        let mut doc = Document {
            blocks: vec![Block::Table(Table {
                rows: vec![TableRow {
                    cells: vec![
                        TableCell::from_paragraphs(vec![Paragraph {
                            runs: vec![Run {
                                text: "A".into(),
                                style: RunStyle::default(),
                                ..Default::default()
                            }],
                            ..Default::default()
                        }]),
                        TableCell::from_paragraphs(vec![Paragraph {
                            runs: vec![Run {
                                text: "B".into(),
                                style: RunStyle::default(),
                                ..Default::default()
                            }],
                            ..Default::default()
                        }]),
                    ],
                }],
                ..Default::default()
            })],
            ..Default::default()
        };
        let mut stack = UndoStack::new();
        let at = Cursor {
            block_idx: 0,
            cell: Some(CellPath::new(0, 0, 0)),
            run_idx: 0,
            byte_offset: 1,
        };
        stack
            .push(
                &mut doc,
                EditCommand::InsertText {
                    at,
                    text: "!".into(),
                },
            )
            .unwrap();
        match &doc.blocks[0] {
            Block::Table(t) => {
                assert_eq!(t.rows[0].cells[0].paragraphs[0].plain_text(), "A!");
                assert_eq!(t.rows[0].cells[1].paragraphs[0].plain_text(), "B");
            }
            _ => panic!("table"),
        }
        stack.undo(&mut doc).unwrap();
        match &doc.blocks[0] {
            Block::Table(t) => {
                assert_eq!(t.rows[0].cells[0].paragraphs[0].plain_text(), "A");
            }
            _ => panic!("table"),
        }
    }

    #[test]
    fn delete_across_cells_is_rejected() {
        use crate::document::cursor::CellPath;
        use crate::document::model::Table;

        let mut doc = Document {
            blocks: vec![Block::Table(Table {
                rows: vec![TableRow {
                    cells: vec![
                        TableCell::from_paragraphs(vec![Paragraph {
                            runs: vec![Run {
                                text: "A".into(),
                                style: RunStyle::default(),
                                ..Default::default()
                            }],
                            ..Default::default()
                        }]),
                        TableCell::from_paragraphs(vec![Paragraph {
                            runs: vec![Run {
                                text: "B".into(),
                                style: RunStyle::default(),
                                ..Default::default()
                            }],
                            ..Default::default()
                        }]),
                    ],
                }],
                ..Default::default()
            })],
            ..Default::default()
        };
        let err = apply_command(
            &mut doc,
            &EditCommand::DeleteRange {
                range: Selection {
                    anchor: Cursor {
                        block_idx: 0,
                        cell: Some(CellPath::new(0, 0, 0)),
                        run_idx: 0,
                        byte_offset: 0,
                    },
                    head: Cursor {
                        block_idx: 0,
                        cell: Some(CellPath::new(0, 1, 0)),
                        run_idx: 0,
                        byte_offset: 1,
                    },
                },
            },
        );
        assert!(err.is_err());
    }

    #[test]
    fn insert_table_undo() {
        let mut doc = doc_with_hello();
        let mut stack = UndoStack::new();
        stack
            .push(
                &mut doc,
                EditCommand::InsertTable {
                    at_block: 1,
                    table: crate::document::model::Table::empty(2, 2),
                },
            )
            .unwrap();
        assert_eq!(doc.blocks.len(), 2);
        match &doc.blocks[1] {
            Block::Table(t) => {
                assert_eq!(t.rows.len(), 2);
                assert_eq!(t.rows[0].cells.len(), 2);
            }
            _ => panic!("table"),
        }
        stack.undo(&mut doc).unwrap();
        assert_eq!(doc.blocks.len(), 1);
    }

    #[test]
    fn insert_table_row_undo() {
        let mut doc = Document {
            blocks: vec![Block::Table(crate::document::model::Table {
                rows: vec![
                    TableRow {
                        cells: vec![
                            TableCell::from_paragraphs(vec![Paragraph::default()]),
                            TableCell::from_paragraphs(vec![Paragraph::default()]),
                        ],
                    },
                    TableRow {
                        cells: vec![
                            TableCell::from_paragraphs(vec![Paragraph::default()]),
                            TableCell::from_paragraphs(vec![Paragraph::default()]),
                        ],
                    },
                ],
                ..Default::default()
            })],
            ..Default::default()
        };
        let mut stack = UndoStack::new();
        stack
            .push(
                &mut doc,
                EditCommand::InsertTableRow {
                    table_idx: 0,
                    at_row: 1,
                },
            )
            .unwrap();
        match &doc.blocks[0] {
            Block::Table(t) => assert_eq!(t.rows.len(), 3),
            _ => panic!("table"),
        }
        stack.undo(&mut doc).unwrap();
        match &doc.blocks[0] {
            Block::Table(t) => assert_eq!(t.rows.len(), 2),
            _ => panic!("table"),
        }
    }

    #[test]
    fn insert_table_column_keeps_width_list() {
        let mut doc = Document {
            blocks: vec![Block::Table(crate::document::model::Table {
                rows: vec![
                    TableRow {
                        cells: vec![
                            TableCell::from_paragraphs(vec![Paragraph::default()]),
                            TableCell::from_paragraphs(vec![Paragraph::default()]),
                        ],
                    },
                    TableRow {
                        cells: vec![
                            TableCell::from_paragraphs(vec![Paragraph::default()]),
                            TableCell::from_paragraphs(vec![Paragraph::default()]),
                        ],
                    },
                ],
                column_widths_twips: vec![2000, 6000],
                ..Default::default()
            })],
            ..Default::default()
        };
        let mut stack = UndoStack::new();
        stack
            .push(
                &mut doc,
                EditCommand::InsertTableColumn {
                    table_idx: 0,
                    at_col: 1,
                },
            )
            .unwrap();
        match &doc.blocks[0] {
            Block::Table(t) => {
                assert_eq!(t.rows[0].cells.len(), 3);
                assert_eq!(t.column_widths_twips.len(), 3);
                assert_eq!(t.column_widths_twips[0], 2000);
                assert_eq!(t.column_widths_twips[2], 6000);
            }
            _ => panic!("table"),
        }
        stack.undo(&mut doc).unwrap();
        match &doc.blocks[0] {
            Block::Table(t) => assert_eq!(t.column_widths_twips, vec![2000, 6000]),
            _ => panic!("table"),
        }
    }

    #[test]
    fn insert_table_column_undo() {
        let mut doc = Document {
            blocks: vec![Block::Table(crate::document::model::Table {
                rows: vec![
                    TableRow {
                        cells: vec![
                            TableCell::from_paragraphs(vec![Paragraph::default()]),
                            TableCell::from_paragraphs(vec![Paragraph::default()]),
                        ],
                    },
                    TableRow {
                        cells: vec![
                            TableCell::from_paragraphs(vec![Paragraph::default()]),
                            TableCell::from_paragraphs(vec![Paragraph::default()]),
                        ],
                    },
                ],
                ..Default::default()
            })],
            ..Default::default()
        };
        let mut stack = UndoStack::new();
        stack
            .push(
                &mut doc,
                EditCommand::InsertTableColumn {
                    table_idx: 0,
                    at_col: 1,
                },
            )
            .unwrap();
        match &doc.blocks[0] {
            Block::Table(t) => {
                assert_eq!(t.rows[0].cells.len(), 3);
                assert_eq!(t.rows[1].cells.len(), 3);
            }
            _ => panic!("table"),
        }
        stack.undo(&mut doc).unwrap();
        match &doc.blocks[0] {
            Block::Table(t) => {
                assert_eq!(t.rows[0].cells.len(), 2);
                assert_eq!(t.rows[1].cells.len(), 2);
            }
            _ => panic!("table"),
        }
    }

    #[test]
    fn insert_image_undo() {
        let mut doc = doc_with_hello();
        let mut stack = UndoStack::new();
        stack
            .push(
                &mut doc,
                EditCommand::InsertImage {
                    at: Cursor::at(1, 0, 0),
                    image: InlineImage {
                        bytes: vec![1, 2, 3],
                        format: ImageFormat::Png,
                        width_px: 1,
                        height_px: 1,
                        r_id: None,
                        part_path: None,
                    },
                },
            )
            .unwrap();
        assert_eq!(doc.blocks.len(), 2);
        stack.undo(&mut doc).unwrap();
        assert_eq!(doc.blocks.len(), 1);
    }

    #[test]
    fn five_commands_round_trip() {
        let mut doc = doc_with_hello();
        let original = doc.clone();
        let mut stack = UndoStack::new();
        stack
            .push(
                &mut doc,
                EditCommand::InsertText {
                    at: Cursor::at(0, 0, 5),
                    text: " world".into(),
                },
            )
            .unwrap();
        stack
            .push(
                &mut doc,
                EditCommand::SetRunStyle {
                    range: Selection {
                        anchor: Cursor::default(),
                        head: Cursor::at(0, 0, 5),
                    },
                    style: RunStylePatch {
                        bold: Some(true),
                        ..Default::default()
                    },
                },
            )
            .unwrap();
        stack
            .push(
                &mut doc,
                EditCommand::SetAlignment {
                    paragraph_idx: 0,
                    alignment: Alignment::Center,
                },
            )
            .unwrap();
        stack
            .push(
                &mut doc,
                EditCommand::ToggleList {
                    paragraph_idx: 0,
                    kind: ListKind::Bullet,
                },
            )
            .unwrap();
        stack
            .push(
                &mut doc,
                EditCommand::InsertText {
                    at: Cursor::at(0, 0, 0),
                    text: "X".into(),
                },
            )
            .unwrap();
        for _ in 0..5 {
            stack.undo(&mut doc).unwrap();
        }
        assert_eq!(doc, original);
    }

    #[test]
    fn remove_cell_image_undo_restores() {
        use crate::document::cursor::CellPath;
        use crate::document::model::{CellImage, ImageFormat, InlineImage, Table};

        let image = CellImage {
            after_paragraph: 0,
            image: InlineImage {
                bytes: vec![1, 2, 3],
                format: ImageFormat::Png,
                width_px: 4,
                height_px: 4,
                r_id: None,
                part_path: None,
            },
        };
        let mut doc = Document {
            blocks: vec![Block::Table(Table {
                rows: vec![TableRow {
                    cells: vec![TableCell {
                        paragraphs: vec![Paragraph {
                            runs: vec![Run {
                                text: "Hi".into(),
                                style: RunStyle::default(),
                                ..Default::default()
                            }],
                            ..Default::default()
                        }],
                        images: vec![image.clone()],
                        ..Default::default()
                    }],
                }],
                ..Default::default()
            })],
            ..Default::default()
        };
        let mut stack = UndoStack::new();
        stack
            .push(
                &mut doc,
                EditCommand::RemoveCellImage {
                    table_idx: 0,
                    row: 0,
                    col: 0,
                    image_idx: 0,
                },
            )
            .unwrap();
        match &doc.blocks[0] {
            Block::Table(t) => assert!(t.rows[0].cells[0].images.is_empty()),
            _ => panic!("table"),
        }
        stack.undo(&mut doc).unwrap();
        match &doc.blocks[0] {
            Block::Table(t) => assert_eq!(t.rows[0].cells[0].images, vec![image]),
            _ => panic!("table"),
        }
        let cursor = Cursor {
            block_idx: 0,
            cell: Some(CellPath::new(0, 0, 0)),
            run_idx: 0,
            byte_offset: 0,
        };
        assert!(paragraph_ref(&doc, cursor).is_some());
    }
}
