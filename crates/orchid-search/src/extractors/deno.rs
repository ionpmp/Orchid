//! Deno lockfile extractor.
//!
//! Package names and remote module URLs are indexed. Integrity hashes and
//! revisions are not. Dispatch lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_deno_lock_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("deno.lock")
}

pub(crate) fn deno_lock_text(input: &str) -> String {
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
            if let Some(spec) = item.as_str() {
                push_line(out, pkg_name(spec));
            } else {
                walk(item, out);
            }
        }
        return;
    }
    let Some(map) = value.as_object() else {
        return;
    };
    for (key, child) in map {
        if key == "remote" {
            if let Some(urls) = child.as_object() {
                for url in urls.keys() {
                    push_line(out, url);
                }
            }
            continue;
        }
        if matches!(key.as_str(), "npm" | "jsr" | "specifiers") {
            walk_specs(child, out);
            continue;
        }
        if matches!(key.as_str(), "integrity" | "version") {
            continue;
        }
        if !child.is_string() && !child.is_number() && !child.is_boolean() {
            walk(child, out);
        }
    }
}

fn walk_specs(value: &serde_json::Value, out: &mut String) {
    let Some(map) = value.as_object() else {
        walk(value, out);
        return;
    };
    for (key, child) in map {
        push_line(out, pkg_name(key));
        let Some(spec) = child.as_object() else {
            continue;
        };
        if let Some(deps) = spec.get("dependencies").and_then(|item| item.as_array()) {
            for dep in deps {
                if let Some(name) = dep.as_str() {
                    push_line(out, pkg_name(name));
                }
            }
        }
    }
}

fn pkg_name(spec: &str) -> &str {
    let spec = spec
        .strip_prefix("npm:")
        .or_else(|| spec.strip_prefix("jsr:"))
        .unwrap_or(spec);
    match spec.rfind('@') {
        Some(at) if at > 0 => &spec[..at],
        _ => spec,
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
    fn indexes_package_names_and_skips_integrity() {
        let text = deno_lock_text(
            r#"{
              "version": "4",
              "specifiers": { "npm:orchid@^1.2.3": "1.2.3" },
              "npm": {
                "orchid@1.2.3": {
                  "integrity": "sha512-SECRET",
                  "dependencies": ["requests@2.31.0"]
                }
              },
              "remote": {
                "https://deno.land/std/mod.ts": "deadbeefcafebabe"
              }
            }"#,
        );
        assert!(text.contains("orchid"), "{text}");
        assert!(text.contains("requests"), "{text}");
        assert!(text.contains("https://deno.land/std/mod.ts"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("deadbeef"), "{text}");
        assert!(!text.contains("1.2.3"), "{text}");
        assert!(!text.contains("2.31.0"), "{text}");
        assert!(deno_lock_text("{").is_empty());
        assert!(is_deno_lock_name("deno.lock"));
        assert!(!is_deno_lock_name("deno.json"));
    }
}
