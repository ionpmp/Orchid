//! Dispatch a path to the appropriate viewer implementation.

use std::sync::Arc;

use crate::archive::ArchiveViewer;
use crate::error::{Result, ViewerError};
use crate::image::ImageViewer;
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
}

/// Pick a viewer kind by sniffing magic bytes from `sample` with a fall
/// back to the file extension. Pure — does not touch the filesystem.
#[must_use]
pub fn kind_for(path: &orchid_fs::FsPath, sample: &[u8]) -> Option<ViewerKind> {
    // Archive signatures win outright.
    if orchid_fs::detect_format(sample).is_some() {
        return Some(ViewerKind::Archive);
    }
    if sample.starts_with(b"%PDF-") {
        return Some(ViewerKind::Pdf);
    }
    if image::guess_format(sample).is_ok() || crate::image::loader::looks_like_svg(sample) {
        return Some(ViewerKind::Image);
    }
    // Fall back to the extension for path-only dispatch (e.g. text files).
    if let Some(ext) = extension_of(path) {
        return match ext.as_str() {
            "pdf" => Some(ViewerKind::Pdf),
            "zip" | "7z" | "tar" | "tgz" | "gz" | "xz" | "txz" => Some(ViewerKind::Archive),
            "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" | "tiff" | "tif" | "avif" | "tga"
            | "svg" => Some(ViewerKind::Image),
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
    // Read a small head — archives usually recognise in the first 512 B.
    let sample = match provider.read(path).await {
        Ok(bytes) => bytes.into_iter().take(4096).collect::<Vec<_>>(),
        Err(e) => return Err(ViewerError::Fs(e)),
    };

    let kind = kind_for(path, &sample).ok_or_else(|| ViewerError::UnsupportedType {
        mime: None,
        extension: extension_of(path),
    })?;

    let viewer: Box<dyn Viewer> = match kind {
        ViewerKind::Image => Box::new(ImageViewer::new()),
        ViewerKind::Pdf => Box::new(PdfViewer::new()),
        ViewerKind::Text => Box::new(TextViewer::new(highlighter)),
        ViewerKind::Archive => Box::new(ArchiveViewer::new()),
    };
    Ok(viewer)
}

fn extension_of(path: &orchid_fs::FsPath) -> Option<String> {
    let name = path.file_name()?;
    let (_, ext) = name.rsplit_once('.')?;
    Some(ext.to_ascii_lowercase())
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
}
