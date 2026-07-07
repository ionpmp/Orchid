//! Synchronous Pdfium page rendering (runs on a blocking thread pool).

use std::sync::Arc;

use pdfium_render::prelude::*;

use super::bindings::shared_pdfium;
use crate::error::{Result, ViewerError};

/// How the viewer chooses the raster width for the current page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitMode {
    /// Scale so the page width matches the viewport width.
    FitWidth,
    /// Scale so the entire page fits inside the viewport.
    FitPage,
    /// Apply [`RenderedPage::zoom`] to the page's natural width at 96 DPI.
    Custom,
}

/// One rendered PDF page bitmap.
#[derive(Debug, Clone)]
pub struct RenderedPage {
    /// RGBA8 row-major pixels.
    pub rgba: Arc<Vec<u8>>,
    /// Bitmap width in pixels.
    pub width_px: u32,
    /// Bitmap height in pixels.
    pub height_px: u32,
    /// Total pages in the document.
    pub page_count: u32,
    /// 1-based page index that was rendered.
    pub current_page: u32,
    /// Zoom multiplier used when [`FitMode::Custom`].
    pub zoom: f32,
}

/// Render `page` (1-based) from in-memory PDF bytes.
///
/// # Errors
///
/// Propagates Pdfium / decode failures as [`ViewerError`].
pub fn render_page(
    bytes: &[u8],
    page: u32,
    viewport: (f32, f32),
    fit_mode: FitMode,
    zoom: f32,
) -> Result<RenderedPage> {
    let pdfium = shared_pdfium()?;
    let guard = pdfium.lock();

    let document = guard
        .load_pdf_from_byte_slice(bytes, None)
        .map_err(|e| ViewerError::PdfRender {
            page,
            reason: format!("load document: {e}"),
        })?;

    let page_count = document.pages().len();
    if page_count == 0 {
        return Err(ViewerError::PdfEmpty);
    }

    let current_page = page.clamp(1, page_count);
    let pdf_page = document
        .pages()
        .get(current_page - 1)
        .map_err(|e| ViewerError::PdfRender {
            page: current_page,
            reason: format!("open page: {e}"),
        })?;

    let page_w_pts = pdf_page.width().value;
    let page_h_pts = pdf_page.height().value;
    let target_width = target_width_px(page_w_pts, page_h_pts, viewport, fit_mode, zoom);

    let mut config = PdfRenderConfig::new().set_target_width(target_width);
    if fit_mode == FitMode::FitPage {
        let target_height = target_height_px(page_w_pts, page_h_pts, viewport);
        config = config.set_target_height(target_height);
    }

    let bitmap = pdf_page
        .render_with_config(&config)
        .map_err(|e| ViewerError::PdfRender {
            page: current_page,
            reason: format!("render: {e}"),
        })?;

    let image = bitmap.as_image().into_rgba8();
    let (width_px, height_px) = image.dimensions();
    let rgba = Arc::new(image.into_raw());

    Ok(RenderedPage {
        rgba,
        width_px,
        height_px,
        page_count,
        current_page,
        zoom,
    })
}

fn target_width_px(
    page_w_pts: f32,
    page_h_pts: f32,
    viewport: (f32, f32),
    fit_mode: FitMode,
    zoom: f32,
) -> i32 {
    let natural_w = points_to_pixels(page_w_pts);
    let (vw, vh) = (viewport.0.max(1.0), viewport.1.max(1.0));

    let px = match fit_mode {
        FitMode::FitWidth => vw,
        FitMode::FitPage => {
            let natural_h = points_to_pixels(page_h_pts);
            let scale = (vw / natural_w).min(vh / natural_h);
            natural_w * scale
        }
        FitMode::Custom => natural_w * zoom.max(0.05),
    };

    px.round().max(1.0) as i32
}

fn target_height_px(page_w_pts: f32, page_h_pts: f32, viewport: (f32, f32)) -> i32 {
    let natural_w = points_to_pixels(page_w_pts);
    let natural_h = points_to_pixels(page_h_pts);
    let (vw, vh) = (viewport.0.max(1.0), viewport.1.max(1.0));
    let scale = (vw / natural_w).min(vh / natural_h);
    (natural_h * scale).round().max(1.0) as i32
}

fn points_to_pixels(points: f32) -> f32 {
    // Match PdfRenderConfig's default 96 DPI mapping.
    points / 72.0 * 96.0
}
