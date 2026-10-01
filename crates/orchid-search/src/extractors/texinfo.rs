//! Texinfo extractor.
//!
//! Chapter titles and body text are indexed. `@ignore` and `@macro` blocks
//! and `@c` comments are not. `@code{...}` keeps the inner text.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from Texinfo manuals.
#[derive(Debug, Default, Clone, Copy)]
pub struct TexinfoExtractor;

#[async_trait]
impl ContentExtractor for TexinfoExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension
            .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "texi" | "texinfo" | "txi"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(texinfo_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn texinfo_text(input: &str) -> String {
    let mut out = String::new();
    let mut skip_until: Option<String> = None;
    for raw in input.lines() {
        let line = raw.trim();
        if let Some(block) = skip_until.as_deref() {
            if is_end(line, block) {
                skip_until = None;
            }
            continue;
        }
        if is_comment(line) {
            continue;
        }
        if let Some(block) = skipped_block(line) {
            skip_until = Some(block);
            continue;
        }
        push_line(&mut out, &expand(line));
    }
    out.trim().to_string()
}

fn is_comment(line: &str) -> bool {
    line == "@c" || line.starts_with("@c ") || line == "@comment" || line.starts_with("@comment ")
}

fn skipped_block(line: &str) -> Option<String> {
    let name = line_command(line)?;
    if matches!(name.to_ascii_lowercase().as_str(), "ignore" | "macro") {
        Some(name.to_ascii_lowercase())
    } else {
        None
    }
}

fn is_end(line: &str, block: &str) -> bool {
    line.strip_prefix("@end")
        .is_some_and(|rest| rest.trim().eq_ignore_ascii_case(block))
}

fn line_command(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('@')?;
    let end = rest
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .unwrap_or(rest.len());
    let name = &rest[..end];
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn expand(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '@' {
            out.push(ch);
            continue;
        }
        let Some(next) = chars.next() else {
            break;
        };
        if matches!(next, '@' | '{' | '}') {
            out.push(next);
            continue;
        }
        let mut name = String::from(next);
        while chars
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '-')
        {
            name.push(chars.next().unwrap_or_default());
        }
        if chars.peek() == Some(&'{') {
            chars.next();
            let mut depth = 1i32;
            let mut inner = String::new();
            for inner_ch in chars.by_ref() {
                if inner_ch == '{' {
                    depth += 1;
                    inner.push(inner_ch);
                    continue;
                }
                if inner_ch == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    inner.push(inner_ch);
                    continue;
                }
                inner.push(inner_ch);
            }
            out.push_str(&expand(&inner));
            continue;
        }
        if chars.peek() == Some(&' ') {
            chars.next();
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
    fn keeps_titles_and_code_and_skips_ignore() {
        let text = texinfo_text(
            "@settitle Orchid Manual\n\
             @chapter Introduction\n\
             Opens files with @code{open}.\n\
             @c SECRET\n\
             @ignore\n\
             SECRET2\n\
             @end ignore\n",
        );
        assert!(text.contains("Orchid Manual"), "{text}");
        assert!(text.contains("Introduction"), "{text}");
        assert!(text.contains("Opens files with open"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("@code"), "{text}");
    }
}
