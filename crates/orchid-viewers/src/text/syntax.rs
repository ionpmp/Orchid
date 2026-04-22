//! Syntax highlighter.
//!
//! The MVP ships an empty grammar registry; every call emits a single
//! [`SyntaxScope::Plain`] segment per line. A follow-up task plugs in
//! tree-sitter grammars (`tree-sitter-rust`, `tree-sitter-python`, …)
//! behind this same API so nothing above the highlighter changes.

use crate::snapshot::{SyntaxLine, SyntaxScope, SyntaxSegment};

/// Pluggable highlighter. The API is stable; grammars land in a separate
/// task.
#[derive(Default)]
pub struct SyntaxHighlighter {
    // Reserved for a future HashMap<&'static str, tree_sitter::Language>.
    _phantom: (),
}

impl std::fmt::Debug for SyntaxHighlighter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyntaxHighlighter").finish_non_exhaustive()
    }
}

impl SyntaxHighlighter {
    /// Build an empty highlighter.
    #[must_use]
    pub fn new() -> Self {
        Self { _phantom: () }
    }

    /// Highlight `count` lines of `source` starting at `first_line`.
    ///
    /// The MVP returns one `Plain` segment per line. The signature is
    /// grammar-agnostic so the future registry can swap in tree-sitter
    /// without touching callers.
    #[must_use]
    pub fn highlight_lines(
        &self,
        _language: &str,
        source: &str,
        first_line: u32,
        line_count: u32,
    ) -> Vec<SyntaxLine> {
        let mut out = Vec::with_capacity(line_count as usize);
        for (offset, line) in source.split('\n').take(line_count as usize).enumerate() {
            out.push(SyntaxLine {
                line_number: first_line + offset as u32,
                segments: vec![SyntaxSegment {
                    text: line.to_string(),
                    scope: SyntaxScope::Plain,
                }],
            });
        }
        out
    }

    /// Languages the registry currently supports. MVP: empty.
    #[must_use]
    pub fn available_languages(&self) -> Vec<&'static str> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_plain_segments_per_line() {
        let h = SyntaxHighlighter::new();
        let lines = h.highlight_lines("rust", "fn main() {}\nprintln!(\"hi\");", 0, 2);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].segments.len(), 1);
        assert_eq!(lines[0].segments[0].scope, SyntaxScope::Plain);
    }
}
