//! Manual-page extractor.
//!
//! Section titles and body text are indexed. Comments, `.ig` blocks, and
//! pure formatting requests are skipped. `\-` becomes a hyphen.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from troff and mdoc manual pages.
#[derive(Debug, Default, Clone, Copy)]
pub struct ManExtractor;

#[async_trait]
impl ContentExtractor for ManExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(is_man_ext)
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(man_text(&decode_best_effort(&raw)))
    }
}

/// `man`, `mdoc`, or a section suffix such as `1`, `3pm`, `1m`.
pub(crate) fn is_man_ext(extension: &str) -> bool {
    let ext = extension.to_ascii_lowercase();
    if matches!(ext.as_str(), "man" | "mdoc") {
        return true;
    }
    let mut chars = ext.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    ('1'..='9').contains(&first) && chars.all(|c| c.is_ascii_alphabetic())
}

pub(crate) fn man_text(input: &str) -> String {
    let mut out = String::new();
    let mut ignoring = false;
    for raw in input.lines() {
        let line = raw.trim();
        if ignoring {
            if line == ".." {
                ignoring = false;
            }
            continue;
        }
        if line == ".ig" || line.starts_with(".ig ") {
            ignoring = true;
            continue;
        }
        if is_comment(line) {
            continue;
        }
        if let Some(text) = request_text(line) {
            push_line(&mut out, &text);
            continue;
        }
        if line.starts_with('.') || line.starts_with('\'') {
            continue;
        }
        push_line(&mut out, &clean_escapes(line));
    }
    out.trim().to_string()
}

fn is_comment(line: &str) -> bool {
    let body = line.trim_start_matches(['.', '\'']).trim_start();
    body.starts_with("\\\"")
}

fn request_text(line: &str) -> Option<String> {
    let control = line.strip_prefix('.').or_else(|| line.strip_prefix('\''))?;
    let control = control.trim_start();
    if control.is_empty() || control.starts_with("\\\"") {
        return None;
    }
    let (name, rest) = split_token(control);
    if is_structural(name) {
        return None;
    }
    let drop_section = name.eq_ignore_ascii_case("TH") || name.eq_ignore_ascii_case("Dt");
    let mut words = Vec::new();
    for arg in parse_args(rest) {
        if drop_section && is_section_number(&arg) {
            continue;
        }
        let cleaned = clean_escapes(&arg);
        if !cleaned.is_empty() {
            words.push(cleaned);
        }
    }
    if words.is_empty() {
        None
    } else {
        Some(words.join(" "))
    }
}

fn is_structural(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "br" | "sp"
            | "nf"
            | "fi"
            | "ft"
            | "ps"
            | "vs"
            | "in"
            | "ti"
            | "ll"
            | "po"
            | "ad"
            | "na"
            | "hy"
            | "nh"
            | "rs"
            | "re"
            | "pp"
            | "p"
            | "lp"
            | "tp"
            | "pd"
            | "de"
            | "ds"
            | "nr"
            | "so"
            | "fam"
            | "ftr"
    )
}

fn is_section_number(arg: &str) -> bool {
    matches!(arg, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
}

fn split_token(input: &str) -> (&str, &str) {
    let input = input.trim_start();
    let end = input.find(char::is_whitespace).unwrap_or(input.len());
    let (name, rest) = input.split_at(end);
    (name, rest.trim_start())
}

fn parse_args(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_whitespace() {
            continue;
        }
        if ch == '"' {
            let mut value = String::new();
            for next in chars.by_ref() {
                if next == '"' {
                    break;
                }
                value.push(next);
            }
            out.push(value);
            continue;
        }
        let mut value = String::from(ch);
        while chars.peek().is_some_and(|c| !c.is_whitespace()) {
            value.push(chars.next().unwrap_or_default());
        }
        out.push(value);
    }
    out
}

fn clean_escapes(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('-') => out.push('-'),
            Some('\\') | Some('e') => out.push('\\'),
            Some(' ' | '0' | '|' | '^' | '&' | '%' | '{' | '}' | '"') => {}
            Some('f') => {
                if chars.peek() == Some(&'(') {
                    chars.next();
                    chars.next();
                    chars.next();
                } else {
                    chars.next();
                }
            }
            Some('(') => {
                chars.next();
                chars.next();
            }
            Some('*') => {
                if chars.peek() == Some(&'(') {
                    chars.next();
                    chars.next();
                    chars.next();
                } else {
                    chars.next();
                }
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
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
    fn keeps_titles_and_body_and_skips_comments() {
        let text = man_text(
            ".\\\" SECRET\n\
             .TH orchid 1 \"October 2026\" \"Orchid Manual\"\n\
             .SH NAME\n\
             orchid \\- desktop shell\n\
             .SH DESCRIPTION\n\
             Opens files.\n\
             .B bold word\n\
             .ig\n\
             SECRET2\n\
             ..\n",
        );
        assert!(text.contains("orchid"), "{text}");
        assert!(text.contains("October 2026"), "{text}");
        assert!(text.contains("Orchid Manual"), "{text}");
        assert!(text.contains("NAME"), "{text}");
        assert!(text.contains("orchid - desktop shell"), "{text}");
        assert!(text.contains("Opens files"), "{text}");
        assert!(text.contains("bold word"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.split_whitespace().any(|word| word == "1"), "{text}");
    }

    #[test]
    fn section_suffix_is_a_man_page() {
        assert!(is_man_ext("1"));
        assert!(is_man_ext("3pm"));
        assert!(is_man_ext("man"));
        assert!(!is_man_ext("10"));
        assert!(!is_man_ext("rs"));
    }
}
