//! Go checksum file extractor.
//!
//! Module paths are indexed. Versions and `h1:` hashes are not. Dispatch
//! lives in [`super::Extractor::extract`] because `text/plain` would keep
//! the hashes.

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_gosum_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("go.sum")
}

pub(crate) fn gosum_text(input: &str) -> String {
    let mut out = String::new();
    let mut last = String::new();
    for raw in input.lines() {
        let Some(token) = raw.split_whitespace().next() else {
            continue;
        };
        if is_version(token) || !(token.contains('/') || token.contains('.')) {
            continue;
        }
        if token == last {
            continue;
        }
        push_line(&mut out, token);
        last = token.to_string();
    }
    out.trim().to_string()
}

fn is_version(token: &str) -> bool {
    let rest = token.strip_prefix('v').unwrap_or(token);
    rest.chars().next().is_some_and(|ch| ch.is_ascii_digit())
}

fn push_line(out: &mut String, value: &str) {
    let value = value.trim();
    if value.is_empty() || out.len() >= MAX_CONTENT_BYTES {
        return;
    }
    if !out.is_empty() {
        out.push('\n');
    }
    let room = MAX_CONTENT_BYTES.saturating_sub(out.len());
    out.push_str(&value.chars().take(room).collect::<String>());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_module_paths_and_skips_hashes() {
        let text = gosum_text(
            "github.com/foo/bar v1.2.3 h1:SECRET=\n\
             github.com/foo/bar v1.2.3/go.mod h1:SECRET2=\n\
             golang.org/x/text v0.14.0 h1:OTHER=\n",
        );
        assert!(text.contains("github.com/foo/bar"), "{text}");
        assert!(text.contains("golang.org/x/text"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("OTHER"), "{text}");
        assert!(!text.contains("1.2.3"), "{text}");
        assert!(!text.contains("0.14.0"), "{text}");
        assert!(is_gosum_name("go.sum"));
        assert!(!is_gosum_name("go.mod"));
    }
}
