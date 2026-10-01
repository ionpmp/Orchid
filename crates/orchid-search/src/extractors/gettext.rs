//! Gettext catalog extractor.
//!
//! `msgid`, `msgid_plural`, `msgstr`, and `msgctxt` strings are indexed, as
//! are translator comments. Source locations (`#:`) and flags (`#,`) are not.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from `.po` and `.pot` catalogs.
#[derive(Debug, Default, Clone, Copy)]
pub struct GettextExtractor;

#[async_trait]
impl ContentExtractor for GettextExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            base.eq_ignore_ascii_case("text/x-gettext-translation")
                || base.eq_ignore_ascii_case("text/x-po")
                || base.eq_ignore_ascii_case("application/x-gettext")
        }) || extension.is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "po" | "pot"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(po_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn po_text(input: &str) -> String {
    let mut out = String::new();
    let mut pending: Option<String> = None;
    for raw in input.lines() {
        let line = raw.trim();
        if line.starts_with('"') {
            if let Some(buf) = pending.as_mut() {
                buf.push_str(&unescape_quoted(line));
            }
            continue;
        }
        flush(&mut out, pending.take());
        if let Some(note) = comment_text(line) {
            push_line(&mut out, note);
            continue;
        }
        if let Some(value) = field_start(line) {
            pending = Some(value);
        }
    }
    flush(&mut out, pending.take());
    out.trim().to_string()
}

fn flush(out: &mut String, pending: Option<String>) {
    if let Some(value) = pending {
        push_line(out, &value);
    }
}

fn comment_text(line: &str) -> Option<&str> {
    let rest = if let Some(rest) = line.strip_prefix("#.") {
        rest
    } else if let Some(rest) = line.strip_prefix("#|") {
        rest
    } else if let Some(rest) = line.strip_prefix("# ") {
        rest
    } else {
        return None;
    };
    let rest = rest.trim();
    if rest.is_empty() {
        None
    } else {
        Some(rest)
    }
}

fn field_start(line: &str) -> Option<String> {
    let (key, rest) = line.split_once(char::is_whitespace)?;
    if !is_message_key(key) {
        return None;
    }
    Some(unescape_quoted(rest.trim()))
}

fn is_message_key(key: &str) -> bool {
    if matches!(key, "msgid" | "msgid_plural" | "msgstr" | "msgctxt") {
        return true;
    }
    let Some(index) = key.strip_prefix("msgstr[") else {
        return false;
    };
    index
        .strip_suffix(']')
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

fn unescape_quoted(token: &str) -> String {
    let token = token.trim();
    if !token.starts_with('"') {
        return String::new();
    }
    let mut out = String::new();
    let mut chars = token[1..].chars();
    while let Some(ch) = chars.next() {
        if ch == '"' {
            break;
        }
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some(other) => out.push(other),
                None => {}
            }
            continue;
        }
        out.push(ch);
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
    fn indexes_messages_and_skips_locations() {
        let text = po_text(
            "# translator note ALPHA\n\
             #: src/main.rs:10\n\
             #, fuzzy\n\
             msgid \"Hello\"\n\
             msgstr \"Привет\"\n\
             \n\
             msgid \"\"\n\
             \"Line \"\n\
             \"two\"\n\
             msgid_plural \"lines\"\n\
             msgstr[0] \"Линия\"\n\
             msgstr[1] \"Линии\"\n",
        );
        assert!(text.contains("ALPHA"), "{text}");
        assert!(text.contains("Hello"), "{text}");
        assert!(text.contains("Привет"), "{text}");
        assert!(
            text.contains("Line \ntwo") || text.contains("Line two"),
            "{text}"
        );
        assert!(text.contains("lines"), "{text}");
        assert!(text.contains("Линия"), "{text}");
        assert!(text.contains("Линии"), "{text}");
        assert!(!text.contains("src/main.rs"), "{text}");
        assert!(!text.contains("fuzzy"), "{text}");
    }
}
