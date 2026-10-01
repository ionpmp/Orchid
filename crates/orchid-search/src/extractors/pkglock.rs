//! npm lockfile extractor.
//!
//! Package names are indexed. Versions, resolved URLs, and integrity
//! hashes are not. Dispatch lives in [`super::Extractor::extract`]
//! because the extension is plain `.json`.

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_pkglock_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "package-lock.json" | "npm-shrinkwrap.json"
    )
}

pub(crate) fn pkglock_text(input: &str) -> String {
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
        if key == "name" {
            if let Some(text) = child.as_str() {
                push_line(out, text);
            }
            continue;
        }
        if !is_meta(key) {
            push_line(out, display_name(key));
        }
        walk(child, out);
    }
}

fn is_meta(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "version"
            | "integrity"
            | "resolved"
            | "requires"
            | "lockfileversion"
            | "packages"
            | "dependencies"
            | "devdependencies"
            | "peerdependencies"
            | "optionaldependencies"
            | "license"
            | "funding"
            | "engines"
            | "bin"
            | "cpu"
            | "os"
            | "url"
            | "type"
            | "email"
            | "shasum"
            | "deprecated"
            | "description"
            | "licensefile"
            | "from"
            | "path"
            | "link"
            | "workspaces"
            | "dev"
            | "optional"
            | "bundled"
            | "peer"
            | "extraneous"
            | "inbundle"
            | "devoptional"
            | "hasinstallscript"
            | "overridden"
    )
}

fn display_name(key: &str) -> &str {
    let rest = key.strip_prefix("node_modules/").unwrap_or(key);
    rest.rsplit("node_modules/").next().unwrap_or(rest)
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
        let text = pkglock_text(
            r#"{
              "name": "orchid",
              "version": "1.2.3",
              "lockfileVersion": 3,
              "packages": {
                "": { "name": "orchid" },
                "node_modules/tantivy": {
                  "version": "0.26.0",
                  "integrity": "sha512-SECRET"
                },
                "node_modules/@scope/pkg": {
                  "version": "1.0.0",
                  "integrity": "sha512-OTHER"
                }
              }
            }"#,
        );
        assert!(text.contains("orchid"), "{text}");
        assert!(text.contains("tantivy"), "{text}");
        assert!(text.contains("@scope/pkg"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("OTHER"), "{text}");
        assert!(!text.contains("1.2.3"), "{text}");
        assert!(!text.contains("0.26.0"), "{text}");

        let v1 = pkglock_text(
            r#"{
              "name": "player",
              "dependencies": {
                "left-pad": {
                  "version": "1.0.0",
                  "integrity": "sha1-SECRET2"
                }
              }
            }"#,
        );
        assert!(v1.contains("player"), "{v1}");
        assert!(v1.contains("left-pad"), "{v1}");
        assert!(!v1.contains("SECRET2"), "{v1}");
        assert!(pkglock_text("{").is_empty());
        assert!(is_pkglock_name("package-lock.json"));
        assert!(!is_pkglock_name("package.json"));
    }
}
