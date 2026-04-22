//! AABB collision detection in cell coordinates.

use orchid_storage::{GridPosition, WidgetSize};

use crate::layout::grid::size_in_cells;

/// Rectangle in cell coordinates (inclusive `left`/`top`, exclusive
/// `right`/`bottom`).
#[derive(Debug, Clone, Copy)]
pub struct CellRect {
    /// Left cell.
    pub left: u16,
    /// Top cell.
    pub top: u16,
    /// Right cell (exclusive).
    pub right: u16,
    /// Bottom cell (exclusive).
    pub bottom: u16,
}

impl CellRect {
    /// Construct from a `(position, size)` pair.
    #[must_use]
    pub fn from_widget(position: GridPosition, size: WidgetSize) -> Self {
        let (w, h) = size_in_cells(size);
        Self {
            left: position.col,
            top: position.row,
            right: position.col.saturating_add(w),
            bottom: position.row.saturating_add(h),
        }
    }
}

/// Axis-aligned bounding box overlap.
#[must_use]
pub fn overlaps(a: CellRect, b: CellRect) -> bool {
    a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(l: u16, t: u16, r: u16, b: u16) -> CellRect {
        CellRect {
            left: l,
            top: t,
            right: r,
            bottom: b,
        }
    }

    #[test]
    fn touching_edges_do_not_overlap() {
        assert!(!overlaps(rect(0, 0, 2, 2), rect(2, 0, 4, 2)));
        assert!(!overlaps(rect(0, 0, 2, 2), rect(0, 2, 2, 4)));
    }

    #[test]
    fn interior_overlap_is_detected() {
        assert!(overlaps(rect(0, 0, 4, 4), rect(2, 2, 6, 6)));
    }

    #[test]
    fn widget_rect_round_trip() {
        let r = CellRect::from_widget(GridPosition { col: 2, row: 3 }, WidgetSize::Small);
        assert_eq!(r.left, 2);
        assert_eq!(r.top, 3);
        assert_eq!(r.right, 4);
        assert_eq!(r.bottom, 5);
    }
}
