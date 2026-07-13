//! Renderer-facing snapshot types consumed by the viewer widget / UI.

use std::sync::Arc;

/// Top-level viewer snapshot.
#[derive(Debug, Clone)]
pub enum ViewerSnapshot {
    /// Viewer is in the process of loading.
    Loading {
        /// Human-readable path for the header / status line.
        path_display: String,
    },
    /// Viewer encountered an error.
    Error {
        /// Human-readable path.
        path_display: String,
        /// Error message.
        message: String,
    },
    /// Image content.
    Image(ImageSnapshot),
    /// PDF content.
    Pdf(PdfSnapshot),
    /// Text content (with optional syntax highlighting).
    Text(TextSnapshot),
    /// Archive listing.
    Archive(ArchiveSnapshot),
}

/// Image snapshot.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct ImageSnapshot {
    pub path_display: String,
    pub width_px: u32,
    pub height_px: u32,
    /// RGBA8 row-major. `rgba_bytes.len() == width_px * height_px * 4`.
    pub rgba_bytes: Arc<Vec<u8>>,
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub rotation_degrees: i16,
    pub flipped_horizontal: bool,
    pub flipped_vertical: bool,
    /// `true` when Fit Screen is active; `false` for Actual Size / custom zoom.
    pub fit_mode: bool,
    /// Short format label (e.g. `PNG`) for the localized status strip.
    pub format_label: String,
    /// Original file size in bytes (for the localized status strip).
    pub size_bytes: u64,
    /// Deprecated: UI builds the status strip from structured fields + Fluent.
    pub info_text: String,
}

/// PDF page snapshot.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct PdfSnapshot {
    pub path_display: String,
    pub page_count: u32,
    pub current_page: u32,
    pub page_width_px: u32,
    pub page_height_px: u32,
    pub page_rgba_bytes: Arc<Vec<u8>>,
    pub zoom: f32,
    /// 0 = FitWidth, 1 = FitPage, 2 = Custom (manual zoom).
    pub fit_mode: u8,
    pub info_text: String,
}

/// Text snapshot.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct TextSnapshot {
    pub path_display: String,
    pub language: String,
    pub encoding: String,
    pub line_ending: String,
    pub dirty: bool,
    pub read_only: bool,
    pub total_lines: u32,
    pub visible_lines: Vec<SyntaxLine>,
    pub first_visible_line: u32,
    pub cursor_line: u32,
    pub cursor_column: u32,
    pub selection: Option<SelectionRange>,
    pub info_text: String,
    /// Full LF-normalised file text (for plain edit / selectable read mode).
    /// Shared via `Arc` so snapshot pump ticks do not re-allocate the document.
    pub plain_text: Arc<str>,
}

/// A single highlighted line.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct SyntaxLine {
    pub line_number: u32,
    pub segments: Vec<SyntaxSegment>,
}

/// A scoped text segment.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct SyntaxSegment {
    pub text: String,
    pub scope: SyntaxScope,
}

/// Token scope the UI colourises.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyntaxScope {
    Plain,
    Keyword,
    String,
    Number,
    Comment,
    Function,
    Type,
    Variable,
    Constant,
    Operator,
    Punctuation,
    Attribute,
    Preprocessor,
    Tag,
    Property,
    Error,
}

/// Selection range (half-open at `end_*`).
#[derive(Debug, Clone, Copy)]
#[allow(missing_docs)]
pub struct SelectionRange {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

/// Transient archive status shown in the footer strip.
#[derive(Debug, Clone, Default)]
pub enum ArchiveStatus {
    /// Default: show format + entry count.
    #[default]
    Idle,
    /// One entry was extracted beside the archive.
    ExtractedSelected {
        /// Destination path display.
        path: String,
    },
    /// All entries were extracted to a sibling folder.
    ExtractedAll {
        /// Number of entries written.
        count: u64,
        /// Destination folder display.
        path: String,
    },
}

/// Archive snapshot.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct ArchiveSnapshot {
    pub path_display: String,
    pub format: String,
    pub total_entries: u32,
    pub current_inner_path: String,
    /// Selected archive member path (empty when none).
    pub selected_path: String,
    pub entries: Vec<ArchiveEntryView>,
    pub preview: Option<ArchivePreview>,
    /// Structured status for the localized footer strip.
    pub status: ArchiveStatus,
    /// Deprecated: UI builds the status strip from [`Self::status`] + Fluent.
    pub info_text: String,
}

/// One row shown in the archive viewer's list.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct ArchiveEntryView {
    pub path_in_archive: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_text: String,
    pub icon: &'static str,
}

/// Preview for a selected archive entry.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub enum ArchivePreview {
    Text(String),
    Binary { size: u64 },
}
