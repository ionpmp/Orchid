//! Selection model — Linear / Block / Word / Line.

/// A point in the scrollback-aware grid. `row` is negative for scrollback,
/// `0..rows` for the visible area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridPoint {
    /// Column (`0..cols`).
    pub col: u16,
    /// Row in `scrollback ∪ visible`. Negative = scrollback.
    pub row: i64,
}

/// Selection mode.
#[derive(Debug, Clone)]
pub enum Selection {
    /// A contiguous range from `start` to `end`.
    Linear {
        /// Start of the range.
        start: GridPoint,
        /// End of the range (exclusive end; same row = single-line).
        end: GridPoint,
    },
    /// A rectangular block bounded by `start` and `end`.
    Block {
        /// Top-left corner.
        start: GridPoint,
        /// Bottom-right corner.
        end: GridPoint,
    },
    /// Word containing `at`.
    Word {
        /// Point within the word.
        at: GridPoint,
    },
    /// Entire line `row`.
    Line {
        /// Row index.
        row: i64,
    },
}
