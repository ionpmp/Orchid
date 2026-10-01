//! Mix lockfile extractor.
//!
//! Package names are indexed. Versions and checksums are not. Dispatch
//! lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_mix_lock_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("mix.lock")
}

pub(crate) fn mix_lock_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        if let Some(name) = quoted_key(raw) {
            push_line(&mut out, name);
        }
    }
    out.trim().to_string()
}

fn quoted_key(line: &str) -> Option<&str> {
    let rest = line.trim().strip_prefix('"')?;
    let end = rest.find('"')?;
    let name = &rest[..end];
    if name.is_empty() || !rest[end + 1..].trim_start().starts_with(':') {
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
    fn indexes_package_names_and_skips_checksums() {
        let text = mix_lock_text(
            "%{\n\
             \u{20}\u{20}\"orchid\": {:hex, :orchid, \"1.2.3\", \"SECRET\", [:mix], [], \"hexpm\", \"CHECK\"},\n\
             \u{20}\u{20}\"jason\": {:hex, :jason, \"1.4.1\", \"OTHER\", [:mix], [{:decimal, \"~> 2.0\", [hex: :decimal, repo: \"hexpm\", optional: false]}], \"hexpm\", \"OUTER\"},\n\
             }.\n",
        );
        assert!(text.contains("orchid"), "{text}");
        assert!(text.contains("jason"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("OTHER"), "{text}");
        assert!(!text.contains("1.2.3"), "{text}");
        assert!(!text.contains("hexpm"), "{text}");
        assert!(is_mix_lock_name("mix.lock"));
        assert!(!is_mix_lock_name("mix.exs"));
    }
}
