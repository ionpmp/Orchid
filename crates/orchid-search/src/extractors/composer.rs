//! Composer lockfile extractor.
//!
//! Package names and descriptions are indexed. Versions, dist URLs, and
//! checksums are not. Dispatch lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_composer_lock_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("composer.lock")
}

pub(crate) fn composer_lock_text(input: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(input) else {
        return String::new();
    };
    let Some(obj) = value.as_object() else {
        return String::new();
    };
    let mut out = String::new();
    for key in ["packages", "packages-dev"] {
        let Some(items) = obj.get(key).and_then(|value| value.as_array()) else {
            continue;
        };
        for item in items {
            let Some(pkg) = item.as_object() else {
                continue;
            };
            if let Some(name) = pkg.get("name").and_then(|value| value.as_str()) {
                push_line(&mut out, name);
            }
            if let Some(description) = pkg.get("description").and_then(|value| value.as_str()) {
                push_line(&mut out, description);
            }
        }
    }
    out.trim().to_string()
}

fn push_line(out: &mut String, value: &str) {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
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
    fn indexes_names_and_skips_checksums() {
        let text = composer_lock_text(
            r#"{
              "content-hash": "SECRET",
              "packages": [
                {
                  "name": "guzzlehttp/guzzle",
                  "version": "7.8.0",
                  "description": "HTTP client",
                  "dist": { "shasum": "SECRET2" }
                }
              ],
              "packages-dev": [
                { "name": "phpunit/phpunit", "description": "Test framework" }
              ]
            }"#,
        );
        assert!(text.contains("guzzlehttp/guzzle"), "{text}");
        assert!(text.contains("HTTP client"), "{text}");
        assert!(text.contains("phpunit/phpunit"), "{text}");
        assert!(text.contains("Test framework"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("7.8.0"), "{text}");
        assert!(composer_lock_text("{").is_empty());
        assert!(is_composer_lock_name("composer.lock"));
        assert!(!is_composer_lock_name("composer.json"));
    }
}
