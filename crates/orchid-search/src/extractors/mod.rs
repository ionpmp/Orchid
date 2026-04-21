//! Content extraction dispatch.
//!
//! The palette of extractors is pluggable: each implements
//! [`ContentExtractor`] and reports which MIME / extension combinations it
//! handles. [`Extractor`] picks one per file.

pub mod pdf;
pub mod text;

use std::sync::Arc;

use async_trait::async_trait;

use crate::error::Result;

/// Per-format content extractor.
#[async_trait]
pub trait ContentExtractor: Send + Sync {
    /// Does this extractor cover the given MIME / extension?
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool;

    /// Extract textual content. May truncate to avoid huge strings.
    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String>;
}

/// Routes files to the first matching [`ContentExtractor`].
#[derive(Clone)]
pub struct Extractor {
    extractors: Vec<Arc<dyn ContentExtractor>>,
}

impl Default for Extractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor {
    /// Build a dispatcher with the built-in text extractor. PDF extraction
    /// requires pdfium at runtime and is opt-in via [`Extractor::with_pdf`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            extractors: vec![Arc::new(text::TextExtractor)],
        }
    }

    /// Plug in an extra extractor at the end of the chain.
    #[must_use]
    pub fn with(mut self, e: Arc<dyn ContentExtractor>) -> Self {
        self.extractors.push(e);
        self
    }

    /// Convenience: enable the PDF extractor.
    #[must_use]
    pub fn with_pdf(self) -> Self {
        self.with(Arc::new(pdf::PdfExtractor))
    }

    /// Route `path` to a matching extractor, returning `None` when none
    /// applies.
    pub async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
        mime: Option<&str>,
    ) -> Result<Option<String>> {
        let extension = path.extension().map(|e| e.to_ascii_lowercase());
        for e in &self.extractors {
            if e.can_handle(mime, extension.as_deref()) {
                return Ok(Some(e.extract(provider, path).await?));
            }
        }
        Ok(None)
    }
}

impl std::fmt::Debug for Extractor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Extractor")
            .field("extractors", &self.extractors.len())
            .finish()
    }
}
