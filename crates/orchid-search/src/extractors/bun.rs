//! Bun lockfile extractor.
//!
//! Package names are indexed. Versions and integrity hashes are not.
//! Dispatch lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_bun_lock_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("bun.lock")
}

pub(crate) fn bun_lock_text(input: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&loosen(input)) else {
        return String::new();
    };
    let mut out = String::new();
    if let Some(workspaces) = value.get("workspaces").and_then(|item| item.as_object()) {
        for workspace in workspaces.values() {
            if let Some(name) = workspace.get("name").and_then(|item| item.as_str()) {
                push_line(&mut out, name);
            }
            push_dep_keys(workspace, &mut out);
        }
    }
    if let Some(packages) = value.get("packages").and_then(|item| item.as_object()) {
        for (key, package) in packages {
            push_line(&mut out, package_key_name(key));
            let Some(meta) = package.as_array().and_then(|items| items.get(2)) else {
                continue;
            };
            push_dep_keys(meta, &mut out);
        }
    }
    out.trim().to_string()
}

fn push_dep_keys(value: &serde_json::Value, out: &mut String) {
    for field in [
        "dependencies",
        "devDependencies",
        "optionalDependencies",
        "peerDependencies",
    ] {
        let Some(deps) = value.get(field).and_then(|item| item.as_object()) else {
            continue;
        };
        for name in deps.keys() {
            push_line(out, name);
        }
    }
}

fn package_key_name(key: &str) -> &str {
    match key.rfind('@') {
        Some(at) if at > 0 => &key[..at],
        _ => key,
    }
}

fn loosen(input: &str) -> String {
    let mut out = String::new();
    let mut in_string = false;
    let mut escape = false;
    let chars: Vec<char> = input.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if in_string {
            out.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if ch == '"' {
            in_string = true;
            out.push(ch);
            index += 1;
            continue;
        }
        if ch == ',' {
            let mut look = index + 1;
            while look < chars.len() && chars[look].is_whitespace() {
                look += 1;
            }
            if look < chars.len() && (chars[look] == '}' || chars[look] == ']') {
                index += 1;
                continue;
            }
        }
        out.push(ch);
        index += 1;
    }
    out
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
        let text = bun_lock_text(
            r#"{
              "lockfileVersion": 1,
              "workspaces": {
                "": {
                  "name": "myapp",
                  "dependencies": {
                    "orchid": "^1.2.3",
                  },
                },
              },
              "packages": {
                "orchid": ["orchid@1.2.3", "", { "dependencies": { "requests": "^2.0.0" } }, "sha512-SECRET"],
              },
            }"#,
        );
        assert!(text.contains("myapp"), "{text}");
        assert!(text.contains("orchid"), "{text}");
        assert!(text.contains("requests"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("1.2.3"), "{text}");
        assert!(!text.contains("sha512"), "{text}");
        assert!(bun_lock_text("{").is_empty());
        assert!(is_bun_lock_name("bun.lock"));
        assert!(!is_bun_lock_name("bun.lockb"));
    }
}
