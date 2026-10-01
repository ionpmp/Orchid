//! Yarn lockfile extractor.
//!
//! Package names are indexed. Versions, resolved URLs, and integrity
//! hashes are not. Dispatch lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_yarn_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("yarn.lock")
}

pub(crate) fn yarn_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let line = raw.trim();
        if !line.ends_with(':') || !line.contains('@') {
            continue;
        }
        let selector = line.trim_end_matches(':').trim();
        for part in selector.split(',') {
            let part = part.trim().trim_matches('"').trim();
            if let Some(name) = name_from_spec(part) {
                push_line(&mut out, name);
            }
        }
    }
    out.trim().to_string()
}

fn name_from_spec(spec: &str) -> Option<&str> {
    let at = spec.rfind('@')?;
    if at == 0 {
        return None;
    }
    let name = spec[..at].trim();
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
    fn indexes_package_names_and_skips_integrity() {
        let text = yarn_text(
            "\"@scope/pkg@^1.0.0\":\n\
             \tversion \"1.2.3\"\n\
             \tresolved \"https://registry.yarnpkg.com/@scope/pkg/-/pkg-1.2.3.tgz#deadbeef\"\n\
             \tintegrity sha512-SECRET\n\
             \n\
             tantivy@^0.26.0:\n\
             \tversion \"0.26.0\"\n\
             \tintegrity sha512-OTHER\n",
        );
        assert!(text.contains("@scope/pkg"), "{text}");
        assert!(text.contains("tantivy"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("OTHER"), "{text}");
        assert!(!text.contains("1.2.3"), "{text}");
        assert!(!text.contains("deadbeef"), "{text}");
        assert!(is_yarn_name("yarn.lock"));
        assert!(!is_yarn_name("package-lock.json"));
    }
}
