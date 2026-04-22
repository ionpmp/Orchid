//! Grid-cell math shared between the layout engine's grid and free modes.

use orchid_storage::{GridPosition, WidgetSize};

/// Logical cell footprint of a [`WidgetSize`].
///
/// The Orchid spec pins these:
/// * `Small`       — 2 × 2
/// * `Medium`      — 4 × 2
/// * `Large`       — 4 × 4
/// * `ExtraLarge`  — 8 × 4
/// * `Free { w, h }` — uses `w × h` directly.
#[must_use]
pub fn size_in_cells(size: WidgetSize) -> (u16, u16) {
    match size {
        WidgetSize::Small => (2, 2),
        WidgetSize::Medium => (4, 2),
        WidgetSize::Large => (4, 4),
        WidgetSize::ExtraLarge => (8, 4),
        WidgetSize::Free { w, h } => (w.max(1), h.max(1)),
    }
}

/// Does the cell rectangle `(position, size)` fit inside a `cols × rows`
/// grid?
#[must_use]
pub fn fits_in_grid(position: GridPosition, size: WidgetSize, cols: u16, rows: u16) -> bool {
    let (w, h) = size_in_cells(size);
    let last_col = position.col.saturating_add(w);
    let last_row = position.row.saturating_add(h);
    last_col <= cols && last_row <= rows
}

/// Snap a position to the grid by clamping to the nearest valid top-left
/// corner that keeps the widget fully inside the grid.
#[must_use]
pub fn snap_position(
    position: GridPosition,
    size: WidgetSize,
    cols: u16,
    rows: u16,
) -> GridPosition {
    let (w, h) = size_in_cells(size);
    let max_col = cols.saturating_sub(w);
    let max_row = rows.saturating_sub(h);
    GridPosition {
        col: position.col.min(max_col),
        row: position.row.min(max_row),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_defaults_match_spec() {
        assert_eq!(size_in_cells(WidgetSize::Small), (2, 2));
        assert_eq!(size_in_cells(WidgetSize::Medium), (4, 2));
        assert_eq!(size_in_cells(WidgetSize::Large), (4, 4));
        assert_eq!(size_in_cells(WidgetSize::ExtraLarge), (8, 4));
        assert_eq!(size_in_cells(WidgetSize::Free { w: 3, h: 5 }), (3, 5));
    }

    #[test]
    fn free_clamps_to_one() {
        assert_eq!(size_in_cells(WidgetSize::Free { w: 0, h: 0 }), (1, 1));
    }

    #[test]
    fn fits_rejects_overflow() {
        assert!(fits_in_grid(
            GridPosition { col: 14, row: 8 },
            WidgetSize::Small,
            16,
            10
        ));
        assert!(!fits_in_grid(
            GridPosition { col: 15, row: 8 },
            WidgetSize::Small,
            16,
            10
        ));
    }

    #[test]
    fn snap_clamps_into_grid() {
        let snapped = snap_position(
            GridPosition { col: 100, row: 100 },
            WidgetSize::Small,
            16,
            10,
        );
        assert_eq!(snapped, GridPosition { col: 14, row: 8 });
    }
}
