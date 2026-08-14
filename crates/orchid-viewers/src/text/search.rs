//! Find / replace helpers for the text viewer (literal, regex, multiline).

use regex::RegexBuilder;

use crate::error::{Result, ViewerError};

/// Find options from the Lister toolbar.
#[derive(Debug, Clone, Copy, Default)]
pub struct FindOptions {
    /// Treat the query as a regular expression.
    pub regex: bool,
    /// `^`/`$` match line edges and `.` matches newlines.
    pub multiline: bool,
}

/// Compile `query` into a case-insensitive matcher.
///
/// # Errors
///
/// Invalid regular expressions when [`FindOptions::regex`] is set.
pub fn compile_query(query: &str, opts: FindOptions) -> Result<regex::Regex> {
    let pattern = if opts.regex {
        query.to_string()
    } else {
        regex::escape(query)
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(true)
        .multi_line(opts.multiline)
        .dot_matches_new_line(opts.multiline)
        .build()
        .map_err(|e| ViewerError::TextDecode(format!("invalid regex: {e}")))
}

/// Non-overlapping match byte ranges `(start, end)` in `haystack`.
///
/// # Errors
///
/// Invalid regular expressions when [`FindOptions::regex`] is set.
pub fn collect_matches(
    haystack: &str,
    query: &str,
    opts: FindOptions,
) -> Result<Vec<(usize, usize)>> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let re = compile_query(query, opts)?;
    Ok(re
        .find_iter(haystack)
        .map(|m| (m.start(), m.end()))
        .collect())
}

/// Pick the next / previous match after the current selection.
#[must_use]
pub fn pick_match(
    matches: &[(usize, usize)],
    sel_lo: usize,
    sel_hi: usize,
    forward: bool,
) -> Option<(usize, usize)> {
    if matches.is_empty() {
        return None;
    }
    if forward {
        matches
            .iter()
            .copied()
            .find(|&(s, _)| s >= sel_hi)
            .or_else(|| matches.first().copied())
    } else {
        matches
            .iter()
            .rev()
            .copied()
            .find(|&(_, e)| e <= sel_lo)
            .or_else(|| matches.last().copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_is_case_insensitive() {
        let m = collect_matches("Hello HELLO hello", "hello", FindOptions::default()).unwrap();
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn regex_multiline_dot() {
        let opts = FindOptions {
            regex: true,
            multiline: true,
        };
        let m = collect_matches("ab\ncd", "a.*d", opts).unwrap();
        assert_eq!(m, vec![(0, 5)]);
    }

    #[test]
    fn pick_wraps_forward() {
        let matches = vec![(0, 1), (4, 5)];
        assert_eq!(pick_match(&matches, 4, 5, true), Some((0, 1)));
        assert_eq!(pick_match(&matches, 0, 1, false), Some((4, 5)));
    }
}
