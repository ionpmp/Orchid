//! Error type for [`orchid_viewers`](crate).

/// Errors surfaced by viewers and the dispatcher.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum ViewerError {
    /// No built-in viewer matched the file.
    #[error("unsupported file type: mime={mime:?}, ext={extension:?}")]
    UnsupportedType {
        /// Detected MIME, if any.
        mime: Option<String>,
        /// File extension, if any.
        extension: Option<String>,
    },

    /// File exceeds the viewer-specific size limit.
    #[error("file too large: {size} bytes (limit {limit})")]
    FileTooLarge {
        /// Actual size.
        size: u64,
        /// Configured limit.
        limit: u64,
    },

    /// Image decode failed.
    #[error("failed to decode image: {0}")]
    ImageDecode(String),

    /// Lossless JPEG/PNG transform or write failed.
    #[error("lossless image transform: {0}")]
    ImageLossless(String),

    /// HEIC/HEIF containers are recognised but the OS codec is missing.
    ///
    /// Display string is the Fluent key so the UI can localise it.
    #[error("viewer-image-heic-unsupported")]
    UnsupportedHeic,

    /// AVIF containers need a WIC / AV1 image codec.
    ///
    /// Display string is the Fluent key so the UI can localise it.
    #[error("viewer-image-avif-unsupported")]
    UnsupportedAvif,

    /// JPEG 2000 needs a WIC codec (not bundled).
    ///
    /// Display string is the Fluent key so the UI can localise it.
    #[error("viewer-image-jp2-unsupported")]
    UnsupportedJpeg2000,

    /// Camera RAW containers are recognised but not decoded yet.
    ///
    /// Display string is the Fluent key so the UI can localise it.
    #[error("viewer-image-raw-unsupported")]
    UnsupportedRaw,

    /// EPS / PostScript has no embedded preview and Ghostscript is missing.
    ///
    /// Display string is the Fluent key so the UI can localise it.
    #[error("viewer-image-eps-unsupported")]
    UnsupportedEps,

    /// CorelDRAW file has no embedded preview bitmap.
    ///
    /// Display string is the Fluent key so the UI can localise it.
    #[error("viewer-image-cdr-unsupported")]
    UnsupportedCdr,

    /// WMF / EMF could not be played (missing GDI or a corrupt metafile).
    ///
    /// Display string is the Fluent key so the UI can localise it.
    #[error("viewer-image-emf-unsupported")]
    UnsupportedEmf,

    /// PDF page render failed.
    #[error("failed to render PDF page {page}: {reason}")]
    PdfRender {
        /// 1-based page number.
        page: u32,
        /// Human-readable reason.
        reason: String,
    },

    /// PDF file has no pages.
    #[error("PDF has no pages")]
    PdfEmpty,

    /// Pdfium shared library could not be loaded.
    #[error(
        "PDF support unavailable: place pdfium.dll next to the executable or see docs/BUILDING.md"
    )]
    PdfUnavailable,

    /// libmpv shared library could not be loaded.
    #[error(
        "Media playback unavailable: place mpv-1.dll (or libmpv-2.dll) next to the executable or see docs/BUILDING.md"
    )]
    MediaUnavailable,

    /// Text decode failed.
    #[error("failed to parse text: {0}")]
    TextDecode(String),

    /// Grammar lookup failed.
    #[error("syntax grammar not found for language: {0}")]
    GrammarNotFound(String),

    /// Edit position outside buffer bounds.
    #[error("edit outside buffer bounds")]
    EditOutOfBounds,

    /// No document is currently open in the document viewer.
    #[error("no document open")]
    DocumentNotOpen,

    /// Failed to parse a DOCX / OOXML package.
    #[error("failed to parse document: {0}")]
    DocumentParse(String),

    /// Failed to save a DOCX / OOXML package.
    #[error("failed to save document: {0}")]
    DocumentSave(String),

    /// Failed to read or write content metadata (EXIF, ID3, Office).
    #[error("metadata: {0}")]
    Metadata(String),

    /// Archive entry missing.
    #[error("archive entry not found: {0}")]
    ArchiveEntryNotFound(String),

    /// Thumbnail generation failed.
    #[error("thumbnail generation failed: {0}")]
    ThumbnailFailed(String),

    /// Propagated IO failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Propagated `orchid-fs` error.
    #[error(transparent)]
    Fs(#[from] orchid_fs::FsError),

    /// Propagated `orchid-core` error.
    #[error(transparent)]
    Core(#[from] orchid_core::CoreError),

    /// Propagated `orchid-storage` error.
    #[error(transparent)]
    Storage(#[from] orchid_storage::StorageError),
}

/// `Result` alias using [`ViewerError`] as the default error.
pub type Result<T, E = ViewerError> = std::result::Result<T, E>;
