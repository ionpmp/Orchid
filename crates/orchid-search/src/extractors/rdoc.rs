//! RDoc extractor.
//!
//! Headings, lists, and paragraphs are indexed. Rules and `:stopdoc:`
//! blocks are not.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from RDoc files.
#[derive(Debug, Default, Clone, Copy)]
pub struct RdocExtractor;

#[async_trait]
impl ContentExtractor for RdocExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(|ext| ext.eq_ignore_ascii_case("rdoc"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(rdoc_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn rdoc_text(input: &str) -> String {
    let mut out = String::new();
    let mut hidden = false;
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line == ":stopdoc:" || line == ":enddoc:" {
            hidden = true;
            continue;
        }
        if line == ":startdoc:" {
            hidden = false;
            continue;
        }
        if hidden || line.starts_with('#') || is_rule(line) {
            continue;
        }
        if line.starts_with('=') {
            if let Some(title) = heading_text(line) {
                push_line(&mut out, title);
            }
            continue;
        }
        push_line(&mut out, strip_list(line));
    }
    out.trim().to_string()
}

fn heading_text(line: &str) -> Option<&str> {
    let level = line.bytes().take_while(|byte| *byte == b'=').count();
    if !(1..=6).contains(&level) {
        return None;
    }
    let title = line[level..].trim();
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

fn is_rule(line: &str) -> bool {
    let mut chars = line.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    matches!(first, '-' | '_' | '*') && chars.all(|c| c == first) && line.chars().count() >= 3
}

fn strip_list(line: &str) -> &str {
    if let Some(rest) = line.strip_prefix("* ").or_else(|| line.strip_prefix("- ")) {
        return rest.trim();
    }
    if let Some((number, rest)) = line.split_once(". ") {
        if !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()) {
            return rest.trim();
        }
    }
    line
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
    fn keeps_headings_and_skips_stopdoc() {
        let text = rdoc_text(
            "= Orchid\n\
             Opens files.\n\
             ---\n\
             :stopdoc:\n\
             SECRET\n\
             :startdoc:\n\
             == Usage\n\
             * See open.\n",
        );
        assert!(text.contains("Orchid"), "{text}");
        assert!(text.contains("Opens files"), "{text}");
        assert!(text.contains("Usage"), "{text}");
        assert!(text.contains("See open"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("---"), "{text}");
        assert!(!text.contains(":stopdoc:"), "{text}");
    }
}
