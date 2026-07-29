//! Cursor and selection over a [`Document`](super::model::Document).

use crate::document::model::{Block, Document, Paragraph};

/// Path to a paragraph or cell image inside a [`Block::Table`](super::model::Block::Table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellPath {
    /// Row index (top-to-bottom).
    pub row: usize,
    /// Column index (left-to-right).
    pub col: usize,
    /// Index into [`TableCell::paragraphs`](super::model::TableCell::paragraphs), or
    /// [`CellImage::after_paragraph`](super::model::CellImage::after_paragraph) when
    /// [`Self::image_idx`] is set.
    pub para_idx: usize,
    /// Index into [`TableCell::images`](super::model::TableCell::images); `None` = paragraph.
    pub image_idx: Option<usize>,
}

impl CellPath {
    /// Paragraph cursor inside a table cell.
    #[must_use]
    pub const fn new(row: usize, col: usize, para_idx: usize) -> Self {
        Self {
            row,
            col,
            para_idx,
            image_idx: None,
        }
    }
}

/// Position inside the document body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cursor {
    /// Index into [`Document::blocks`](super::model::Document::blocks).
    pub block_idx: usize,
    /// When `Some`, this cursor addresses a paragraph inside a table at `block_idx`.
    /// When `None`, `block_idx` must be a top-level [`Block::Paragraph`].
    pub cell: Option<CellPath>,
    /// Index into the paragraph's `runs`.
    pub run_idx: usize,
    /// UTF-8 byte offset inside the run's text.
    pub byte_offset: usize,
}

impl Cursor {
    /// Cursor in a top-level body paragraph.
    #[must_use]
    pub const fn at(block_idx: usize, run_idx: usize, byte_offset: usize) -> Self {
        Self {
            block_idx,
            cell: None,
            run_idx,
            byte_offset,
        }
    }

    /// Cursor on a table-cell image (`para_idx` = `after_paragraph`).
    #[must_use]
    pub const fn on_cell_image(
        block_idx: usize,
        row: usize,
        col: usize,
        after_paragraph: usize,
        image_idx: usize,
    ) -> Self {
        Self {
            block_idx,
            cell: Some(CellPath {
                row,
                col,
                para_idx: after_paragraph,
                image_idx: Some(image_idx),
            }),
            run_idx: 0,
            byte_offset: 0,
        }
    }

    /// Whether `self` and `other` address the same paragraph or cell image.
    #[must_use]
    pub fn same_paragraph(self, other: Self) -> bool {
        self.block_idx == other.block_idx && self.cell == other.cell
    }

    /// Whether both cursors are inside the same table cell (any paragraphs).
    #[must_use]
    pub fn same_cell(self, other: Self) -> bool {
        match (self.cell, other.cell) {
            (Some(a), Some(b)) => {
                self.block_idx == other.block_idx && a.row == b.row && a.col == b.col
            }
            _ => false,
        }
    }
}

/// A half-open selection from `anchor` to `head`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    /// Selection start (may be after `head` if the user dragged backwards).
    pub anchor: Cursor,
    /// Selection end / caret.
    pub head: Cursor,
}

impl Selection {
    /// Return `(start, end)` with start ≤ end in document order.
    #[must_use]
    pub fn normalized(&self) -> (Cursor, Cursor) {
        if cmp_cursor(self.anchor, self.head) <= 0 {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// Whether anchor and head are the same position.
    #[must_use]
    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.head
    }
}

/// Borrow the paragraph addressed by `cursor`.
#[must_use]
pub fn paragraph_ref(doc: &Document, cursor: Cursor) -> Option<&Paragraph> {
    match cursor.cell {
        None => match doc.blocks.get(cursor.block_idx)? {
            Block::Paragraph(p) => Some(p),
            _ => None,
        },
        Some(path) => {
            let Block::Table(t) = doc.blocks.get(cursor.block_idx)? else {
                return None;
            };
            t.rows
                .get(path.row)?
                .cells
                .get(path.col)?
                .paragraphs
                .get(path.para_idx)
        }
    }
}

/// Mutably borrow the paragraph addressed by `cursor`.
pub fn paragraph_mut(doc: &mut Document, cursor: Cursor) -> Option<&mut Paragraph> {
    paragraph_mut_in_blocks(&mut doc.blocks, cursor)
}

/// Mutably borrow a paragraph inside a blocks slice (body or table cell).
pub fn paragraph_mut_in_blocks(blocks: &mut [Block], cursor: Cursor) -> Option<&mut Paragraph> {
    match cursor.cell {
        None => match blocks.get_mut(cursor.block_idx)? {
            Block::Paragraph(p) => Some(p),
            _ => None,
        },
        Some(path) => {
            let Block::Table(t) = blocks.get_mut(cursor.block_idx)? else {
                return None;
            };
            t.rows
                .get_mut(path.row)?
                .cells
                .get_mut(path.col)?
                .paragraphs
                .get_mut(path.para_idx)
        }
    }
}

fn para_len(p: &Paragraph) -> usize {
    p.runs.iter().map(|r| r.text.len()).sum()
}

fn cursor_in_para(
    block_idx: usize,
    cell: Option<CellPath>,
    p: &Paragraph,
    mut remaining: usize,
) -> Cursor {
    if p.runs.is_empty() {
        return Cursor {
            block_idx,
            cell,
            run_idx: 0,
            byte_offset: 0,
        };
    }
    for (ri, run) in p.runs.iter().enumerate() {
        if remaining <= run.text.len() {
            return Cursor {
                block_idx,
                cell,
                run_idx: ri,
                byte_offset: remaining,
            };
        }
        remaining -= run.text.len();
    }
    let last = p.runs.len() - 1;
    Cursor {
        block_idx,
        cell,
        run_idx: last,
        byte_offset: p.runs[last].text.len(),
    }
}

fn end_of_para(block_idx: usize, cell: Option<CellPath>, p: &Paragraph) -> Cursor {
    if p.runs.is_empty() {
        return Cursor {
            block_idx,
            cell,
            run_idx: 0,
            byte_offset: 0,
        };
    }
    let last = p.runs.len() - 1;
    Cursor {
        block_idx,
        cell,
        run_idx: last,
        byte_offset: p.runs[last].text.len(),
    }
}

/// Map a UTF-8 byte offset in [`Document::plain_text`] into a [`Cursor`].
///
/// Paragraphs (body and table-cell) are joined with a single `\n` (matching `plain_text()`).
#[must_use]
pub fn cursor_from_plain_offset(doc: &Document, mut offset: usize) -> Cursor {
    if doc.blocks.is_empty() {
        return Cursor::default();
    }
    let mut emitted = false;
    let mut last_end = Cursor::default();
    for (bi, block) in doc.blocks.iter().enumerate() {
        match block {
            Block::Paragraph(p) => {
                if emitted {
                    if offset == 0 {
                        return last_end;
                    }
                    offset -= 1;
                }
                emitted = true;
                let len = para_len(p);
                if offset <= len {
                    return cursor_in_para(bi, None, p, offset);
                }
                offset -= len;
                last_end = end_of_para(bi, None, p);
            }
            Block::Table(t) => {
                for (ri, row) in t.rows.iter().enumerate() {
                    for (ci, cell) in row.cells.iter().enumerate() {
                        for (pi, p) in cell.paragraphs.iter().enumerate() {
                            let path = Some(CellPath::new(ri, ci, pi));
                            if emitted {
                                if offset == 0 {
                                    return last_end;
                                }
                                offset -= 1;
                            }
                            emitted = true;
                            let len = para_len(p);
                            if offset <= len {
                                return cursor_in_para(bi, path, p, offset);
                            }
                            offset -= len;
                            last_end = end_of_para(bi, path, p);
                        }
                    }
                }
            }
            Block::Image(_) => {}
        }
    }
    if emitted {
        return last_end;
    }
    Cursor::default()
}

/// Build a selection from plain-text UTF-8 byte offsets.
#[must_use]
pub fn selection_from_plain_offsets(doc: &Document, anchor: usize, head: usize) -> Selection {
    Selection {
        anchor: cursor_from_plain_offset(doc, anchor),
        head: cursor_from_plain_offset(doc, head),
    }
}

/// Map a [`Cursor`] back to a UTF-8 byte offset in [`Document::plain_text`].
#[must_use]
pub fn plain_offset_from_cursor(doc: &Document, cursor: Cursor) -> usize {
    let mut offset = 0usize;
    let mut emitted = false;
    for (bi, block) in doc.blocks.iter().enumerate() {
        match block {
            Block::Paragraph(p) => {
                if emitted {
                    offset += 1;
                }
                emitted = true;
                if bi == cursor.block_idx && cursor.cell.is_none() {
                    return offset + offset_in_para(p, cursor);
                }
                offset += para_len(p);
            }
            Block::Table(t) => {
                for (ri, row) in t.rows.iter().enumerate() {
                    for (ci, cell) in row.cells.iter().enumerate() {
                        let cell_start = offset;
                        for (pi, p) in cell.paragraphs.iter().enumerate() {
                            if emitted {
                                offset += 1;
                            }
                            emitted = true;
                            if bi == cursor.block_idx {
                                if let Some(cpath) = cursor.cell {
                                    if cpath.row == ri
                                        && cpath.col == ci
                                        && cpath.para_idx == pi
                                    {
                                        if cpath.image_idx.is_some() {
                                            return offset + para_len(p);
                                        }
                                        return offset + offset_in_para(p, cursor);
                                    }
                                }
                            }
                            offset += para_len(p);
                        }
                        if bi == cursor.block_idx {
                            if let Some(cpath) = cursor.cell {
                                if cpath.row == ri
                                    && cpath.col == ci
                                    && cpath.image_idx.is_some()
                                    && cell.paragraphs.is_empty()
                                {
                                    return cell_start;
                                }
                            }
                        }
                    }
                }
            }
            Block::Image(_) => {}
        }
    }
    offset
}

fn offset_in_para(p: &Paragraph, cursor: Cursor) -> usize {
    let mut run_off = 0usize;
    for (ri, run) in p.runs.iter().enumerate() {
        if ri == cursor.run_idx {
            return run_off + cursor.byte_offset.min(run.text.len());
        }
        run_off += run.text.len();
    }
    run_off
}

/// Cursors addressing paragraphs touched by `selection` (body or same-cell).
///
/// Cross-cell and body↔table selections return an empty list.
#[must_use]
pub fn paragraph_cursors_in_selection(doc: &Document, selection: Selection) -> Vec<Cursor> {
    let (start, end) = selection.normalized();
    if start.same_cell(end) {
        let Some(path) = start.cell else {
            return Vec::new();
        };
        let Some(Block::Table(t)) = doc.blocks.get(start.block_idx) else {
            return Vec::new();
        };
        let Some(cell) = t.rows.get(path.row).and_then(|r| r.cells.get(path.col)) else {
            return Vec::new();
        };
        let lo = path.para_idx.min(end.cell.map(|c| c.para_idx).unwrap_or(path.para_idx));
        let hi = path.para_idx.max(end.cell.map(|c| c.para_idx).unwrap_or(path.para_idx));
        let hi = hi.min(cell.paragraphs.len().saturating_sub(1));
        return (lo..=hi)
            .map(|para_idx| Cursor {
                block_idx: start.block_idx,
                cell: Some(CellPath::new(path.row, path.col, para_idx)),
                run_idx: 0,
                byte_offset: 0,
            })
            .collect();
    }
    if start.cell.is_some() || end.cell.is_some() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for bi in start.block_idx..=end.block_idx.min(doc.blocks.len().saturating_sub(1)) {
        if matches!(doc.blocks.get(bi), Some(Block::Paragraph(_))) {
            out.push(Cursor::at(bi, 0, 0));
        }
    }
    out
}

/// Indices of **top-level** paragraph blocks touched by `selection` (inclusive).
///
/// Cell paragraphs are omitted; prefer [`paragraph_cursors_in_selection`] for edits.
#[must_use]
pub fn paragraph_indices_in_selection(doc: &Document, selection: Selection) -> Vec<usize> {
    paragraph_cursors_in_selection(doc, selection)
        .into_iter()
        .filter(|c| c.cell.is_none())
        .map(|c| c.block_idx)
        .collect()
}

/// Move to the next (`forward`) or previous table cell in row-major order.
///
/// Returns `None` when `cursor` is not in a table cell, or when there is no
/// further cell in that direction. The caret lands at the start of the target
/// cell's first paragraph.
#[must_use]
pub fn adjacent_cell_cursor(doc: &Document, cursor: Cursor, forward: bool) -> Option<Cursor> {
    let path = cursor.cell?;
    let Block::Table(t) = doc.blocks.get(cursor.block_idx)? else {
        return None;
    };
    let mut cells = Vec::new();
    for (ri, row) in t.rows.iter().enumerate() {
        for (ci, cell) in row.cells.iter().enumerate() {
            if !cell.paragraphs.is_empty() {
                cells.push((ri, ci));
            }
        }
    }
    let idx = cells
        .iter()
        .position(|&(r, c)| r == path.row && c == path.col)?;
    let next_idx = if forward {
        idx.checked_add(1).filter(|&i| i < cells.len())?
    } else {
        idx.checked_sub(1)?
    };
    let (row, col) = cells[next_idx];
    Some(Cursor {
        block_idx: cursor.block_idx,
        cell: Some(CellPath::new(row, col, 0)),
        run_idx: 0,
        byte_offset: 0,
    })
}

/// Whether `cursor` addresses a body [`Block::Image`] or a table-cell image.
#[must_use]
pub fn is_image_cursor(doc: &Document, cursor: Cursor) -> bool {
    if cursor
        .cell
        .is_some_and(|path| path.image_idx.is_some())
    {
        return true;
    }
    if cursor.cell.is_none() {
        return matches!(
            doc.blocks.get(cursor.block_idx),
            Some(Block::Image(_))
        );
    }
    false
}

/// Step among paragraphs and cell images in document order within one cell.
#[must_use]
pub fn adjacent_in_cell(doc: &Document, cursor: Cursor, forward: bool) -> Option<Cursor> {
    let path = cursor.cell?;
    let Block::Table(t) = doc.blocks.get(cursor.block_idx)? else {
        return None;
    };
    let cell = t.rows.get(path.row)?.cells.get(path.col)?;

    if let Some(image_idx) = path.image_idx {
        if forward {
            if image_idx + 1 < cell.images.len()
                && cell.images[image_idx + 1].after_paragraph == path.para_idx
            {
                return Some(Cursor::on_cell_image(
                    cursor.block_idx,
                    path.row,
                    path.col,
                    path.para_idx,
                    image_idx + 1,
                ));
            }
            let next_para = path.para_idx + 1;
            if next_para < cell.paragraphs.len() {
                return Some(Cursor {
                    block_idx: cursor.block_idx,
                    cell: Some(CellPath::new(path.row, path.col, next_para)),
                    run_idx: 0,
                    byte_offset: 0,
                });
            }
            return first_orphan_image_after(cursor, cell, path.para_idx);
        }
        if image_idx > 0 && cell.images[image_idx - 1].after_paragraph == path.para_idx {
            return Some(Cursor::on_cell_image(
                cursor.block_idx,
                path.row,
                path.col,
                path.para_idx,
                image_idx - 1,
            ));
        }
        return end_of_cell_paragraph(cursor, cell, path.para_idx);
    }

    let p = cell.paragraphs.get(path.para_idx)?;
    let at_end = cursor_at_end_of_para(p, cursor);
    let at_start = cursor.run_idx == 0 && cursor.byte_offset == 0;

    if forward && at_end {
        if let Some((idx, _)) = cell
            .images
            .iter()
            .enumerate()
            .find(|(_, img)| img.after_paragraph == path.para_idx)
        {
            return Some(Cursor::on_cell_image(
                cursor.block_idx,
                path.row,
                path.col,
                path.para_idx,
                idx,
            ));
        }
        if path.para_idx + 1 < cell.paragraphs.len() {
            return Some(Cursor {
                block_idx: cursor.block_idx,
                cell: Some(CellPath::new(path.row, path.col, path.para_idx + 1)),
                run_idx: 0,
                byte_offset: 0,
            });
        }
        return first_orphan_image_after(cursor, cell, path.para_idx);
    }

    if !forward && at_start {
        if let Some(idx) = last_image_after_paragraph(cell, path.para_idx.saturating_sub(1)) {
            return Some(Cursor::on_cell_image(
                cursor.block_idx,
                path.row,
                path.col,
                path.para_idx.saturating_sub(1),
                idx,
            ));
        }
        if path.para_idx > 0 {
            let prev = path.para_idx - 1;
            let prev_p = cell.paragraphs.get(prev)?;
            return Some(end_of_para(
                cursor.block_idx,
                Some(CellPath::new(path.row, path.col, prev)),
                prev_p,
            ));
        }
    }

    None
}

fn cursor_at_end_of_para(p: &Paragraph, cursor: Cursor) -> bool {
    if p.runs.is_empty() {
        return true;
    }
    let last = p.runs.len() - 1;
    cursor.run_idx == last && cursor.byte_offset >= p.runs[last].text.len()
}

fn last_image_after_paragraph(
    cell: &crate::document::model::TableCell,
    after: usize,
) -> Option<usize> {
    cell.images
        .iter()
        .enumerate()
        .rev()
        .find_map(|(idx, img)| (img.after_paragraph == after).then_some(idx))
}

fn first_orphan_image_after(
    cursor: Cursor,
    cell: &crate::document::model::TableCell,
    after: usize,
) -> Option<Cursor> {
    let path = cursor.cell?;
    let last = cell.paragraphs.len().saturating_sub(1);
    if after < last {
        return None;
    }
    let (idx, img) = cell
        .images
        .iter()
        .enumerate()
        .find(|(_, img)| img.after_paragraph > last)?;
    Some(Cursor::on_cell_image(
        cursor.block_idx,
        path.row,
        path.col,
        img.after_paragraph,
        idx,
    ))
}

fn end_of_cell_paragraph(
    cursor: Cursor,
    cell: &crate::document::model::TableCell,
    after_paragraph: usize,
) -> Option<Cursor> {
    let path = cursor.cell?;
    if after_paragraph < cell.paragraphs.len() {
        let p = cell.paragraphs.get(after_paragraph)?;
        return Some(end_of_para(
            cursor.block_idx,
            Some(CellPath::new(path.row, path.col, after_paragraph)),
            p,
        ));
    }
    if cell.paragraphs.is_empty() {
        return Some(Cursor {
            block_idx: cursor.block_idx,
            cell: Some(CellPath::new(path.row, path.col, 0)),
            run_idx: 0,
            byte_offset: 0,
        });
    }
    None
}

fn cmp_cell_path(a: CellPath, b: CellPath) -> i8 {
    use std::cmp::Ordering;
    match (a.row.cmp(&b.row), a.col.cmp(&b.col)) {
        (Ordering::Less, _) | (_, Ordering::Less) => return -1,
        (Ordering::Greater, _) | (_, Ordering::Greater) => return 1,
        _ => {}
    }
    let a_img = a.image_idx.is_some();
    let b_img = b.image_idx.is_some();
    match (a_img, b_img) {
        (false, false) => match a.para_idx.cmp(&b.para_idx) {
            Ordering::Less => -1,
            Ordering::Greater => 1,
            Ordering::Equal => 0,
        },
        (true, true) => match a.para_idx.cmp(&b.para_idx) {
            Ordering::Less => -1,
            Ordering::Greater => 1,
            Ordering::Equal => match a.image_idx.cmp(&b.image_idx) {
                Ordering::Less => -1,
                Ordering::Greater => 1,
                Ordering::Equal => 0,
            },
        },
        (false, true) => {
            if a.para_idx < b.para_idx {
                -1
            } else if a.para_idx > b.para_idx {
                1
            } else {
                -1
            }
        }
        (true, false) => {
            if a.para_idx < b.para_idx {
                -1
            } else if a.para_idx > b.para_idx {
                1
            } else {
                1
            }
        }
    }
}

fn cmp_cursor(a: Cursor, b: Cursor) -> i8 {
    use std::cmp::Ordering;
    match a.block_idx.cmp(&b.block_idx) {
        Ordering::Less => return -1,
        Ordering::Greater => return 1,
        Ordering::Equal => {}
    }
    match (a.cell, b.cell) {
        (None, None) => {}
        (None, Some(_)) => return -1,
        (Some(_), None) => return 1,
        (Some(ca), Some(cb)) => match cmp_cell_path(ca, cb) {
            -1 => return -1,
            1 => return 1,
            _ => {}
        },
    }
    if a.cell.is_some_and(|c| c.image_idx.is_some()) || b.cell.is_some_and(|c| c.image_idx.is_some())
    {
        return 0;
    }
    match (a.run_idx.cmp(&b.run_idx), a.byte_offset.cmp(&b.byte_offset)) {
        (Ordering::Less, _) | (_, Ordering::Less) => -1,
        (Ordering::Greater, _) | (_, Ordering::Greater) => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::model::{Paragraph, Run, RunStyle, Table, TableCell, TableRow};

    fn para(text: &str) -> Paragraph {
        Paragraph {
            runs: vec![Run {
                text: text.into(),
                style: RunStyle::default(),
            }],
            ..Default::default()
        }
    }

    fn cell(text: &str) -> TableCell {
        TableCell::from_paragraphs(vec![para(text)])
    }

    #[test]
    fn normalized_swaps_backwards_selection() {
        let sel = Selection {
            anchor: Cursor::at(2, 0, 5),
            head: Cursor::at(1, 0, 0),
        };
        let (start, end) = sel.normalized();
        assert_eq!(start.block_idx, 1);
        assert_eq!(end.block_idx, 2);
    }

    #[test]
    fn plain_offset_round_trips() {
        let doc = Document {
            blocks: vec![
                Block::Paragraph(para("Hi")),
                Block::Paragraph(para("there")),
            ],
            ..Default::default()
        };
        for off in [0usize, 1, 2, 3, 5, 8] {
            let c = cursor_from_plain_offset(&doc, off);
            let back = plain_offset_from_cursor(&doc, c);
            assert_eq!(back, off.min(doc.plain_text().len()), "off={off}");
        }
    }

    #[test]
    fn table_cells_round_trip_plain_offsets() {
        let doc = Document {
            blocks: vec![Block::Table(Table {
                rows: vec![
                    TableRow {
                        cells: vec![cell("A"), cell("B")],
                    },
                    TableRow {
                        cells: vec![cell("C"), cell("D")],
                    },
                ],
                ..Default::default()
            })],
            ..Default::default()
        };
        assert_eq!(doc.plain_text(), "A\nB\nC\nD");
        for off in 0..=doc.plain_text().len() {
            let c = cursor_from_plain_offset(&doc, off);
            assert!(c.cell.is_some(), "off={off} should be in a cell");
            assert_eq!(plain_offset_from_cursor(&doc, c), off, "off={off}");
        }
        let in_b = cursor_from_plain_offset(&doc, 2); // start of "B"
        assert_eq!(
            in_b.cell,
            Some(CellPath::new(0, 1, 0))
        );
    }

    #[test]
    fn body_after_table_maps_correctly() {
        let doc = Document {
            blocks: vec![
                Block::Paragraph(para("Hi")),
                Block::Table(Table {
                    rows: vec![TableRow {
                        cells: vec![cell("X")],
                    }],
                    ..Default::default()
                }),
                Block::Paragraph(para("Lo")),
            ],
            ..Default::default()
        };
        // "Hi\nX\nLo" → offset of 'L' is 5
        assert_eq!(doc.plain_text(), "Hi\nX\nLo");
        let c = cursor_from_plain_offset(&doc, 5);
        assert_eq!(c.block_idx, 2);
        assert!(c.cell.is_none());
        assert_eq!(c.byte_offset, 0);
        assert_eq!(plain_offset_from_cursor(&doc, c), 5);
    }

    #[test]
    fn adjacent_cell_wraps_rows() {
        let doc = Document {
            blocks: vec![Block::Table(Table {
                rows: vec![
                    TableRow {
                        cells: vec![cell("A"), cell("B")],
                    },
                    TableRow {
                        cells: vec![cell("C"), cell("D")],
                    },
                ],
                ..Default::default()
            })],
            ..Default::default()
        };
        let a = Cursor {
            block_idx: 0,
            cell: Some(CellPath::new(0, 0, 0)),
            run_idx: 0,
            byte_offset: 0,
        };
        let b = adjacent_cell_cursor(&doc, a, true).unwrap();
        assert_eq!(b.cell.map(|c| (c.row, c.col)), Some((0, 1)));
        let c = adjacent_cell_cursor(&doc, b, true).unwrap();
        assert_eq!(c.cell.map(|c| (c.row, c.col)), Some((1, 0)));
        let d = adjacent_cell_cursor(&doc, c, true).unwrap();
        assert_eq!(d.cell.map(|c| (c.row, c.col)), Some((1, 1)));
        assert!(adjacent_cell_cursor(&doc, d, true).is_none());
        assert_eq!(
            adjacent_cell_cursor(&doc, d, false)
                .unwrap()
                .cell
                .map(|c| (c.row, c.col)),
            Some((1, 0))
        );
    }

    #[test]
    fn paragraph_cursors_same_cell_and_rejects_cross_cell() {
        let doc = Document {
            blocks: vec![Block::Table(Table {
                rows: vec![TableRow {
                    cells: vec![
                        TableCell::from_paragraphs(vec![para("A"), para("B")]),
                        cell("C"),
                    ],
                }],
                ..Default::default()
            })],
            ..Default::default()
        };
        let start = Cursor {
            block_idx: 0,
            cell: Some(CellPath::new(0, 0, 0)),
            run_idx: 0,
            byte_offset: 0,
        };
        let end = Cursor {
            block_idx: 0,
            cell: Some(CellPath::new(0, 0, 1)),
            run_idx: 0,
            byte_offset: 1,
        };
        let cursors = paragraph_cursors_in_selection(
            &doc,
            Selection {
                anchor: start,
                head: end,
            },
        );
        assert_eq!(cursors.len(), 2);
        assert_eq!(cursors[0].cell.map(|c| c.para_idx), Some(0));
        assert_eq!(cursors[1].cell.map(|c| c.para_idx), Some(1));

        let other_cell = Cursor {
            block_idx: 0,
            cell: Some(CellPath::new(0, 1, 0)),
            run_idx: 0,
            byte_offset: 0,
        };
        let cross = paragraph_cursors_in_selection(
            &doc,
            Selection {
                anchor: start,
                head: other_cell,
            },
        );
        assert!(cross.is_empty());
    }

    #[test]
    fn cmp_orders_cells_in_same_table() {
        let a = Cursor {
            block_idx: 0,
            cell: Some(CellPath::new(0, 0, 0)),
            run_idx: 0,
            byte_offset: 0,
        };
        let b = Cursor {
            block_idx: 0,
            cell: Some(CellPath::new(0, 1, 0)),
            run_idx: 0,
            byte_offset: 0,
        };
        assert!(cmp_cursor(a, b) < 0);
        let sel = Selection {
            anchor: b,
            head: a,
        };
        let (start, end) = sel.normalized();
        assert_eq!(start.cell.map(|c| c.col), Some(0));
        assert_eq!(end.cell.map(|c| c.col), Some(1));
    }

    #[test]
    fn cmp_orders_paragraph_before_cell_image() {
        use crate::document::model::{CellImage, ImageFormat, InlineImage};

        let para_cursor = Cursor {
            block_idx: 0,
            cell: Some(CellPath::new(0, 0, 0)),
            run_idx: 0,
            byte_offset: 1,
        };
        let image_cursor = Cursor::on_cell_image(0, 0, 0, 0, 0);
        assert!(cmp_cursor(para_cursor, image_cursor) < 0);
        assert!(cmp_cursor(image_cursor, para_cursor) > 0);

        let p = para("Hi");
        let doc = Document {
            blocks: vec![Block::Table(Table {
                rows: vec![TableRow {
                    cells: vec![TableCell {
                        paragraphs: vec![p.clone()],
                        images: vec![CellImage {
                            after_paragraph: 0,
                            image: InlineImage {
                                bytes: vec![],
                                format: ImageFormat::Png,
                                width_px: 1,
                                height_px: 1,
                                r_id: None,
                                part_path: None,
                            },
                        }],
                    }],
                }],
                ..Default::default()
            })],
            ..Default::default()
        };
        let end_para = end_of_para(0, Some(CellPath::new(0, 0, 0)), &p);
        let next = adjacent_in_cell(&doc, end_para, true).unwrap();
        assert_eq!(next, image_cursor);
    }
}
