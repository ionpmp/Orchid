//! NuGet lockfile extractor.
//!
//! Package ids are indexed. Versions and content hashes are not.
//! Dispatch lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_nuget_lock_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("packages.lock.json")
}

pub(crate) fn nuget_lock_text(input: &str) -> String {
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
        if is_meta(key) {
            if child.is_string() || child.is_number() || child.is_boolean() {
                continue;
            }
            if key.eq_ignore_ascii_case("dependencies") {
                walk_dependencies(child, out);
            } else {
                walk(child, out);
            }
            continue;
        }
        if is_package_entry(child) {
            push_line(out, key);
        }
        if !(child.is_string() || child.is_number() || child.is_boolean()) {
            walk(child, out);
        }
    }
}

fn walk_dependencies(value: &serde_json::Value, out: &mut String) {
    let Some(map) = value.as_object() else {
        walk(value, out);
        return;
    };
    for (key, child) in map {
        if child.is_string() || is_package_entry(child) {
            push_line(out, key);
        }
        if !(child.is_string() || child.is_number() || child.is_boolean()) {
            walk(child, out);
        }
    }
}

fn is_package_entry(value: &serde_json::Value) -> bool {
    let Some(map) = value.as_object() else {
        return false;
    };
    map.keys().any(|key| {
        matches!(
            key.to_ascii_lowercase().as_str(),
            "contenthash" | "resolved" | "type"
        )
    })
}

fn is_meta(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "version" | "dependencies" | "contenthash" | "resolved" | "requested" | "type"
    )
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
    fn indexes_package_ids_and_skips_hashes() {
        let text = nuget_lock_text(
            r#"{
              "version": 1,
              "dependencies": {
                "net8.0": {
                  "Newtonsoft.Json": {
                    "type": "Direct",
                    "requested": "[13.0.3, )",
                    "resolved": "13.0.3",
                    "contentHash": "SECRET"
                  },
                  "Orchid.Core": {
                    "type": "Transitive",
                    "resolved": "1.2.3",
                    "contentHash": "OTHER",
                    "dependencies": { "Newtonsoft.Json": "13.0.3" }
                  }
                }
              }
            }"#,
        );
        assert!(text.contains("Newtonsoft.Json"), "{text}");
        assert!(text.contains("Orchid.Core"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("OTHER"), "{text}");
        assert!(!text.contains("13.0.3"), "{text}");
        assert!(!text.contains("net8.0"), "{text}");
        assert!(nuget_lock_text("{").is_empty());
        assert!(is_nuget_lock_name("packages.lock.json"));
        assert!(!is_nuget_lock_name("package-lock.json"));
    }
}
