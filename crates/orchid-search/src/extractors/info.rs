//! GNU Info extractor.
//!
//! Node titles and body text are indexed. Tag tables, indirect tables,
//! and underline rules are not.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from GNU Info manuals.
#[derive(Debug, Default, Clone, Copy)]
pub struct InfoExtractor;

#[async_trait]
impl ContentExtractor for InfoExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(is_info_ext)
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(info_text(&decode_best_effort(&raw)))
    }
}

/// `info`, or a split volume such as `info-1`.
pub(crate) fn is_info_ext(extension: &str) -> bool {
    let ext = extension.to_ascii_lowercase();
    ext == "info"
        || ext
            .strip_prefix("info-")
            .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
}

pub(crate) fn info_text(input: &str) -> String {
    let mut out = String::new();
    for chunk in input.split('\u{1f}') {
        let chunk = chunk.trim_matches(|c: char| c == '\u{0c}' || c.is_whitespace());
        if chunk.is_empty() || is_meta_table(chunk) {
            continue;
        }
        let mut lines = chunk.lines();
        if let Some(first) = lines.next() {
            if is_node_header(first) {
                for value in header_values(first) {
                    push_line(&mut out, &value);
                }
            } else {
                push_body_line(&mut out, first);
            }
        }
        for line in lines {
            push_body_line(&mut out, line);
        }
    }
    out.trim().to_string()
}

fn is_meta_table(chunk: &str) -> bool {
    let Some(first) = chunk.lines().find(|line| !line.trim().is_empty()) else {
        return false;
    };
    let first = first.trim();
    if first.eq_ignore_ascii_case("Tag Table:")
        || first.eq_ignore_ascii_case("End Tag Table")
        || first.eq_ignore_ascii_case("Indirect:")
    {
        return true;
    }
    is_node_header(first)
        && header_values(first)
            .iter()
            .any(|value| value.eq_ignore_ascii_case("Tag Table"))
}

fn is_node_header(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("node:")
        && (lower.contains("file:") || lower.contains("up:") || lower.contains("next:"))
}

fn header_values(line: &str) -> Vec<String> {
    let mut values = Vec::new();
    for part in line.split(',') {
        let Some((key, value)) = part.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() || value == "(dir)" {
            continue;
        }
        if matches!(
            key.trim().to_ascii_lowercase().as_str(),
            "node" | "next" | "previous" | "prev" | "up"
        ) {
            values.push(value.to_string());
        }
    }
    values
}

fn push_body_line(out: &mut String, line: &str) {
    if line.contains('\u{7f}') {
        return;
    }
    let line = line.trim();
    if line.is_empty() || is_rule(line) {
        return;
    }
    push_line(out, line);
}

fn is_rule(line: &str) -> bool {
    let mut chars = line.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    matches!(first, '*' | '=' | '-' | '.') && chars.all(|c| c == first) && line.chars().count() >= 4
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
    fn keeps_nodes_and_skips_the_tag_table() {
        let text = info_text(
            "This is orchid.info, produced by makeinfo.\n\
             \u{1f}\n\
             File: orchid.info,  Node: Introduction,  Next: Usage,  Up: Top\n\
             \n\
             Opens files.\n\
             ********\n\
             * Usage:: How to open\n\
             \u{1f}\n\
             Tag Table:\n\
             Node: SECRET\u{7f}12345\n\
             \u{1f}\n\
             End Tag Table\n",
        );
        assert!(text.contains("Introduction"), "{text}");
        assert!(text.contains("Opens files"), "{text}");
        assert!(text.contains("How to open"), "{text}");
        assert!(text.contains("produced by makeinfo"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("12345"), "{text}");
        assert!(!text.contains("****"), "{text}");
        assert!(is_info_ext("info-1"));
        assert!(!is_info_ext("inf"));
    }
}
