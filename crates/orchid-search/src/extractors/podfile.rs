//! CocoaPods lockfile extractor.
//!
//! Pod names are indexed. Versions, commits, and checksums are not.
//! Dispatch lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_podfile_lock_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("Podfile.lock")
}

pub(crate) fn podfile_lock_text(input: &str) -> String {
    let mut out = String::new();
    let mut mode = Mode::Skip;
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let header = !raw.starts_with(|ch: char| ch.is_whitespace()) && !line.starts_with('-');
        if header {
            let title = line.split(':').next().unwrap_or(line).trim();
            mode = match title {
                "PODS" => Mode::Pods,
                "DEPENDENCIES" => Mode::Deps,
                "SPEC REPOS" => Mode::Repos,
                "EXTERNAL SOURCES" => Mode::External,
                _ => Mode::Skip,
            };
            continue;
        }
        match mode {
            Mode::Pods | Mode::Deps => {
                if let Some(name) = list_name(line) {
                    push_line(&mut out, name);
                }
            }
            Mode::Repos => {
                if line.ends_with(':') {
                    continue;
                }
                if let Some(name) = list_name(line) {
                    push_line(&mut out, name);
                }
            }
            Mode::External => {
                if line.starts_with(':') {
                    continue;
                }
                if let Some(name) = line.strip_suffix(':') {
                    push_line(&mut out, name.trim());
                }
            }
            Mode::Skip => {}
        }
    }
    out.trim().to_string()
}

enum Mode {
    Skip,
    Pods,
    Deps,
    Repos,
    External,
}

fn list_name(line: &str) -> Option<&str> {
    let line = line.trim_start_matches('-').trim();
    let name = line.split([' ', '(']).next().unwrap_or("");
    let name = name.trim_end_matches(':');
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
    fn indexes_pod_names_and_skips_checksums() {
        let text = podfile_lock_text(
            r#"
PODS:
  - Orchid (1.2.3)
  - Alamofire (5.8.0):
    - Orchid

DEPENDENCIES:
  - Orchid (~> 1.0)

SPEC REPOS:
  trunk:
    - Orchid

EXTERNAL SOURCES:
  Orchid:
    :commit: deadbeefcafebabe

SPEC CHECKSUMS:
  Alamofire: SECRET
  Orchid: OTHER

PODFILE CHECKSUM: MORESECRET

COCOAPODS: 1.14.3
"#,
        );
        assert!(text.contains("Orchid"), "{text}");
        assert!(text.contains("Alamofire"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("OTHER"), "{text}");
        assert!(!text.contains("MORESECRET"), "{text}");
        assert!(!text.contains("deadbeef"), "{text}");
        assert!(!text.contains("1.2.3"), "{text}");
        assert!(!text.contains("1.14.3"), "{text}");
        assert!(is_podfile_lock_name("Podfile.lock"));
        assert!(!is_podfile_lock_name("Podfile"));
    }
}
