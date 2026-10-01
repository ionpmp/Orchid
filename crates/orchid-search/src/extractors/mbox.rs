//! Unix mailbox (`.mbox`) text extractor.
//!
//! Each message is split on an envelope `From ` line that follows a blank
//! line (or the start of the file) and indexed with the same rules as `.eml`.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::eml::eml_text;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from mbox mailboxes.
#[derive(Debug, Default, Clone, Copy)]
pub struct MboxExtractor;

#[async_trait]
impl ContentExtractor for MboxExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            base.eq_ignore_ascii_case("application/mbox")
        }) || extension.is_some_and(|e| e.eq_ignore_ascii_case("mbox"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(mbox_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn mbox_text(input: &str) -> String {
    let mut out = String::new();
    for message in split_mbox(input) {
        let text = eml_text(message);
        if text.is_empty() || out.len() >= MAX_CONTENT_BYTES {
            continue;
        }
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        let room = MAX_CONTENT_BYTES.saturating_sub(out.len());
        out.push_str(&text.chars().take(room).collect::<String>());
    }
    out.trim().to_string()
}

/// Split on mbox envelope lines. A `From ` line counts only at the start of
/// the file or after a blank line, so a body sentence is left intact.
fn split_mbox(input: &str) -> Vec<&str> {
    let bytes = input.as_bytes();
    let mut envelope_at = Vec::new();
    let mut i = 0;
    let mut line_start = 0;
    let mut prev_blank = true;
    while i <= bytes.len() {
        if i == bytes.len() || bytes[i] == b'\n' {
            let mut end = i;
            if end > line_start && bytes[end - 1] == b'\r' {
                end -= 1;
            }
            let line = &input[line_start..end];
            if prev_blank && line.starts_with("From ") {
                envelope_at.push(line_start);
            }
            prev_blank = line.is_empty();
            if i == bytes.len() {
                break;
            }
            i += 1;
            line_start = i;
            continue;
        }
        i += 1;
    }
    if envelope_at.is_empty() {
        return vec![input];
    }
    let mut messages = Vec::with_capacity(envelope_at.len());
    for (idx, start) in envelope_at.iter().copied().enumerate() {
        let next = envelope_at.get(idx + 1).copied().unwrap_or(input.len());
        let after_envelope = input[start..]
            .find('\n')
            .map(|n| start + n + 1)
            .unwrap_or(next);
        let begin = after_envelope.min(next);
        messages.push(input[begin..next].trim());
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_each_message_and_keeps_from_inside_a_paragraph() {
        let text = mbox_text(
            "From a@example.com Mon Jan 1 00:00:00 2020\n\
             Subject: First note\n\
             Content-Type: text/plain\n\
             \n\
             Hello one\n\
             From the author continued\n\
             \n\
             From b@example.com Mon Jan 2 00:00:00 2020\n\
             Subject: Second note\n\
             Content-Type: text/plain\n\
             \n\
             Hello two\n",
        );
        assert!(text.contains("First note"), "{text}");
        assert!(text.contains("Hello one"), "{text}");
        assert!(text.contains("From the author continued"), "{text}");
        assert!(text.contains("Second note"), "{text}");
        assert!(text.contains("Hello two"), "{text}");
    }
}
