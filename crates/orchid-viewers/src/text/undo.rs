//! Undo/redo stack with a small merge window for natural typing granularity.

use std::time::{Duration, Instant};

/// Kind of text edit op.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextOpKind {
    Insert,
    Delete,
}

/// One edit op + position + timestamp.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct TextOp {
    pub kind: TextOpKind,
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub text: String,
    pub timestamp: Instant,
}

/// Undo + redo stacks with merge-on-typing.
pub struct UndoStack {
    past: Vec<TextOp>,
    future: Vec<TextOp>,
    merge_window: Duration,
}

impl std::fmt::Debug for UndoStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UndoStack")
            .field("past", &self.past.len())
            .field("future", &self.future.len())
            .finish()
    }
}

impl UndoStack {
    /// Build a stack with the given merge window in milliseconds.
    #[must_use]
    pub fn new(merge_window_ms: u64) -> Self {
        Self {
            past: Vec::new(),
            future: Vec::new(),
            merge_window: Duration::from_millis(merge_window_ms),
        }
    }

    /// Push a new op. Merges with the previous op when adjacent and same
    /// kind / within the merge window.
    pub fn push(&mut self, op: TextOp) {
        self.future.clear();
        if let Some(prev) = self.past.last_mut() {
            if can_merge(prev, &op, self.merge_window) {
                // Simple merge: extend the text, keep the earlier start,
                // advance the end.
                prev.text.push_str(&op.text);
                prev.end_line = op.end_line;
                prev.end_column = op.end_column;
                prev.timestamp = op.timestamp;
                return;
            }
        }
        self.past.push(op);
    }

    /// Pop the most recent op — caller applies its inverse, then pushes
    /// the original into the redo stack via [`UndoStack::store_redo`].
    pub fn undo(&mut self) -> Option<TextOp> {
        let op = self.past.pop()?;
        self.future.push(op.clone());
        Some(op)
    }

    /// Pop the next redo op.
    pub fn redo(&mut self) -> Option<TextOp> {
        let op = self.future.pop()?;
        self.past.push(op.clone());
        Some(op)
    }

    /// Drop all history.
    pub fn clear(&mut self) {
        self.past.clear();
        self.future.clear();
    }

    /// How many undo frames are currently recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.past.len()
    }

    /// Whether the undo stack is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.past.is_empty()
    }
}

fn can_merge(prev: &TextOp, next: &TextOp, window: Duration) -> bool {
    if prev.kind != next.kind || prev.kind != TextOpKind::Insert {
        return false;
    }
    if next.timestamp.duration_since(prev.timestamp) > window {
        return false;
    }
    // Adjacent: next starts exactly where prev ended.
    prev.end_line == next.start_line && prev.end_column == next.start_column
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(col: u32, text: &str, t: Instant) -> TextOp {
        TextOp {
            kind: TextOpKind::Insert,
            start_line: 0,
            start_column: col,
            end_line: 0,
            end_column: col + text.chars().count() as u32,
            text: text.into(),
            timestamp: t,
        }
    }

    #[test]
    fn adjacent_inserts_merge_within_window() {
        let mut stack = UndoStack::new(500);
        let t0 = Instant::now();
        stack.push(op(0, "a", t0));
        stack.push(op(1, "b", t0 + Duration::from_millis(100)));
        stack.push(op(2, "c", t0 + Duration::from_millis(200)));
        assert_eq!(stack.len(), 1);
        let merged = stack.undo().unwrap();
        assert_eq!(merged.text, "abc");
    }

    #[test]
    fn gap_breaks_merge() {
        let mut stack = UndoStack::new(50);
        let t0 = Instant::now();
        stack.push(op(0, "a", t0));
        stack.push(op(1, "b", t0 + Duration::from_millis(200)));
        assert_eq!(stack.len(), 2);
    }

    #[test]
    fn undo_then_redo_round_trips() {
        let mut stack = UndoStack::new(500);
        stack.push(op(0, "a", Instant::now()));
        let u = stack.undo().unwrap();
        assert_eq!(u.text, "a");
        let r = stack.redo().unwrap();
        assert_eq!(r.text, "a");
    }
}
