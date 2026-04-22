//! In-buffer (scrollback) search helpers.

/// A single match found inside the terminal scrollback / visible grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    /// Line index (negative values index into scrollback).
    pub line: i64,
    /// Inclusive starting column.
    pub col_start: u16,
    /// Exclusive ending column.
    pub col_end: u16,
}
