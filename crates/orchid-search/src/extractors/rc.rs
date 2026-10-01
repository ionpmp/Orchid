//! Windows resource script (`.rc`) extractor.
//!
//! Quoted UI strings are indexed: string tables, dialog captions, and menu
//! items. Comments and preprocessor lines (`#include`) are skipped.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable UI text from `.rc` resource scripts.
#[derive(Debug, Default, Clone, Copy)]
pub struct RcExtractor;

#[async_trait]
impl ContentExtractor for RcExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(|e| e.eq_ignore_ascii_case("rc"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(rc_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn rc_text(input: &str) -> String {
    let cleaned = strip_comments(input);
    let mut out = String::new();
    for raw in cleaned.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        for value in quoted_strings(line) {
            push_line(&mut out, &value);
        }
    }
    out.trim().to_string()
}

fn strip_comments(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
                continue;
            }
            if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            out.push(ch);
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'/') {
            chars.next();
            for next in chars.by_ref() {
                if next == '\n' {
                    out.push('\n');
                    break;
                }
            }
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            while let Some(next) = chars.next() {
                if next == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
                if next == '\n' {
                    out.push('\n');
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn quoted_strings(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '"' {
            continue;
        }
        let mut value = String::new();
        while let Some(next) = chars.next() {
            if next == '\\' {
                match chars.next() {
                    Some('\\') => value.push('\\'),
                    Some('"') => value.push('"'),
                    Some('n') => value.push('\n'),
                    Some('t') => value.push('\t'),
                    Some('r') => value.push('\r'),
                    Some(other) => value.push(other),
                    None => {}
                }
                continue;
            }
            if next == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    value.push('"');
                    continue;
                }
                break;
            }
            value.push(next);
        }
        if !value.is_empty() {
            out.push(value);
        }
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
    fn keeps_ui_strings_and_skips_includes_and_comments() {
        let text = rc_text(
            "#include \"resource.h\"\n\
             // \"SECRET line\"\n\
             /* \"SECRET block\" */\n\
             STRINGTABLE\n\
             BEGIN\n\
             \x20   IDS_TITLE \"Night Drive\"\n\
             \x20   IDS_HELLO \"Hello\"\n\
             END\n\
             IDD_MAIN DIALOG 0, 0, 100, 50\n\
             CAPTION \"Main Window\"\n\
             BEGIN\n\
             \x20   DEFPUSHBUTTON \"OK\", IDOK, 10, 10, 40, 14\n\
             \x20   LTEXT \"Name:\", IDC_STATIC, 10, 30, 20, 8\n\
             END\n\
             IDR_MENU MENU\n\
             BEGIN\n\
             \x20   POPUP \"&File\"\n\
             \x20   BEGIN\n\
             \x20       MENUITEM \"&Open\", ID_OPEN\n\
             \x20       MENUITEM SEPARATOR\n\
             \x20   END\n\
             END\n",
        );
        assert!(text.contains("Night Drive"), "{text}");
        assert!(text.contains("Hello"), "{text}");
        assert!(text.contains("Main Window"), "{text}");
        assert!(text.contains("OK"), "{text}");
        assert!(text.contains("Name:"), "{text}");
        assert!(text.contains("&File"), "{text}");
        assert!(text.contains("&Open"), "{text}");
        assert!(!text.contains("resource.h"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("SEPARATOR"), "{text}");
        assert!(!text.contains("IDS_TITLE"), "{text}");
    }
}
