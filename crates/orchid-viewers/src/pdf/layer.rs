//! Text layer and PDF ↔ screen coordinate mapping.
//!
//! PDF page space has its origin at the **bottom-left**. Screen overlays use
//! **top-left** pixels of the rasterized page image.

use crate::error::{Result, ViewerError};
use crate::snapshot::PdfOverlayRect;

use super::bindings::with_pdfium;

/// Axis-aligned rectangle in PDF points (origin bottom-left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PtsRect {
    /// Left edge.
    pub left: f32,
    /// Bottom edge.
    pub bottom: f32,
    /// Right edge.
    pub right: f32,
    /// Top edge.
    pub top: f32,
}

impl PtsRect {
    /// Build from Pdfium-style `(bottom, left, top, right)` values.
    #[must_use]
    pub fn from_pdf_values(bottom: f32, left: f32, top: f32, right: f32) -> Self {
        Self {
            left: left.min(right),
            bottom: bottom.min(top),
            right: left.max(right),
            top: bottom.max(top),
        }
    }

    /// Width in points.
    #[must_use]
    pub fn width(self) -> f32 {
        (self.right - self.left).max(0.0)
    }

    /// Height in points.
    #[must_use]
    pub fn height(self) -> f32 {
        (self.top - self.bottom).max(0.0)
    }

    /// Union of two rects.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            left: self.left.min(other.left),
            bottom: self.bottom.min(other.bottom),
            right: self.right.max(other.right),
            top: self.top.max(other.top),
        }
    }

    /// Whether `(x, y)` in PDF points lies inside this rect (inclusive).
    #[must_use]
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.left && x <= self.right && y >= self.bottom && y <= self.top
    }
}

/// One Unicode character with its loose PDF bounds.
#[derive(Debug, Clone)]
pub struct LayerChar {
    /// Display character (control chars are skipped at extract time).
    pub ch: char,
    /// Loose glyph bounds in PDF points.
    pub bounds: PtsRect,
}

/// Extracted text layer for the current rasterized page.
#[derive(Debug, Clone)]
pub struct TextLayer {
    /// Page width in PDF points.
    pub page_w_pts: f32,
    /// Page height in PDF points.
    pub page_h_pts: f32,
    /// Raster width in pixels.
    pub width_px: u32,
    /// Raster height in pixels.
    pub height_px: u32,
    /// Characters in document order.
    pub chars: Vec<LayerChar>,
}

impl TextLayer {
    /// Update raster size after zoom / fit without re-extracting glyphs.
    pub fn set_raster_size(&mut self, width_px: u32, height_px: u32) {
        self.width_px = width_px.max(1);
        self.height_px = height_px.max(1);
    }

    /// Map a PDF-space rect to a screen overlay (top-left origin).
    #[must_use]
    pub fn overlay(&self, rect: PtsRect, kind: u8) -> PdfOverlayRect {
        overlay_from_pts(
            rect,
            self.page_w_pts,
            self.page_h_pts,
            self.width_px,
            self.height_px,
            kind,
        )
    }

    /// Convert page-image pixels to PDF points.
    #[must_use]
    pub fn px_to_pts(&self, x: f32, y: f32) -> (f32, f32) {
        px_to_pts(
            x,
            y,
            self.page_w_pts,
            self.page_h_pts,
            self.width_px,
            self.height_px,
        )
    }

    /// Character whose loose bounds contain the PDF point, else the nearest.
    #[must_use]
    pub fn hit_char(&self, x_pts: f32, y_pts: f32) -> Option<usize> {
        if self.chars.is_empty() {
            return None;
        }
        for (i, ch) in self.chars.iter().enumerate() {
            if ch.bounds.contains(x_pts, y_pts) {
                return Some(i);
            }
        }
        let mut best = 0usize;
        let mut best_d = f32::MAX;
        for (i, ch) in self.chars.iter().enumerate() {
            let cx = (ch.bounds.left + ch.bounds.right) * 0.5;
            let cy = (ch.bounds.bottom + ch.bounds.top) * 0.5;
            let d = (cx - x_pts).hypot(cy - y_pts);
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        Some(best)
    }

    /// Inclusive character-index range as concatenated Unicode.
    #[must_use]
    pub fn text_range(&self, a: usize, b: usize) -> String {
        if self.chars.is_empty() {
            return String::new();
        }
        let lo = a.min(b).min(self.chars.len() - 1);
        let hi = a.max(b).min(self.chars.len() - 1);
        self.chars[lo..=hi].iter().map(|c| c.ch).collect()
    }

    /// Merged line-run rects covering `[a, b]` inclusive.
    #[must_use]
    pub fn rects_range(&self, a: usize, b: usize) -> Vec<PtsRect> {
        if self.chars.is_empty() {
            return Vec::new();
        }
        let lo = a.min(b).min(self.chars.len() - 1);
        let hi = a.max(b).min(self.chars.len() - 1);
        merge_line_rects(&self.chars[lo..=hi])
    }

    /// Expand `idx` to a whitespace-delimited word.
    #[must_use]
    pub fn word_range(&self, idx: usize) -> Option<(usize, usize)> {
        if self.chars.is_empty() || idx >= self.chars.len() {
            return None;
        }
        if self.chars[idx].ch.is_whitespace() {
            return Some((idx, idx));
        }
        let mut lo = idx;
        while lo > 0 && !self.chars[lo - 1].ch.is_whitespace() {
            lo -= 1;
        }
        let mut hi = idx;
        while hi + 1 < self.chars.len() && !self.chars[hi + 1].ch.is_whitespace() {
            hi += 1;
        }
        Some((lo, hi))
    }
}

/// Map a PDF-space rect to a top-left screen overlay.
#[must_use]
pub fn overlay_from_pts(
    rect: PtsRect,
    page_w_pts: f32,
    page_h_pts: f32,
    width_px: u32,
    height_px: u32,
    kind: u8,
) -> PdfOverlayRect {
    let sx = width_px as f32 / page_w_pts.max(1.0);
    let sy = height_px as f32 / page_h_pts.max(1.0);
    let x = rect.left * sx;
    let y = (page_h_pts - rect.top) * sy;
    PdfOverlayRect {
        x,
        y,
        w: rect.width() * sx,
        h: rect.height() * sy,
        kind,
    }
}

/// Convert page-image pixels (top-left) to PDF points (bottom-left).
#[must_use]
pub fn px_to_pts(
    x_px: f32,
    y_px: f32,
    page_w_pts: f32,
    page_h_pts: f32,
    width_px: u32,
    height_px: u32,
) -> (f32, f32) {
    let sx = page_w_pts / (width_px.max(1) as f32);
    let sy = page_h_pts / (height_px.max(1) as f32);
    (x_px * sx, page_h_pts - y_px * sy)
}

fn merge_line_rects(chars: &[LayerChar]) -> Vec<PtsRect> {
    let mut out = Vec::new();
    let mut current: Option<PtsRect> = None;
    let mut last_top = 0.0f32;
    for ch in chars {
        match current {
            None => {
                current = Some(ch.bounds);
                last_top = ch.bounds.top;
            }
            Some(run) => {
                let same_line = (ch.bounds.top - last_top).abs() < 2.0
                    && (ch.bounds.bottom - run.bottom).abs() < ch.bounds.height().max(2.0);
                if same_line {
                    current = Some(run.union(ch.bounds));
                } else {
                    out.push(run);
                    current = Some(ch.bounds);
                    last_top = ch.bounds.top;
                }
            }
        }
    }
    if let Some(run) = current {
        out.push(run);
    }
    out
}

/// Extract the Unicode text layer for `page` (1-based).
///
/// # Errors
///
/// Pdfium bind / load / page failures.
pub fn extract_layer(bytes: &[u8], page: u32, width_px: u32, height_px: u32) -> Result<TextLayer> {
    with_pdfium(|pdfium| {
        let document =
            pdfium
                .load_pdf_from_byte_slice(bytes, None)
                .map_err(|e| ViewerError::PdfRender {
                    page,
                    reason: format!("load document: {e}"),
                })?;
        let count = i32::from(document.pages().len()).max(0) as u32;
        if count == 0 {
            return Err(ViewerError::PdfEmpty);
        }
        let current = page.clamp(1, count);
        let pdf_page = document
            .pages()
            .get(current.saturating_sub(1) as i32)
            .map_err(|e| ViewerError::PdfRender {
                page: current,
                reason: format!("open page: {e}"),
            })?;
        let page_w_pts = pdf_page.width().value.max(1.0);
        let page_h_pts = pdf_page.height().value.max(1.0);
        let mut chars = Vec::new();
        if let Ok(text) = pdf_page.text() {
            for ch in text.chars().iter() {
                let Some(unicode) = ch.unicode_char() else {
                    continue;
                };
                if unicode == '\0' {
                    continue;
                }
                let Ok(bounds) = ch.loose_bounds() else {
                    continue;
                };
                chars.push(LayerChar {
                    ch: unicode,
                    bounds: PtsRect::from_pdf_values(
                        bounds.bottom().value,
                        bounds.left().value,
                        bounds.top().value,
                        bounds.right().value,
                    ),
                });
            }
        }
        Ok(TextLayer {
            page_w_pts,
            page_h_pts,
            width_px: width_px.max(1),
            height_px: height_px.max(1),
            chars,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_flips_y_axis() {
        let rect = PtsRect::from_pdf_values(0.0, 10.0, 20.0, 40.0);
        let ov = overlay_from_pts(rect, 100.0, 200.0, 100, 200, 0);
        assert!((ov.x - 10.0).abs() < 0.01);
        assert!((ov.w - 30.0).abs() < 0.01);
        assert!((ov.h - 20.0).abs() < 0.01);
        assert!(
            ov.y > 150.0,
            "bottom-of-page PDF rect should flip to large screen y, got {}",
            ov.y
        );
        assert!((ov.y - 180.0).abs() < 0.01);

        let top = PtsRect::from_pdf_values(180.0, 0.0, 200.0, 10.0);
        let ov_top = overlay_from_pts(top, 100.0, 200.0, 100, 200, 1);
        assert!(ov_top.y.abs() < 0.01);
        assert_eq!(ov_top.kind, 1);
    }

    #[test]
    fn px_to_pts_corners() {
        let (tl_x, tl_y) = px_to_pts(0.0, 0.0, 612.0, 792.0, 612, 792);
        assert!(tl_x.abs() < 0.01);
        assert!((tl_y - 792.0).abs() < 0.01, "top-left screen → PDF top");

        let (br_x, br_y) = px_to_pts(612.0, 792.0, 612.0, 792.0, 612, 792);
        assert!((br_x - 612.0).abs() < 0.01);
        assert!(br_y.abs() < 0.01, "bottom-right screen → PDF origin");

        let (tr_x, tr_y) = px_to_pts(612.0, 0.0, 612.0, 792.0, 612, 792);
        assert!((tr_x - 612.0).abs() < 0.01);
        assert!((tr_y - 792.0).abs() < 0.01);
    }
}
