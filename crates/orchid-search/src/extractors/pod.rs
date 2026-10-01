//! Perl POD extractor.
//!
//! Headings, items, and paragraphs are indexed. `=cut` code sections and
//! `=begin comment` blocks are not.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from Perl POD.
#[derive(Debug, Default, Clone, Copy)]
pub struct PodExtractor;

#[async_trait]
impl ContentExtractor for PodExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(|e| e.eq_ignore_ascii_case("pod"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(pod_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn pod_text(input: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    let mut comment_end: Option<String> = None;
    for raw in input.lines() {
        if let Some(end_name) = comment_end.as_deref() {
            if let Some((name, _)) = pod_command(raw.trim()) {
                if name.eq_ignore_ascii_case("end") || name.eq_ignore_ascii_case(end_name) {
                    comment_end = None;
                }
            }
            continue;
        }
        if let Some((name, args)) = pod_command(raw.trim()) {
            in_code = false;
            if name.eq_ignore_ascii_case("cut") {
                in_code = true;
                continue;
            }
            if name.eq_ignore_ascii_case("begin") && args.eq_ignore_ascii_case("comment") {
                comment_end = Some("comment".to_string());
                continue;
            }
            if name.eq_ignore_ascii_case("for") && args.to_ascii_lowercase().starts_with("comment")
            {
                continue;
            }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "pod" | "over" | "back" | "encoding" | "begin" | "end"
            ) {
                continue;
            }
            push_line(&mut out, &strip_codes(args));
            continue;
        }
        if in_code {
            continue;
        }
        push_line(&mut out, &strip_codes(raw.trim()));
    }
    out.trim().to_string()
}

fn pod_command(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix('=')?;
    if rest.starts_with('=') {
        return None;
    }
    let (name, args) = split_token(rest);
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some((name, args))
}

fn split_token(input: &str) -> (&str, &str) {
    let end = input.find(char::is_whitespace).unwrap_or(input.len());
    let (name, rest) = input.split_at(end);
    (name, rest.trim_start())
}

fn strip_codes(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_ascii_alphabetic() && chars.peek() == Some(&'<') {
            chars.next();
            let mut inner = String::new();
            for next in chars.by_ref() {
                if next == '>' {
                    break;
                }
                inner.push(next);
            }
            if !out.is_empty() && !out.ends_with(|c: char| c.is_whitespace()) {
                out.push(' ');
            }
            out.push_str(&inner.replace('|', " "));
            continue;
        }
        out.push(ch);
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
    fn keeps_headings_and_skips_code_and_comments() {
        let text = pod_text(
            "=head1 NAME\n\
             \n\
             orchid - desktop shell\n\
             \n\
             Use C<open> or B<bold>.\n\
             \n\
             =item Hello\n\
             \n\
             World\n\
             \n\
             =begin comment\n\
             \n\
             SECRET\n\
             \n\
             =end\n\
             \n\
             =cut\n\
             \n\
             not indexed\n",
        );
        assert!(text.contains("NAME"), "{text}");
        assert!(text.contains("orchid - desktop shell"), "{text}");
        assert!(text.contains("open"), "{text}");
        assert!(text.contains("bold"), "{text}");
        assert!(text.contains("Hello"), "{text}");
        assert!(text.contains("World"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("not indexed"), "{text}");
        assert!(!text.contains("C<"), "{text}");
    }
}
