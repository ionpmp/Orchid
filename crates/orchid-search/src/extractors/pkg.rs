//! npm and Composer manifest extractor.
//!
//! Names, descriptions, keywords, and dependency names are indexed.
//! Scripts and versions are not. Dispatch lives in
//! [`super::Extractor::extract`] because the extension is plain `.json`.

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_pkg_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "package.json" | "composer.json"
    )
}

pub(crate) fn pkg_text(input: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(input) else {
        return String::new();
    };
    let Some(obj) = value.as_object() else {
        return String::new();
    };
    let mut out = String::new();
    push_str(&mut out, obj.get("name"));
    push_str(&mut out, obj.get("description"));
    push_str(&mut out, obj.get("homepage"));
    push_str(&mut out, obj.get("license"));
    push_list(&mut out, obj.get("keywords"));
    push_list(&mut out, obj.get("license"));
    push_person(&mut out, obj.get("author"));
    push_people(&mut out, obj.get("authors"));
    push_people(&mut out, obj.get("contributors"));
    push_keys(&mut out, obj.get("dependencies"));
    push_keys(&mut out, obj.get("devDependencies"));
    push_keys(&mut out, obj.get("peerDependencies"));
    push_keys(&mut out, obj.get("optionalDependencies"));
    push_keys(&mut out, obj.get("require"));
    push_keys(&mut out, obj.get("require-dev"));
    push_repo(&mut out, obj.get("repository"));
    push_bin(&mut out, obj.get("bin"));
    out.trim().to_string()
}

fn push_str(out: &mut String, value: Option<&serde_json::Value>) {
    if let Some(text) = value.and_then(|value| value.as_str()) {
        push_line(out, text);
    }
}

fn push_list(out: &mut String, value: Option<&serde_json::Value>) {
    let Some(items) = value.and_then(|value| value.as_array()) else {
        return;
    };
    for item in items {
        push_str(out, Some(item));
    }
}

fn push_person(out: &mut String, value: Option<&serde_json::Value>) {
    let Some(value) = value else {
        return;
    };
    if let Some(text) = value.as_str() {
        push_line(out, text);
        return;
    }
    let Some(obj) = value.as_object() else {
        return;
    };
    push_str(out, obj.get("name"));
    push_str(out, obj.get("email"));
}

fn push_people(out: &mut String, value: Option<&serde_json::Value>) {
    let Some(items) = value.and_then(|value| value.as_array()) else {
        return;
    };
    for item in items {
        push_person(out, Some(item));
    }
}

fn push_keys(out: &mut String, value: Option<&serde_json::Value>) {
    let Some(obj) = value.and_then(|value| value.as_object()) else {
        return;
    };
    for key in obj.keys() {
        push_line(out, key);
    }
}

fn push_repo(out: &mut String, value: Option<&serde_json::Value>) {
    let Some(value) = value else {
        return;
    };
    if value.as_str().is_some() {
        push_str(out, Some(value));
        return;
    }
    if let Some(obj) = value.as_object() {
        push_str(out, obj.get("url"));
    }
}

fn push_bin(out: &mut String, value: Option<&serde_json::Value>) {
    let Some(value) = value else {
        return;
    };
    if value.as_str().is_some() {
        push_str(out, Some(value));
        return;
    }
    let Some(obj) = value.as_object() else {
        return;
    };
    for (key, path) in obj {
        push_line(out, key);
        push_str(out, Some(path));
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
    fn indexes_names_and_skips_scripts() {
        let text = pkg_text(
            r#"{
              "name": "orchid",
              "version": "1.2.3",
              "description": "File manager",
              "keywords": ["search"],
              "author": "Ada <ada@example.com>",
              "scripts": { "build": "SECRET" },
              "dependencies": { "tantivy": "0.26.0" }
            }"#,
        );
        assert!(text.contains("orchid"), "{text}");
        assert!(text.contains("File manager"), "{text}");
        assert!(text.contains("search"), "{text}");
        assert!(text.contains("ada@example.com"), "{text}");
        assert!(text.contains("tantivy"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("1.2.3"), "{text}");
        assert!(!text.contains("0.26.0"), "{text}");

        let composer = pkg_text(
            r#"{
              "name": "example/orchid",
              "description": "Player",
              "require": { "guzzlehttp/guzzle": "^7" },
              "scripts": { "test": "SECRET2" }
            }"#,
        );
        assert!(composer.contains("example/orchid"), "{composer}");
        assert!(composer.contains("guzzlehttp/guzzle"), "{composer}");
        assert!(!composer.contains("SECRET2"), "{composer}");
        assert!(pkg_text("{").is_empty());
        assert!(is_pkg_name("package.json"));
        assert!(!is_pkg_name("package-lock.json"));
    }
}
