//! PDF content extractor backed by `pdfium-render`.
//!
//! This extractor needs the pdfium shared library available at runtime. When
//! pdfium is not on the load path the extractor returns
//! [`crate::SearchError::Extraction`] at first use and the indexer logs a
//! warning but otherwise continues.

use async_trait::async_trait;

use crate::error::{Result, SearchError};
use crate::extractors::ContentExtractor;
use crate::extractors::text::MAX_CONTENT_BYTES;

/// Extract text from PDF pages via pdfium.
#[derive(Debug, Default, Clone, Copy)]
pub struct PdfExtractor;

#[async_trait]
impl ContentExtractor for PdfExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.map(|m| m == "application/pdf").unwrap_or(false)
            || extension.map(|e| e.eq_ignore_ascii_case("pdf")).unwrap_or(false)
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let bytes = provider.read(path).await?;
        let path_str = path.to_string();
        tokio::task::spawn_blocking(move || extract_sync(&bytes, &path_str))
            .await
            .map_err(|e| SearchError::Extraction {
                path: String::new(),
                reason: format!("join: {e}"),
            })?
    }
}

fn extract_sync(bytes: &[u8], path: &str) -> Result<String> {
    use pdfium_render::prelude::*;

    let pdfium = match Pdfium::bind_to_system_library() {
        Ok(bindings) => Pdfium::new(bindings),
        Err(e) => {
            return Err(SearchError::Extraction {
                path: path.to_string(),
                reason: format!("pdfium library not available: {e}"),
            });
        }
    };
    let document = pdfium
        .load_pdf_from_byte_slice(bytes, None)
        .map_err(|e| SearchError::Extraction {
            path: path.to_string(),
            reason: format!("load: {e}"),
        })?;
    let mut out = String::new();
    for page in document.pages().iter() {
        let text = page
            .text()
            .map_err(|e| SearchError::Extraction {
                path: path.to_string(),
                reason: format!("page text: {e}"),
            })?
            .all();
        out.push_str(&text);
        out.push_str("\n\n");
        if out.len() >= MAX_CONTENT_BYTES {
            out.truncate(MAX_CONTENT_BYTES);
            break;
        }
    }
    Ok(out)
}
