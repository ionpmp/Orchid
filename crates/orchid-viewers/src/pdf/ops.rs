//! Document-level PDF operations that run off the raster worker:
//! outline extraction, full-document search, and highlight export.

use std::path::Path;

use pdfium_render::prelude::*;

use crate::error::{Result, ViewerError};
use crate::image::export::unique_export_dest;
use crate::snapshot::PdfOutlineItem;

use super::bindings::with_pdfium;
use super::layer::PtsRect;

/// One search hit: 1-based page plus PDF-space rects for the match.
#[derive(Debug, Clone)]
pub struct FindHit {
    /// 1-based page index.
    pub page: u32,
    /// Segment bounds in PDF points.
    pub rects: Vec<PtsRect>,
}

/// Walk bookmarks into a flat outline (depth via `parent()`).
///
/// # Errors
///
/// Pdfium bind / load failures.
pub fn extract_outline(bytes: &[u8]) -> Result<Vec<PdfOutlineItem>> {
    with_pdfium(|pdfium| {
        let document =
            pdfium
                .load_pdf_from_byte_slice(bytes, None)
                .map_err(|e| ViewerError::PdfRender {
                    page: 1,
                    reason: format!("load document: {e}"),
                })?;
        let mut items = Vec::new();
        for bm in document.bookmarks().iter() {
            let title = bm.title().unwrap_or_default();
            let page = bm
                .destination()
                .and_then(|d| d.page_index().ok())
                .map(|idx| (idx.max(0) as u32).saturating_add(1))
                .unwrap_or(0);
            let mut depth = 0u32;
            let mut parent = bm.parent();
            while let Some(p) = parent {
                depth = depth.saturating_add(1);
                if depth > 64 {
                    break;
                }
                parent = p.parent();
            }
            items.push(PdfOutlineItem { title, page, depth });
        }
        Ok(items)
    })
}

/// Search every page for `query`. Empty query yields no hits.
///
/// # Errors
///
/// Pdfium bind / load failures.
pub fn search_document(bytes: &[u8], query: &str, match_case: bool) -> Result<Vec<FindHit>> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    with_pdfium(|pdfium| {
        let document =
            pdfium
                .load_pdf_from_byte_slice(bytes, None)
                .map_err(|e| ViewerError::PdfRender {
                    page: 1,
                    reason: format!("load document: {e}"),
                })?;
        let count = i32::from(document.pages().len()).max(0) as u32;
        let options = PdfSearchOptions::new().match_case(match_case);
        let mut hits = Vec::new();
        for i in 0..count {
            let page_1 = i + 1;
            let pdf_page = match document.pages().get(i as i32) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let Ok(text) = pdf_page.text() else {
                continue;
            };
            let Ok(search) = text.search(query, &options) else {
                continue;
            };
            while let Some(segments) = search.find_next() {
                let mut rects = Vec::new();
                for seg in segments.iter() {
                    let b = seg.bounds();
                    let rect = PtsRect::from_pdf_values(
                        b.bottom().value,
                        b.left().value,
                        b.top().value,
                        b.right().value,
                    );
                    if rect.width() > 0.0 && rect.height() > 0.0 {
                        rects.push(rect);
                    }
                }
                if !rects.is_empty() {
                    hits.push(FindHit {
                        page: page_1,
                        rects,
                    });
                }
            }
        }
        Ok(hits)
    })
}

/// Write highlight annotations over `rects` to `dest`.
///
/// # Errors
///
/// Empty rects, Pdfium failures, or I/O.
pub fn save_highlight(
    bytes: &[u8],
    page: u32,
    rects: &[PtsRect],
    dest: &Path,
) -> Result<std::path::PathBuf> {
    if rects.is_empty() {
        return Err(ViewerError::PdfHighlightEmpty);
    }
    let dest = dest.to_path_buf();
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
        let mut pdf_page = document
            .pages()
            .get(current.saturating_sub(1) as i32)
            .map_err(|e| ViewerError::PdfRender {
                page: current,
                reason: format!("open page: {e}"),
            })?;
        let color = PdfColor::new(255, 230, 0, 80);
        for rect in rects {
            let pdf_rect = PdfRect::new_from_values(rect.bottom, rect.left, rect.top, rect.right);
            let mut ann = pdf_page
                .annotations_mut()
                .create_highlight_annotation()
                .map_err(|e| ViewerError::PdfRender {
                    page: current,
                    reason: format!("highlight: {e}"),
                })?;
            ann.set_position(pdf_rect.left(), pdf_rect.bottom())
                .map_err(|e| ViewerError::PdfRender {
                    page: current,
                    reason: format!("highlight position: {e}"),
                })?;
            ann.set_stroke_color(color)
                .map_err(|e| ViewerError::PdfRender {
                    page: current,
                    reason: format!("highlight stroke: {e}"),
                })?;
            ann.set_fill_color(color)
                .map_err(|e| ViewerError::PdfRender {
                    page: current,
                    reason: format!("highlight fill: {e}"),
                })?;
            ann.set_width(pdf_rect.width())
                .map_err(|e| ViewerError::PdfRender {
                    page: current,
                    reason: format!("highlight width: {e}"),
                })?;
            ann.set_height(pdf_rect.height())
                .map_err(|e| ViewerError::PdfRender {
                    page: current,
                    reason: format!("highlight height: {e}"),
                })?;
            ann.attachment_points_mut()
                .create_attachment_point_at_end(PdfQuadPoints::from_rect(&pdf_rect))
                .map_err(|e| ViewerError::PdfRender {
                    page: current,
                    reason: format!("highlight quad: {e}"),
                })?;
        }
        document
            .save_to_file(&dest)
            .map_err(|e| ViewerError::PdfRender {
                page: current,
                reason: format!("save highlight: {e}"),
            })?;
        Ok(dest)
    })
}

/// Sibling `*-hl.pdf` next to `src_path` (fallback when the open file is not writable).
///
/// # Errors
///
/// Same as [`save_highlight`].
pub fn save_highlight_sibling(
    bytes: &[u8],
    page: u32,
    rects: &[PtsRect],
    src_path: &Path,
) -> Result<std::path::PathBuf> {
    let dest = unique_export_dest(src_path, "hl", "pdf");
    save_highlight(bytes, page, rects, &dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::layer::extract_layer;
    use crate::pdf::render::MINIMAL_PDF;

    #[test]
    fn outline_and_search_on_minimal_pdf() {
        let layer = extract_layer(MINIMAL_PDF, 1, 100, 100)
            .expect("pdfium should open minimal PDF when available");
        assert!(
            layer.page_w_pts > 0.0,
            "minimal page MediaBox width should be positive"
        );
        let outline = extract_outline(MINIMAL_PDF).expect("outline");
        assert!(outline.is_empty(), "minimal PDF has no bookmarks");
        let hits = search_document(MINIMAL_PDF, "no-such-text", false).expect("search");
        assert!(hits.is_empty(), "minimal PDF has no text to match");
    }
}
