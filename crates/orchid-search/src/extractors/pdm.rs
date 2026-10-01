//! PDM lockfile extractor.
//!
//! Package names and summaries are indexed. Versions and file hashes are
//! not. Dispatch lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_pdm_lock_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("pdm.lock")
}

pub(crate) fn pdm_lock_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let line = raw.trim();
        let Some(rest) = line
            .strip_prefix("name = ")
            .or_else(|| line.strip_prefix("summary = "))
        else {
            continue;
        };
        push_line(&mut out, unquote(rest));
    }
    out.trim().to_string()
}

fn unquote(value: &str) -> &str {
    value.trim().trim_matches(|ch| ch == '"' || ch == '\'')
}

fn push_line(out: &mut String, value: &str) {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
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
    fn indexes_names_and_skips_hashes() {
        let text = pdm_lock_text(
            "[metadata]\n\
             content_hash = \"sha256:SECRET\"\n\
             \n\
             [[package]]\n\
             name = \"certifi\"\n\
             version = \"2024.2.2\"\n\
             summary = \"CA bundle\"\n\
             files = [\n\
             \t{file = \"certifi.whl\", hash = \"sha256:SECRET2\"},\n\
             ]\n",
        );
        assert!(text.contains("certifi"), "{text}");
        assert!(text.contains("CA bundle"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("2024.2.2"), "{text}");
        assert!(is_pdm_lock_name("pdm.lock"));
        assert!(!is_pdm_lock_name("pdm.toml"));
    }
}
