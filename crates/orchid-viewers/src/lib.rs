//! Content viewers for Orchid: images, PDF (stub), text with syntax
//! highlighting, archives, and thumbnails.

#![warn(missing_docs)]
#![warn(clippy::all)]
#![allow(clippy::result_large_err)]

pub mod archive;
pub mod dispatch;
pub mod error;
pub mod image;
pub mod pdf;
pub mod snapshot;
pub mod text;
pub mod thumbnail;
pub mod viewer_trait;

pub use archive::ArchiveViewer;
pub use dispatch::{kind_for, select_viewer, ViewerKind};
pub use error::{Result, ViewerError};
pub use image::{ImageFormat, ImageViewer, LoadedImage, ViewTransform};
pub use pdf::PdfViewer;
pub use snapshot::{
    ArchiveEntryView, ArchivePreview, ArchiveSnapshot, ImageSnapshot, PdfSnapshot,
    SelectionRange, SyntaxLine, SyntaxScope, SyntaxSegment, TextSnapshot, ViewerSnapshot,
};
pub use text::{
    CursorPos, LineEnding, SyntaxHighlighter, TextBuffer, TextOp, TextOpKind, TextViewer,
    TextViewerMode, UndoStack,
};
pub use thumbnail::{Thumbnail, ThumbnailCache, ThumbnailService, ThumbnailSize};
pub use viewer_trait::Viewer;

/// Crate version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
