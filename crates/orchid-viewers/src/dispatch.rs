//! Dispatch a path to the appropriate viewer implementation.

use std::sync::Arc;

use crate::archive::ArchiveViewer;
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

/// Pick a viewer kind by sniffing magic bytes from `sample` with a fall
/// back to the file extension. Pure — does not touch the filesystem.
#[must_use]
pub fn kind_for(path: &orchid_fs::FsPath, sample: &[u8]) -> Option<ViewerKind> {
    // OOXML Office files are ZIP containers — check extension before archive magic
    // so `.docx` does not open as a generic archive browser.
    // TODO: sniff `[Content_Types].xml` inside the zip to distinguish xlsx/pptx.
    if is_docx_path(path) && looks_like_zip(sample) {
        return Some(ViewerKind::Document);
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
        || crate::image::loader::looks_like_raw(sample)
    {
        return Some(ViewerKind::Image);
    }
    // Fall back to the extension for path-only dispatch (e.g. text files).
    if let Some(ext) = extension_of(path) {
        return match ext.as_str() {
            "pdf" => Some(ViewerKind::Pdf),
            "docx" | "docm" => Some(ViewerKind::Document),
            "zip" | "7z" | "tar" | "tgz" | "gz" | "xz" | "txz" => Some(ViewerKind::Archive),
            other if crate::html::is_html_file_extension(other) => Some(ViewerKind::Html),
            other if crate::media::is_media_file_extension(other) => Some(ViewerKind::Media),
            other if crate::image::loader::is_image_file_extension(other) => {
                Some(ViewerKind::Image)
            }
            _ => Some(ViewerKind::Text),
        };
    }
    // Empty files / unknown extensions → assume text so the user sees
    // *something* rather than an error.
    Some(ViewerKind::Text)
}

/// Pick a viewer instance for `path`. Reads at most 4 KiB from the
/// provider for magic-byte sniffing.
///
/// # Errors
///
/// Propagates provider / IO errors and returns
/// [`ViewerError::UnsupportedType`] when no viewer matches.
pub async fn select_viewer(
    path: &orchid_fs::FsPath,
    registry: Arc<orchid_fs::FsProviderRegistry>,
    highlighter: Arc<SyntaxHighlighter>,
) -> Result<Box<dyn Viewer>> {
    let provider = registry
        .for_path(path)
        .ok_or_else(|| orchid_fs::FsError::ProviderNotFound(path.scheme().to_string()))?;
    // Read a small head only — archives usually recognise in the first 512 B.
    // Avoid `provider.read` here: opening a large image/PDF must not load the
    // whole file just to sniff magic bytes (open() reads again below).
    let sample = orchid_fs::read_prefix(provider.as_ref(), path, 4096)
        .await
        .map_err(ViewerError::Fs)?;

    let kind = kind_for(path, &sample).ok_or_else(|| ViewerError::UnsupportedType {
        mime: None,
        extension: extension_of(path),
    })?;

    let viewer: Box<dyn Viewer> = match kind {
        ViewerKind::Image => Box::new(ImageViewer::new()),
        ViewerKind::Pdf => Box::new(PdfViewer::new()),
        ViewerKind::Text => Box::new(TextViewer::new(highlighter)),
        ViewerKind::Archive => Box::new(ArchiveViewer::new()),
        ViewerKind::Document => Box::new(DocumentViewer::new()),
        ViewerKind::Media => Box::new(MediaViewer::new()),
        ViewerKind::Html => Box::new(HtmlViewer::new()),
    };
    Ok(viewer)
}

fn extension_of(path: &orchid_fs::FsPath) -> Option<String> {
    let name = path.file_name()?;
    let (_, ext) = name.rsplit_once('.')?;
    Some(ext.to_ascii_lowercase())
}

fn is_docx_path(path: &orchid_fs::FsPath) -> bool {
    matches!(extension_of(path).as_deref(), Some("docx") | Some("docm"))
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
        for ext in ["cr2", "nef", "arw", "dng", "raf", "orf", "rw2"] {
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
    }

    #[test]
    fn avif_extension_still_routes_to_image() {
        let kind = kind_for(&path("local:/a/b.avif"), b"").unwrap();
        assert_eq!(kind, ViewerKind::Image);
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
