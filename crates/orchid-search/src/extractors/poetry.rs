//! Poetry lockfile extractor.
//!
//! Package names and descriptions are indexed. Versions and file hashes
//! are not. Dispatch lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_poetry_lock_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("poetry.lock")
}

pub(crate) fn poetry_lock_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let line = raw.trim();
        let Some(rest) = line
            .strip_prefix("name = ")
            .or_else(|| line.strip_prefix("description = "))
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
        let text = poetry_lock_text(
            "[metadata]\n\
             lock-version = \"2.0\"\n\
             content-hash = \"SECRET\"\n\
             \n\
             [[package]]\n\
             name = \"requests\"\n\
             version = \"2.31.0\"\n\
             description = \"HTTP library\"\n\
             files = [\n\
             \t{file = \"requests-2.31.0.tar.gz\", hash = \"sha256:SECRET2\"},\n\
             ]\n",
        );
        assert!(text.contains("requests"), "{text}");
        assert!(text.contains("HTTP library"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("2.31.0"), "{text}");
        assert!(poetry_lock_text("").is_empty());
        assert!(is_poetry_lock_name("poetry.lock"));
        assert!(!is_poetry_lock_name("pyproject.toml"));
    }
}
