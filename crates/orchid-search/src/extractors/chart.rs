//! Helm chart lockfile extractor.
//!
//! Dependency names and repositories are indexed. Versions and the digest
//! are not. Dispatch lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_chart_lock_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("Chart.lock")
}

pub(crate) fn chart_lock_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let line = raw.trim().trim_start_matches('-').trim();
        let Some(rest) = line
            .strip_prefix("name:")
            .or_else(|| line.strip_prefix("repository:"))
        else {
            continue;
        };
        push_line(&mut out, rest.trim().trim_matches('"'));
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
    fn indexes_charts_and_skips_digest() {
        let text = chart_lock_text(
            "dependencies:\n\
             - name: orchid\n\
             \u{20}\u{20}repository: https://charts.example.com\n\
             \u{20}\u{20}version: 1.2.3\n\
             digest: sha256:SECRET\n\
             generated: \"2024-01-01T00:00:00Z\"\n",
        );
        assert!(text.contains("orchid"), "{text}");
        assert!(text.contains("https://charts.example.com"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("1.2.3"), "{text}");
        assert!(!text.contains("2024-01-01"), "{text}");
        assert!(is_chart_lock_name("Chart.lock"));
        assert!(!is_chart_lock_name("Chart.yaml"));
    }
}
