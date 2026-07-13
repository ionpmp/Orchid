use orchid_i18n::LocaleManager;
use orchid_widgets::ViewerPayload;
use slint::{Image, ModelRc, SharedString, VecModel};
use std::collections::HashMap;
use std::sync::Arc;

use super::super::errors::viewer_localized_error;
use crate::slint_generated::{
    ViewerArchiveEntry, ViewerArchiveModel, ViewerEmptyModel, ViewerImageModel, ViewerModel,
    ViewerPdfModel, ViewerStatusModel, ViewerSyntaxLine, ViewerSyntaxSegment, ViewerTextModel,
};

/// Reuse Slint images when the underlying RGBA `Arc` is unchanged (pan/zoom).
struct RgbaImageCacheEntry {
    width: u32,
    height: u32,
    image: Image,
}

thread_local! {
    static RGBA_IMAGE_CACHE: std::cell::RefCell<HashMap<usize, RgbaImageCacheEntry>> = std::cell::RefCell::new(HashMap::new());
}
const RGBA_IMAGE_CACHE_CAP: usize = 8;

fn slint_image_from_rgba(rgba: &Arc<Vec<u8>>, width: u32, height: u32) -> Image {
    if width == 0 || height == 0 || rgba.is_empty() {
        return Image::default();
    }
    let key = Arc::as_ptr(rgba) as usize;

    let cached = RGBA_IMAGE_CACHE.with(|cache| {
        cache.borrow().get(&key).and_then(|c| {
            if c.width == width && c.height == height {
                Some(c.image.clone())
            } else {
                None
            }
        })
    });

    if let Some(image) = cached {
        return image;
    }

    let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        rgba.as_slice(),
        width,
        height,
    );
    let image = Image::from_rgba8(buf);

    RGBA_IMAGE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= RGBA_IMAGE_CACHE_CAP {
            // Drop an arbitrary entry; keys are stable Arc addresses while buffers live.
            if let Some(old) = cache.keys().next().copied() {
                cache.remove(&old);
            }
        }
        cache.insert(
            key,
            RgbaImageCacheEntry {
                width,
                height,
                image: image.clone(),
            },
        );
    });

    image
}

fn viewer_syntax_label(locale: &LocaleManager, language_id: &str) -> SharedString {
    let key = format!("viewer-syntax-{language_id}");
    let label = locale.tr(&key);
    if label == key {
        language_id.into()
    } else {
        label.into()
    }
}

fn viewer_format_slug(raw: &str) -> String {
    raw.trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn viewer_image_format_label(locale: &LocaleManager, format_label: &str) -> String {
    let slug = viewer_format_slug(format_label);
    if slug.is_empty() {
        return format_label.to_string();
    }
    let key = format!("viewer-image-format-{slug}");
    let label = locale.tr(&key);
    if label == key {
        format_label.to_string()
    } else {
        label
    }
}

fn viewer_archive_format_label(locale: &LocaleManager, format_label: &str) -> String {
    let slug = viewer_format_slug(format_label);
    if slug.is_empty() {
        return format_label.to_string();
    }
    let key = format!("viewer-archive-format-{slug}");
    let label = locale.tr(&key);
    if label == key {
        format_label.to_string()
    } else {
        label
    }
}

fn viewer_encoding_label(locale: &LocaleManager, encoding: &str) -> SharedString {
    let slug = viewer_format_slug(encoding);
    if slug.is_empty() {
        return encoding.into();
    }
    let key = format!("viewer-encoding-{slug}");
    let label = locale.tr(&key);
    if label == key {
        encoding.into()
    } else {
        label.into()
    }
}

fn empty_viewer_image_model(locale: &LocaleManager) -> ViewerImageModel {
    ViewerImageModel {
        width_px: 0,
        height_px: 0,
        rgba_image: Image::default(),
        zoom: 1.0,
        pan_x: 0.0,
        pan_y: 0.0,
        rotation_deg: 0,
        flipped_h: false,
        flipped_v: false,
        fit_mode: true,
        info_text: SharedString::new(),
        path_display: SharedString::new(),
        fit_label: locale.tr("viewer-image-fit-screen").into(),
        actual_size_label: locale.tr("viewer-image-actual-size").into(),
    }
}

fn empty_viewer_pdf_model(locale: &LocaleManager) -> ViewerPdfModel {
    ViewerPdfModel {
        page_count: 0,
        current_page: 0,
        page_width_px: 0,
        page_height_px: 0,
        page_image: Image::default(),
        zoom: 1.0,
        fit_mode: 0,
        info_text: SharedString::new(),
        path_display: SharedString::new(),
        available: true,
        unavailable_reason: locale.tr("viewer-pdf-unavailable").into(),
        page_of_label: SharedString::new(),
        fit_width_label: locale.tr("viewer-pdf-fit-width").into(),
        fit_page_label: locale.tr("viewer-pdf-fit-page").into(),
        go_label: locale.tr("viewer-pdf-go").into(),
        copy_text_label: locale.tr("viewer-pdf-copy-text").into(),
    }
}

fn empty_viewer_text_model(locale: &LocaleManager) -> ViewerTextModel {
    let lines_args = orchid_i18n::FluentArgs::new().with("count", "0");
    ViewerTextModel {
        language: viewer_syntax_label(locale, "plaintext"),
        encoding: viewer_encoding_label(locale, "UTF-8"),
        line_ending: locale.tr("viewer-text-line-ending-lf").into(),
        dirty: false,
        read_only: true,
        total_lines: 0,
        first_visible_line: 0,
        cursor_line: 0,
        cursor_col: 0,
        visible_lines: ModelRc::new(VecModel::default()),
        info_text: SharedString::new(),
        path_display: SharedString::new(),
        plain_text: SharedString::new(),
        mode_label: locale.tr("viewer-text-read-only").into(),
        save_label: locale.tr("viewer-text-save").into(),
        lines_label: locale.tr_args("viewer-text-lines", &lines_args).into(),
    }
}

fn empty_viewer_archive_model(locale: &LocaleManager) -> ViewerArchiveModel {
    ViewerArchiveModel {
        format: SharedString::new(),
        total_entries: 0,
        current_inner_path: SharedString::new(),
        header_label: SharedString::new(),
        path_label: locale.tr("viewer-archive-root").into(),
        breadcrumbs: ModelRc::new(VecModel::default()),
        entries: ModelRc::new(VecModel::default()),
        selected_path: SharedString::new(),
        has_file_selected: false,
        extract_all_label: locale.tr("viewer-archive-extract-all").into(),
        extract_selected_label: locale.tr("viewer-archive-extract-selected").into(),
        nothing_selected_label: locale.tr("viewer-archive-nothing-selected").into(),
        preview_kind: 0,
        preview_text: locale.tr("viewer-archive-select-preview").into(),
        preview_binary_size: SharedString::new(),
        info_text: SharedString::new(),
        path_display: SharedString::new(),
    }
}

pub(crate) fn empty_viewer_model(locale: &LocaleManager) -> ViewerModel {
    ViewerModel {
        kind: 0,
        status: ViewerStatusModel {
            path_display: SharedString::new(),
            message: SharedString::new(),
            icon: SharedString::new(),
        },
        empty: ViewerEmptyModel {
            placeholder_text: locale.tr("viewer-no-file").into(),
        },
        image: empty_viewer_image_model(locale),
        pdf: empty_viewer_pdf_model(locale),
        text: empty_viewer_text_model(locale),
        archive: empty_viewer_archive_model(locale),
    }
}

pub(crate) fn build_viewer_model(p: &ViewerPayload, locale: &LocaleManager) -> ViewerModel {
    use orchid_viewers::ViewerError;
    use orchid_viewers::ViewerSnapshot as Vs;

    let mut model = empty_viewer_model(locale);

    match &p.snapshot {
        Vs::Loading { path_display } if path_display.is_empty() => {
            model.kind = 0;
        }
        Vs::Loading { path_display } => {
            model.kind = 1;
            model.status.path_display = path_display.clone().into();
            model.status.icon = "loading".into();
            let args = orchid_i18n::FluentArgs::new().with("path", path_display.as_str());
            model.status.message = locale.tr_args("viewer-loading-path", &args).into();
        }
        Vs::Error {
            path_display,
            message,
        } if *message == ViewerError::PdfUnavailable.to_string() => {
            model.kind = 4;
            model.pdf.path_display = path_display.clone().into();
            model.pdf.available = false;
            model.pdf.unavailable_reason = locale.tr("viewer-pdf-unavailable").into();
        }
        Vs::Error {
            path_display,
            message,
        } if *message == ViewerError::UnsupportedHeic.to_string()
            || *message == ViewerError::UnsupportedRaw.to_string()
            || message == "viewer-archive-nothing-selected"
            || message == "viewer-archive-cannot-extract-folder" =>
        {
            model.kind = 2;
            model.status.path_display = path_display.clone().into();
            model.status.icon = "error".into();
            model.status.message = locale.tr(message).into();
        }
        Vs::Error {
            path_display,
            message,
        } => {
            model.kind = 2;
            model.status.path_display = path_display.clone().into();
            model.status.icon = "error".into();
            let reason = viewer_localized_error(locale, message);
            let args = orchid_i18n::FluentArgs::new().with("reason", reason);
            model.status.message = locale.tr_args("viewer-error-with-reason", &args).into();
        }
        Vs::Image(s) => {
            model.kind = 3;
            model.image = build_image_snapshot(s, locale);
        }
        Vs::Pdf(s) => {
            model.kind = 4;
            model.pdf = build_pdf_snapshot(s, locale);
        }
        Vs::Text(s) => {
            model.kind = 5;
            model.text = build_text_snapshot(s, locale);
        }
        Vs::Archive(s) => {
            model.kind = 6;
            model.archive = build_archive_snapshot(s, locale);
        }
    }

    model
}

fn build_image_snapshot(
    s: &orchid_viewers::ImageSnapshot,
    locale: &LocaleManager,
) -> ViewerImageModel {
    let image = slint_image_from_rgba(&s.rgba_bytes, s.width_px, s.height_px);

    ViewerImageModel {
        width_px: s.width_px as i32,
        height_px: s.height_px as i32,
        rgba_image: image,
        zoom: s.zoom,
        pan_x: s.pan_x,
        pan_y: s.pan_y,
        rotation_deg: i32::from(s.rotation_degrees),
        flipped_h: s.flipped_horizontal,
        flipped_v: s.flipped_vertical,
        fit_mode: s.fit_mode,
        info_text: {
            let args = orchid_i18n::FluentArgs::new()
                .with("width", s.width_px.to_string())
                .with("height", s.height_px.to_string())
                .with("size", locale.format_byte_size(s.size_bytes))
                .with("format", viewer_image_format_label(locale, &s.format_label));
            locale.tr_args("viewer-image-info", &args).into()
        },
        path_display: s.path_display.clone().into(),
        fit_label: locale.tr("viewer-image-fit-screen").into(),
        actual_size_label: locale.tr("viewer-image-actual-size").into(),
    }
}

fn build_pdf_snapshot(s: &orchid_viewers::PdfSnapshot, locale: &LocaleManager) -> ViewerPdfModel {
    let available = !s.page_rgba_bytes.is_empty() && s.page_count > 0;
    let image = if available {
        slint_image_from_rgba(&s.page_rgba_bytes, s.page_width_px, s.page_height_px)
    } else {
        Image::default()
    };

    let page_args = orchid_i18n::FluentArgs::new()
        .with("current", s.current_page.to_string())
        .with("total", s.page_count.to_string());
    let info_args = orchid_i18n::FluentArgs::new()
        .with("current", s.current_page.to_string())
        .with("total", s.page_count.to_string())
        .with("width", s.page_width_px.to_string())
        .with("height", s.page_height_px.to_string())
        .with("zoom", format!("{:.0}", s.zoom * 100.0));

    ViewerPdfModel {
        page_count: s.page_count as i32,
        current_page: s.current_page as i32,
        page_width_px: s.page_width_px as i32,
        page_height_px: s.page_height_px as i32,
        page_image: image,
        zoom: s.zoom,
        fit_mode: i32::from(s.fit_mode),
        info_text: locale.tr_args("viewer-pdf-info", &info_args).into(),
        path_display: s.path_display.clone().into(),
        available,
        unavailable_reason: if available {
            SharedString::new()
        } else {
            locale.tr("viewer-pdf-unavailable").into()
        },
        page_of_label: locale.tr_args("viewer-pdf-page-of", &page_args).into(),
        fit_width_label: locale.tr("viewer-pdf-fit-width").into(),
        fit_page_label: locale.tr("viewer-pdf-fit-page").into(),
        go_label: locale.tr("viewer-pdf-go").into(),
        copy_text_label: locale.tr("viewer-pdf-copy-text").into(),
    }
}

fn build_text_snapshot(
    s: &orchid_viewers::TextSnapshot,
    locale: &LocaleManager,
) -> ViewerTextModel {
    let lines: Vec<ViewerSyntaxLine> = s
        .visible_lines
        .iter()
        .map(|line| {
            let segments: Vec<ViewerSyntaxSegment> = line
                .segments
                .iter()
                .map(|seg| ViewerSyntaxSegment {
                    text: seg.text.clone().into(),
                    scope: syntax_scope_to_int(&seg.scope),
                })
                .collect();
            ViewerSyntaxLine {
                line_number: line.line_number as i32,
                segments: ModelRc::new(VecModel::from(segments)),
            }
        })
        .collect();

    let line_ending_key = match s.line_ending.as_str() {
        "CRLF" => "viewer-text-line-ending-crlf",
        _ => "viewer-text-line-ending-lf",
    };
    ViewerTextModel {
        language: viewer_syntax_label(locale, &s.language),
        encoding: viewer_encoding_label(locale, &s.encoding),
        line_ending: locale.tr(line_ending_key).into(),
        dirty: s.dirty,
        read_only: s.read_only,
        total_lines: s.total_lines as i32,
        first_visible_line: s.first_visible_line as i32,
        cursor_line: s.cursor_line as i32,
        cursor_col: s.cursor_column as i32,
        visible_lines: ModelRc::new(VecModel::from(lines)),
        info_text: SharedString::new(),
        path_display: s.path_display.clone().into(),
        plain_text: SharedString::from(s.plain_text.as_ref()),
        mode_label: if s.read_only {
            locale.tr("viewer-text-read-only").into()
        } else {
            locale.tr("viewer-text-editing").into()
        },
        save_label: locale.tr("viewer-text-save").into(),
        lines_label: locale
            .tr_args(
                "viewer-text-lines",
                &orchid_i18n::FluentArgs::new().with("count", s.total_lines.to_string()),
            )
            .into(),
    }
}

fn syntax_scope_to_int(scope: &orchid_viewers::SyntaxScope) -> i32 {
    use orchid_viewers::SyntaxScope::*;
    match scope {
        Plain => 0,
        Keyword => 1,
        String => 2,
        Number => 3,
        Comment => 4,
        Function => 5,
        Type => 6,
        Variable => 7,
        Constant => 8,
        Operator => 9,
        Punctuation => 10,
        Attribute => 11,
        Preprocessor => 12,
        Tag => 13,
        Property => 14,
        Error => 15,
    }
}

fn build_archive_snapshot(
    s: &orchid_viewers::ArchiveSnapshot,
    locale: &LocaleManager,
) -> ViewerArchiveModel {
    let mut entries: Vec<ViewerArchiveEntry> = Vec::with_capacity(s.entries.len() + 1);

    if !s.current_inner_path.is_empty() {
        entries.push(ViewerArchiveEntry {
            path_in_archive: SharedString::new(),
            name: locale.tr("viewer-archive-parent").into(),
            is_dir: true,
            size_text: SharedString::new(),
            modified_text: SharedString::new(),
            icon: "up".into(),
            is_up: true,
        });
    }

    for e in &s.entries {
        entries.push(ViewerArchiveEntry {
            path_in_archive: e.path_in_archive.clone().into(),
            name: e.name.clone().into(),
            is_dir: e.is_dir,
            size_text: locale.format_byte_size(e.size).into(),
            modified_text: e.modified_text.clone().into(),
            icon: e.icon.into(),
            is_up: false,
        });
    }

    let breadcrumbs: Vec<SharedString> = s
        .current_inner_path
        .split('/')
        .filter(|seg| !seg.is_empty())
        .map(|p| p.into())
        .collect();

    let (preview_kind, preview_text, preview_binary) = match &s.preview {
        Some(orchid_viewers::ArchivePreview::Text(t)) => (1, t.clone().into(), SharedString::new()),
        Some(orchid_viewers::ArchivePreview::Binary { size }) => {
            let args = orchid_i18n::FluentArgs::new().with("size", locale.format_byte_size(*size));
            (
                2,
                SharedString::new(),
                locale
                    .tr_args("viewer-archive-binary-preview", &args)
                    .into(),
            )
        }
        None => (
            0,
            locale.tr("viewer-archive-select-preview").into(),
            SharedString::new(),
        ),
    };

    let has_file_selected = !s.selected_path.is_empty()
        && s.entries
            .iter()
            .any(|e| e.path_in_archive == s.selected_path && !e.is_dir);

    let header_args = orchid_i18n::FluentArgs::new()
        .with("format", viewer_archive_format_label(locale, &s.format))
        .with("count", s.total_entries.to_string());
    let header_label: SharedString = locale.tr_args("viewer-archive-info", &header_args).into();
    let path_label: SharedString = if s.current_inner_path.is_empty() {
        locale.tr("viewer-archive-root").into()
    } else {
        s.current_inner_path.clone().into()
    };

    let info_text: SharedString = match &s.status {
        orchid_viewers::ArchiveStatus::Idle => SharedString::new(),
        orchid_viewers::ArchiveStatus::ExtractedSelected { path } => {
            let args = orchid_i18n::FluentArgs::new().with("path", path.clone());
            locale
                .tr_args("viewer-archive-extracted-selected", &args)
                .into()
        }
        orchid_viewers::ArchiveStatus::ExtractedAll { count, path } => {
            let args = orchid_i18n::FluentArgs::new()
                .with("count", count.to_string())
                .with("path", path.clone());
            locale.tr_args("viewer-archive-extracted-all", &args).into()
        }
    };

    ViewerArchiveModel {
        format: viewer_archive_format_label(locale, &s.format).into(),
        total_entries: s.total_entries as i32,
        current_inner_path: s.current_inner_path.clone().into(),
        header_label,
        path_label,
        breadcrumbs: ModelRc::new(VecModel::from(breadcrumbs)),
        entries: ModelRc::new(VecModel::from(entries)),
        selected_path: s.selected_path.clone().into(),
        has_file_selected,
        extract_all_label: locale.tr("viewer-archive-extract-all").into(),
        extract_selected_label: locale.tr("viewer-archive-extract-selected").into(),
        nothing_selected_label: locale.tr("viewer-archive-nothing-selected").into(),
        preview_kind,
        preview_text,
        preview_binary_size: preview_binary,
        info_text,
        path_display: s.path_display.clone().into(),
    }
}
