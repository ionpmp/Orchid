//! Cursor and selection over a [`Document`](super::model::Document).

use crate::document::model::{Block, Document};

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

    /// Whether anchor and head are the same position.
    #[must_use]
    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.head
    }
}

/// Map a UTF-8 byte offset in [`Document::plain_text`] into a [`Cursor`].
///
/// Paragraphs are joined with a single `\n` (matching `plain_text()`).
#[must_use]
pub fn cursor_from_plain_offset(doc: &Document, mut offset: usize) -> Cursor {
    if doc.blocks.is_empty() {
        return Cursor::default();
    }
    for (bi, block) in doc.blocks.iter().enumerate() {
        let Block::Paragraph(p) = block else {
            continue;
        };
        let para_len: usize = p.runs.iter().map(|r| r.text.len()).sum();
        if offset <= para_len {
            let mut remaining = offset;
            if p.runs.is_empty() {
                return Cursor {
                    block_idx: bi,
                    run_idx: 0,
                    byte_offset: 0,
                };
            }
            for (ri, run) in p.runs.iter().enumerate() {
                if remaining <= run.text.len() {
                    return Cursor {
                        block_idx: bi,
                        run_idx: ri,
                        byte_offset: remaining,
                    };
                }
                remaining -= run.text.len();
            }
            let last = p.runs.len() - 1;
            return Cursor {
                block_idx: bi,
                run_idx: last,
                byte_offset: p.runs[last].text.len(),
            };
        }
        offset -= para_len;
        // Separator `\n` between paragraphs.
        if bi + 1 < doc.blocks.len() {
            if offset == 0 {
                // Caret sits on the newline → end of this paragraph.
                let last = p.runs.len().saturating_sub(1);
                let byte_offset = p.runs.get(last).map(|r| r.text.len()).unwrap_or(0);
                return Cursor {
                    block_idx: bi,
                    run_idx: last,
                    byte_offset,
                };
            }
            offset -= 1;
        }
    }
    // Past the end → last paragraph end.
    for bi in (0..doc.blocks.len()).rev() {
        if let Block::Paragraph(p) = &doc.blocks[bi] {
            let last = p.runs.len().saturating_sub(1);
            let byte_offset = p.runs.get(last).map(|r| r.text.len()).unwrap_or(0);
            return Cursor {
                block_idx: bi,
                run_idx: last,
                byte_offset,
            };
        }
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

/// Indices of paragraph blocks touched by `selection` (inclusive).
#[must_use]
pub fn paragraph_indices_in_selection(doc: &Document, selection: Selection) -> Vec<usize> {
    let (start, end) = selection.normalized();
    let mut out = Vec::new();
    for bi in start.block_idx..=end.block_idx.min(doc.blocks.len().saturating_sub(1)) {
        if matches!(doc.blocks.get(bi), Some(Block::Paragraph(_))) {
            out.push(bi);
        }
    }
    out
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
