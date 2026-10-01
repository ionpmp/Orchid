//! Cargo.lock extractor.
//!
//! Package names are indexed. Versions, sources, and checksums are not.
//! Dispatch lives in [`super::Extractor::extract`] because `text/plain`
//! would keep the checksums.

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_cargo_lock_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("Cargo.lock")
}

pub(crate) fn cargo_lock_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let Some(rest) = raw.trim().strip_prefix("name = ") else {
            continue;
        };
        push_line(&mut out, unquote(rest));
    }
    out.trim().to_string()
}

fn unquote(value: &str) -> &str {
    value.trim().trim_end_matches(',').trim().trim_matches('"')
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
    fn indexes_package_names_and_skips_checksums() {
        let text = cargo_lock_text(
            "version = 3\n\
             \n\
             [[package]]\n\
             name = \"tantivy\"\n\
             version = \"0.26.0\"\n\
             source = \"registry+https://github.com/rust-lang/crates.io-index\"\n\
             checksum = \"deadbeefcafebabe\"\n\
             \n\
             [[package]]\n\
             name = \"libc\"\n",
        );
        assert!(text.contains("tantivy"), "{text}");
        assert!(text.contains("libc"), "{text}");
        assert!(!text.contains("0.26.0"), "{text}");
        assert!(!text.contains("deadbeef"), "{text}");
        assert!(!text.contains("crates.io"), "{text}");
        assert!(is_cargo_lock_name("Cargo.lock"));
        assert!(!is_cargo_lock_name("Cargo.toml"));
    }
}
