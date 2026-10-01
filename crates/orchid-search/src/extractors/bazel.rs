//! Bazel module lockfile extractor.
//!
//! Module and repository names are indexed. Integrity hashes are not.
//! Dispatch lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_bazel_lock_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("MODULE.bazel.lock")
}

pub(crate) fn bazel_lock_text(input: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(input) else {
        return String::new();
    };
    let mut out = String::new();
    if let Some(hashes) = value
        .get("registryFileHashes")
        .and_then(|item| item.as_object())
    {
        for url in hashes.keys() {
            if let Some(name) = module_from_url(url) {
                push_line(&mut out, name);
            }
        }
    }
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
    if let Some(specs) = map
        .get("generatedRepoSpecs")
        .and_then(|item| item.as_object())
    {
        for (name, spec) in specs {
            push_line(out, name);
            let artifact = spec
                .get("attributes")
                .and_then(|item| item.get("artifact"))
                .and_then(|item| item.as_str());
            if let Some(artifact) = artifact {
                if !looks_like_hash(artifact) {
                    push_line(out, artifact);
                }
            }
        }
    }
    for (key, child) in map {
        if key == "registryFileHashes" || key == "bzlTransitiveDigest" {
            continue;
        }
        if !child.is_string() && !child.is_number() && !child.is_boolean() {
            walk(child, out);
        }
    }
}

fn module_from_url(url: &str) -> Option<&str> {
    let rest = url.split_once("/modules/")?.1;
    let name = rest.split('/').next().unwrap_or("");
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn looks_like_hash(value: &str) -> bool {
    value.starts_with("sha256")
        || (value.len() >= 32 && value.chars().all(|ch| ch.is_ascii_hexdigit()))
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
    fn indexes_module_names_and_skips_hashes() {
        let text = bazel_lock_text(
            r#"{
              "lockFileVersion": 10,
              "registryFileHashes": {
                "https://bcr.bazel.build/modules/abseil-cpp/20240116.2/MODULE.bazel": "sha256-SECRET",
                "https://bcr.bazel.build/bazel_registry.json": "sha256-OTHER"
              },
              "moduleExtensions": {
                "//:ext.bzl%maven": {
                  "general": {
                    "bzlTransitiveDigest": "DIGESTSECRET",
                    "generatedRepoSpecs": {
                      "com_google_guava_guava": {
                        "attributes": { "artifact": "com.google.guava:guava:32.1.3-jre" }
                      }
                    }
                  }
                }
              }
            }"#,
        );
        assert!(text.contains("abseil-cpp"), "{text}");
        assert!(text.contains("com_google_guava_guava"), "{text}");
        assert!(text.contains("com.google.guava:guava:32.1.3-jre"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("OTHER"), "{text}");
        assert!(!text.contains("DIGESTSECRET"), "{text}");
        assert!(bazel_lock_text("{").is_empty());
        assert!(is_bazel_lock_name("MODULE.bazel.lock"));
        assert!(!is_bazel_lock_name("MODULE.bazel"));
    }
}
