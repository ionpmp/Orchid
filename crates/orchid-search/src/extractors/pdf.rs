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

/// Soft ceiling for mapping a PDF during search indexing (keeps RSS bounded).
/// Content extracted into the index is still capped at [`MAX_CONTENT_BYTES`].
pub const MAX_PDF_MMAP_BYTES: u64 = 64 * 1024 * 1024;

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
        let path_str = path.to_string();
        if path.is_local() {
            let os_path = path.to_local()?;
            let path_for_err = path_str.clone();
            return tokio::task::spawn_blocking(move || extract_local_mmap(&os_path, &path_str))
                .await
                .map_err(|e| SearchError::Extraction {
                    path: path_for_err,
                    reason: format!("join: {e}"),
                })?;
        }
        let bytes = provider.read(path).await?;
        let path_for_err = path_str.clone();
        tokio::task::spawn_blocking(move || extract_sync(&bytes, &path_str))
            .await
            .map_err(|e| SearchError::Extraction {
                path: path_for_err,
                reason: format!("join: {e}"),
            })?
    }
}

fn extract_local_mmap(os_path: &std::path::Path, path: &str) -> Result<String> {
    let meta = std::fs::metadata(os_path)?;
    if meta.len() > MAX_PDF_MMAP_BYTES {
        return Err(SearchError::Extraction {
            path: path.to_string(),
            reason: format!(
                "PDF too large to index ({} bytes > {} byte limit)",
                meta.len(),
                MAX_PDF_MMAP_BYTES
            ),
        });
    }
    let file = std::fs::File::open(os_path)?;
    // SAFETY: read-only mapping; extract finishes before the map is dropped.
    // Concurrent writers may change bytes while mapped — pdfium may then fail.
    let map = unsafe { memmap2::Mmap::map(&file)? };
    extract_sync(&map, path)
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
