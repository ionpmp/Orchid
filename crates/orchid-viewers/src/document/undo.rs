//! Edit commands and undo/redo for the document model.

use crate::document::cursor::{Cursor, Selection};
use crate::document::model::{
    Alignment, Block, Document, InlineImage, ListKind, Paragraph, Run, RunStyle, TableCell,
    TableRow,
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
    /// Insert an image as a new block at `at.block_idx`.
    InsertImage {
        /// Insertion cursor (block index used).
        at: Cursor,
        /// Image payload.
        image: InlineImage,
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
    /// Set colour (`Some(None)` clears).
    pub color: Option<Option<[u8; 3]>>,
    /// Set font family.
    pub font_family: Option<Option<String>>,
    /// Set font size.
    pub font_size_pt: Option<Option<f32>>,
}

impl RunStylePatch {
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
            let (start, _) = range.normalized();
            let Block::Paragraph(p) = doc
                .blocks
                .get(start.block_idx)
                .ok_or(ViewerError::EditOutOfBounds)?
            else {
                return Err(ViewerError::EditOutOfBounds);
            };
            let previous = p.clone();
            apply_set_run_style(doc, *range, style)?;
            Ok(EditCommand::ReplaceParagraph {
                paragraph_idx: start.block_idx,
                previous,
            })
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
            Ok(EditCommand::ReplaceParagraph {
                paragraph_idx: *paragraph_idx,
                previous,
            })
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
                    .map(|_| TableCell {
                        paragraphs: vec![Paragraph::default()],
                    })
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
            if *row_idx >= t.rows.len() {
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
        EditCommand::InsertImage { at, image } => {
            let idx = at.block_idx.min(doc.blocks.len());
            doc.blocks.insert(idx, Block::Image(image.clone()));
            Ok(EditCommand::RemoveBlock { block_idx: idx })
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
    let Block::Paragraph(p) = doc
        .blocks
        .get_mut(at.block_idx)
        .ok_or(ViewerError::EditOutOfBounds)?
    else {
        return Err(ViewerError::EditOutOfBounds);
    };
    if p.runs.is_empty() {
        p.runs.push(Run {
            text: text.to_string(),
            style: RunStyle::default(),
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
    if start.block_idx != end.block_idx {
        return Err(ViewerError::EditOutOfBounds);
    }
    let Block::Paragraph(p) = doc
        .blocks
        .get_mut(start.block_idx)
        .ok_or(ViewerError::EditOutOfBounds)?
    else {
        return Err(ViewerError::EditOutOfBounds);
    };
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
    if start.block_idx != end.block_idx {
        return Err(ViewerError::EditOutOfBounds);
    }
    let Block::Paragraph(p) = doc
        .blocks
        .get_mut(start.block_idx)
        .ok_or(ViewerError::EditOutOfBounds)?
    else {
        return Err(ViewerError::EditOutOfBounds);
    };
    for idx in start.run_idx..=end.run_idx {
        if let Some(run) = p.runs.get_mut(idx) {
            patch.apply_to(&mut run.style);
        }
    }
    Ok(())
}

fn extract_text(doc: &Document, start: Cursor, end: Cursor) -> Result<String> {
    if start.block_idx != end.block_idx {
        return Err(ViewerError::EditOutOfBounds);
    }
    let Block::Paragraph(p) = doc
        .blocks
        .get(start.block_idx)
        .ok_or(ViewerError::EditOutOfBounds)?
    else {
        return Err(ViewerError::EditOutOfBounds);
    };
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
                    at: Cursor {
                        block_idx: 0,
                        run_idx: 0,
                        byte_offset: 5,
                    },
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
                        head: Cursor {
                            block_idx: 0,
                            run_idx: 0,
                            byte_offset: 5,
                        },
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
    fn insert_table_row_undo() {
        let mut doc = Document {
            blocks: vec![Block::Table(crate::document::model::Table {
                rows: vec![
                    TableRow {
                        cells: vec![
                            TableCell {
                                paragraphs: vec![Paragraph::default()],
                            },
                            TableCell {
                                paragraphs: vec![Paragraph::default()],
                            },
                        ],
                    },
                    TableRow {
                        cells: vec![
                            TableCell {
                                paragraphs: vec![Paragraph::default()],
                            },
                            TableCell {
                                paragraphs: vec![Paragraph::default()],
                            },
                        ],
                    },
                ],
                unsupported: vec![],
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
    fn insert_image_undo() {
        let mut doc = doc_with_hello();
        let mut stack = UndoStack::new();
        stack
            .push(
                &mut doc,
                EditCommand::InsertImage {
                    at: Cursor {
                        block_idx: 1,
                        run_idx: 0,
                        byte_offset: 0,
                    },
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
                    at: Cursor {
                        block_idx: 0,
                        run_idx: 0,
                        byte_offset: 5,
                    },
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
                        head: Cursor {
                            block_idx: 0,
                            run_idx: 0,
                            byte_offset: 5,
                        },
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
                    at: Cursor {
                        block_idx: 0,
                        run_idx: 0,
                        byte_offset: 0,
                    },
                    text: "X".into(),
                },
            )
            .unwrap();
        for _ in 0..5 {
            stack.undo(&mut doc).unwrap();
        }
        assert_eq!(doc, original);
    }
}
