//! Synchronous Pdfium page rendering.
//!
//! Rendering runs on a dedicated worker thread so a loaded document can be
//! reused across page / zoom / viewport changes without re-parsing the PDF
//! bytes on every frame.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TryRecvError};
use std::sync::{Arc, OnceLock};

/// Bound on the worker inbox. Excess sends block the caller (back-pressure);
/// [`take_coalesced`] still drops superseded renders already in the queue.
const WORKER_QUEUE_CAP: usize = 32;

use pdfium_render::prelude::*;

use super::bindings::with_pdfium;
use crate::error::{Result, ViewerError};

/// How the viewer chooses the raster width for the current page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct RasterKey {
    session: PdfSessionId,
    page: u32,
    vw: u32,
    vh: u32,
    fit: FitMode,
    zoom_milli: u32,
}

impl RasterKey {
    fn new(
        session: PdfSessionId,
        page: u32,
        viewport: (f32, f32),
        fit: FitMode,
        zoom: f32,
    ) -> Self {
        Self {
            session,
            page,
            vw: quantize_px(viewport.0),
            vh: quantize_px(viewport.1),
            fit,
            zoom_milli: (zoom.clamp(0.05, 16.0) * 100.0).round() as u32,
        }
    }
}

fn quantize_px(v: f32) -> u32 {
    ((v.max(1.0) / 8.0).round() as u32).max(1) * 8
}

/// LRU of rasterized pages so page/zoom toggles skip Pdfium when the
/// quantized viewport matches a recent render.
struct RasterLru {
    map: HashMap<RasterKey, RenderedPage>,
    order: VecDeque<RasterKey>,
    bytes: usize,
}

impl RasterLru {
    const MAX_BYTES: usize = 32 * 1024 * 1024;
    const MAX_PAGES: usize = 8;

    fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
        }
    }

    fn get(&mut self, key: &RasterKey) -> Option<RenderedPage> {
        if !self.map.contains_key(key) {
            return None;
        }
        self.touch(*key);
        self.map.get(key).cloned()
    }

    fn insert(&mut self, key: RasterKey, page: RenderedPage) {
        if let Some(old) = self.map.remove(&key) {
            self.bytes = self.bytes.saturating_sub(old.rgba.len());
            self.order.retain(|k| k != &key);
        }
        self.bytes = self.bytes.saturating_add(page.rgba.len());
        self.map.insert(key, page);
        self.order.push_back(key);
        self.evict();
    }

    fn touch(&mut self, key: RasterKey) {
        self.order.retain(|k| k != &key);
        self.order.push_back(key);
    }

    fn evict(&mut self) {
        while self.map.len() > Self::MAX_PAGES || self.bytes > Self::MAX_BYTES {
            let Some(old_key) = self.order.pop_front() else {
                break;
            };
            if let Some(old) = self.map.remove(&old_key) {
                self.bytes = self.bytes.saturating_sub(old.rgba.len());
            }
        }
    }

    fn remove_session(&mut self, session: PdfSessionId) {
        self.order.retain(|k| {
            if k.session == session {
                if let Some(old) = self.map.remove(k) {
                    self.bytes = self.bytes.saturating_sub(old.rgba.len());
                }
                false
            } else {
                true
            }
        });
    }
}

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

static WORKER_TX: OnceLock<SyncSender<WorkerRequest>> = OnceLock::new();

fn worker_sender() -> SyncSender<WorkerRequest> {
    WORKER_TX
        .get_or_init(|| {
            let (tx, rx) = mpsc::sync_channel(WORKER_QUEUE_CAP);
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
    let mut raster_cache = RasterLru::new();

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
                    let count = page_count_u32(document.pages().len());
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
                raster_cache.remove_session(session);
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

                    fulfill_pdf_request(&document, req, &mut raster_cache);

                    // Keep the parsed document warm for a bounded burst of
                    // same-session Render/ExtractText. Cap the burst so other
                    // sessions (Open/Close/render) are not starved.
                    const MAX_WARM_FOLLOWUPS: usize = 64;
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
                                fulfill_pdf_request(&document, next, &mut raster_cache);
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

fn fulfill_pdf_request(document: &PdfDocument<'_>, req: WorkerRequest, cache: &mut RasterLru) {
    match req {
        WorkerRequest::Render {
            session,
            page,
            viewport,
            fit_mode,
            zoom,
            reply,
        } => {
            let key = RasterKey::new(session, page, viewport, fit_mode, zoom);
            if let Some(hit) = cache.get(&key) {
                let _ = reply.send(Ok(hit));
                return;
            }
            let rendered = render_from_document(document, page, viewport, fit_mode, zoom);
            if let Ok(ref page) = rendered {
                cache.insert(key, page.clone());
            }
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

fn drain_pending(rx: &Receiver<WorkerRequest>, inbox: &mut VecDeque<WorkerRequest>) {
    loop {
        match rx.try_recv() {
            Ok(req) => inbox.push_back(req),
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => return,
        }
    }
}

/// Pull the next request, draining the channel first so a burst of page
/// flips collapses to the latest render per session.
fn next_request(
    rx: &Receiver<WorkerRequest>,
    inbox: &mut VecDeque<WorkerRequest>,
) -> Option<WorkerRequest> {
    if inbox.is_empty() {
        match rx.recv() {
            Ok(req) => inbox.push_back(req),
            Err(_) => return None,
        }
    }
    drain_pending(rx, inbox);
    take_coalesced(inbox)
}

fn take_coalesced(inbox: &mut VecDeque<WorkerRequest>) -> Option<WorkerRequest> {
    let first = inbox.pop_front()?;
    if let WorkerRequest::Render { session, .. } = &first {
        let session = *session;
        return Some(take_latest_render(inbox, first, session));
    }
    Some(first)
}

fn take_latest_render(
    inbox: &mut VecDeque<WorkerRequest>,
    mut latest: WorkerRequest,
    session: PdfSessionId,
) -> WorkerRequest {
    let mut i = 0;
    while i < inbox.len() {
        let same_session_render = matches!(
            &inbox[i],
            WorkerRequest::Render {
                session: next_session,
                ..
            } if *next_session == session
        );
        if same_session_render {
            let newer = inbox.remove(i).expect("index still in range");
            if let WorkerRequest::Render { reply, .. } = std::mem::replace(&mut latest, newer) {
                let _ = reply.send(Err(ViewerError::PdfStale));
            }
        } else {
            i += 1;
        }
    }
    latest
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

/// Rasterize the first page to RGBA8 with the long edge capped at `max_edge`.
pub(crate) fn rasterize_first_page(bytes: &[u8], max_edge: u32) -> Result<(Vec<u8>, u32, u32)> {
    with_pdfium(|pdfium| {
        let document =
            pdfium
                .load_pdf_from_byte_slice(bytes, None)
                .map_err(|e| ViewerError::PdfRender {
                    page: 1,
                    reason: format!("load document: {e}"),
                })?;
        let count = page_count_u32(document.pages().len());
        if count == 0 {
            return Err(ViewerError::PdfEmpty);
        }
        let pdf_page = document
            .pages()
            .get(0)
            .map_err(|e| ViewerError::PdfRender {
                page: 1,
                reason: format!("open page: {e}"),
            })?;
        let page_w = pdf_page.width().value.max(1.0);
        let page_h = pdf_page.height().value.max(1.0);
        let long = page_w.max(page_h);
        let cap = max_edge.max(1) as f32;
        let target_width = ((page_w / long) * cap).round().clamp(1.0, cap) as i32;
        let config = PdfRenderConfig::new().set_target_width(target_width);
        let bitmap = pdf_page
            .render_with_config(&config)
            .map_err(|e| ViewerError::PdfRender {
                page: 1,
                reason: format!("render: {e}"),
            })?;
        let image = bitmap
            .as_image()
            .map_err(|e| ViewerError::PdfRender {
                page: 1,
                reason: format!("image: {e}"),
            })?
            .into_rgba8();
        let (width, height) = image.dimensions();
        Ok((image.into_raw(), width, height))
    })
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
    let page_count = page_count_u32(document.pages().len());
    if page_count == 0 {
        return Err(ViewerError::PdfEmpty);
    }
    let current_page = page.clamp(1, page_count);
    let pdf_page = document
        .pages()
        .get(page_index(current_page))
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
    let page_count = page_count_u32(document.pages().len());
    if page_count == 0 {
        return Err(ViewerError::PdfEmpty);
    }

    let current_page = page.clamp(1, page_count);
    let pdf_page = document
        .pages()
        .get(page_index(current_page))
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

    let image = bitmap
        .as_image()
        .map_err(|e| ViewerError::PdfRender {
            page: current_page,
            reason: format!("image: {e}"),
        })?
        .into_rgba8();
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

/// Convert pdfium's signed page count (`PdfPageIndex` / `c_int`) to `u32`.
fn page_count_u32(len: impl Into<i32>) -> u32 {
    len.into().max(0) as u32
}

/// 1-based UI page number → 0-based pdfium page index.
fn page_index(current_page: u32) -> i32 {
    current_page.saturating_sub(1) as i32
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
    fn raster_cache_returns_same_pixels_for_repeat_render() {
        let bytes = Arc::new(MINIMAL_PDF.to_vec());
        let (session, _) = open_document(Arc::clone(&bytes)).expect("open");
        let a = render_page(session, 1, (640.0, 480.0), FitMode::FitWidth, 1.0).expect("a");
        let b = render_page(session, 1, (640.0, 480.0), FitMode::FitWidth, 1.0).expect("b");
        close_document(session);
        assert_eq!(a.width_px, b.width_px);
        assert_eq!(a.height_px, b.height_px);
        assert_eq!(a.rgba.as_ref(), b.rgba.as_ref());
    }

    fn render_req(
        session: u64,
        page: u32,
    ) -> (
        WorkerRequest,
        std::sync::mpsc::Receiver<Result<RenderedPage>>,
    ) {
        let (reply, rx) = std::sync::mpsc::channel();
        (
            WorkerRequest::Render {
                session: PdfSessionId(session),
                page,
                viewport: (100.0, 100.0),
                fit_mode: FitMode::FitWidth,
                zoom: 1.0,
                reply,
            },
            rx,
        )
    }

    #[test]
    fn coalesce_keeps_latest_render_per_session() {
        let (a, ra) = render_req(1, 1);
        let (other, _ro) = render_req(2, 9);
        let (b, rb) = render_req(1, 3);
        let mut inbox = VecDeque::from([a, other, b]);
        let taken = take_coalesced(&mut inbox).expect("request");
        match taken {
            WorkerRequest::Render { session, page, .. } => {
                assert_eq!(session, PdfSessionId(1));
                assert_eq!(page, 3);
            }
            _ => panic!("expected render"),
        }
        assert!(matches!(ra.try_recv(), Ok(Err(ViewerError::PdfStale))));
        assert!(rb.try_recv().is_err());
        match inbox.pop_front() {
            Some(WorkerRequest::Render { session, page, .. }) => {
                assert_eq!(session, PdfSessionId(2));
                assert_eq!(page, 9);
            }
            _ => panic!("other session should remain"),
        }
        assert!(inbox.is_empty());
    }

    #[test]
    fn coalesce_leaves_non_render_requests() {
        let (reply, _rx) = std::sync::mpsc::channel();
        let mut inbox = VecDeque::from([WorkerRequest::Close {
            session: PdfSessionId(1),
            reply,
        }]);
        assert!(matches!(
            take_coalesced(&mut inbox),
            Some(WorkerRequest::Close { .. })
        ));
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
