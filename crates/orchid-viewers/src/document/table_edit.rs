//! Table cell merge / unmerge helpers for the DOCX model.

use crate::document::model::{Paragraph, Table, TableCell, TableRow, VMerge};
use crate::error::{Result, ViewerError};

/// Prefer horizontal merge with the next cell; otherwise merge down.
pub fn merge_at(table: &mut Table, row: usize, col: usize) -> Result<()> {
    if can_merge_horizontal(table, row, col) {
        merge_horizontal(table, row, col)
    } else if can_merge_vertical(table, row, col) {
        merge_vertical(table, row, col)
    } else {
        Err(ViewerError::EditOutOfBounds)
    }
}

/// Split a merged cell under `(row, col)` back into individual slots.
pub fn unmerge_at(table: &mut Table, row: usize, col: usize) -> Result<()> {
    let (row, col) = resolve_merge_origin(table, row, col)?;
    let colspan = cell_colspan(&table.rows[row].cells[col]);
    let col0 = cell_grid_col(table, row, col)?;
    let rowspan = vmerge_rowspan(table, row, col0);
    let has_h = colspan > 1;
    let has_v = rowspan > 1;
    if !has_h && !has_v {
        return Err(ViewerError::EditOutOfBounds);
    }

    if has_v {
        for r in row..row + rowspan {
            let Some(ci) = cell_index_at_grid_col(&table.rows[r], col0) else {
                continue;
            };
            table.rows[r].cells[ci].v_merge = None;
        }
    }

    if has_h {
        for r in row..row + rowspan.max(1) {
            let Some(origin_ci) = cell_index_at_grid_col(&table.rows[r], col0) else {
                continue;
            };
            table.rows[r].cells[origin_ci].grid_span = None;
            for _ in 1..colspan {
                let insert_at = (origin_ci + 1).min(table.rows[r].cells.len());
                table.rows[r].cells.insert(
                    insert_at,
                    TableCell::from_paragraphs(vec![Paragraph::default()]),
                );
            }
        }
    }

    Ok(())
}

fn can_merge_horizontal(table: &Table, row: usize, col: usize) -> bool {
    let Some(row_ref) = table.rows.get(row) else {
        return false;
    };
    if col + 1 >= row_ref.cells.len() {
        return false;
    }
    let left = &row_ref.cells[col];
    let right = &row_ref.cells[col + 1];
    if matches!(left.v_merge, Some(VMerge::Continue))
        || matches!(right.v_merge, Some(VMerge::Continue))
    {
        return false;
    }
    true
}

fn can_merge_vertical(table: &Table, row: usize, col: usize) -> bool {
    if row + 1 >= table.rows.len() {
        return false;
    }
    let Ok(col0) = cell_grid_col(table, row, col) else {
        return false;
    };
    let Some(below_ci) = cell_index_at_grid_col(&table.rows[row + 1], col0) else {
        return false;
    };
    let above = &table.rows[row].cells[col];
    let below = &table.rows[row + 1].cells[below_ci];
    if matches!(above.v_merge, Some(VMerge::Continue)) {
        return false;
    }
    cell_colspan(above) == cell_colspan(below)
}

fn merge_horizontal(table: &mut Table, row: usize, col: usize) -> Result<()> {
    if !can_merge_horizontal(table, row, col) {
        return Err(ViewerError::EditOutOfBounds);
    }
    let left_span = cell_colspan(&table.rows[row].cells[col]);
    let right_span = cell_colspan(&table.rows[row].cells[col + 1]);
    let right = table.rows[row].cells.remove(col + 1);
    let left_para_count = table.rows[row].cells[col].paragraphs.len();
    append_cell_content(&mut table.rows[row].cells[col], right, left_para_count);
    table.rows[row].cells[col].grid_span = Some(left_span + right_span);
    Ok(())
}

fn merge_vertical(table: &mut Table, row: usize, col: usize) -> Result<()> {
    if !can_merge_vertical(table, row, col) {
        return Err(ViewerError::EditOutOfBounds);
    }
    let col0 = cell_grid_col(table, row, col)?;
    let below_ci =
        cell_index_at_grid_col(&table.rows[row + 1], col0).ok_or(ViewerError::EditOutOfBounds)?;
    let span = table.rows[row].cells[col].grid_span;
    let below = std::mem::replace(
        &mut table.rows[row + 1].cells[below_ci],
        TableCell {
            paragraphs: vec![Paragraph::default()],
            images: Vec::new(),
            grid_span: span,
            v_merge: Some(VMerge::Continue),
        },
    );
    let left_para_count = table.rows[row].cells[col].paragraphs.len();
    append_cell_content(&mut table.rows[row].cells[col], below, left_para_count);
    table.rows[row].cells[col].v_merge = Some(VMerge::Restart);
    Ok(())
}

fn append_cell_content(dst: &mut TableCell, mut src: TableCell, para_offset: usize) {
    for img in &mut src.images {
        img.after_paragraph = img.after_paragraph.saturating_add(para_offset);
    }
    if dst.paragraphs.is_empty() {
        dst.paragraphs = src.paragraphs;
    } else if !src.paragraphs.is_empty() {
        let skip = src.paragraphs.len() == 1 && src.paragraphs[0].plain_text().is_empty();
        if !skip {
            dst.paragraphs.append(&mut src.paragraphs);
        }
    }
    dst.images.append(&mut src.images);
}

fn resolve_merge_origin(table: &Table, row: usize, col: usize) -> Result<(usize, usize)> {
    let cell = table
        .rows
        .get(row)
        .and_then(|r| r.cells.get(col))
        .ok_or(ViewerError::EditOutOfBounds)?;
    if matches!(cell.v_merge, Some(VMerge::Continue)) {
        let col0 = cell_grid_col(table, row, col)?;
        for r in (0..row).rev() {
            let Some(ci) = cell_index_at_grid_col(&table.rows[r], col0) else {
                continue;
            };
            match table.rows[r].cells[ci].v_merge {
                Some(VMerge::Restart) | None => return Ok((r, ci)),
                Some(VMerge::Continue) => continue,
            }
        }
        return Err(ViewerError::EditOutOfBounds);
    }
    Ok((row, col))
}

fn cell_colspan(cell: &TableCell) -> u32 {
    cell.grid_span.unwrap_or(1).max(1)
}

fn cell_grid_col(table: &Table, row: usize, col: usize) -> Result<usize> {
    let row_ref = table.rows.get(row).ok_or(ViewerError::EditOutOfBounds)?;
    if col >= row_ref.cells.len() {
        return Err(ViewerError::EditOutOfBounds);
    }
    let mut c = 0usize;
    for (i, cell) in row_ref.cells.iter().enumerate() {
        if i == col {
            return Ok(c);
        }
        c += cell_colspan(cell) as usize;
    }
    Err(ViewerError::EditOutOfBounds)
}

fn cell_index_at_grid_col(row: &TableRow, col0: usize) -> Option<usize> {
    let mut c = 0usize;
    for (i, cell) in row.cells.iter().enumerate() {
        let span = cell_colspan(cell) as usize;
        if col0 >= c && col0 < c + span {
            return Some(i);
        }
        c += span;
    }
    None
}

fn vmerge_rowspan(table: &Table, row: usize, col0: usize) -> usize {
    let Some(ci) = cell_index_at_grid_col(&table.rows[row], col0) else {
        return 1;
    };
    let cell = &table.rows[row].cells[ci];
    if matches!(cell.v_merge, Some(VMerge::Continue)) {
        return 0;
    }
    if !matches!(cell.v_merge, Some(VMerge::Restart)) {
        return 1;
    }
    let mut n = 1;
    for r in (row + 1)..table.rows.len() {
        match cell_index_at_grid_col(&table.rows[r], col0).map(|i| &table.rows[r].cells[i]) {
            Some(c) if matches!(c.v_merge, Some(VMerge::Continue)) => n += 1,
            _ => break,
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::model::{Paragraph, Run, RunStyle};

    fn cell(text: &str) -> TableCell {
        TableCell::from_paragraphs(vec![Paragraph {
            runs: vec![Run {
                text: text.into(),
                style: RunStyle::default(),
                ..Default::default()
            }],
            ..Default::default()
        }])
    }

    fn cell_text(cell: &TableCell) -> String {
        cell.paragraphs
            .iter()
            .map(Paragraph::plain_text)
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn table_2x2() -> Table {
        Table {
            rows: vec![
                TableRow {
                    cells: vec![cell("A"), cell("B")],
                },
                TableRow {
                    cells: vec![cell("C"), cell("D")],
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn merge_horizontal_then_unmerge() {
        let mut t = table_2x2();
        merge_at(&mut t, 0, 0).unwrap();
        assert_eq!(t.rows[0].cells.len(), 1);
        assert_eq!(t.rows[0].cells[0].grid_span, Some(2));
        let text = cell_text(&t.rows[0].cells[0]);
        assert!(text.contains('A') && text.contains('B'), "{text}");
        unmerge_at(&mut t, 0, 0).unwrap();
        assert_eq!(t.rows[0].cells.len(), 2);
        assert!(t.rows[0].cells[0].grid_span.is_none());
    }

    #[test]
    fn merge_vertical_then_unmerge() {
        let mut t = table_2x2();
        merge_at(&mut t, 0, 1).unwrap();
        assert_eq!(t.rows[0].cells[1].v_merge, Some(VMerge::Restart));
        assert_eq!(t.rows[1].cells[1].v_merge, Some(VMerge::Continue));
        let text = cell_text(&t.rows[0].cells[1]);
        assert!(text.contains('B') && text.contains('D'), "{text}");
        unmerge_at(&mut t, 1, 1).unwrap();
        assert!(t.rows[0].cells[1].v_merge.is_none());
        assert!(t.rows[1].cells[1].v_merge.is_none());
    }
}
