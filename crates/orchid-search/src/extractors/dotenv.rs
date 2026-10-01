//! dotenv extractor.
//!
//! Variable names are indexed. Values are not. Dispatch lives in
//! [`super::Extractor::extract`].

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn is_dotenv_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == ".env" || lower.starts_with(".env.") || lower.ends_with(".env")
}

pub(crate) fn dotenv_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line).trim();
        let Some((key, _)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().trim_matches(|ch| ch == '"' || ch == '\'');
        if is_key(key) {
            push_line(&mut out, key);
        }
    }
    out.trim().to_string()
}

fn is_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
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
    fn indexes_names_and_skips_values() {
        let text = dotenv_text(
            "API_KEY=SECRET\n\
             export TOKEN=\"OTHER\"\n\
             # PASSWORD=hidden\n\
             DATABASE_URL=postgres://user:pass@localhost/db\n",
        );
        assert!(text.contains("API_KEY"), "{text}");
        assert!(text.contains("TOKEN"), "{text}");
        assert!(text.contains("DATABASE_URL"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("OTHER"), "{text}");
        assert!(!text.contains("hidden"), "{text}");
        assert!(!text.contains("pass"), "{text}");
        assert!(is_dotenv_name(".env"));
        assert!(is_dotenv_name(".env.local"));
        assert!(is_dotenv_name("production.env"));
        assert!(!is_dotenv_name(".envrc"));
        assert!(!is_dotenv_name("notes.txt"));
    }
}
