//! PDF viewer backed by Pdfium via `pdfium-render`.

mod bindings;
mod layer;
mod ops;
mod render;

use std::any::Any;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;

pub use render::FitMode;

use crate::error::{Result, ViewerError};
use crate::snapshot::{PdfOutlineItem, PdfOverlayRect, PdfSnapshot, ViewerSnapshot};
use crate::viewer_trait::Viewer;

use layer::{PtsRect, TextLayer};
use ops::FindHit;
use render::RenderedPage;

/// Cap overlay rects sent to the UI.
const MAX_OVERLAYS: usize = 64;

/// Rasterize page 1 of a PDF (or PDF-based AI) to RGBA8.
pub(crate) fn rasterize_first_page(bytes: &[u8], max_edge: u32) -> Result<(Vec<u8>, u32, u32)> {
    render::rasterize_first_page(bytes, max_edge)
}

/// Default viewport until the UI reports the widget frame size.
const DEFAULT_VIEWPORT: (f32, f32) = (800.0, 600.0);

/// Zoom step for toolbar buttons (~25%).
const ZOOM_STEP: f32 = 1.25;

/// Max PDF payload accepted by the viewer. 256 MiB.
pub const DEFAULT_SIZE_LIMIT: u64 = 256 * 1024 * 1024;

struct PdfSelection {
    text: String,
    rects: Vec<PtsRect>,
}

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
    /// Bumped on every render / close so a slower earlier job cannot overwrite
    /// a newer page after the user has already moved on.
    render_gen: AtomicU64,
    size_limit: u64,
    outline: RwLock<Vec<PdfOutlineItem>>,
    layer: RwLock<Option<TextLayer>>,
    selection: RwLock<Option<PdfSelection>>,
    drag_anchor: RwLock<Option<usize>>,
    find_query: RwLock<String>,
    find_match_case: RwLock<bool>,
    find_hits: RwLock<Vec<FindHit>>,
    find_index: RwLock<i32>,
    highlight_rects: RwLock<Vec<PtsRect>>,
}

impl std::fmt::Debug for PdfViewer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfViewer")
            .field(
                "path",
                &self.path.read().as_ref().map(|p| p.as_str().to_string()),
            )
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
            render_gen: AtomicU64::new(0),
            size_limit: DEFAULT_SIZE_LIMIT,
            outline: RwLock::new(Vec::new()),
            layer: RwLock::new(None),
            selection: RwLock::new(None),
            drag_anchor: RwLock::new(None),
            find_query: RwLock::new(String::new()),
            find_match_case: RwLock::new(false),
            find_hits: RwLock::new(Vec::new()),
            find_index: RwLock::new(0),
            highlight_rects: RwLock::new(Vec::new()),
        }
    }

    fn clear_interaction(&self) {
        *self.selection.write() = None;
        *self.drag_anchor.write() = None;
        self.highlight_rects.write().clear();
        self.find_hits.write().clear();
        self.find_query.write().clear();
        *self.find_index.write() = 0;
        *self.find_match_case.write() = false;
        self.outline.write().clear();
        *self.layer.write() = None;
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
    ///
    /// # Errors
    ///
    /// Propagates render failures.
    pub async fn prev_page(&self) -> Result<()> {
        let page = (*self.current_page.read()).saturating_sub(1).max(1);
        if page == *self.current_page.read() {
            return Ok(());
        }
        self.rerender_at_page(page).await
    }

    /// Next page, no-op on the last page.
    ///
    /// # Errors
    ///
    /// Propagates render failures.
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
    ///
    /// # Errors
    ///
    /// Propagates render failures.
    pub async fn fit_width(&self, viewport_w: f32) -> Result<()> {
        {
            let mut vp = self.viewport.write();
            vp.0 = viewport_w.max(1.0);
        }
        *self.fit_mode.write() = FitMode::FitWidth;
        self.rerender_current().await
    }

    /// Fit the entire current page inside the viewport.
    ///
    /// # Errors
    ///
    /// Propagates render failures.
    pub async fn fit_page(&self, viewport_w: f32, viewport_h: f32) -> Result<()> {
        *self.viewport.write() = (viewport_w.max(1.0), viewport_h.max(1.0));
        *self.fit_mode.write() = FitMode::FitPage;
        self.rerender_current().await
    }

    /// Zoom in by [`ZOOM_STEP`].
    ///
    /// # Errors
    ///
    /// Propagates render failures.
    pub async fn zoom_in(&self) -> Result<()> {
        self.zoom_by(ZOOM_STEP).await
    }

    /// Zoom out by [`ZOOM_STEP`].
    ///
    /// # Errors
    ///
    /// Propagates render failures.
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
        let gen = self.render_gen.fetch_add(1, Ordering::Relaxed) + 1;
        let session = self.session.read().ok_or(ViewerError::PdfEmpty)?;
        let viewport = *self.viewport.read();
        let fit_mode = *self.fit_mode.read();
        let zoom = *self.zoom.read();
        let rendered = match tokio::task::spawn_blocking(move || {
            render::render_page(session, page, viewport, fit_mode, zoom)
        })
        .await
        .map_err(|e| ViewerError::PdfRender {
            page,
            reason: format!("join: {e}"),
        })? {
            Ok(page) => page,
            Err(ViewerError::PdfStale) => return Ok(()),
            Err(e) => return Err(e),
        };
        if self.render_gen.load(Ordering::Relaxed) != gen {
            return Ok(());
        }
        let page_changed = rendered.current_page != *self.current_page.read();
        *self.page_count.write() = rendered.page_count;
        *self.current_page.write() = rendered.current_page;
        *self.zoom.write() = rendered.zoom;
        *self.rendered.write() = Some(rendered);
        if page_changed {
            *self.selection.write() = None;
            *self.drag_anchor.write() = None;
            self.highlight_rects.write().clear();
            self.reload_layer().await;
        } else {
            self.sync_layer_raster();
        }
        Ok(())
    }

    fn sync_layer_raster(&self) {
        let Some((w, h)) = self
            .rendered
            .read()
            .as_ref()
            .map(|r| (r.width_px, r.height_px))
        else {
            return;
        };
        if let Some(layer) = self.layer.write().as_mut() {
            layer.set_raster_size(w, h);
        }
    }

    async fn reload_layer(&self) {
        let Some(bytes) = self.bytes.read().clone() else {
            *self.layer.write() = None;
            return;
        };
        let page = (*self.current_page.read()).max(1);
        let (w, h) = self
            .rendered
            .read()
            .as_ref()
            .map(|r| (r.width_px, r.height_px))
            .unwrap_or((1, 1));
        match tokio::task::spawn_blocking(move || {
            layer::extract_layer(bytes.as_slice(), page, w, h)
        })
        .await
        {
            Ok(Ok(layer)) => *self.layer.write() = Some(layer),
            _ => *self.layer.write() = None,
        }
    }

    async fn reload_outline(&self) {
        let Some(bytes) = self.bytes.read().clone() else {
            self.outline.write().clear();
            return;
        };
        match tokio::task::spawn_blocking(move || ops::extract_outline(bytes.as_slice())).await {
            Ok(Ok(items)) => *self.outline.write() = items,
            _ => self.outline.write().clear(),
        }
    }

    fn build_overlays(&self) -> Vec<PdfOverlayRect> {
        let Some(layer) = self.layer.read().clone() else {
            return Vec::new();
        };
        let page = *self.current_page.read();
        let mut out = Vec::new();
        let find_index = *self.find_index.read();
        for (i, hit) in self.find_hits.read().iter().enumerate() {
            if hit.page != page {
                continue;
            }
            let kind = if i + 1 == find_index as usize { 1 } else { 0 };
            for rect in &hit.rects {
                out.push(layer.overlay(*rect, kind));
                if out.len() >= MAX_OVERLAYS {
                    return out;
                }
            }
        }
        if let Some(sel) = self.selection.read().as_ref() {
            for rect in &sel.rects {
                out.push(layer.overlay(*rect, 2));
                if out.len() >= MAX_OVERLAYS {
                    return out;
                }
            }
        }
        for rect in self.highlight_rects.read().iter() {
            out.push(layer.overlay(*rect, 3));
            if out.len() >= MAX_OVERLAYS {
                return out;
            }
        }
        out
    }

    /// Write the current rasterized page as a sibling PNG `{stem}-p007.png`.
    ///
    /// # Errors
    ///
    /// No document, a non-local path, or encode / I/O failure.
    pub fn extract_current_page(&self) -> Result<std::path::PathBuf> {
        let path = self.path.read().clone().ok_or(ViewerError::PdfEmpty)?;
        let os = path.to_local().map_err(ViewerError::Fs)?;
        let rendered = self.rendered.read().clone().ok_or(ViewerError::PdfEmpty)?;
        let img = crate::image::loader::LoadedImage {
            rgba: Arc::clone(&rendered.rgba),
            width: rendered.width_px,
            height: rendered.height_px,
            format: crate::image::loader::ImageFormat::Png,
            original_size_bytes: rendered.rgba.len() as u64,
            ..crate::image::loader::LoadedImage::meta_defaults()
        };
        crate::image::edit::save_sibling(&os, &img, &format!("p{:03}", rendered.current_page))
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

    /// Copy selection text, or the full current page when nothing is selected.
    ///
    /// # Errors
    ///
    /// [`ViewerError::PdfEmpty`] when no document is open, or Pdfium failures.
    pub async fn copy_text(&self) -> Result<String> {
        let selected = self
            .selection
            .read()
            .as_ref()
            .map(|s| s.text.clone())
            .unwrap_or_default();
        if !selected.is_empty() {
            return Ok(selected);
        }
        self.current_page_text().await
    }

    /// Find in the document. `dir`: `0` new search, `1` next, `-1` previous.
    /// An empty query clears the current search.
    ///
    /// # Errors
    ///
    /// [`ViewerError::PdfEmpty`] or Pdfium / join failures.
    pub async fn find(&self, query: String, match_case: bool, dir: i32) -> Result<()> {
        if query.is_empty() {
            self.find_query.write().clear();
            self.find_hits.write().clear();
            *self.find_index.write() = 0;
            return Ok(());
        }
        let Some(bytes) = self.bytes.read().clone() else {
            return Err(ViewerError::PdfEmpty);
        };
        let same = *self.find_query.read() == query && *self.find_match_case.read() == match_case;
        if dir == 0 || !same || self.find_hits.read().is_empty() {
            let q = query.clone();
            let hits = tokio::task::spawn_blocking(move || {
                ops::search_document(bytes.as_slice(), &q, match_case)
            })
            .await
            .map_err(|e| ViewerError::PdfRender {
                page: *self.current_page.read(),
                reason: format!("join: {e}"),
            })??;
            *self.find_query.write() = query;
            *self.find_match_case.write() = match_case;
            let count = hits.len() as i32;
            *self.find_hits.write() = hits;
            *self.find_index.write() = if count == 0 { 0 } else { 1 };
        } else {
            let count = self.find_hits.read().len() as i32;
            if count > 0 {
                let cur = *self.find_index.read();
                let next = if dir < 0 {
                    if cur <= 1 {
                        count
                    } else {
                        cur - 1
                    }
                } else if dir > 0 {
                    if cur >= count {
                        1
                    } else {
                        cur + 1
                    }
                } else {
                    1
                };
                *self.find_index.write() = next;
            }
        }
        self.goto_current_find_hit().await
    }

    async fn goto_current_find_hit(&self) -> Result<()> {
        let page = {
            let hits = self.find_hits.read();
            let idx = *self.find_index.read();
            if idx <= 0 {
                return Ok(());
            }
            let Some(hit) = hits.get((idx as usize).saturating_sub(1)) else {
                return Ok(());
            };
            hit.page
        };
        if page > 0 && page != *self.current_page.read() {
            self.rerender_at_page(page).await?;
        }
        Ok(())
    }

    /// Pointer interaction on the page image (`x`/`y` in page-image pixels).
    /// `phase`: `0` press, `1` drag, `2` release, `3` double-click.
    pub fn pointer(&self, phase: i32, x: f32, y: f32) {
        let Some(layer) = self.layer.read().clone() else {
            return;
        };
        let (x_pts, y_pts) = layer.px_to_pts(x, y);
        let Some(idx) = layer.hit_char(x_pts, y_pts) else {
            if phase == 0 {
                *self.selection.write() = None;
                *self.drag_anchor.write() = None;
            }
            return;
        };
        match phase {
            0 => {
                *self.drag_anchor.write() = Some(idx);
                *self.selection.write() = Some(PdfSelection {
                    text: layer.text_range(idx, idx),
                    rects: layer.rects_range(idx, idx),
                });
            }
            1 | 2 => {
                let anchor = self.drag_anchor.read().unwrap_or(idx);
                *self.selection.write() = Some(PdfSelection {
                    text: layer.text_range(anchor, idx),
                    rects: layer.rects_range(anchor, idx),
                });
            }
            3 => {
                if let Some((lo, hi)) = layer.word_range(idx) {
                    *self.drag_anchor.write() = Some(lo);
                    *self.selection.write() = Some(PdfSelection {
                        text: layer.text_range(lo, hi),
                        rects: layer.rects_range(lo, hi),
                    });
                }
            }
            _ => {}
        }
    }

    /// Jump to an outline destination page (1-based). `0` is ignored.
    ///
    /// # Errors
    ///
    /// Propagates render failures.
    pub async fn outline_goto(&self, page: u32) -> Result<()> {
        if page == 0 {
            return Ok(());
        }
        self.go_to_page(page).await
    }

    /// Local filesystem path of the open document, when available.
    ///
    /// # Errors
    ///
    /// [`ViewerError::PdfEmpty`] or a non-local path.
    pub fn local_path(&self) -> Result<PathBuf> {
        let path = self.path.read().clone().ok_or(ViewerError::PdfEmpty)?;
        path.to_local().map_err(ViewerError::Fs)
    }

    /// In-memory PDF bytes for print / export fallbacks.
    ///
    /// # Errors
    ///
    /// [`ViewerError::PdfEmpty`] when no document is open.
    pub fn payload_bytes(&self) -> Result<Arc<Vec<u8>>> {
        self.bytes.read().clone().ok_or(ViewerError::PdfEmpty)
    }

    /// Export the current text selection as a sibling highlight PDF.
    ///
    /// # Errors
    ///
    /// [`ViewerError::PdfHighlightEmpty`] when nothing is selected, or Pdfium / I/O failures.
    pub async fn highlight_selection(&self) -> Result<PathBuf> {
        let (rects, page) = {
            let sel = self.selection.read();
            let Some(sel) = sel.as_ref() else {
                return Err(ViewerError::PdfHighlightEmpty);
            };
            if sel.rects.is_empty() {
                return Err(ViewerError::PdfHighlightEmpty);
            }
            (sel.rects.clone(), (*self.current_page.read()).max(1))
        };
        let src = self.local_path()?;
        let bytes = self.payload_bytes()?;
        let dest = tokio::task::spawn_blocking(move || {
            ops::save_highlight(bytes.as_slice(), page, &rects, &src)
        })
        .await
        .map_err(|e| ViewerError::PdfRender {
            page,
            reason: format!("join: {e}"),
        })??;
        *self.highlight_rects.write() = self
            .selection
            .read()
            .as_ref()
            .map(|s| s.rects.clone())
            .unwrap_or_default();
        Ok(dest)
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

        self.render_gen.fetch_add(1, Ordering::Relaxed);
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
        *self.selection.write() = None;
        *self.drag_anchor.write() = None;
        self.highlight_rects.write().clear();
        self.find_hits.write().clear();
        self.find_query.write().clear();
        *self.find_index.write() = 0;
        self.reload_outline().await;
        self.reload_layer().await;
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        // Take the session out before awaiting so the parking_lot guard is not held
        // across `.await` (that would make this future !Send).
        self.render_gen.fetch_add(1, Ordering::Relaxed);
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
        self.clear_interaction();
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
        let find_match_count = self.find_hits.read().len() as i32;
        let has_selection = self
            .selection
            .read()
            .as_ref()
            .is_some_and(|s| !s.text.is_empty() || !s.rects.is_empty());
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
            outline: self.outline.read().clone(),
            overlays: self.build_overlays(),
            find_match_index: *self.find_index.read(),
            find_match_count,
            has_selection,
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
