//! Dispatch a path to the appropriate viewer implementation.

use std::path::Path;
use std::sync::Arc;

use orchid_crypto::ChunkStore;

use crate::archive::ArchiveViewer;
use crate::document::orchid_io::{
    is_docx_raw_meta, looks_like_orchid, materialize_orchid_raw_temp, peek_orchid_raw_meta,
    DOCX_MIME,
};
use crate::document::DocumentViewer;
use crate::error::{Result, ViewerError};
use crate::html::HtmlViewer;
use crate::image::ImageViewer;
use crate::media::MediaViewer;
use crate::pdf::PdfViewer;
use crate::text::{SyntaxHighlighter, TextViewer};
use crate::viewer_trait::Viewer;

/// What kind of viewer should handle this path.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerKind {
    Image,
    Pdf,
    Text,
    Archive,
    Document,
    Media,
    Html,
}

/// Viewer instance plus the path that should be passed to [`Viewer::open`].
///
/// For non-document `.orchid` wraps, `open_path` is a temp file containing the
/// Raw payload so Image/Pdf/… viewers can open the unwrapped bytes.
pub struct SelectedViewer {
    /// Concrete viewer implementation.
    pub viewer: Box<dyn Viewer>,
    /// Path for [`Viewer::open`] (may differ from the user-facing file).
    pub open_path: orchid_fs::FsPath,
}

/// Pick a viewer kind by sniffing magic bytes from `sample` with a fall
/// back to the file extension. Pure — does not touch the filesystem.
#[must_use]
pub fn kind_for(path: &orchid_fs::FsPath, sample: &[u8]) -> Option<ViewerKind> {
    // Native `.orchid` container — default Document; [`select_viewer`] peeks
    // Raw TOC to unwrap wraps into Image/Pdf/….
    if looks_like_orchid(sample)
        || extension_of(path)
            .as_deref()
            .is_some_and(|e| e.eq_ignore_ascii_case("orchid"))
    {
        return Some(ViewerKind::Document);
    }
    kind_for_payload(path, sample)
}

/// Classify by Raw TOC name / MIME and optional Raw head bytes (no `.orchid`
/// short-circuit). Used when opening wrapped payloads.
#[must_use]
pub fn kind_for_orchid_raw(
    raw_name: Option<&str>,
    content_type: Option<&str>,
    raw_sample: &[u8],
) -> ViewerKind {
    if is_docx_raw_meta(raw_name, content_type) {
        return ViewerKind::Document;
    }
    // Prefer extension from Raw TOC name without requiring a valid FsPath
    // (names may contain spaces).
    if let Some(ext) = raw_name
        .and_then(|n| Path::new(n).extension())
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
    {
        if let Some(kind) = kind_from_extension(&ext) {
            // Still honor magic when sample present (e.g. vector .ai as PDF bytes).
            if !raw_sample.is_empty() {
                if let Ok(synth) = orchid_fs::FsPath::new(&format!("local:/unwrap/x.{ext}")) {
                    if let Some(k) = kind_for_payload(&synth, raw_sample) {
                        return k;
                    }
                }
            }
            return kind;
        }
    }
    if let Some(kind) = kind_from_mime(content_type) {
        return kind;
    }
    if !raw_sample.is_empty() {
        if let Ok(anon) = orchid_fs::FsPath::new("local:/unwrap.bin") {
            if let Some(kind) = kind_for_payload(&anon, raw_sample) {
                if kind != ViewerKind::Text {
                    return kind;
                }
            }
        }
    }
    // Editor envelopes and unknown Raw → document (Clean-Text / DOCX path).
    ViewerKind::Document
}

fn kind_from_mime(content_type: Option<&str>) -> Option<ViewerKind> {
    let ct = content_type?.trim().to_ascii_lowercase();
    let base = ct.split(';').next()?.trim();
    if base.eq_ignore_ascii_case(DOCX_MIME) || base.contains("wordprocessingml") {
        return Some(ViewerKind::Document);
    }
    if base == "application/pdf" {
        return Some(ViewerKind::Pdf);
    }
    if base.starts_with("image/") {
        return Some(ViewerKind::Image);
    }
    if base.starts_with("audio/") || base.starts_with("video/") {
        return Some(ViewerKind::Media);
    }
    if base == "text/html" || base == "application/xhtml+xml" {
        return Some(ViewerKind::Html);
    }
    if base.starts_with("text/") {
        return Some(ViewerKind::Text);
    }
    if matches!(
        base,
        "application/zip"
            | "application/x-zip-compressed"
            | "application/x-7z-compressed"
            | "application/x-tar"
            | "application/gzip"
            | "application/x-xz"
    ) {
        return Some(ViewerKind::Archive);
    }
    None
}

fn kind_for_payload(path: &orchid_fs::FsPath, sample: &[u8]) -> Option<ViewerKind> {
    // OOXML Office files are ZIP containers — check extension before archive magic
    // so `.docx` does not open as a generic archive browser.
    // TODO: sniff `[Content_Types].xml` inside the zip to distinguish xlsx/pptx.
    if is_docx_path(path) && looks_like_zip(sample) {
        return Some(ViewerKind::Document);
    }
    // PDF-based .ai and ZIP-based .cdr / .svgz must not steal the PDF / archive viewers.
    if is_vector_image_path(path) {
        return Some(ViewerKind::Image);
    }
    // Archive signatures win outright.
    if orchid_fs::detect_format(sample).is_some() {
        return Some(ViewerKind::Archive);
    }
    if sample.starts_with(b"%PDF-") {
        return Some(ViewerKind::Pdf);
    }
    if image::guess_format(sample).is_ok()
        || crate::image::loader::looks_like_svg(sample)
        || crate::image::loader::looks_like_heic(sample)
        || crate::image::loader::looks_like_avif(sample)
        || crate::image::loader::looks_like_raw(sample)
        || crate::image::extra::looks_like_extra_image(sample)
        || crate::image::vector::looks_like_vector(sample)
    {
        return Some(ViewerKind::Image);
    }
    // Fall back to the extension for path-only dispatch (e.g. text files).
    if let Some(ext) = extension_of(path) {
        return kind_from_extension(&ext);
    }
    // Empty files / unknown extensions → assume text so the user sees
    // *something* rather than an error.
    Some(ViewerKind::Text)
}

fn kind_from_extension(ext: &str) -> Option<ViewerKind> {
    match ext {
        "pdf" => Some(ViewerKind::Pdf),
        "docx" | "docm" => Some(ViewerKind::Document),
        "orchid" => Some(ViewerKind::Document),
        "zip" | "7z" | "tar" | "tgz" | "gz" | "xz" | "txz" => Some(ViewerKind::Archive),
        other if crate::html::is_html_file_extension(other) => Some(ViewerKind::Html),
        other if crate::media::is_media_file_extension(other) => Some(ViewerKind::Media),
        other if crate::image::loader::is_image_file_extension(other) => Some(ViewerKind::Image),
        other if crate::image::vector::is_vector_extension(other) => Some(ViewerKind::Image),
        _ => Some(ViewerKind::Text),
    }
}

/// Pick a viewer instance for `path`. Reads at most 4 KiB from the
/// provider for magic-byte sniffing.
///
/// When `path` is a `.orchid` wrap whose Raw is not a DOCX envelope, Raw is
/// materialized to a temp file and `open_path` points there. Linked wraps need
/// `chunk_store`.
///
/// # Errors
///
/// Propagates provider / IO errors and returns
/// [`ViewerError::UnsupportedType`] when no viewer matches.
pub async fn select_viewer(
    path: &orchid_fs::FsPath,
    registry: Arc<orchid_fs::FsProviderRegistry>,
    highlighter: Arc<SyntaxHighlighter>,
    chunk_store: Option<&ChunkStore>,
) -> Result<SelectedViewer> {
    let provider = registry
        .for_path(path)
        .ok_or_else(|| orchid_fs::FsError::ProviderNotFound(path.scheme().to_string()))?;
    // Read a small head only — archives usually recognise in the first 512 B.
    // Avoid `provider.read` here: opening a large image/PDF must not load the
    // whole file just to sniff magic bytes (open() reads again below).
    let sample = orchid_fs::read_prefix(provider.as_ref(), path, 4096)
        .await
        .map_err(ViewerError::Fs)?;

    let is_orchid = looks_like_orchid(&sample)
        || extension_of(path)
            .as_deref()
            .is_some_and(|e| e.eq_ignore_ascii_case("orchid"));

    if is_orchid {
        return select_orchid_viewer(path, highlighter, chunk_store).await;
    }

    let kind = kind_for(path, &sample).ok_or_else(|| ViewerError::UnsupportedType {
        mime: None,
        extension: extension_of(path),
    })?;

    Ok(SelectedViewer {
        viewer: viewer_for_kind(kind, highlighter),
        open_path: path.clone(),
    })
}

async fn select_orchid_viewer(
    path: &orchid_fs::FsPath,
    highlighter: Arc<SyntaxHighlighter>,
    chunk_store: Option<&ChunkStore>,
) -> Result<SelectedViewer> {
    let os = path
        .to_local()
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    let (raw_name, raw_ctype) = peek_orchid_raw_meta(&os)?;
    let kind = kind_for_orchid_raw(
        raw_name.as_deref(),
        raw_ctype.as_deref(),
        &[], // TOC name/MIME first; materialize only when unwrapping
    );

    if kind == ViewerKind::Document {
        return Ok(SelectedViewer {
            viewer: viewer_for_kind(ViewerKind::Document, highlighter),
            open_path: path.clone(),
        });
    }

    let tmp = materialize_orchid_raw_temp(&os, chunk_store, raw_name.as_deref()).await?;
    // Re-sniff with real Raw head for ZIP-vs-DOCX edge cases lacking TOC MIME.
    let head = read_local_prefix(&tmp, 4096).unwrap_or_default();
    let kind = kind_for_orchid_raw(raw_name.as_deref(), raw_ctype.as_deref(), &head);
    if kind == ViewerKind::Document {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Ok(SelectedViewer {
            viewer: viewer_for_kind(ViewerKind::Document, highlighter),
            open_path: path.clone(),
        });
    }

    let open_path =
        orchid_fs::FsPath::from_local(&tmp).map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(SelectedViewer {
        viewer: viewer_for_kind(kind, highlighter),
        open_path,
    })
}

fn read_local_prefix(path: &Path, max: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; max];
    let n = f.read(&mut buf)?;
    buf.truncate(n);
    Ok(buf)
}

fn viewer_for_kind(kind: ViewerKind, highlighter: Arc<SyntaxHighlighter>) -> Box<dyn Viewer> {
    match kind {
        ViewerKind::Image => Box::new(ImageViewer::new()),
        ViewerKind::Pdf => Box::new(PdfViewer::new()),
        ViewerKind::Text => Box::new(TextViewer::new(highlighter)),
        ViewerKind::Archive => Box::new(ArchiveViewer::new()),
        ViewerKind::Document => Box::new(DocumentViewer::new()),
        ViewerKind::Media => Box::new(MediaViewer::new()),
        ViewerKind::Html => Box::new(HtmlViewer::new()),
    }
}

fn extension_of(path: &orchid_fs::FsPath) -> Option<String> {
    let name = path.file_name()?;
    let (_, ext) = name.rsplit_once('.')?;
    Some(ext.to_ascii_lowercase())
}

fn is_docx_path(path: &orchid_fs::FsPath) -> bool {
    matches!(extension_of(path).as_deref(), Some("docx") | Some("docm"))
}

fn is_vector_image_path(path: &orchid_fs::FsPath) -> bool {
    extension_of(path).is_some_and(|ext| crate::image::vector::is_vector_extension(&ext))
}

fn looks_like_zip(sample: &[u8]) -> bool {
    sample.starts_with(b"PK\x03\x04")
        || sample.starts_with(b"PK\x05\x06")
        || sample.starts_with(b"PK\x07\x08")
        || sample.is_empty() // extension-only dispatch when no sample yet
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(s: &str) -> orchid_fs::FsPath {
        orchid_fs::FsPath::new(s).unwrap()
    }

    #[test]
    fn orchid_extension_and_magic_default_to_document() {
        let kind = kind_for(&path("local:/a/b.orchid"), b"").unwrap();
        assert_eq!(kind, ViewerKind::Document);
        let mut sample = vec![0u8; 8];
        sample[0..4].copy_from_slice(b"ORCD");
        let kind = kind_for(&path("local:/a/b.bin"), &sample).unwrap();
        assert_eq!(kind, ViewerKind::Document);
    }

    #[test]
    fn orchid_raw_pdf_name_routes_to_pdf() {
        assert_eq!(
            kind_for_orchid_raw(Some("report.pdf"), Some("application/pdf"), &[]),
            ViewerKind::Pdf
        );
        assert_eq!(
            kind_for_orchid_raw(Some("report.pdf"), None, &[]),
            ViewerKind::Pdf
        );
    }

    #[test]
    fn orchid_raw_png_routes_to_image() {
        assert_eq!(
            kind_for_orchid_raw(Some("shot.png"), Some("image/png"), &[]),
            ViewerKind::Image
        );
    }

    #[test]
    fn orchid_raw_docx_stays_document() {
        assert_eq!(
            kind_for_orchid_raw(Some("document.docx"), Some(DOCX_MIME), &[]),
            ViewerKind::Document
        );
        assert_eq!(
            kind_for_orchid_raw(Some("original.docx"), None, b"PK\x03\x04"),
            ViewerKind::Document
        );
    }

    #[test]
    fn orchid_raw_txt_routes_to_text() {
        assert_eq!(
            kind_for_orchid_raw(Some("note.txt"), Some("text/plain"), &[]),
            ViewerKind::Text
        );
    }

    #[test]
    fn pdf_magic_wins() {
        let kind = kind_for(&path("local:/a/b.unknown"), b"%PDF-1.4\n").unwrap();
        assert_eq!(kind, ViewerKind::Pdf);
    }

    #[test]
    fn zip_magic_wins() {
        let kind = kind_for(&path("local:/a/b.unknown"), b"PK\x03\x04rest").unwrap();
        assert_eq!(kind, ViewerKind::Archive);
    }

    #[test]
    fn docx_extension_with_zip_magic_is_document_not_archive() {
        let kind = kind_for(&path("local:/a/b.docx"), b"PK\x03\x04rest").unwrap();
        assert_eq!(kind, ViewerKind::Document);
    }

    #[test]
    fn zip_extension_stays_archive() {
        let kind = kind_for(&path("local:/a/b.zip"), b"PK\x03\x04rest").unwrap();
        assert_eq!(kind, ViewerKind::Archive);
    }

    #[test]
    fn docx_extension_fallback_without_sample() {
        let kind = kind_for(&path("local:/a/b.docx"), b"").unwrap();
        assert_eq!(kind, ViewerKind::Document);
    }

    #[test]
    fn extension_fallback_for_text() {
        let kind = kind_for(&path("local:/a/b.rs"), b"fn main() {}").unwrap();
        assert_eq!(kind, ViewerKind::Text);
    }

    #[test]
    fn image_extension_fallback() {
        let kind = kind_for(&path("local:/a/b.png"), b"").unwrap();
        assert_eq!(kind, ViewerKind::Image);
    }

    #[test]
    fn svg_extension_routes_to_image() {
        let kind = kind_for(&path("local:/a/b.svg"), b"").unwrap();
        assert_eq!(kind, ViewerKind::Image);
    }

    #[test]
    fn svg_magic_routes_to_image() {
        let sample = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"></svg>";
        let kind = kind_for(&path("local:/a/b.unknown"), sample).unwrap();
        assert_eq!(kind, ViewerKind::Image);
    }

    #[test]
    fn xz_magic_routes_to_archive() {
        let sample = b"\xFD\x37\x7A\x58\x5A\x00rest";
        let kind = kind_for(&path("local:/a/b.unknown"), sample).unwrap();
        assert_eq!(kind, ViewerKind::Archive);
    }

    #[test]
    fn xz_extension_fallback() {
        let kind = kind_for(&path("local:/a/b.xz"), b"").unwrap();
        assert_eq!(kind, ViewerKind::Archive);
    }

    #[test]
    fn txz_extension_fallback() {
        let kind = kind_for(&path("local:/a/b.txz"), b"").unwrap();
        assert_eq!(kind, ViewerKind::Archive);
    }

    #[test]
    fn heic_extension_routes_to_image() {
        for ext in ["heic", "heif"] {
            let kind = kind_for(&path(&format!("local:/a/b.{ext}")), b"").unwrap();
            assert_eq!(kind, ViewerKind::Image, "{ext}");
        }
    }

    #[test]
    fn raw_extension_routes_to_image() {
        for ext in [
            "cr2", "cr3", "nef", "arw", "dng", "raf", "orf", "pef", "rw2", "srw", "x3f", "rwl",
            "dcr",
        ] {
            let kind = kind_for(&path(&format!("local:/a/b.{ext}")), b"").unwrap();
            assert_eq!(kind, ViewerKind::Image, "{ext}");
        }
    }

    #[test]
    fn heic_magic_routes_to_image() {
        let mut sample = vec![0, 0, 0, 0x18];
        sample.extend_from_slice(b"ftyp");
        sample.extend_from_slice(b"heic");
        sample.extend_from_slice(&[0, 0, 0, 0]);
        sample.extend_from_slice(b"mif1");
        let kind = kind_for(&path("local:/a/b.unknown"), &sample).unwrap();
        assert_eq!(kind, ViewerKind::Image);
    }

    #[test]
    fn raw_magic_routes_to_image() {
        let kind = kind_for(&path("local:/a/b.unknown"), b"FUJIFILMCCD-RAW \x00").unwrap();
        assert_eq!(kind, ViewerKind::Image);
        let mut cr3 = vec![0, 0, 0, 0x18];
        cr3.extend_from_slice(b"ftyp");
        cr3.extend_from_slice(b"crx ");
        cr3.extend_from_slice(&[0, 0, 0, 0]);
        cr3.extend_from_slice(b"isom");
        assert_eq!(
            kind_for(&path("local:/a/b.bin"), &cr3).unwrap(),
            ViewerKind::Image
        );
    }

    #[test]
    fn avif_extension_still_routes_to_image() {
        let kind = kind_for(&path("local:/a/b.avif"), b"").unwrap();
        assert_eq!(kind, ViewerKind::Image);
    }

    #[test]
    fn extra_raster_extensions_route_to_image() {
        for ext in [
            "jxl", "psd", "xcf", "pcx", "ico", "cur", "jp2", "exr", "hdr", "dds",
        ] {
            let kind = kind_for(&path(&format!("local:/a/b.{ext}")), b"").unwrap();
            assert_eq!(kind, ViewerKind::Image, "{ext}");
        }
    }

    #[test]
    fn vector_extensions_route_to_image_not_pdf_or_archive() {
        assert_eq!(
            kind_for(&path("local:/a/b.ai"), b"%PDF-1.4\n").unwrap(),
            ViewerKind::Image
        );
        assert_eq!(
            kind_for(&path("local:/a/b.cdr"), b"PK\x03\x04rest").unwrap(),
            ViewerKind::Image
        );
        assert_eq!(
            kind_for(&path("local:/a/b.svgz"), b"\x1f\x8b\x08").unwrap(),
            ViewerKind::Image
        );
        assert_eq!(
            kind_for(&path("local:/a/b.eps"), b"%!PS-Adobe-3.0\n").unwrap(),
            ViewerKind::Image
        );
        assert_eq!(
            kind_for(&path("local:/a/b.emf"), b"").unwrap(),
            ViewerKind::Image
        );
    }

    #[test]
    fn extra_magic_routes_to_image() {
        assert_eq!(
            kind_for(&path("local:/a/b.bin"), b"8BPS\0\0").unwrap(),
            ViewerKind::Image
        );
        assert_eq!(
            kind_for(&path("local:/a/b.bin"), b"gimp xcf file\0").unwrap(),
            ViewerKind::Image
        );
        assert_eq!(
            kind_for(&path("local:/a/b.bin"), &[0xFF, 0x0A, 0, 0]).unwrap(),
            ViewerKind::Image
        );
    }

    #[test]
    fn html_extension_routes_to_html() {
        let kind = kind_for(&path("local:/a/b.html"), b"<html></html>").unwrap();
        assert_eq!(kind, ViewerKind::Html);
    }

    #[test]
    fn mp4_extension_routes_to_media() {
        let kind = kind_for(&path("local:/a/b.mp4"), b"").unwrap();
        assert_eq!(kind, ViewerKind::Media);
    }
}
