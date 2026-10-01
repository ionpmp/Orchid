//! Swift Package Manager resolved-file extractor.
//!
//! Package identities and repository URLs are indexed. Revisions and
//! versions are not. Dispatch lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_resolved_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("Package.resolved")
}

pub(crate) fn resolved_text(input: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(input) else {
        return String::new();
    };
    let mut out = String::new();
    walk(&value, &mut out);
    out.trim().to_string()
}

fn walk(value: &serde_json::Value, out: &mut String) {
    if let Some(items) = value.as_array() {
        for item in items {
            walk(item, out);
        }
        return;
    }
    let Some(map) = value.as_object() else {
        return;
    };
    for (key, child) in map {
        if matches!(key.as_str(), "revision" | "version") {
            continue;
        }
        if matches!(
            key.as_str(),
            "identity" | "package" | "location" | "repositoryURL" | "repositoryUrl"
        ) {
            if let Some(text) = child.as_str() {
                push_line(out, text);
            }
        }
        if !child.is_string() && !child.is_number() && !child.is_boolean() {
            walk(child, out);
        }
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
    fn indexes_identities_and_skips_revisions() {
        let text = resolved_text(
            r#"{
              "pins": [
                {
                  "identity": "orchid",
                  "location": "https://github.com/example/orchid.git",
                  "state": { "revision": "deadbeefcafebabe", "version": "1.2.3" }
                }
              ],
              "version": 2
            }"#,
        );
        assert!(text.contains("orchid"), "{text}");
        assert!(
            text.contains("https://github.com/example/orchid.git"),
            "{text}"
        );
        assert!(!text.contains("deadbeef"), "{text}");
        assert!(!text.contains("1.2.3"), "{text}");
        let legacy = resolved_text(
            r#"{
              "object": {
                "pins": [
                  {
                    "package": "Alamofire",
                    "repositoryURL": "https://github.com/Alamofire/Alamofire.git",
                    "state": { "revision": "SECRET", "version": "5.8.0" }
                  }
                ]
              },
              "version": 1
            }"#,
        );
        assert!(legacy.contains("Alamofire"), "{legacy}");
        assert!(!legacy.contains("SECRET"), "{legacy}");
        assert!(resolved_text("{").is_empty());
        assert!(is_resolved_name("Package.resolved"));
        assert!(!is_resolved_name("Package.swift"));
    }
}
