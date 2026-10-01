//! Pipenv lockfile extractor.
//!
//! Package names are indexed. Versions and hashes are not. Dispatch
//! lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_pipfile_lock_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("Pipfile.lock")
}

pub(crate) fn pipfile_lock_text(input: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(input) else {
        return String::new();
    };
    let Some(obj) = value.as_object() else {
        return String::new();
    };
    let mut out = String::new();
    for section in ["default", "develop"] {
        let Some(packages) = obj.get(section).and_then(|value| value.as_object()) else {
            continue;
        };
        for name in packages.keys() {
            push_line(&mut out, name);
        }
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
        let text = pipfile_lock_text(
            r#"{
              "_meta": { "hash": { "sha256": "SECRET" } },
              "default": {
                "requests": { "version": "==2.31.0", "hashes": ["sha256:SECRET2"] }
              },
              "develop": {
                "pytest": { "version": "==7.4.0", "hashes": ["sha256:OTHER"] }
              }
            }"#,
        );
        assert!(text.contains("requests"), "{text}");
        assert!(text.contains("pytest"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("OTHER"), "{text}");
        assert!(!text.contains("2.31.0"), "{text}");
        assert!(pipfile_lock_text("{").is_empty());
        assert!(is_pipfile_lock_name("Pipfile.lock"));
        assert!(!is_pipfile_lock_name("Pipfile"));
    }
}
