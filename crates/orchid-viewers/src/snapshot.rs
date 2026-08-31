//! Renderer-facing snapshot types consumed by the viewer widget / UI.

use std::sync::Arc;

/// Top-level viewer snapshot.
#[derive(Debug, Clone)]
// Variants are boxed at the payload level already; boxing here would add an
// allocation to every snapshot publish on the render path.
#[allow(clippy::large_enum_variant)]
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
    /// Rich-text document (DOCX).
    Document(DocumentSnapshot),
    /// Audio / video — libmpv when available, else system player via Open.
    Media(MediaSnapshot),
    /// HTML source + open in the system browser.
    Html(HtmlSnapshot),
}

/// Document (DOCX) snapshot for the UI layer.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct DocumentSnapshot {
    pub path_display: String,
    pub dirty: bool,
    pub block_count: u32,
    /// Whitespace-separated word count over [`Self::plain_text`].
    pub word_count: u32,
    /// Character count over [`Self::plain_text`] (newlines excluded).
    pub char_count: u32,
    /// Plain-text extraction for search / fallback display.
    pub plain_text: Arc<str>,
    /// Non-fatal warnings (e.g. unsupported OOXML features).
    pub warnings: Vec<String>,
    pub info_text: String,
    /// Aggregate style at the selection caret (toolbar accents).
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub highlight: bool,
    pub superscript: bool,
    pub subscript: bool,
    /// Font size in points at the caret (`0` = document default).
    pub font_size_pt: f32,
    /// Font family at the caret (empty = document default).
    pub font_family: String,
    /// Text colour as `0x00RRGGBB` (`0` = theme/default).
    pub color_rgb: u32,
    /// 0=left, 1=center, 2=right, 3=justify.
    pub alignment: u8,
    /// 0=none, 1=bullet, 2=numbered.
    pub list_kind: u8,
    pub can_undo: bool,
    pub can_redo: bool,
    /// Soft-rendered rich preview (RGBA8). Empty when unavailable.
    pub preview_rgba: Arc<Vec<u8>>,
    pub preview_width_px: u32,
    pub preview_height_px: u32,
    /// When true, the UI shows the plain-text editor instead of the preview.
    pub source_mode: bool,
    /// Bumped when find selects a match (Slint syncs source-mode selection).
    pub find_gen: i32,
    /// Find selection anchor (UTF-8 byte offset in [`Self::plain_text`]).
    pub find_anchor: i32,
    /// Find selection head (UTF-8 byte offset in [`Self::plain_text`]).
    pub find_cursor: i32,
    /// 1-based index of the current find match (`0` when none).
    pub find_match_index: i32,
    /// Total non-overlapping matches for the last find query.
    pub find_match_count: i32,
    /// Preview image Y (CSS px) for scrolling to the current find match (`-1` = none).
    pub find_scroll_y_px: i32,
    /// Preview display zoom percent (`100` = 100%; layout width unchanged).
    pub preview_zoom_percent: i32,
    /// Preview pointer is over an external hyperlink (pointer cursor affordance).
    pub link_hover: bool,
    /// External hyperlink URL at the selection caret (empty when none).
    pub link_url: String,
    /// True when page size matches ISO A4 (else treated as US Letter for the Pg control).
    pub page_is_a4: bool,
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
    pub rotation_degrees: f32,
    pub flipped_horizontal: bool,
    pub flipped_vertical: bool,
    /// 0 custom, 1 window, 2 width, 3 height, 4 shrink (see [`crate::ImageFitMode`]).
    pub fit_mode: u8,
    /// 0 theme, 1 black, 2 white, 3 gray, 4 checkerboard, 5 custom.
    pub background: u8,
    pub bg_r: u8,
    pub bg_g: u8,
    pub bg_b: u8,
    pub chrome_hidden: bool,
    pub kiosk: bool,
    pub color_source: String,
    pub color_dest: String,
    pub orientation: u32,
    /// Short format label (e.g. `PNG`) for the localized status strip.
    pub format_label: String,
    /// Original file size in bytes (for the localized status strip).
    pub size_bytes: u64,
    /// Deprecated: UI builds the status strip from structured fields + Fluent.
    pub info_text: String,
    /// 1-based index in the folder playlist (`0` when unknown).
    pub folder_index: u32,
    /// Image files in the same folder.
    pub folder_count: u32,
    /// Wrap next/prev at the folder ends.
    pub loop_folder: bool,
    /// Recently viewed image paths in this viewer (newest first).
    pub recent_paths: Vec<String>,
    /// Magnifier overlay follows the pointer.
    pub lens: bool,
    /// Folder thumbnails for the strip / grid (`rgba` may still be empty).
    pub thumbs: Vec<ImageThumbItem>,
    /// 0 hidden, 1 bottom, 2 top.
    pub thumb_strip: u8,
    /// Full-folder thumbnail grid instead of the single image.
    pub thumb_grid: bool,
    /// 0 small, 1 medium, 2 large.
    pub thumb_size: u8,
    /// Show name / size / date / rating under each thumbnail.
    pub thumb_show_meta: bool,
    pub slideshow_playing: bool,
    pub slideshow_paused: bool,
    pub slideshow_interval_ms: u32,
    pub slideshow_random: bool,
    /// 0 none, 1 fade, 2 slide, 3 dissolve, 4 wipe.
    pub slideshow_transition: u8,
    pub slideshow_transition_ms: u32,
    pub slideshow_overlay: bool,
    pub slideshow_overlay_text: String,
    pub slideshow_music: String,
    /// Bumped on each slide so the UI restarts the transition.
    pub slideshow_gen: u32,
    pub prev_rgba: Option<Arc<Vec<u8>>>,
    pub prev_width: u32,
    pub prev_height: u32,
    pub bit_depth: u8,
    pub color_model: String,
    pub meta_panel: bool,
    pub meta_overlay: bool,
    pub meta_text: String,
    pub meta_overlay_text: String,
    pub hist_rgba: Option<Arc<Vec<u8>>>,
    pub hist_width: u32,
    pub hist_height: u32,
    pub hist_mode: u8,
    pub probe_text: String,
    pub gps_label: String,
    pub has_gps: bool,
    pub meta_edit_title: String,
    pub meta_edit_creator: String,
    pub meta_edit_copyright: String,
    pub meta_edit_keywords: String,
    pub meta_edit_description: String,
    pub meta_edit_date: String,
    pub meta_edit_gps: String,
    /// Animated frame count (`0` when the file is a still).
    pub anim_count: u32,
    /// 1-based current frame (`0` when still).
    pub anim_index: u32,
    pub anim_playing: bool,
    pub anim_delay_ms: u32,
    /// `GIF` / `APNG` / `WebP`.
    pub anim_label: String,
    /// Frame-strip thumbs for the current animation.
    pub anim_thumbs: Vec<ImageThumbItem>,
    /// GIF / APNG / WebP can auto-play; TIFF / ICO pages cannot.
    pub anim_can_play: bool,
    /// 0 photo, 1 timeline, 2 map, 3 calendar.
    pub browse_mode: u8,
    /// Hide toolbar / strip after idle pointer time.
    pub overlay_autohide: bool,
    pub cal_title: String,
    pub cal_year: i32,
    pub cal_month: u8,
    pub cal_days: Vec<CalDayItem>,
    pub map_pins: Vec<MapPinItem>,
    pub timeline: Vec<ImageThumbItem>,
}

/// One folder sibling in the image thumbnail strip or grid.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct ImageThumbItem {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub date_text: String,
    pub rating: u8,
    pub rgba: Option<Arc<Vec<u8>>>,
    pub width: u32,
    pub height: u32,
    pub selected: bool,
    /// 1-based playlist index.
    pub index: u32,
    /// Shoot date (EXIF) or mtime, milliseconds since epoch.
    pub taken_ms: i64,
    pub has_gps: bool,
    pub gps_lat: f32,
    pub gps_lon: f32,
}

/// GPS pin on the folder map browse canvas (x/y in 0…1, north up).
#[derive(Debug, Clone, Default)]
#[allow(missing_docs)]
pub struct MapPinItem {
    pub path: String,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub selected: bool,
    pub rgba: Option<Arc<Vec<u8>>>,
    pub width: u32,
    pub height: u32,
}

/// One cell in the folder calendar browse grid (`day == 0` is padding).
#[derive(Debug, Clone, Default)]
#[allow(missing_docs)]
pub struct CalDayItem {
    pub day: u8,
    pub count: u32,
    pub selected: bool,
    pub path: String,
    pub rgba: Option<Arc<Vec<u8>>>,
    pub width: u32,
    pub height: u32,
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
    /// `0` text, `1` hex dump, `2` binary hex stream.
    pub display_mode: u8,
    /// Bumped when find selects a match (Slint syncs `TextInput` selection).
    pub find_gen: i32,
    /// Find selection anchor (UTF-8 byte offset in [`Self::plain_text`]).
    pub find_anchor: i32,
    /// Find selection head (UTF-8 byte offset in [`Self::plain_text`]).
    pub find_cursor: i32,
    /// 1-based index of the current find match (`0` when none).
    pub find_match_index: i32,
    /// Total non-overlapping matches for the last find query.
    pub find_match_count: i32,
    pub can_undo: bool,
    pub can_redo: bool,
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

/// Audio / video snapshot (libmpv transport + optional RGBA frame).
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct MediaSnapshot {
    pub path_display: String,
    /// `"audio"` or `"video"`.
    pub kind_label: String,
    pub info_text: String,
    /// libmpv loaded and usable.
    pub available: bool,
    pub playing: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
    /// 0..1 progress fraction.
    pub progress: f32,
    pub volume: u32,
    pub muted: bool,
    pub speed: f32,
    pub has_video: bool,
    pub frame_rgba: Arc<Vec<u8>>,
    pub frame_width: u32,
    pub frame_height: u32,
    /// Still album art when there is no video track (APIC / folder cover).
    pub has_cover: bool,
    pub cover_rgba: Arc<Vec<u8>>,
    pub cover_width: u32,
    pub cover_height: u32,
    pub title: String,
    pub artist: String,
    /// 0-based index into the folder playlist.
    pub playlist_index: u32,
    pub playlist_count: u32,
    pub playlist_shuffle: bool,
    pub playlist_loop: bool,
    pub sub_label: String,
    pub sub_visible: bool,
    pub audio_label: String,
    pub chapter_label: String,
    pub ab_label: String,
    pub eq_label: String,
    pub hwdec_label: String,
    pub sleep_label: String,
    pub aspect_label: String,
    pub rotate_label: String,
    pub audio_delay_label: String,
    /// Transient on-screen status (volume / seek / speed).
    pub osd_text: String,
    pub error: String,
}

/// HTML snapshot (source preview + Open in browser).
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct HtmlSnapshot {
    pub path_display: String,
    pub source_preview: Arc<str>,
    pub info_text: String,
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
