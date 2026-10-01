//! Debian control extractor.
//!
//! Package names, descriptions, and relationships are indexed. File lists
//! and checksum paragraphs are not.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from Debian `.dsc` and `.changes` files.
#[derive(Debug, Default, Clone, Copy)]
pub struct DebianExtractor;

#[async_trait]
impl ContentExtractor for DebianExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "dsc" | "changes"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(debian_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn debian_text(input: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;
    for raw in input.lines() {
        if raw.trim().is_empty() {
            skipping = false;
            continue;
        }
        if raw.starts_with([' ', '\t']) {
            if !skipping {
                push_line(&mut out, raw.trim());
            }
            continue;
        }
        if raw.starts_with('#') {
            continue;
        }
        let Some((key, value)) = raw.split_once(':') else {
            skipping = false;
            continue;
        };
        if is_skipped_field(key) {
            skipping = true;
            continue;
        }
        skipping = false;
        push_line(&mut out, value.trim());
    }
    out.trim().to_string()
}

fn is_skipped_field(key: &str) -> bool {
    matches!(
        key.trim().to_ascii_lowercase().as_str(),
        "files" | "checksums-sha1" | "checksums-sha256" | "checksums-sha512" | "checksums-md5"
    )
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
    fn indexes_descriptions_and_skips_checksums() {
        let text = debian_text(
            "Source: orchid\n\
             Maintainer: Ada <ada@example.com>\n\
             # SECRET\n\
             Checksums-Sha256:\n \
             deadbeef123 456 orchid.dsc\n\
             Files:\n \
             abcdef 10 utils orchid_1.2.3.dsc\n\
             Description: File manager\n \
             Opens local folders.\n",
        );
        assert!(text.contains("orchid"), "{text}");
        assert!(text.contains("ada@example.com"), "{text}");
        assert!(text.contains("File manager"), "{text}");
        assert!(text.contains("Opens local folders"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("deadbeef"), "{text}");
        assert!(!text.contains("abcdef"), "{text}");
    }
}
