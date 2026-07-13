//! Synchronous Pdfium page rendering.
//!
//! Rendering runs on a dedicated worker thread so a loaded document can be
//! reused across page / zoom / viewport changes without re-parsing the PDF
//! bytes on every frame.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock};

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

/// Opaque handle for an opened PDF in the worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PdfSessionId(u64);

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

enum WorkerRequest {
    Open {
        bytes: Arc<Vec<u8>>,
        reply: Sender<Result<(PdfSessionId, u32)>>,
    },
    Render {
        session: PdfSessionId,
        page: u32,
        viewport: (f32, f32),
        fit_mode: FitMode,
        zoom: f32,
        reply: Sender<Result<RenderedPage>>,
    },
    ExtractText {
        session: PdfSessionId,
        page: u32,
        reply: Sender<Result<String>>,
    },
    Close {
        session: PdfSessionId,
        reply: Sender<()>,
    },
}

static WORKER_TX: OnceLock<Sender<WorkerRequest>> = OnceLock::new();

fn worker_sender() -> Sender<WorkerRequest> {
    WORKER_TX
        .get_or_init(|| {
            let (tx, rx) = mpsc::channel();
            std::thread::Builder::new()
                .name("orchid-pdfium".into())
                .spawn(move || worker_loop(rx))
                .expect("spawn pdfium worker");
            tx
        })
        .clone()
}

fn worker_loop(rx: Receiver<WorkerRequest>) {
    let mut inbox: VecDeque<WorkerRequest> = VecDeque::new();
    let mut documents: HashMap<PdfSessionId, Arc<Vec<u8>>> = HashMap::new();

    while let Some(req) = next_request(&rx, &mut inbox) {
        match req {
            WorkerRequest::Open { bytes, reply } => {
                let page_count = with_pdfium(|pdfium| {
                    let document = pdfium
                        .load_pdf_from_byte_slice(bytes.as_slice(), None)
                        .map_err(|e| ViewerError::PdfRender {
                            page: 1,
                            reason: format!("load document: {e}"),
                        })?;
                    let count = u32::from(document.pages().len());
                    if count == 0 {
                        return Err(ViewerError::PdfEmpty);
                    }
                    Ok(count)
                });
                match page_count {
                    Ok(count) => {
                        let id = PdfSessionId(NEXT_SESSION.fetch_add(1, Ordering::Relaxed));
                        documents.insert(id, bytes);
                        let _ = reply.send(Ok((id, count)));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(e));
                    }
                }
            }
            WorkerRequest::Close { session, reply } => {
                documents.remove(&session);
                let _ = reply.send(());
            }
            // Render and ExtractText share a warm parsed document for the
            // same session so page/zoom/text churn does not re-parse bytes.
            req @ (WorkerRequest::Render { .. } | WorkerRequest::ExtractText { .. }) => {
                let session = match &req {
                    WorkerRequest::Render { session, .. }
                    | WorkerRequest::ExtractText { session, .. } => *session,
                    _ => unreachable!(),
                };
                let Some(bytes) = documents.get(&session).cloned() else {
                    match req {
                        WorkerRequest::Render { reply, .. } => {
                            let _ = reply.send(Err(ViewerError::PdfEmpty));
                        }
                        WorkerRequest::ExtractText { reply, .. } => {
                            let _ = reply.send(Err(ViewerError::PdfEmpty));
                        }
                        _ => unreachable!(),
                    }
                    continue;
                };

                // Ensure Pdfium is bound before moving `req` into the worker
                // closure — otherwise a bind failure drops the reply channel
                // and the caller blocks forever on recv.
                if let Err(e) = with_pdfium(|_| Ok::<(), ViewerError>(())) {
                    match req {
                        WorkerRequest::Render { reply, .. } => {
                            let _ = reply.send(Err(e));
                        }
                        WorkerRequest::ExtractText { reply, .. } => {
                            let _ = reply.send(Err(e));
                        }
                        _ => unreachable!(),
                    }
                    continue;
                }

                let interrupted = with_pdfium(|pdfium| -> Result<Option<WorkerRequest>> {
                    let document = match pdfium.load_pdf_from_byte_slice(bytes.as_slice(), None) {
                        Ok(doc) => doc,
                        Err(e) => {
                            let err = ViewerError::PdfRender {
                                page: 1,
                                reason: format!("load document: {e}"),
                            };
                            match req {
                                WorkerRequest::Render { reply, .. } => {
                                    let _ = reply.send(Err(err));
                                }
                                WorkerRequest::ExtractText { reply, .. } => {
                                    let _ = reply.send(Err(err));
                                }
                                _ => unreachable!(),
                            }
                            return Ok(None);
                        }
                    };

                    fulfill_pdf_request(&document, req);

                    // Keep the parsed document warm for a bounded burst of
                    // same-session Render/ExtractText. Cap the burst so other
                    // sessions (Open/Close/render) are not starved.
                    const MAX_WARM_FOLLOWUPS: usize = 16;
                    let mut followups = 0usize;
                    loop {
                        match next_request(&rx, &mut inbox) {
                            Some(
                                next @ (WorkerRequest::Render {
                                    session: next_session,
                                    ..
                                }
                                | WorkerRequest::ExtractText {
                                    session: next_session,
                                    ..
                                }),
                            ) if next_session == session => {
                                if followups >= MAX_WARM_FOLLOWUPS {
                                    // Yield: re-queue this request so the main
                                    // loop can service other sessions first.
                                    return Ok(Some(next));
                                }
                                fulfill_pdf_request(&document, next);
                                followups += 1;
                            }
                            Some(other) => return Ok(Some(other)),
                            None => return Ok(None),
                        }
                    }
                });

                match interrupted {
                    Ok(Some(other)) => inbox.push_front(other),
                    Ok(None) => {}
                    Err(e) => {
                        // Bind already succeeded above; unexpected here.
                        tracing::error!(error = %e, "pdf worker session failed unexpectedly");
                    }
                }
            }
        }
    }
}

fn fulfill_pdf_request(document: &PdfDocument<'_>, req: WorkerRequest) {
    match req {
        WorkerRequest::Render {
            page,
            viewport,
            fit_mode,
            zoom,
            reply,
            ..
        } => {
            let rendered = render_from_document(document, page, viewport, fit_mode, zoom);
            let _ = reply.send(rendered);
        }
        WorkerRequest::ExtractText { page, reply, .. } => {
            let extracted = extract_text_from_document(document, page);
            let _ = reply.send(extracted);
        }
        WorkerRequest::Open { .. } | WorkerRequest::Close { .. } => {
            unreachable!("fulfill_pdf_request only handles Render/ExtractText")
        }
    }
}

fn next_request(
    rx: &Receiver<WorkerRequest>,
    inbox: &mut VecDeque<WorkerRequest>,
) -> Option<WorkerRequest> {
    inbox.pop_front().or_else(|| rx.recv().ok())
}

/// Open a PDF in the worker and return `(session, page_count)`.
///
/// # Errors
///
/// Propagates Pdfium load failures.
pub fn open_document(bytes: Arc<Vec<u8>>) -> Result<(PdfSessionId, u32)> {
    let (reply_tx, reply_rx) = mpsc::channel();
    worker_sender()
        .send(WorkerRequest::Open {
            bytes,
            reply: reply_tx,
        })
        .map_err(|_| ViewerError::PdfUnavailable)?;
    reply_rx.recv().map_err(|_| ViewerError::PdfUnavailable)?
}

/// Drop a previously opened session.
pub fn close_document(session: PdfSessionId) {
    let (reply_tx, reply_rx) = mpsc::channel();
    if worker_sender()
        .send(WorkerRequest::Close {
            session,
            reply: reply_tx,
        })
        .is_ok()
    {
        let _ = reply_rx.recv();
    }
}

/// Render `page` (1-based) from an opened session.
///
/// # Errors
///
/// Propagates Pdfium / decode failures as [`ViewerError`].
pub fn render_page(
    session: PdfSessionId,
    page: u32,
    viewport: (f32, f32),
    fit_mode: FitMode,
    zoom: f32,
) -> Result<RenderedPage> {
    let (reply_tx, reply_rx) = mpsc::channel();
    worker_sender()
        .send(WorkerRequest::Render {
            session,
            page,
            viewport,
            fit_mode,
            zoom,
            reply: reply_tx,
        })
        .map_err(|_| ViewerError::PdfUnavailable)?;
    reply_rx.recv().map_err(|_| ViewerError::PdfUnavailable)?
}

/// Extract Unicode text for `page` (1-based) from an opened session.
///
/// # Errors
///
/// Propagates Pdfium failures as [`ViewerError`].
pub fn extract_page_text(session: PdfSessionId, page: u32) -> Result<String> {
    let (reply_tx, reply_rx) = mpsc::channel();
    worker_sender()
        .send(WorkerRequest::ExtractText {
            session,
            page,
            reply: reply_tx,
        })
        .map_err(|_| ViewerError::PdfUnavailable)?;
    reply_rx.recv().map_err(|_| ViewerError::PdfUnavailable)?
}

/// One-shot helper for tests: open bytes, render, then close.
///
/// # Errors
///
/// Propagates Pdfium failures.
#[cfg(test)]
pub fn render_page_from_bytes(
    bytes: &[u8],
    page: u32,
    viewport: (f32, f32),
    fit_mode: FitMode,
    zoom: f32,
) -> Result<RenderedPage> {
    let bytes = Arc::new(bytes.to_vec());
    let (session, _) = open_document(bytes)?;
    let rendered = render_page(session, page, viewport, fit_mode, zoom);
    close_document(session);
    rendered
}

fn extract_text_from_document(document: &PdfDocument<'_>, page: u32) -> Result<String> {
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
    let text = pdf_page
        .text()
        .map_err(|e| ViewerError::PdfRender {
            page: current_page,
            reason: format!("load text: {e}"),
        })?
        .all();
    Ok(text)
}

fn render_from_document(
    document: &PdfDocument<'_>,
    page: u32,
    viewport: (f32, f32),
    fit_mode: FitMode,
    zoom: f32,
) -> Result<RenderedPage> {
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
        let page = render_page_from_bytes(MINIMAL_PDF, 1, (640.0, 480.0), FitMode::FitWidth, 1.0)
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

    #[test]
    fn reuse_open_document_across_renders() {
        let bytes = Arc::new(MINIMAL_PDF.to_vec());
        let (session, count) = open_document(Arc::clone(&bytes)).expect("open");
        assert_eq!(count, 1);
        let a = render_page(session, 1, (640.0, 480.0), FitMode::FitWidth, 1.0).expect("render a");
        let b = render_page(session, 1, (800.0, 600.0), FitMode::FitPage, 1.0).expect("render b");
        close_document(session);
        assert_eq!(a.page_count, 1);
        assert_eq!(b.page_count, 1);
        assert!(b.width_px > 0);
    }
}
