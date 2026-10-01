//! HTML / XHTML text extractor.
//!
//! Tags, scripts, and styles are dropped so the index stores visible words.
//! Registered ahead of the plain-text extractor, which would otherwise keep
//! the raw markup for `.html` files.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::epub_odf::html_text;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract visible text from HTML and XHTML files.
#[derive(Debug, Default, Clone, Copy)]
pub struct HtmlTextExtractor;

#[async_trait]
impl ContentExtractor for HtmlTextExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            base.eq_ignore_ascii_case("text/html")
                || base.eq_ignore_ascii_case("application/xhtml+xml")
        }) || extension.is_some_and(|e| {
            e.eq_ignore_ascii_case("html")
                || e.eq_ignore_ascii_case("htm")
                || e.eq_ignore_ascii_case("xhtml")
        })
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(html_text(&decode_best_effort(&raw)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_markup_and_script() {
        let text = html_text(
            "<html><body><h1>Hello</h1><script>secret()</script><p>page</p></body></html>",
        );
        assert!(text.contains("Hello"));
        assert!(text.contains("page"));
        assert!(!text.contains("secret"));
        assert!(!text.contains("<h1>"));
    }
}
