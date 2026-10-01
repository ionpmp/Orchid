//! npmrc extractor.
//!
//! Registry URLs are indexed. Auth tokens and passwords are not. Dispatch
//! lives in [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_npmrc_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == ".npmrc" || lower.ends_with(".npmrc")
}

pub(crate) fn npmrc_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if is_secret_key(key) {
            continue;
        }
        push_line(&mut out, value.trim().trim_matches('"'));
    }
    out.trim().to_string()
}

fn is_secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.contains("auth")
        || key.contains("password")
        || key.contains("token")
        || key.contains("secret")
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
    fn indexes_registries_and_skips_tokens() {
        let text = npmrc_text(
            "registry=https://registry.npmjs.org/\n\
             @orchid:registry=https://npm.example.com/\n\
             //registry.npmjs.org/:_authToken=SECRET\n\
             ; password=OTHER\n",
        );
        assert!(text.contains("https://registry.npmjs.org/"), "{text}");
        assert!(text.contains("https://npm.example.com/"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("OTHER"), "{text}");
        assert!(is_npmrc_name(".npmrc"));
        assert!(is_npmrc_name("project.npmrc"));
        assert!(!is_npmrc_name("package.json"));
    }
}
