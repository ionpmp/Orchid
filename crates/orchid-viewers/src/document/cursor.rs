//! Cursor and selection over a [`Document`](super::model::Document).

/// Position inside the document body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cursor {
    /// Index into [`Document::blocks`](super::model::Document::blocks).
    pub block_idx: usize,
    /// Index into the paragraph's `runs` (ignored for non-paragraph blocks).
    pub run_idx: usize,
    /// UTF-8 byte offset inside the run's text.
    pub byte_offset: usize,
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
}

fn cmp_cursor(a: Cursor, b: Cursor) -> i8 {
    use std::cmp::Ordering;
    match (
        a.block_idx.cmp(&b.block_idx),
        a.run_idx.cmp(&b.run_idx),
        a.byte_offset.cmp(&b.byte_offset),
    ) {
        (Ordering::Less, _, _) | (_, Ordering::Less, _) | (_, _, Ordering::Less) => -1,
        (Ordering::Greater, _, _) | (_, Ordering::Greater, _) | (_, _, Ordering::Greater) => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_swaps_backwards_selection() {
        let sel = Selection {
            anchor: Cursor {
                block_idx: 2,
                run_idx: 0,
                byte_offset: 5,
            },
            head: Cursor {
                block_idx: 1,
                run_idx: 0,
                byte_offset: 0,
            },
        };
        let (start, end) = sel.normalized();
        assert_eq!(start.block_idx, 1);
        assert_eq!(end.block_idx, 2);
    }
}
