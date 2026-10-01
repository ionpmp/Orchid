//! pnpm lockfile extractor.
//!
//! Package names are indexed. Versions and integrity hashes are not.
//! Dispatch lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_pnpm_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("pnpm-lock.yaml")
}

pub(crate) fn pnpm_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let line = raw.trim();
        let Some(rest) = line.strip_prefix('/') else {
            continue;
        };
        let spec = rest
            .trim_end_matches(':')
            .split('(')
            .next()
            .unwrap_or(rest)
            .trim();
        push_line(&mut out, package_name(spec));
    }
    out.trim().to_string()
}

fn package_name(spec: &str) -> &str {
    if let Some(at) = spec.rfind('@') {
        if at > 0 && spec[at + 1..].starts_with(|ch: char| ch.is_ascii_digit()) {
            return &spec[..at];
        }
    }
    if let Some((name, version)) = spec.rsplit_once('/') {
        if version.starts_with(|ch: char| ch.is_ascii_digit()) {
            return name;
        }
    }
    spec
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
        let text = pnpm_text(
            "lockfileVersion: '9.0'\n\
             \n\
             packages:\n\
             \t/tantivy@0.26.0:\n\
             \t\tresolution: {integrity: sha512-SECRET}\n\
             \n\
             \t/@scope/pkg@1.0.0:\n\
             \t\tresolution: {integrity: sha512-OTHER}\n",
        );
        assert!(text.contains("tantivy"), "{text}");
        assert!(text.contains("@scope/pkg"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("OTHER"), "{text}");
        assert!(!text.contains("0.26.0"), "{text}");
        assert!(!text.contains("1.0.0"), "{text}");
        assert!(is_pnpm_name("pnpm-lock.yaml"));
        assert!(!is_pnpm_name("yarn.lock"));
    }
}
