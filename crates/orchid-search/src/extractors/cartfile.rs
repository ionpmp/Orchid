//! Carthage resolved-file extractor.
//!
//! Repository names and URLs are indexed. Versions and commits are not.
//! Dispatch lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_cartfile_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("Cartfile.resolved")
}

pub(crate) fn cartfile_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = first_quoted(line) {
            push_line(&mut out, name);
        }
    }
    out.trim().to_string()
}

fn first_quoted(line: &str) -> Option<&str> {
    let rest = line.split_once('"')?.1;
    let name = rest.split_once('"')?.0;
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
    fn indexes_repos_and_skips_commits() {
        let text = cartfile_text(
            "github \"Alamofire/Alamofire\" \"5.8.0\"\n\
             git \"https://github.com/example/orchid.git\" \"deadbeefcafebabe\"\n\
             # comment \"SECRET\"\n",
        );
        assert!(text.contains("Alamofire/Alamofire"), "{text}");
        assert!(
            text.contains("https://github.com/example/orchid.git"),
            "{text}"
        );
        assert!(!text.contains("deadbeef"), "{text}");
        assert!(!text.contains("5.8.0"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(is_cartfile_name("Cartfile.resolved"));
        assert!(!is_cartfile_name("Cartfile"));
    }
}
