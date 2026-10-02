//! PyPI config extractor.
//!
//! Server names and repository URLs are indexed. Usernames and passwords
//! are not. Dispatch lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_pypirc_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == ".pypirc" || lower == "pypirc"
}

pub(crate) fn pypirc_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = section_name(line) {
            if !name.eq_ignore_ascii_case("distutils") {
                push_line(&mut out, name);
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("repository") {
            push_line(&mut out, value.trim().trim_matches('"'));
        }
    }
    out.trim().to_string()
}

fn section_name(line: &str) -> Option<&str> {
    let name = line.strip_prefix('[')?.strip_suffix(']')?.trim();
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
    fn indexes_servers_and_skips_passwords() {
        let text = pypirc_text(
            "[distutils]\n\
             index-servers =\n\
             \u{20}orchid\n\
             \n\
             [orchid]\n\
             repository = https://upload.example.com/legacy/\n\
             username = alice\n\
             password = SECRET\n\
             \n\
             [pypi]\n\
             repository = https://upload.pypi.org/legacy/\n\
             password = OTHER\n",
        );
        assert!(text.contains("orchid"), "{text}");
        assert!(text.contains("pypi"), "{text}");
        assert!(
            text.contains("https://upload.example.com/legacy/"),
            "{text}"
        );
        assert!(text.contains("https://upload.pypi.org/legacy/"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("OTHER"), "{text}");
        assert!(!text.contains("alice"), "{text}");
        assert!(is_pypirc_name(".pypirc"));
        assert!(!is_pypirc_name("pip.conf"));
    }
}
