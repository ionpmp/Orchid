//! PDF viewer — stub.
//!
//! Full support via `pdfium-render` requires bundling the PDFium shared
//! library (`pdfium.dll` on Windows, `libpdfium.so` on Linux,
//! `libpdfium.dylib` on macOS). That packaging work — plus the `build.rs`
//! that copies the library next to the Orchid binary — is scheduled as a
//! dedicated task. Until then the viewer compiles, fits the `Viewer`
//! trait, and produces an explanatory error snapshot.

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;

use crate::error::{Result, ViewerError};
use crate::snapshot::ViewerSnapshot;
use crate::viewer_trait::Viewer;

/// PDF viewer stub.
pub struct PdfViewer {
    path: RwLock<Option<orchid_fs::FsPath>>,
}

impl std::fmt::Debug for PdfViewer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfViewer").finish_non_exhaustive()
    }
}

impl Default for PdfViewer {
    fn default() -> Self {
        Self::new()
    }
}

impl PdfViewer {
    /// Build a new PDF viewer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            path: RwLock::new(None),
        }
    }

    /// Go to a specific page (1-based). Today rejects with
    /// [`ViewerError::PdfUnavailable`].
    ///
    /// # Errors
    ///
    /// Always returns [`ViewerError::PdfUnavailable`] in the stub.
    pub fn go_to_page(&self, _page: u32) -> Result<()> {
        Err(ViewerError::PdfUnavailable)
    }

    /// Stub: no-op until PDFium is wired.
    pub fn next_page(&self) -> Result<()> {
        Err(ViewerError::PdfUnavailable)
    }

    /// Stub: no-op until PDFium is wired.
    pub fn prev_page(&self) -> Result<()> {
        Err(ViewerError::PdfUnavailable)
    }

    /// Stub.
    pub fn fit_width(&self, _viewport_w: f32) -> Result<()> {
        Err(ViewerError::PdfUnavailable)
    }

    /// Stub.
    pub fn fit_page(&self, _viewport_w: f32, _viewport_h: f32) -> Result<()> {
        Err(ViewerError::PdfUnavailable)
    }

    /// Stub.
    pub fn zoom_in(&self) -> Result<()> {
        Err(ViewerError::PdfUnavailable)
    }

    /// Stub.
    pub fn zoom_out(&self) -> Result<()> {
        Err(ViewerError::PdfUnavailable)
    }
}

#[async_trait]
impl Viewer for PdfViewer {
    fn type_id(&self) -> &'static str {
        "pdf"
    }

    async fn open(
        &mut self,
        path: orchid_fs::FsPath,
        _registry: Arc<orchid_fs::FsProviderRegistry>,
    ) -> Result<()> {
        *self.path.write() = Some(path);
        Err(ViewerError::PdfUnavailable)
    }

    async fn close(&mut self) -> Result<()> {
        *self.path.write() = None;
        Ok(())
    }

    fn snapshot(&self) -> ViewerSnapshot {
        let path_display = self
            .path
            .read()
            .as_ref()
            .map(|p| p.as_str().to_string())
            .unwrap_or_default();
        ViewerSnapshot::Error {
            path_display,
            message: ViewerError::PdfUnavailable.to_string(),
        }
    }

    fn current_path(&self) -> Option<&orchid_fs::FsPath> {
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
