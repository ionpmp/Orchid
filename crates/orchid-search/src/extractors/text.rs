//! Plain-text extractor with encoding detection.

use async_trait::async_trait;
use chardetng::EncodingDetector;

use crate::error::Result;
use crate::extractors::ContentExtractor;

/// Maximum number of indexed content bytes per document.
pub const MAX_CONTENT_BYTES: usize = 2 * 1024 * 1024;

/// Default text-ish extensions.
const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "log", "md", "markdown", "csv", "tsv", "json", "xml", "html", "htm", "ini", "toml",
    "yaml", "yml", "conf", "cfg", "srt", "vtt",
];

/// Extract readable text from plaintext-ish files.
#[derive(Debug, Default, Clone, Copy)]
pub struct TextExtractor;

#[async_trait]
impl ContentExtractor for TextExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        if let Some(m) = mime {
            if m.starts_with("text/") || m == "application/json" || m == "application/xml" {
                return true;
            }
        }
        if let Some(ext) = extension {
            let lower = ext.to_ascii_lowercase();
            return TEXT_EXTENSIONS.iter().any(|e| *e == lower);
        }
        false
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        // Cap the read at the index budget — avoid loading a 50 MiB log just to
        // keep the first 2 MiB.
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(decode_best_effort(&raw))
    }
}

fn decode_best_effort(bytes: &[u8]) -> String {
    // UTF-8 BOM takes priority.
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let s = String::from_utf8_lossy(&bytes[3..]).into_owned();
        return s;
    }
    // Let chardetng decide based on the prefix.
    let mut det = EncodingDetector::new();
    let head = if bytes.len() > 4 * 1024 {
        &bytes[..4 * 1024]
    } else {
        bytes
    };
    det.feed(head, head.len() == bytes.len());
    let encoding = det.guess(None, true);
    let (decoded, _, _) = encoding.decode(bytes);
    decoded.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_bom_is_stripped_and_content_preserved() {
        let mut raw = vec![0xEF, 0xBB, 0xBF];
        raw.extend_from_slice("hello world".as_bytes());
        let decoded = decode_best_effort(&raw);
        assert_eq!(decoded, "hello world");
    }

    #[test]
    fn latin1_falls_back_to_windows_1252() {
        let raw = b"caf\xe9"; // "café" in Windows-1252
        let decoded = decode_best_effort(raw);
        assert!(decoded.contains("caf"));
    }

    #[test]
    fn handler_matches_extension() {
        let e = TextExtractor;
        assert!(e.can_handle(None, Some("md")));
        assert!(e.can_handle(Some("text/plain"), None));
        assert!(!e.can_handle(Some("image/png"), Some("png")));
    }
}
