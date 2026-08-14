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
pub use image::adjust::{
    apply_adjust, apply_adjust_file, pack_adjust_params, parse_adjust_line, AdjustOp, AdjustParams,
    CurveSet, SelectiveBand,
};
pub use image::annotate::{
    apply_annotate, apply_annotate_file, parse_annotate_line, AnnotateOp, DrawStyle, WatermarkPos,
};
pub use image::batch::{
    compare_files, compare_strip, composite, composite_files, convert_file, diff_files,
    export_thumb_file, image_date_token, load_batch_recipes, merge_hdr, merge_hdr_files,
    pick_best_file, pixel_diff, planned_sibling, save_batch_recipe, stitch_panorama,
    stitch_panorama_files, BatchRecipe, CompositeMode, DiffStats, EncodeFormat,
};
pub use image::edit::{
    apply_edit, apply_edit_file, parse_canvas_line, parse_resize_line, parse_resize_spec,
    save_sibling, CropKeep, EditOp, ResizeSpec,
};
pub use image::exif::{format_exif_report, is_exif_extension, read_exif_fields};
pub use image::export::{
    capture_screenshot, encode_png, export_file, export_loaded, loaded_from_rgba,
    parse_export_line, parse_screenshot_line, prepare_mail_attachment, set_wallpaper,
    share_intent_url, unique_export_dest, write_mail_eml, write_screenshot, ExportFormat,
    ExportSpec, ScreenshotKind, ScreenshotSpec,
};
pub use image::filter::{
    apply_filter, apply_filter_file, builtin_looks, load_filter_presets, parse_filter_line,
    parse_filter_line_in, save_filter_preset, FilterOp, FilterPreset,
};
pub use image::lossless::{apply_lossless, format_from_extension, LosslessOp};
pub use image::meta_edit::{
    apply_editable_meta, copy_image_metadata, export_metadata_csv, export_metadata_xml,
    import_metadata_csv, inspect_to_edit, load_templates, pack_editable_meta, parse_gps_pair,
    parse_shift, save_template, unpack_editable_meta, EditableMeta, MetaTemplate,
};
pub use image::metadata::{
    camera_overlay, compute_histogram, describe_pixel, format_inspect_panel, format_sidecar_report,
    inspect_image_bytes, inspect_image_file, render_histogram, ChannelHistogram, GpsFix, HistMode,
    ImageInspect,
};
pub use image::operations::ResizeFilter;
pub use image::print::{
    parse_print_line, render_print_files, render_print_pages, send_to_printer, write_print_preview,
    write_print_temps, PaperSize, PrintFit, PrintItem, PrintSpec,
};
pub use image::slideshow::{
    export_slideshow_pack, export_slideshow_video, is_slideshow_audio_extension, overlay_text,
    SlideTransition, SlideshowExport,
};
pub use image::{
    is_image_file_extension, load_image, load_image_file, ImageBackground, ImageFitMode,
    ImageFormat, ImageViewer, LoadedImage, ViewTransform, DEFAULT_SIZE_LIMIT,
    IMAGE_FILE_EXTENSIONS,
};
pub use media::{is_media_file_extension, MediaViewer, MEDIA_FILE_EXTENSIONS};
pub use pdf::PdfViewer;
pub use snapshot::{
    ArchiveEntryView, ArchivePreview, ArchiveSnapshot, ArchiveStatus, DocumentSnapshot,
    HtmlSnapshot, ImageSnapshot, ImageThumbItem, MediaSnapshot, PdfSnapshot, SelectionRange,
    SyntaxLine, SyntaxScope, SyntaxSegment, TextSnapshot, ViewerSnapshot,
};
pub use text::{
    CursorPos, FindOptions, LineEnding, SyntaxHighlighter, TextBuffer, TextDisplayMode, TextOp,
    TextOpKind, TextViewer, TextViewerMode, UndoStack, VIEWER_ENCODINGS,
};
pub use thumbnail::contact_sheet::{compose_contact_sheet, encode_contact_sheet_png};
pub use thumbnail::exif_preview::rating_from_bytes;
pub use thumbnail::{Thumbnail, ThumbnailCache, ThumbnailService, ThumbnailSize};
pub use viewer_trait::Viewer;

/// Crate version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
