//! Free-form placement helpers.

use orchid_storage::{GridPosition, WidgetSize};

use crate::layout::collision::{overlaps, CellRect};
use crate::layout::grid::{fits_in_grid, size_in_cells};

/// Spiral search for a free slot starting from `(0, 0)` with 2-cell stride.
/// Returns `None` when the grid has no room.
#[must_use]
pub fn spiral_place(
    size: WidgetSize,
    cols: u16,
    rows: u16,
    occupied: &[CellRect],
) -> Option<GridPosition> {
    if !fits_in_grid(GridPosition { col: 0, row: 0 }, size, cols, rows) {
        return None;
    }

    let (w, h) = size_in_cells(size);
    let max_col = cols.saturating_sub(w);
    let max_row = rows.saturating_sub(h);

    // Spiral walk with stride = 2 cells.
    let stride: i32 = 2;
    let center_col = (max_col / 2) as i32;
    let center_row = (max_row / 2) as i32;

    let mut leg = 1_i32;
    let mut col = center_col;
    let mut row = center_row;

    let candidate = GridPosition {
        col: col as u16,
        row: row as u16,
    };
    if !collides(candidate, size, occupied) {
        return Some(candidate);
    }

    for _ in 0..(cols as i32 * rows as i32) {
        for dir in [(1_i32, 0_i32), (0, 1), (-1, 0), (0, -1)] {
            for _ in 0..leg {
                col += dir.0 * stride;
                row += dir.1 * stride;
                if col < 0 || row < 0 || col > max_col as i32 || row > max_row as i32 {
                    continue;
                }
                let candidate = GridPosition {
                    col: col as u16,
                    row: row as u16,
                };
                if !collides(candidate, size, occupied) {
                    return Some(candidate);
                }
            }
            if dir == (0, 1) || dir == (0, -1) {
                leg += 1;
            }
        }
    }
    None
}

fn collides(position: GridPosition, size: WidgetSize, occupied: &[CellRect]) -> bool {
    let rect = CellRect::from_widget(position, size);
    occupied.iter().any(|r| overlaps(rect, *r))
}
