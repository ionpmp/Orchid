//! `.orchid` Clean-Text extractor for Tantivy ingest.

use async_trait::async_trait;
use orchid_format::{SealedFile, MIME_TYPE};

use crate::error::{Result, SearchError};
use crate::extractors::text::MAX_CONTENT_BYTES;
use crate::extractors::ContentExtractor;

/// Extract Clean-Text from sealed `.orchid` files.
#[derive(Debug, Default, Clone, Copy)]
pub struct OrchidExtractor;

#[async_trait]
impl ContentExtractor for OrchidExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.map(|m| m == MIME_TYPE).unwrap_or(false)
            || extension
                .map(|e| e.eq_ignore_ascii_case("orchid"))
                .unwrap_or(false)
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
            return tokio::task::spawn_blocking(move || extract_local(&os_path))
                .await
                .map_err(|e| SearchError::Extraction {
                    path: path_for_err,
                    reason: format!("join: {e}"),
                })?;
        }
        let bytes = provider.read(path).await.map_err(SearchError::from)?;
        let path_for_err = path_str;
        tokio::task::spawn_blocking(move || extract_bytes(&bytes))
            .await
            .map_err(|e| SearchError::Extraction {
                path: path_for_err,
                reason: format!("join: {e}"),
            })?
    }
}

fn extract_local(path: &std::path::Path) -> Result<String> {
    let file = SealedFile::open(path).map_err(|e| SearchError::Extraction {
        path: path.display().to_string(),
        reason: e.to_string(),
    })?;
    clean_text_truncated(&file)
}

fn extract_bytes(bytes: &[u8]) -> Result<String> {
    // SealedFile is mmap-oriented; for remote bytes write a temp and open.
    let dir = tempfile::tempdir().map_err(SearchError::from)?;
    let path = dir.path().join("remote.orchid");
    std::fs::write(&path, bytes).map_err(SearchError::from)?;
    extract_local(&path)
}

/// Document-level vector from a sealed `.orchid` Embedding region, if any.
#[must_use]
pub fn document_vector_local(path: &std::path::Path) -> Option<Vec<f32>> {
    let file = SealedFile::open(path).ok()?;
    file.embeddings(None)
        .ok()?
        .document_vector()
        .map(|v| v.to_vec())
}

fn clean_text_truncated(file: &SealedFile) -> Result<String> {
    let raw = file.clean_text(None).map_err(|e| SearchError::Extraction {
        path: String::new(),
        reason: e.to_string(),
    })?;
    let slice = if raw.len() > MAX_CONTENT_BYTES {
        &raw[..MAX_CONTENT_BYTES]
    } else {
        &raw[..]
    };
    Ok(String::from_utf8_lossy(slice).into_owned())
}
