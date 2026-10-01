//! pip requirements extractor.
//!
//! Package names are indexed. Versions and `--hash` values are not.
//! Dispatch lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_requirements_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "requirements.txt"
        || lower.ends_with("-requirements.txt")
        || (lower.starts_with("requirements-") && lower.ends_with(".txt"))
}

pub(crate) fn requirements_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.split('#').next().unwrap_or(line).trim();
        if line.is_empty() || line.starts_with('-') {
            continue;
        }
        let Some(token) = line.split_whitespace().next() else {
            continue;
        };
        if token.contains("://") {
            continue;
        }
        let name = token
            .split(['=', '>', '<', '!', '~', ';', '[', '@'])
            .next()
            .unwrap_or("");
        push_line(&mut out, name);
    }
    out.trim().to_string()
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
        let text = requirements_text(
            "# comment\n\
             requests==2.31.0 --hash=sha256:SECRET\n\
             orchid[extra]>=1.0\n\
             -r base.txt\n\
             --index-url https://pypi.org/simple\n\
             git+https://github.com/example/foo.git@deadbeef#egg=foo\n",
        );
        assert!(text.contains("requests"), "{text}");
        assert!(text.contains("orchid"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("2.31.0"), "{text}");
        assert!(!text.contains("deadbeef"), "{text}");
        assert!(!text.contains("base.txt"), "{text}");
        assert!(!text.contains("pypi"), "{text}");
        assert!(is_requirements_name("requirements.txt"));
        assert!(is_requirements_name("requirements-dev.txt"));
        assert!(is_requirements_name("dev-requirements.txt"));
        assert!(!is_requirements_name("notes.txt"));
    }
}
