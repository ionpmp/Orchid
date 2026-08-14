//! Content viewers for Orchid: images, PDF, text with syntax
//! highlighting, archives, and thumbnails.

#![warn(missing_docs)]
#![warn(clippy::all)]
#![allow(clippy::result_large_err)]

pub mod archive;
pub mod audio_tags;
pub mod dispatch;
pub mod document;
pub mod error;
pub mod html;
pub mod image;
pub mod media;
pub mod pdf;
pub mod snapshot;
pub mod text;
pub mod thumbnail;
pub mod viewer_trait;

pub use archive::ArchiveViewer;
pub use audio_tags::{format_id3_report, is_id3_extension, read_id3_fields, AudioTagField};
pub use dispatch::{kind_for, select_viewer, ViewerKind};
pub use document::ooxml::core_props::{
    format_office_report, is_office_extension, pack_office_props, read_office_core_props,
    unpack_office_props, write_office_core_props, OfficeCoreProps,
};
pub use document::{
    create_sample_docx, sample_document, Alignment, Block, CellImage, Document, DocumentViewer,
    EditCommand, ListKind, UndoStack as DocumentUndoStack,
};
pub use error::{Result, ViewerError};
pub use html::{is_html_file_extension, HtmlViewer};
pub use image::exif::{format_exif_report, is_exif_extension, read_exif_fields};
pub use image::lossless::{apply_lossless, format_from_extension, LosslessOp};
pub use image::{
    is_image_file_extension, ImageBackground, ImageFitMode, ImageFormat, ImageViewer, LoadedImage,
    ViewTransform, IMAGE_FILE_EXTENSIONS,
};
pub use media::{is_media_file_extension, MediaViewer, MEDIA_FILE_EXTENSIONS};
pub use pdf::PdfViewer;
pub use snapshot::{
    ArchiveEntryView, ArchivePreview, ArchiveSnapshot, ArchiveStatus, DocumentSnapshot,
    HtmlSnapshot, ImageSnapshot, MediaSnapshot, PdfSnapshot, SelectionRange, SyntaxLine,
    SyntaxScope, SyntaxSegment, TextSnapshot, ViewerSnapshot,
};
pub use text::{
    CursorPos, FindOptions, LineEnding, SyntaxHighlighter, TextBuffer, TextDisplayMode, TextOp,
    TextOpKind, TextViewer, TextViewerMode, UndoStack, VIEWER_ENCODINGS,
};
pub use thumbnail::{Thumbnail, ThumbnailCache, ThumbnailService, ThumbnailSize};
pub use viewer_trait::Viewer;

/// Crate version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
