//! Nix flake lockfile extractor.
//!
//! Node names, owners, repos, and refs are indexed. narHash values and
//! revisions are not. Dispatch lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_flake_lock_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("flake.lock")
}

pub(crate) fn flake_lock_text(input: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(input) else {
        return String::new();
    };
    let mut out = String::new();
    walk(&value, &mut out, "");
    out.trim().to_string()
}

fn walk(value: &serde_json::Value, out: &mut String, parent: &str) {
    if let Some(items) = value.as_array() {
        for item in items {
            walk(item, out, parent);
        }
        return;
    }
    let Some(map) = value.as_object() else {
        return;
    };
    for (key, child) in map {
        if parent == "nodes" {
            push_line(out, key);
        }
        if matches!(key.as_str(), "owner" | "repo" | "ref") {
            if let Some(text) = child.as_str() {
                if !looks_like_hash(text) {
                    push_line(out, text);
                }
            }
        }
        if matches!(key.as_str(), "narHash" | "rev" | "lastModified" | "hash") {
            continue;
        }
        walk(child, out, key);
    }
}

fn looks_like_hash(value: &str) -> bool {
    let value = value.strip_prefix("sha256-").unwrap_or(value);
    value.len() >= 16 && value.chars().all(|ch| ch.is_ascii_hexdigit())
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
    fn indexes_node_names_and_skips_hashes() {
        let text = flake_lock_text(
            r#"{
              "nodes": {
                "nixpkgs": {
                  "locked": {
                    "narHash": "sha256-SECRET",
                    "owner": "NixOS",
                    "repo": "nixpkgs",
                    "rev": "deadbeefcafebabe",
                    "type": "github"
                  },
                  "original": {
                    "owner": "NixOS",
                    "ref": "nixos-unstable",
                    "repo": "nixpkgs",
                    "type": "github"
                  }
                }
              },
              "root": "root",
              "version": 7
            }"#,
        );
        assert!(text.contains("nixpkgs"), "{text}");
        assert!(text.contains("NixOS"), "{text}");
        assert!(text.contains("nixos-unstable"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("deadbeef"), "{text}");
        assert!(!text.contains("github"), "{text}");
        assert!(flake_lock_text("{").is_empty());
        assert!(is_flake_lock_name("flake.lock"));
        assert!(!is_flake_lock_name("flake.nix"));
    }
}
