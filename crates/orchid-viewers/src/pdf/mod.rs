//! PDF viewer backed by Pdfium via `pdfium-render`.

mod bindings;
mod render;

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;

pub use render::FitMode;

use crate::error::{Result, ViewerError};
use crate::snapshot::{PdfSnapshot, ViewerSnapshot};
use crate::viewer_trait::Viewer;

use render::RenderedPage;

/// Default viewport until the UI reports the widget frame size.
const DEFAULT_VIEWPORT: (f32, f32) = (800.0, 600.0);

/// Zoom step for toolbar buttons (~25%).
const ZOOM_STEP: f32 = 1.25;

/// Max PDF payload accepted by the viewer. 256 MiB.
pub const DEFAULT_SIZE_LIMIT: u64 = 256 * 1024 * 1024;

/// PDF viewer.
pub struct PdfViewer {
    path: RwLock<Option<orchid_fs::FsPath>>,
    /// Shared payload kept for diagnostics; the pdfium worker owns the live
    /// parsed document keyed by [`session`].
    bytes: RwLock<Option<Arc<Vec<u8>>>>,
    session: RwLock<Option<render::PdfSessionId>>,
    page_count: RwLock<u32>,
    current_page: RwLock<u32>,
    zoom: RwLock<f32>,
    viewport: RwLock<(f32, f32)>,
    fit_mode: RwLock<FitMode>,
    rendered: RwLock<Option<RenderedPage>>,
    size_limit: u64,
}

impl std::fmt::Debug for PdfViewer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfViewer")
            .field("path", &self.path.read().as_ref().map(|p| p.as_str().to_string()))
            .finish_non_exhaustive()
    }
}

impl Default for PdfViewer {
    fn default() -> Self {
        Self::new()
    }
}

impl PdfViewer {
    /// Build an empty PDF viewer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            path: RwLock::new(None),
            bytes: RwLock::new(None),
            session: RwLock::new(None),
            page_count: RwLock::new(0),
            current_page: RwLock::new(1),
            zoom: RwLock::new(1.0),
            viewport: RwLock::new(DEFAULT_VIEWPORT),
            fit_mode: RwLock::new(FitMode::FitWidth),
            rendered: RwLock::new(None),
            size_limit: DEFAULT_SIZE_LIMIT,
        }
    }

    /// Update the viewport used for fit-width / fit-page math.
    pub fn set_viewport(&self, width: f32, height: f32) {
        *self.viewport.write() = (width.max(1.0), height.max(1.0));
    }

    /// Update the viewport and re-render when a fit mode is active.
    ///
    /// # Errors
    ///
    /// Propagates render failures when a document is open and fit mode is not custom.
    pub async fn apply_viewport(&self, width: f32, height: f32) -> Result<()> {
        self.set_viewport(width, height);
        if self.bytes.read().is_none() {
            return Ok(());
        }
        if *self.fit_mode.read() == FitMode::Custom {
            return Ok(());
        }
        let page = *self.current_page.read();
        self.rerender_at_page(page.max(1)).await
    }

    /// Go to a specific page (1-based).
    ///
    /// # Errors
    ///
    /// Returns [`ViewerError::PdfRender`] or [`ViewerError::PdfUnavailable`].
    pub async fn go_to_page(&self, page: u32) -> Result<()> {
        self.rerender_at_page(page).await
    }

    /// Previous page, no-op on page 1.
    pub async fn prev_page(&self) -> Result<()> {
        let page = (*self.current_page.read()).saturating_sub(1).max(1);
        if page == *self.current_page.read() {
            return Ok(());
        }
        self.rerender_at_page(page).await
    }

    /// Next page, no-op on the last page.
    pub async fn next_page(&self) -> Result<()> {
        let count = *self.page_count.read();
        if count == 0 {
            return Ok(());
        }
        let page = (*self.current_page.read() + 1).min(count);
        if page == *self.current_page.read() {
            return Ok(());
        }
        self.rerender_at_page(page).await
    }

    /// Fit the current page to the viewport width.
    pub async fn fit_width(&self, viewport_w: f32) -> Result<()> {
        {
            let mut vp = self.viewport.write();
            vp.0 = viewport_w.max(1.0);
        }
        *self.fit_mode.write() = FitMode::FitWidth;
        self.rerender_current().await
    }

    /// Fit the entire current page inside the viewport.
    pub async fn fit_page(&self, viewport_w: f32, viewport_h: f32) -> Result<()> {
        *self.viewport.write() = (viewport_w.max(1.0), viewport_h.max(1.0));
        *self.fit_mode.write() = FitMode::FitPage;
        self.rerender_current().await
    }

    /// Zoom in by [`ZOOM_STEP`].
    pub async fn zoom_in(&self) -> Result<()> {
        self.zoom_by(ZOOM_STEP).await
    }

    /// Zoom out by [`ZOOM_STEP`].
    pub async fn zoom_out(&self) -> Result<()> {
        self.zoom_by(1.0 / ZOOM_STEP).await
    }

    async fn zoom_by(&self, factor: f32) -> Result<()> {
        *self.fit_mode.write() = FitMode::Custom;
        {
            let mut z = self.zoom.write();
            *z = (*z * factor).clamp(0.05, 16.0);
        }
        self.rerender_current().await
    }

    async fn rerender_current(&self) -> Result<()> {
        let page = *self.current_page.read();
        self.rerender_at_page(page).await
    }

    async fn rerender_at_page(&self, page: u32) -> Result<()> {
        let session = self
            .session
            .read()
            .ok_or(ViewerError::PdfEmpty)?;
        let viewport = *self.viewport.read();
        let fit_mode = *self.fit_mode.read();
        let zoom = *self.zoom.read();
        let rendered = tokio::task::spawn_blocking(move || {
            render::render_page(session, page, viewport, fit_mode, zoom)
        })
        .await
        .map_err(|e| ViewerError::PdfRender {
            page,
            reason: format!("join: {e}"),
        })??;
        *self.page_count.write() = rendered.page_count;
        *self.current_page.write() = rendered.current_page;
        *self.zoom.write() = rendered.zoom;
        *self.rendered.write() = Some(rendered);
        Ok(())
    }

    /// Extract Unicode text for the current page (for clipboard copy).
    ///
    /// # Errors
    ///
    /// Returns [`ViewerError::PdfEmpty`] when no document is open, or Pdfium failures.
    pub async fn current_page_text(&self) -> Result<String> {
        let session = self.session.read().ok_or(ViewerError::PdfEmpty)?;
        let page = (*self.current_page.read()).max(1);
        tokio::task::spawn_blocking(move || render::extract_page_text(session, page))
            .await
            .map_err(|e| ViewerError::PdfRender {
                page,
                reason: format!("join: {e}"),
            })?
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
        registry: Arc<orchid_fs::FsProviderRegistry>,
    ) -> Result<()> {
        let provider = registry
            .for_path(&path)
            .ok_or_else(|| orchid_fs::FsError::ProviderNotFound(path.scheme().to_string()))?;
        let bytes = provider.read(&path).await.map_err(ViewerError::Fs)?;
        if bytes.len() as u64 > self.size_limit {
            return Err(ViewerError::FileTooLarge {
                size: bytes.len() as u64,
                limit: self.size_limit,
            });
        }

        let viewport = *self.viewport.read();
        let fit_mode = *self.fit_mode.read();
        let zoom = *self.zoom.read();
        let path_for_task = path.clone();
        let bytes = Arc::new(bytes);
        let bytes_for_worker = Arc::clone(&bytes);
        let (session, rendered) = tokio::task::spawn_blocking(move || {
            let (session, _) = render::open_document(bytes_for_worker)?;
            let rendered = render::render_page(session, 1, viewport, fit_mode, zoom)?;
            Ok::<_, ViewerError>((session, rendered))
        })
        .await
        .map_err(|e| ViewerError::PdfRender {
            page: 1,
            reason: format!("join: {e}"),
        })??;

        if let Some(old) = self.session.write().replace(session) {
            render::close_document(old);
        }
        *self.path.write() = Some(path_for_task);
        *self.bytes.write() = Some(bytes);
        *self.page_count.write() = rendered.page_count;
        *self.current_page.write() = rendered.current_page;
        *self.zoom.write() = rendered.zoom;
        *self.rendered.write() = Some(rendered);
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        // Take the session out before awaiting so the parking_lot guard is not held
        // across `.await` (that would make this future !Send).
        let session = self.session.write().take();
        if let Some(session) = session {
            tokio::task::spawn_blocking(move || render::close_document(session))
                .await
                .map_err(|e| ViewerError::PdfRender {
                    page: 0,
                    reason: format!("join: {e}"),
                })?;
        }
        *self.path.write() = None;
        *self.bytes.write() = None;
        *self.page_count.write() = 0;
        *self.current_page.write() = 1;
        *self.zoom.write() = 1.0;
        *self.fit_mode.write() = FitMode::FitWidth;
        *self.rendered.write() = None;
        Ok(())
    }

    fn snapshot(&self) -> ViewerSnapshot {
        let path_display = self
            .path
            .read()
            .as_ref()
            .map(|p| p.as_str().to_string())
            .unwrap_or_default();

        let Some(rendered) = self.rendered.read().clone() else {
            return ViewerSnapshot::Loading { path_display };
        };

        let fit_mode = match *self.fit_mode.read() {
            FitMode::FitWidth => 0,
            FitMode::FitPage => 1,
            FitMode::Custom => 2,
        };
        ViewerSnapshot::Pdf(PdfSnapshot {
            path_display,
            page_count: rendered.page_count,
            current_page: rendered.current_page,
            page_width_px: rendered.width_px,
            page_height_px: rendered.height_px,
            page_rgba_bytes: rendered.rgba,
            zoom: rendered.zoom,
            fit_mode,
            // Status line is localized in orchid-ui (`viewer-pdf-info`).
            info_text: String::new(),
        })
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
