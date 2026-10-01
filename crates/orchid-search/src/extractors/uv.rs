//! uv lockfile extractor.
//!
//! Package and dependency names are indexed. Versions and hashes are
//! not. Dispatch lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_uv_lock_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("uv.lock")
}

pub(crate) fn uv_lock_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        collect_names(line, &mut out);
    }
    out.trim().to_string()
}

fn collect_names(line: &str, out: &mut String) {
    let mut rest = line;
    while let Some(idx) = rest.find("name") {
        let boundary = idx == 0
            || !rest.as_bytes()[idx - 1].is_ascii_alphanumeric()
                && rest.as_bytes()[idx - 1] != b'_';
        let after = rest[idx + 4..].trim_start();
        if boundary && after.starts_with('=') {
            if let Some(name) = quoted_value(after[1..].trim_start()) {
                push_line(out, name);
            }
        }
        rest = &rest[idx + 4..];
    }
}

fn quoted_value(value: &str) -> Option<&str> {
    let mut chars = value.chars();
    let quote = chars.next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = chars.as_str();
    let end = rest.find(quote)?;
    let name = &rest[..end];
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
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
    fn indexes_package_names_and_skips_hashes() {
        let text = uv_lock_text(
            r#"
version = 1
requires-python = ">=3.12"

[[package]]
name = "orchid"
version = "1.2.3"
dependencies = [
    { name = "requests" },
]
sdist = { url = "https://example.com/orchid.tar.gz", hash = "sha256:SECRET", size = 12 }

[[package]]
name = "requests"
version = "2.31.0"
"#,
        );
        assert!(text.contains("orchid"), "{text}");
        assert!(text.contains("requests"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("1.2.3"), "{text}");
        assert!(!text.contains("2.31.0"), "{text}");
        assert!(is_uv_lock_name("uv.lock"));
        assert!(!is_uv_lock_name("Cargo.lock"));
    }
}
