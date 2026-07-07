//! Synchronous Pdfium page rendering (runs on a blocking thread pool).

use std::sync::Arc;

use pdfium_render::prelude::*;

use super::bindings::with_pdfium;
use crate::error::{Result, ViewerError};

/// How the viewer chooses the raster width for the current page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitMode {
    /// Scale so the page width matches the viewport width.
    FitWidth,
    /// Scale so the entire page fits inside the viewport.
    FitPage,
    /// Apply `zoom` to the page's natural width at 96 DPI.
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
    with_pdfium(|pdfium| render_with_pdfium(pdfium, bytes, page, viewport, fit_mode, zoom))
}

fn render_with_pdfium(
    pdfium: &Pdfium,
    bytes: &[u8],
    page: u32,
    viewport: (f32, f32),
    fit_mode: FitMode,
    zoom: f32,
) -> Result<RenderedPage> {
    let document = pdfium
        .load_pdf_from_byte_slice(bytes, None)
        .map_err(|e| ViewerError::PdfRender {
            page,
            reason: format!("load document: {e}"),
        })?;

    let page_count = u32::from(document.pages().len());
    if page_count == 0 {
        return Err(ViewerError::PdfEmpty);
    }

    let current_page = page.clamp(1, page_count);
    let pdf_page = document
        .pages()
        .get(u16::try_from(current_page - 1).unwrap_or(0))
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
    points / 72.0 * 96.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_width_uses_viewport_width() {
        let px = target_width_px(612.0, 792.0, (800.0, 600.0), FitMode::FitWidth, 1.0);
        assert_eq!(px, 800);
    }

    #[test]
    fn custom_zoom_scales_natural_width() {
        let px = target_width_px(612.0, 792.0, (800.0, 600.0), FitMode::Custom, 2.0);
        assert!((px as f32 - 612.0 / 72.0 * 96.0 * 2.0).abs() < 2.0);
    }

    /// Smallest valid single-page PDF (US Letter).
    const MINIMAL_PDF: &[u8] = br"%PDF-1.1
1 0 obj<< /Type /Catalog /Pages 2 0 R>>endobj
2 0 obj<< /Type /Pages /Kids [3 0 R] /Count 1>>endobj
3 0 obj<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792]>>endobj
xref
0 4
0000000000 65535 f 
0000000009 00000 n 
0000000052 00000 n 
0000000101 00000 n 
trailer<< /Root 1 0 R /Size 4>>
startxref
178
%%EOF";

    #[test]
    fn render_minimal_pdf_page() {
        let page = render_page(MINIMAL_PDF, 1, (640.0, 480.0), FitMode::FitWidth, 1.0)
            .expect("pdfium should render minimal PDF when available");
        assert_eq!(page.page_count, 1);
        assert_eq!(page.current_page, 1);
        assert!(page.width_px > 0);
        assert!(page.height_px > 0);
        assert_eq!(
            page.rgba.len(),
            (page.width_px * page.height_px * 4) as usize
        );
    }
}
