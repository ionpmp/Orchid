use orchid_i18n::LocaleManager;
use orchid_widgets::ViewerPayload;
use slint::{Image, ModelRc, SharedString, VecModel};
use std::collections::HashMap;
use std::sync::Arc;

use super::super::errors::viewer_localized_error;
use crate::slint_generated::{
    ViewerArchiveEntry, ViewerArchiveModel, ViewerCalDay, ViewerDocumentModel, ViewerEmptyModel,
    ViewerHtmlModel, ViewerImageModel, ViewerImageThumb, ViewerMapPin, ViewerMediaModel,
    ViewerModel, ViewerPdfModel, ViewerStatusModel, ViewerSyntaxLine, ViewerSyntaxSegment,
    ViewerTextModel,
};

/// Reuse Slint images when the underlying RGBA `Arc` is unchanged (pan/zoom).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct RgbaCacheKey {
    ptr: usize,
    len: usize,
    width: u32,
    height: u32,
    /// First/last sample to detect allocator address reuse with different bytes.
    tip: u64,
}

struct RgbaImageCacheEntry {
    image: Image,
}

thread_local! {
    static RGBA_IMAGE_CACHE: std::cell::RefCell<HashMap<RgbaCacheKey, RgbaImageCacheEntry>> =
        std::cell::RefCell::new(HashMap::new());
}
const RGBA_IMAGE_CACHE_CAP: usize = 96;

fn rgba_cache_key(rgba: &Arc<Vec<u8>>, width: u32, height: u32) -> RgbaCacheKey {
    RgbaCacheKey {
        ptr: Arc::as_ptr(rgba) as usize,
        len: rgba.len(),
        width,
        height,
        tip: rgba_tip(rgba.as_slice()),
    }
}

fn rgba_tip(bytes: &[u8]) -> u64 {
    let mut tip = 0u64;
    for (i, b) in bytes.iter().take(8).enumerate() {
        tip |= u64::from(*b) << (i * 8);
    }
    if bytes.len() > 8 {
        let mut tail = 0u64;
        for (i, b) in bytes.iter().rev().take(8).enumerate() {
            tail |= u64::from(*b) << (i * 8);
        }
        tip ^= tail.rotate_left(17);
    }
    tip
}

fn slint_image_from_rgba(rgba: &Arc<Vec<u8>>, width: u32, height: u32) -> Image {
    if width == 0 || height == 0 || rgba.is_empty() {
        return Image::default();
    }
    let key = rgba_cache_key(rgba, width, height);

    let cached = RGBA_IMAGE_CACHE.with(|cache| cache.borrow().get(&key).map(|c| c.image.clone()));

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
            if let Some(old) = cache.keys().next().copied() {
                cache.remove(&old);
            }
        }
        cache.insert(
            key,
            RgbaImageCacheEntry {
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
        rotation_deg: 0.0,
        flipped_h: false,
        flipped_v: false,
        fit_mode: 1,
        bg_kind: 0,
        bg_r: 26,
        bg_g: 26,
        bg_b: 46,
        chrome_hidden: false,
        kiosk: false,
        info_text: SharedString::new(),
        path_display: SharedString::new(),
        fit_label: locale.tr("viewer-image-fit-screen").into(),
        fit_width_label: locale.tr("viewer-image-fit-width").into(),
        fit_height_label: locale.tr("viewer-image-fit-height").into(),
        fit_shrink_label: locale.tr("viewer-image-fit-shrink").into(),
        actual_size_label: locale.tr("viewer-image-actual-size").into(),
        bg_label: locale.tr("viewer-image-background").into(),
        fullscreen_label: locale.tr("viewer-image-fullscreen").into(),
        kiosk_label: locale.tr("viewer-image-kiosk").into(),
        next_monitor_label: locale.tr("viewer-image-next-monitor").into(),
        folder_index: 0,
        folder_count: 0,
        loop_folder: true,
        recent_paths: ModelRc::new(VecModel::default()),
        nav_index_label: SharedString::new(),
        prev_label: locale.tr("viewer-image-prev").into(),
        next_label: locale.tr("viewer-image-next").into(),
        first_label: locale.tr("viewer-image-first").into(),
        last_label: locale.tr("viewer-image-last").into(),
        random_label: locale.tr("viewer-image-random").into(),
        loop_label: locale.tr("viewer-image-loop").into(),
        folder_label: locale.tr("viewer-image-folder").into(),
        recent_label: locale.tr("viewer-image-recent").into(),
        goto_label: locale.tr("viewer-image-goto").into(),
        lens: false,
        zoom_percent_label: locale.tr("viewer-image-zoom-percent").into(),
        zoom_selection_label: locale.tr("viewer-image-zoom-selection").into(),
        lens_label: locale.tr("viewer-image-lens").into(),
        navigator_label: locale.tr("viewer-image-navigator").into(),
        rotate_180_label: locale.tr("viewer-image-rotate-180").into(),
        rotate_angle_label: locale.tr("viewer-image-rotate-angle").into(),
        reset_transform_label: locale.tr("viewer-image-reset-transform").into(),
        lossless_rotate_label: locale.tr("viewer-image-lossless-rotate").into(),
        lossless_flip_label: locale.tr("viewer-image-lossless-flip").into(),
        lossless_crop_label: locale.tr("viewer-image-lossless-crop").into(),
        lossless_exif_label: locale.tr("viewer-image-lossless-exif").into(),
        lossless_folder_label: locale.tr("viewer-image-lossless-folder").into(),
        edit_crop_label: locale.tr("viewer-image-edit-crop").into(),
        edit_aspect_label: locale.tr("viewer-image-edit-aspect").into(),
        edit_keep_label: locale.tr("viewer-image-edit-keep").into(),
        edit_resize_label: locale.tr("viewer-image-edit-resize").into(),
        edit_canvas_label: locale.tr("viewer-image-edit-canvas").into(),
        edit_straighten_label: locale.tr("viewer-image-edit-straighten").into(),
        edit_auto_straighten_label: locale.tr("viewer-image-edit-auto-straighten").into(),
        edit_perspective_label: locale.tr("viewer-image-edit-perspective").into(),
        adjust_label: locale.tr("viewer-image-adjust").into(),
        adjust_apply_label: locale.tr("viewer-image-adjust-apply").into(),
        adjust_hint_label: locale.tr("viewer-image-adjust-hint").into(),
        adjust_auto_levels_label: locale.tr("viewer-image-adjust-auto-levels").into(),
        adjust_auto_contrast_label: locale.tr("viewer-image-adjust-auto-contrast").into(),
        adjust_auto_color_label: locale.tr("viewer-image-adjust-auto-color").into(),
        adjust_gray_label: locale.tr("viewer-image-adjust-gray").into(),
        adjust_sepia_label: locale.tr("viewer-image-adjust-sepia").into(),
        adjust_invert_label: locale.tr("viewer-image-adjust-invert").into(),
        filter_label: locale.tr("viewer-image-filter").into(),
        filter_apply_label: locale.tr("viewer-image-filter-apply").into(),
        filter_hint_label: locale.tr("viewer-image-filter-hint").into(),
        filter_sharpen_label: locale.tr("viewer-image-filter-sharpen").into(),
        filter_blur_label: locale.tr("viewer-image-filter-blur").into(),
        filter_despeckle_label: locale.tr("viewer-image-filter-despeckle").into(),
        filter_cartoon_label: locale.tr("viewer-image-filter-cartoon").into(),
        filter_sketch_label: locale.tr("viewer-image-filter-sketch").into(),
        filter_vignette_label: locale.tr("viewer-image-filter-vignette").into(),
        filter_redeye_label: locale.tr("viewer-image-filter-redeye").into(),
        filter_look_vivid_label: locale.tr("viewer-image-filter-look-vivid").into(),
        filter_look_soft_label: locale.tr("viewer-image-filter-look-soft").into(),
        filter_look_drama_label: locale.tr("viewer-image-filter-look-drama").into(),
        filter_look_clean_label: locale.tr("viewer-image-filter-look-clean").into(),
        annotate_label: locale.tr("viewer-image-annotate").into(),
        annotate_apply_label: locale.tr("viewer-image-annotate-apply").into(),
        annotate_hint_label: locale.tr("viewer-image-annotate-hint").into(),
        annotate_line_label: locale.tr("viewer-image-annotate-line").into(),
        annotate_arrow_label: locale.tr("viewer-image-annotate-arrow").into(),
        annotate_rect_label: locale.tr("viewer-image-annotate-rect").into(),
        annotate_ellipse_label: locale.tr("viewer-image-annotate-ellipse").into(),
        annotate_poly_label: locale.tr("viewer-image-annotate-poly").into(),
        annotate_pen_label: locale.tr("viewer-image-annotate-pen").into(),
        annotate_text_label: locale.tr("viewer-image-annotate-text").into(),
        annotate_callout_label: locale.tr("viewer-image-annotate-callout").into(),
        annotate_privacy_label: locale.tr("viewer-image-annotate-privacy").into(),
        annotate_highlight_label: locale.tr("viewer-image-annotate-highlight").into(),
        annotate_watermark_label: locale.tr("viewer-image-annotate-watermark").into(),
        annotate_wm_image_label: locale.tr("viewer-image-annotate-wm-image").into(),
        annotate_stamp_label: locale.tr("viewer-image-annotate-stamp").into(),
        print_label: locale.tr("viewer-image-print").into(),
        print_preview_label: locale.tr("viewer-image-print-preview").into(),
        print_sheet_label: locale.tr("viewer-image-print-sheet").into(),
        print_hint_label: locale.tr("viewer-image-print-hint").into(),
        export_label: locale.tr("viewer-image-export").into(),
        export_save_label: locale.tr("viewer-image-export-save").into(),
        export_ico_label: locale.tr("viewer-image-export-ico").into(),
        export_favicon_label: locale.tr("viewer-image-export-favicon").into(),
        export_copy_label: locale.tr("viewer-image-export-copy").into(),
        export_paste_label: locale.tr("viewer-image-export-paste").into(),
        export_wallpaper_label: locale.tr("viewer-image-export-wallpaper").into(),
        export_email_label: locale.tr("viewer-image-export-email").into(),
        export_share_label: locale.tr("viewer-image-export-share").into(),
        export_shot_label: locale.tr("viewer-image-export-shot").into(),
        export_shot_window_label: locale.tr("viewer-image-export-shot-window").into(),
        export_shot_delay_label: locale.tr("viewer-image-export-shot-delay").into(),
        export_hint_label: locale.tr("viewer-image-export-hint").into(),
        thumbs: ModelRc::new(VecModel::default()),
        thumb_strip: 1,
        thumb_grid: false,
        thumb_size: 1,
        thumb_show_meta: true,
        thumbs_label: locale.tr("viewer-image-thumbs").into(),
        thumb_grid_label: locale.tr("viewer-image-thumb-grid").into(),
        thumb_size_label: locale.tr("viewer-image-thumb-size").into(),
        thumb_meta_label: locale.tr("viewer-image-thumb-meta").into(),
        contact_sheet_label: locale.tr("viewer-image-contact-sheet").into(),
        slideshow_playing: false,
        slideshow_paused: false,
        slideshow_interval_ms: 4000,
        slideshow_random: false,
        slideshow_transition: 1,
        slideshow_transition_ms: 500,
        slideshow_overlay: true,
        slideshow_overlay_text: SharedString::new(),
        slideshow_music: SharedString::new(),
        slideshow_gen: 0,
        prev_rgba_image: Image::default(),
        prev_width_px: 0,
        prev_height_px: 0,
        slideshow_label: locale.tr("viewer-image-slideshow").into(),
        slideshow_pause_label: locale.tr("viewer-image-slideshow-pause").into(),
        slideshow_speed_label: format!("{} {}s", locale.tr("viewer-image-slideshow-speed"), 4)
            .into(),
        slideshow_random_label: locale.tr("viewer-image-slideshow-random").into(),
        slideshow_transition_label: locale.tr("viewer-image-slideshow-transition").into(),
        slideshow_overlay_label: locale.tr("viewer-image-slideshow-overlay").into(),
        slideshow_music_label: locale.tr("viewer-image-slideshow-music").into(),
        slideshow_export_html_label: locale.tr("viewer-image-slideshow-export-html").into(),
        slideshow_export_video_label: locale.tr("viewer-image-slideshow-export-video").into(),
        slideshow_export_exe_label: locale.tr("viewer-image-slideshow-export-exe").into(),
        slideshow_export_scr_label: locale.tr("viewer-image-slideshow-export-scr").into(),
        meta_panel: false,
        meta_overlay: false,
        meta_text: SharedString::new(),
        meta_overlay_text: SharedString::new(),
        hist_image: Image::default(),
        hist_width: 0,
        hist_height: 0,
        hist_mode: 0,
        probe_text: SharedString::new(),
        gps_label: SharedString::new(),
        has_gps: false,
        meta_label: locale.tr("viewer-image-meta").into(),
        meta_overlay_label: locale.tr("viewer-image-meta-overlay").into(),
        histogram_label: locale.tr("viewer-image-histogram").into(),
        gps_map_label: locale.tr("viewer-image-gps-map").into(),
        meta_edit_title: SharedString::new(),
        meta_edit_creator: SharedString::new(),
        meta_edit_copyright: SharedString::new(),
        meta_edit_keywords: SharedString::new(),
        meta_edit_description: SharedString::new(),
        meta_edit_date: SharedString::new(),
        meta_edit_gps: SharedString::new(),
        meta_save_label: locale.tr("viewer-image-meta-save").into(),
        meta_strip_label: locale.tr("viewer-image-meta-strip").into(),
        meta_strip_gps_label: locale.tr("viewer-image-meta-strip-gps").into(),
        meta_export_csv_label: locale.tr("viewer-image-meta-export-csv").into(),
        meta_export_xml_label: locale.tr("viewer-image-meta-export-xml").into(),
        meta_title_field_label: locale.tr("viewer-image-meta-title-field").into(),
        meta_creator_field_label: locale.tr("viewer-image-meta-creator-field").into(),
        meta_copyright_field_label: locale.tr("viewer-image-meta-copyright-field").into(),
        meta_keywords_field_label: locale.tr("viewer-image-meta-keywords-field").into(),
        meta_description_field_label: locale.tr("viewer-image-meta-description-field").into(),
        meta_date_field_label: locale.tr("viewer-image-meta-date-field").into(),
        meta_gps_field_label: locale.tr("viewer-image-meta-gps-field").into(),
        anim_count: 0,
        anim_index: 0,
        anim_playing: false,
        anim_delay_ms: 0,
        anim_label: SharedString::new(),
        anim_index_label: SharedString::new(),
        anim_thumbs: ModelRc::new(VecModel::default()),
        anim_play_label: locale.tr("viewer-image-anim-play").into(),
        anim_pause_label: locale.tr("viewer-image-anim-pause").into(),
        anim_prev_label: locale.tr("viewer-image-anim-prev").into(),
        anim_next_label: locale.tr("viewer-image-anim-next").into(),
        anim_first_label: locale.tr("viewer-image-anim-first").into(),
        anim_last_label: locale.tr("viewer-image-anim-last").into(),
        anim_export_label: locale.tr("viewer-image-anim-export").into(),
        anim_extract_label: locale.tr("viewer-image-page-extract").into(),
        anim_hint_label: locale.tr("viewer-image-anim-hint").into(),
        anim_can_play: false,
        browse_mode: 0,
        overlay_autohide: true,
        timeline: ModelRc::new(VecModel::default()),
        map_pins: ModelRc::new(VecModel::default()),
        cal_days: ModelRc::new(VecModel::default()),
        cal_title: SharedString::new(),
        timeline_label: locale.tr("viewer-image-timeline").into(),
        map_view_label: locale.tr("viewer-image-map-view").into(),
        calendar_label: locale.tr("viewer-image-calendar").into(),
        browse_empty_label: locale.tr("viewer-image-browse-empty").into(),
        map_empty_label: locale.tr("viewer-image-map-empty").into(),
        cal_empty_label: locale.tr("viewer-image-cal-empty").into(),
        cal_prev_label: locale.tr("viewer-image-cal-prev").into(),
        cal_next_label: locale.tr("viewer-image-cal-next").into(),
        overlay_autohide_label: locale.tr("viewer-image-overlay-autohide").into(),
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
        extract_page_label: locale.tr("viewer-pdf-extract-page").into(),
    }
}

fn text_chrome_labels(
    locale: &LocaleManager,
) -> (
    SharedString,
    SharedString,
    SharedString,
    SharedString,
    SharedString,
    SharedString,
    SharedString,
    SharedString,
    SharedString,
    SharedString,
    SharedString,
    SharedString,
    SharedString,
    SharedString,
    SharedString,
    SharedString,
) {
    (
        locale.tr("viewer-text-mode-text").into(),
        locale.tr("viewer-text-mode-hex").into(),
        locale.tr("viewer-text-mode-binary").into(),
        locale.tr("viewer-text-wrap").into(),
        locale.tr("viewer-text-no-wrap").into(),
        locale.tr("viewer-text-find").into(),
        locale.tr("viewer-text-find-placeholder").into(),
        locale.tr("viewer-text-find-no-match").into(),
        locale.tr("viewer-text-replace").into(),
        locale.tr("viewer-text-replace-placeholder").into(),
        locale.tr("viewer-text-replace-one").into(),
        locale.tr("viewer-text-replace-all").into(),
        locale.tr("viewer-text-regex").into(),
        locale.tr("viewer-text-multiline").into(),
        locale.tr("viewer-text-print").into(),
        locale.tr("viewer-text-undo").into(),
    )
}

fn empty_viewer_text_model(locale: &LocaleManager) -> ViewerTextModel {
    let lines_args = orchid_i18n::FluentArgs::new().with("count", "0");
    let encodings: Vec<SharedString> = orchid_viewers::VIEWER_ENCODINGS
        .iter()
        .map(|e| SharedString::from(*e))
        .collect();
    let (
        mode_text_label,
        mode_hex_label,
        mode_bin_label,
        wrap_label,
        no_wrap_label,
        find_label,
        find_placeholder,
        find_no_match_label,
        replace_label,
        replace_placeholder,
        replace_one_label,
        replace_all_label,
        regex_label,
        multiline_label,
        print_label,
        undo_label,
    ) = text_chrome_labels(locale);
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
        display_mode: 0,
        can_undo: false,
        can_redo: false,
        find_gen: 0,
        find_anchor: 0,
        find_cursor: 0,
        find_match_index: 0,
        find_match_count: 0,
        encodings: ModelRc::new(VecModel::from(encodings)),
        mode_text_label,
        mode_hex_label,
        mode_bin_label,
        wrap_label,
        no_wrap_label,
        find_label,
        find_placeholder,
        find_no_match_label,
        replace_label,
        replace_placeholder,
        replace_one_label,
        replace_all_label,
        regex_label,
        multiline_label,
        print_label,
        undo_label,
        redo_label: locale.tr("viewer-text-redo").into(),
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

fn empty_viewer_media_model(locale: &LocaleManager) -> ViewerMediaModel {
    ViewerMediaModel {
        path_display: SharedString::new(),
        kind_label: SharedString::new(),
        play_label: locale.tr("viewer-media-play").into(),
        hint_label: locale.tr("viewer-media-hint").into(),
    }
}

fn empty_viewer_html_model(locale: &LocaleManager) -> ViewerHtmlModel {
    ViewerHtmlModel {
        path_display: SharedString::new(),
        source_preview: SharedString::new(),
        open_label: locale.tr("viewer-html-open").into(),
        hint_label: locale.tr("viewer-html-hint").into(),
        source_label: locale.tr("viewer-html-source").into(),
    }
}

fn empty_viewer_document_model(locale: &LocaleManager) -> ViewerDocumentModel {
    ViewerDocumentModel {
        path_display: SharedString::new(),
        plain_text: SharedString::new(),
        dirty: false,
        info_text: SharedString::new(),
        bold: false,
        italic: false,
        underline: false,
        strikethrough: false,
        highlight: false,
        superscript: false,
        subscript: false,
        font_size_pt: 0.0,
        font_size_label: SharedString::from("14"),
        font_family: SharedString::new(),
        font_family_label: locale.tr("viewer-document-font-default").into(),
        color_rgb: 0,
        alignment: 0,
        list_kind: 0,
        can_undo: false,
        can_redo: false,
        source_mode: false,
        preview_image: Image::default(),
        preview_width_px: 0,
        preview_height_px: 0,
        save_label: locale.tr("viewer-document-save").into(),
        undo_label: locale.tr("viewer-document-undo").into(),
        redo_label: locale.tr("viewer-document-redo").into(),
        bold_label: locale.tr("viewer-document-bold").into(),
        italic_label: locale.tr("viewer-document-italic").into(),
        underline_label: locale.tr("viewer-document-underline").into(),
        strikethrough_label: locale.tr("viewer-document-strikethrough").into(),
        highlight_label: locale.tr("viewer-document-highlight").into(),
        superscript_label: locale.tr("viewer-document-superscript").into(),
        subscript_label: locale.tr("viewer-document-subscript").into(),
        clear_formatting_label: locale.tr("viewer-document-clear-formatting").into(),
        link_label: locale.tr("viewer-document-link").into(),
        link_placeholder: locale.tr("viewer-document-link-placeholder").into(),
        link_apply_label: locale.tr("viewer-document-link-apply").into(),
        link_remove_label: locale.tr("viewer-document-link-remove").into(),
        tip_link: locale.tr("viewer-document-tip-link").into(),
        link_url: SharedString::new(),
        font_smaller_label: locale.tr("viewer-document-font-smaller").into(),
        font_larger_label: locale.tr("viewer-document-font-larger").into(),
        font_family_prev_label: locale.tr("viewer-document-font-prev").into(),
        font_family_next_label: locale.tr("viewer-document-font-next").into(),
        align_left_label: locale.tr("viewer-document-align-left").into(),
        align_center_label: locale.tr("viewer-document-align-center").into(),
        align_right_label: locale.tr("viewer-document-align-right").into(),
        align_justify_label: locale.tr("viewer-document-align-justify").into(),
        list_bullet_label: locale.tr("viewer-document-list-bullet").into(),
        list_numbered_label: locale.tr("viewer-document-list-numbered").into(),
        image_insert_label: locale.tr("viewer-document-image-insert").into(),
        table_insert_label: locale.tr("viewer-document-table-insert").into(),
        table_row_insert_label: locale.tr("viewer-document-table-row-insert").into(),
        table_row_delete_label: locale.tr("viewer-document-table-row-delete").into(),
        table_col_insert_label: locale.tr("viewer-document-table-col-insert").into(),
        table_col_delete_label: locale.tr("viewer-document-table-col-delete").into(),
        source_label: locale.tr("viewer-document-source").into(),
        preview_label: locale.tr("viewer-document-preview").into(),
        find_label: locale.tr("viewer-document-find").into(),
        find_placeholder: locale.tr("viewer-document-find-placeholder").into(),
        replace_label: locale.tr("viewer-document-replace").into(),
        replace_placeholder: locale.tr("viewer-document-replace-placeholder").into(),
        replace_one_label: locale.tr("viewer-document-replace-one").into(),
        replace_all_label: locale.tr("viewer-document-replace-all").into(),
        tip_find: locale.tr("viewer-document-tip-find").into(),
        tip_find_next: locale.tr("viewer-document-tip-find-next").into(),
        tip_find_prev: locale.tr("viewer-document-tip-find-prev").into(),
        tip_find_close: locale.tr("viewer-document-tip-find-close").into(),
        find_no_match_label: locale.tr("viewer-document-find-no-match").into(),
        find_gen: 0,
        find_anchor: 0,
        find_cursor: 0,
        find_match_index: 0,
        find_match_count: 0,
        link_hover: false,
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
        document: empty_viewer_document_model(locale),
        media: empty_viewer_media_model(locale),
        html: empty_viewer_html_model(locale),
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
            || *message == ViewerError::UnsupportedAvif.to_string()
            || *message == ViewerError::UnsupportedJpeg2000.to_string()
            || *message == ViewerError::UnsupportedEps.to_string()
            || *message == ViewerError::UnsupportedCdr.to_string()
            || *message == ViewerError::UnsupportedEmf.to_string()
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
        Vs::Document(s) => {
            model.kind = 7;
            model.document = build_document_snapshot(s, locale);
        }
        Vs::Media(s) => {
            model.kind = 8;
            model.media = ViewerMediaModel {
                path_display: s.path_display.clone().into(),
                kind_label: s.kind_label.clone().into(),
                play_label: locale.tr("viewer-media-play").into(),
                hint_label: locale.tr("viewer-media-hint").into(),
            };
        }
        Vs::Html(s) => {
            model.kind = 9;
            model.html = ViewerHtmlModel {
                path_display: s.path_display.clone().into(),
                source_preview: SharedString::from(s.source_preview.as_ref()),
                open_label: locale.tr("viewer-html-open").into(),
                hint_label: locale.tr("viewer-html-hint").into(),
                source_label: locale.tr("viewer-html-source").into(),
            };
        }
    }

    model
}

fn build_document_snapshot(
    s: &orchid_viewers::DocumentSnapshot,
    locale: &LocaleManager,
) -> ViewerDocumentModel {
    let args = orchid_i18n::FluentArgs::new()
        .with("blocks", s.block_count.to_string())
        .with(
            "warnings",
            if s.warnings.is_empty() {
                "0".into()
            } else {
                s.warnings.len().to_string()
            },
        );
    let mut model = empty_viewer_document_model(locale);
    model.path_display = s.path_display.clone().into();
    model.plain_text = s.plain_text.as_ref().into();
    model.dirty = s.dirty;
    model.info_text = locale.tr_args("viewer-document-info", &args).into();
    model.bold = s.bold;
    model.italic = s.italic;
    model.underline = s.underline;
    model.strikethrough = s.strikethrough;
    model.highlight = s.highlight;
    model.superscript = s.superscript;
    model.subscript = s.subscript;
    model.font_size_pt = s.font_size_pt;
    model.font_size_label = if s.font_size_pt > 0.0 {
        format!("{}", s.font_size_pt.round() as i32).into()
    } else {
        SharedString::from("14")
    };
    model.font_family = s.font_family.clone().into();
    model.font_family_label = if s.font_family.is_empty() {
        locale.tr("viewer-document-font-default").into()
    } else {
        s.font_family.clone().into()
    };
    model.color_rgb = s.color_rgb as i32;
    model.alignment = i32::from(s.alignment);
    model.list_kind = i32::from(s.list_kind);
    model.can_undo = s.can_undo;
    model.can_redo = s.can_redo;
    model.source_mode = s.source_mode;
    model.preview_width_px = s.preview_width_px as i32;
    model.preview_height_px = s.preview_height_px as i32;
    model.preview_image =
        slint_image_from_rgba(&s.preview_rgba, s.preview_width_px, s.preview_height_px);
    model.find_gen = s.find_gen;
    model.find_anchor = s.find_anchor;
    model.find_cursor = s.find_cursor;
    model.find_match_index = s.find_match_index;
    model.find_match_count = s.find_match_count;
    model.link_hover = s.link_hover;
    model.link_url = s.link_url.clone().into();
    model
}

fn composite_checkerboard(rgba: &Arc<Vec<u8>>, width: u32, height: u32) -> Arc<Vec<u8>> {
    const TILE: u32 = 8;
    let mut out = rgba.as_ref().clone();
    let w = width as usize;
    for y in 0..height {
        for x in 0..width {
            let i = (y as usize * w + x as usize) * 4;
            let Some(px) = out.get_mut(i..i + 4) else {
                continue;
            };
            let a = u32::from(px[3]);
            if a >= 255 {
                continue;
            }
            let light = ((x / TILE) + (y / TILE)) % 2 == 0;
            let c = if light { 204u32 } else { 102u32 };
            if a == 0 {
                px[0] = c as u8;
                px[1] = c as u8;
                px[2] = c as u8;
                px[3] = 255;
            } else {
                let ia = 255 - a;
                px[0] = ((u32::from(px[0]) * a + c * ia) / 255) as u8;
                px[1] = ((u32::from(px[1]) * a + c * ia) / 255) as u8;
                px[2] = ((u32::from(px[2]) * a + c * ia) / 255) as u8;
                px[3] = 255;
            }
        }
    }
    Arc::new(out)
}

fn build_image_snapshot(
    s: &orchid_viewers::ImageSnapshot,
    locale: &LocaleManager,
) -> ViewerImageModel {
    let pixels = if s.background == 4 {
        composite_checkerboard(&s.rgba_bytes, s.width_px, s.height_px)
    } else {
        Arc::clone(&s.rgba_bytes)
    };
    let image = slint_image_from_rgba(&pixels, s.width_px, s.height_px);

    let args = orchid_i18n::FluentArgs::new()
        .with("width", s.width_px.to_string())
        .with("height", s.height_px.to_string())
        .with("size", locale.format_byte_size(s.size_bytes))
        .with("format", viewer_image_format_label(locale, &s.format_label));
    let mut info = locale.tr_args("viewer-image-info", &args);
    if s.bit_depth > 0 {
        info.push_str(" · ");
        info.push_str(&format!("{}-bit", s.bit_depth));
        if !s.color_model.is_empty() {
            info.push(' ');
            info.push_str(&s.color_model);
        }
    }
    if !s.color_source.is_empty() {
        info.push_str(" · ");
        info.push_str(&s.color_source);
        if s.color_dest != s.color_source {
            info.push_str(" → ");
            info.push_str(&s.color_dest);
        }
    }
    if s.orientation > 1 {
        info.push_str(" · EXIF ");
        info.push_str(&s.orientation.to_string());
    }
    let nav_index_label = if s.folder_count > 0 {
        locale.tr_args(
            "viewer-image-index",
            &orchid_i18n::FluentArgs::new()
                .with("current", s.folder_index.to_string())
                .with("total", s.folder_count.to_string()),
        )
    } else {
        String::new()
    };
    if !nav_index_label.is_empty() {
        info.push_str(" · ");
        info.push_str(&nav_index_label);
    }
    let anim_index_label = if s.anim_count > 1 {
        locale.tr_args(
            "viewer-image-anim-index",
            &orchid_i18n::FluentArgs::new()
                .with("current", s.anim_index.to_string())
                .with("total", s.anim_count.to_string()),
        )
    } else {
        String::new()
    };
    if !anim_index_label.is_empty() {
        info.push_str(" · ");
        if !s.anim_label.is_empty() {
            info.push_str(&s.anim_label);
            info.push(' ');
        }
        info.push_str(&anim_index_label);
    }
    let recent: Vec<SharedString> = s.recent_paths.iter().map(|p| p.clone().into()).collect();
    let thumbs: Vec<ViewerImageThumb> = s
        .thumbs
        .iter()
        .map(|t| slint_folder_thumb(t, locale))
        .collect();
    let timeline: Vec<ViewerImageThumb> = s
        .timeline
        .iter()
        .map(|t| slint_folder_thumb(t, locale))
        .collect();
    let map_pins: Vec<ViewerMapPin> = s
        .map_pins
        .iter()
        .map(|p| {
            let thumbnail = match &p.rgba {
                Some(rgba) if p.width > 0 && p.height > 0 => {
                    slint_image_from_rgba(rgba, p.width, p.height)
                }
                _ => Image::default(),
            };
            ViewerMapPin {
                path: p.path.clone().into(),
                name: p.name.clone().into(),
                x: p.x,
                y: p.y,
                selected: p.selected,
                has_image: p.rgba.is_some(),
                thumbnail,
            }
        })
        .collect();
    let cal_days: Vec<ViewerCalDay> = s
        .cal_days
        .iter()
        .map(|d| {
            let thumbnail = match &d.rgba {
                Some(rgba) if d.width > 0 && d.height > 0 => {
                    slint_image_from_rgba(rgba, d.width, d.height)
                }
                _ => Image::default(),
            };
            ViewerCalDay {
                day: i32::from(d.day),
                count: d.count as i32,
                selected: d.selected,
                path: d.path.clone().into(),
                has_image: d.rgba.is_some(),
                thumbnail,
            }
        })
        .collect();
    ViewerImageModel {
        width_px: s.width_px as i32,
        height_px: s.height_px as i32,
        rgba_image: image,
        zoom: s.zoom,
        pan_x: s.pan_x,
        pan_y: s.pan_y,
        rotation_deg: s.rotation_degrees,
        flipped_h: s.flipped_horizontal,
        flipped_v: s.flipped_vertical,
        fit_mode: i32::from(s.fit_mode),
        bg_kind: i32::from(s.background),
        bg_r: i32::from(s.bg_r),
        bg_g: i32::from(s.bg_g),
        bg_b: i32::from(s.bg_b),
        chrome_hidden: s.chrome_hidden,
        kiosk: s.kiosk,
        info_text: info.into(),
        path_display: s.path_display.clone().into(),
        fit_label: locale.tr("viewer-image-fit-screen").into(),
        fit_width_label: locale.tr("viewer-image-fit-width").into(),
        fit_height_label: locale.tr("viewer-image-fit-height").into(),
        fit_shrink_label: locale.tr("viewer-image-fit-shrink").into(),
        actual_size_label: locale.tr("viewer-image-actual-size").into(),
        bg_label: locale.tr("viewer-image-background").into(),
        fullscreen_label: locale.tr("viewer-image-fullscreen").into(),
        kiosk_label: locale.tr("viewer-image-kiosk").into(),
        next_monitor_label: locale.tr("viewer-image-next-monitor").into(),
        folder_index: s.folder_index as i32,
        folder_count: s.folder_count as i32,
        loop_folder: s.loop_folder,
        recent_paths: ModelRc::new(VecModel::from(recent)),
        nav_index_label: nav_index_label.into(),
        prev_label: locale.tr("viewer-image-prev").into(),
        next_label: locale.tr("viewer-image-next").into(),
        first_label: locale.tr("viewer-image-first").into(),
        last_label: locale.tr("viewer-image-last").into(),
        random_label: locale.tr("viewer-image-random").into(),
        loop_label: locale.tr("viewer-image-loop").into(),
        folder_label: locale.tr("viewer-image-folder").into(),
        recent_label: locale.tr("viewer-image-recent").into(),
        goto_label: locale.tr("viewer-image-goto").into(),
        lens: s.lens,
        zoom_percent_label: locale.tr("viewer-image-zoom-percent").into(),
        zoom_selection_label: locale.tr("viewer-image-zoom-selection").into(),
        lens_label: locale.tr("viewer-image-lens").into(),
        navigator_label: locale.tr("viewer-image-navigator").into(),
        rotate_180_label: locale.tr("viewer-image-rotate-180").into(),
        rotate_angle_label: locale.tr("viewer-image-rotate-angle").into(),
        reset_transform_label: locale.tr("viewer-image-reset-transform").into(),
        lossless_rotate_label: locale.tr("viewer-image-lossless-rotate").into(),
        lossless_flip_label: locale.tr("viewer-image-lossless-flip").into(),
        lossless_crop_label: locale.tr("viewer-image-lossless-crop").into(),
        lossless_exif_label: locale.tr("viewer-image-lossless-exif").into(),
        lossless_folder_label: locale.tr("viewer-image-lossless-folder").into(),
        edit_crop_label: locale.tr("viewer-image-edit-crop").into(),
        edit_aspect_label: locale.tr("viewer-image-edit-aspect").into(),
        edit_keep_label: locale.tr("viewer-image-edit-keep").into(),
        edit_resize_label: locale.tr("viewer-image-edit-resize").into(),
        edit_canvas_label: locale.tr("viewer-image-edit-canvas").into(),
        edit_straighten_label: locale.tr("viewer-image-edit-straighten").into(),
        edit_auto_straighten_label: locale.tr("viewer-image-edit-auto-straighten").into(),
        edit_perspective_label: locale.tr("viewer-image-edit-perspective").into(),
        adjust_label: locale.tr("viewer-image-adjust").into(),
        adjust_apply_label: locale.tr("viewer-image-adjust-apply").into(),
        adjust_hint_label: locale.tr("viewer-image-adjust-hint").into(),
        adjust_auto_levels_label: locale.tr("viewer-image-adjust-auto-levels").into(),
        adjust_auto_contrast_label: locale.tr("viewer-image-adjust-auto-contrast").into(),
        adjust_auto_color_label: locale.tr("viewer-image-adjust-auto-color").into(),
        adjust_gray_label: locale.tr("viewer-image-adjust-gray").into(),
        adjust_sepia_label: locale.tr("viewer-image-adjust-sepia").into(),
        adjust_invert_label: locale.tr("viewer-image-adjust-invert").into(),
        filter_label: locale.tr("viewer-image-filter").into(),
        filter_apply_label: locale.tr("viewer-image-filter-apply").into(),
        filter_hint_label: locale.tr("viewer-image-filter-hint").into(),
        filter_sharpen_label: locale.tr("viewer-image-filter-sharpen").into(),
        filter_blur_label: locale.tr("viewer-image-filter-blur").into(),
        filter_despeckle_label: locale.tr("viewer-image-filter-despeckle").into(),
        filter_cartoon_label: locale.tr("viewer-image-filter-cartoon").into(),
        filter_sketch_label: locale.tr("viewer-image-filter-sketch").into(),
        filter_vignette_label: locale.tr("viewer-image-filter-vignette").into(),
        filter_redeye_label: locale.tr("viewer-image-filter-redeye").into(),
        filter_look_vivid_label: locale.tr("viewer-image-filter-look-vivid").into(),
        filter_look_soft_label: locale.tr("viewer-image-filter-look-soft").into(),
        filter_look_drama_label: locale.tr("viewer-image-filter-look-drama").into(),
        filter_look_clean_label: locale.tr("viewer-image-filter-look-clean").into(),
        annotate_label: locale.tr("viewer-image-annotate").into(),
        annotate_apply_label: locale.tr("viewer-image-annotate-apply").into(),
        annotate_hint_label: locale.tr("viewer-image-annotate-hint").into(),
        annotate_line_label: locale.tr("viewer-image-annotate-line").into(),
        annotate_arrow_label: locale.tr("viewer-image-annotate-arrow").into(),
        annotate_rect_label: locale.tr("viewer-image-annotate-rect").into(),
        annotate_ellipse_label: locale.tr("viewer-image-annotate-ellipse").into(),
        annotate_poly_label: locale.tr("viewer-image-annotate-poly").into(),
        annotate_pen_label: locale.tr("viewer-image-annotate-pen").into(),
        annotate_text_label: locale.tr("viewer-image-annotate-text").into(),
        annotate_callout_label: locale.tr("viewer-image-annotate-callout").into(),
        annotate_privacy_label: locale.tr("viewer-image-annotate-privacy").into(),
        annotate_highlight_label: locale.tr("viewer-image-annotate-highlight").into(),
        annotate_watermark_label: locale.tr("viewer-image-annotate-watermark").into(),
        annotate_wm_image_label: locale.tr("viewer-image-annotate-wm-image").into(),
        annotate_stamp_label: locale.tr("viewer-image-annotate-stamp").into(),
        print_label: locale.tr("viewer-image-print").into(),
        print_preview_label: locale.tr("viewer-image-print-preview").into(),
        print_sheet_label: locale.tr("viewer-image-print-sheet").into(),
        print_hint_label: locale.tr("viewer-image-print-hint").into(),
        export_label: locale.tr("viewer-image-export").into(),
        export_save_label: locale.tr("viewer-image-export-save").into(),
        export_ico_label: locale.tr("viewer-image-export-ico").into(),
        export_favicon_label: locale.tr("viewer-image-export-favicon").into(),
        export_copy_label: locale.tr("viewer-image-export-copy").into(),
        export_paste_label: locale.tr("viewer-image-export-paste").into(),
        export_wallpaper_label: locale.tr("viewer-image-export-wallpaper").into(),
        export_email_label: locale.tr("viewer-image-export-email").into(),
        export_share_label: locale.tr("viewer-image-export-share").into(),
        export_shot_label: locale.tr("viewer-image-export-shot").into(),
        export_shot_window_label: locale.tr("viewer-image-export-shot-window").into(),
        export_shot_delay_label: locale.tr("viewer-image-export-shot-delay").into(),
        export_hint_label: locale.tr("viewer-image-export-hint").into(),
        thumbs: ModelRc::new(VecModel::from(thumbs)),
        thumb_strip: i32::from(s.thumb_strip),
        thumb_grid: s.thumb_grid,
        thumb_size: i32::from(s.thumb_size),
        thumb_show_meta: s.thumb_show_meta,
        thumbs_label: locale.tr("viewer-image-thumbs").into(),
        thumb_grid_label: locale.tr("viewer-image-thumb-grid").into(),
        thumb_size_label: locale.tr("viewer-image-thumb-size").into(),
        thumb_meta_label: locale.tr("viewer-image-thumb-meta").into(),
        contact_sheet_label: locale.tr("viewer-image-contact-sheet").into(),
        slideshow_playing: s.slideshow_playing,
        slideshow_paused: s.slideshow_paused,
        slideshow_interval_ms: s.slideshow_interval_ms as i32,
        slideshow_random: s.slideshow_random,
        slideshow_transition: i32::from(s.slideshow_transition),
        slideshow_transition_ms: s.slideshow_transition_ms as i32,
        slideshow_overlay: s.slideshow_overlay,
        slideshow_overlay_text: s.slideshow_overlay_text.clone().into(),
        slideshow_music: s.slideshow_music.clone().into(),
        slideshow_gen: s.slideshow_gen as i32,
        prev_rgba_image: match &s.prev_rgba {
            Some(rgba) if s.prev_width > 0 && s.prev_height > 0 => {
                slint_image_from_rgba(rgba, s.prev_width, s.prev_height)
            }
            _ => Image::default(),
        },
        prev_width_px: s.prev_width as i32,
        prev_height_px: s.prev_height as i32,
        slideshow_label: locale.tr("viewer-image-slideshow").into(),
        slideshow_pause_label: locale.tr("viewer-image-slideshow-pause").into(),
        slideshow_speed_label: format!(
            "{} {}s",
            locale.tr("viewer-image-slideshow-speed"),
            (s.slideshow_interval_ms / 1000).max(1)
        )
        .into(),
        slideshow_random_label: locale.tr("viewer-image-slideshow-random").into(),
        slideshow_transition_label: locale.tr("viewer-image-slideshow-transition").into(),
        slideshow_overlay_label: locale.tr("viewer-image-slideshow-overlay").into(),
        slideshow_music_label: locale.tr("viewer-image-slideshow-music").into(),
        slideshow_export_html_label: locale.tr("viewer-image-slideshow-export-html").into(),
        slideshow_export_video_label: locale.tr("viewer-image-slideshow-export-video").into(),
        slideshow_export_exe_label: locale.tr("viewer-image-slideshow-export-exe").into(),
        slideshow_export_scr_label: locale.tr("viewer-image-slideshow-export-scr").into(),
        meta_panel: s.meta_panel,
        meta_overlay: s.meta_overlay,
        meta_text: s.meta_text.clone().into(),
        meta_overlay_text: s.meta_overlay_text.clone().into(),
        hist_image: match &s.hist_rgba {
            Some(rgba) if s.hist_width > 0 && s.hist_height > 0 => {
                slint_image_from_rgba(rgba, s.hist_width, s.hist_height)
            }
            _ => Image::default(),
        },
        hist_width: s.hist_width as i32,
        hist_height: s.hist_height as i32,
        hist_mode: i32::from(s.hist_mode),
        probe_text: s.probe_text.clone().into(),
        gps_label: s.gps_label.clone().into(),
        has_gps: s.has_gps,
        meta_label: locale.tr("viewer-image-meta").into(),
        meta_overlay_label: locale.tr("viewer-image-meta-overlay").into(),
        histogram_label: locale.tr("viewer-image-histogram").into(),
        gps_map_label: locale.tr("viewer-image-gps-map").into(),
        meta_edit_title: s.meta_edit_title.clone().into(),
        meta_edit_creator: s.meta_edit_creator.clone().into(),
        meta_edit_copyright: s.meta_edit_copyright.clone().into(),
        meta_edit_keywords: s.meta_edit_keywords.clone().into(),
        meta_edit_description: s.meta_edit_description.clone().into(),
        meta_edit_date: s.meta_edit_date.clone().into(),
        meta_edit_gps: s.meta_edit_gps.clone().into(),
        meta_save_label: locale.tr("viewer-image-meta-save").into(),
        meta_strip_label: locale.tr("viewer-image-meta-strip").into(),
        meta_strip_gps_label: locale.tr("viewer-image-meta-strip-gps").into(),
        meta_export_csv_label: locale.tr("viewer-image-meta-export-csv").into(),
        meta_export_xml_label: locale.tr("viewer-image-meta-export-xml").into(),
        meta_title_field_label: locale.tr("viewer-image-meta-title-field").into(),
        meta_creator_field_label: locale.tr("viewer-image-meta-creator-field").into(),
        meta_copyright_field_label: locale.tr("viewer-image-meta-copyright-field").into(),
        meta_keywords_field_label: locale.tr("viewer-image-meta-keywords-field").into(),
        meta_description_field_label: locale.tr("viewer-image-meta-description-field").into(),
        meta_date_field_label: locale.tr("viewer-image-meta-date-field").into(),
        meta_gps_field_label: locale.tr("viewer-image-meta-gps-field").into(),
        anim_count: s.anim_count as i32,
        anim_index: s.anim_index as i32,
        anim_playing: s.anim_playing,
        anim_delay_ms: s.anim_delay_ms as i32,
        anim_label: s.anim_label.clone().into(),
        anim_index_label: anim_index_label.into(),
        anim_thumbs: ModelRc::new(VecModel::from({
            s.anim_thumbs
                .iter()
                .map(|t| {
                    let thumb_img = match &t.rgba {
                        Some(rgba) if t.width > 0 && t.height > 0 => {
                            slint_image_from_rgba(rgba, t.width, t.height)
                        }
                        _ => Image::default(),
                    };
                    ViewerImageThumb {
                        path: t.path.clone().into(),
                        name: t.name.clone().into(),
                        size_text: t.date_text.clone().into(),
                        date_text: SharedString::new(),
                        rating: 0,
                        has_image: t.rgba.is_some(),
                        thumbnail: thumb_img,
                        selected: t.selected,
                        index: t.index as i32,
                        has_gps: false,
                        gps_lat: 0.0,
                        gps_lon: 0.0,
                    }
                })
                .collect::<Vec<_>>()
        })),
        anim_play_label: locale.tr("viewer-image-anim-play").into(),
        anim_pause_label: locale.tr("viewer-image-anim-pause").into(),
        anim_prev_label: locale.tr("viewer-image-anim-prev").into(),
        anim_next_label: locale.tr("viewer-image-anim-next").into(),
        anim_first_label: locale.tr("viewer-image-anim-first").into(),
        anim_last_label: locale.tr("viewer-image-anim-last").into(),
        anim_export_label: locale.tr("viewer-image-anim-export").into(),
        anim_extract_label: locale.tr("viewer-image-page-extract").into(),
        anim_hint_label: locale.tr("viewer-image-anim-hint").into(),
        anim_can_play: s.anim_can_play,
        browse_mode: i32::from(s.browse_mode),
        overlay_autohide: s.overlay_autohide,
        timeline: ModelRc::new(VecModel::from(timeline)),
        map_pins: ModelRc::new(VecModel::from(map_pins)),
        cal_days: ModelRc::new(VecModel::from(cal_days)),
        cal_title: s.cal_title.clone().into(),
        timeline_label: locale.tr("viewer-image-timeline").into(),
        map_view_label: locale.tr("viewer-image-map-view").into(),
        calendar_label: locale.tr("viewer-image-calendar").into(),
        browse_empty_label: locale.tr("viewer-image-browse-empty").into(),
        map_empty_label: locale.tr("viewer-image-map-empty").into(),
        cal_empty_label: locale.tr("viewer-image-cal-empty").into(),
        cal_prev_label: locale.tr("viewer-image-cal-prev").into(),
        cal_next_label: locale.tr("viewer-image-cal-next").into(),
        overlay_autohide_label: locale.tr("viewer-image-overlay-autohide").into(),
    }
}

fn slint_folder_thumb(
    t: &orchid_viewers::ImageThumbItem,
    locale: &LocaleManager,
) -> ViewerImageThumb {
    let thumbnail = match &t.rgba {
        Some(rgba) if t.width > 0 && t.height > 0 => slint_image_from_rgba(rgba, t.width, t.height),
        _ => Image::default(),
    };
    ViewerImageThumb {
        path: t.path.clone().into(),
        name: t.name.clone().into(),
        size_text: locale.format_byte_size(t.size_bytes).into(),
        date_text: t.date_text.clone().into(),
        rating: i32::from(t.rating),
        has_image: t.rgba.is_some(),
        thumbnail,
        selected: t.selected,
        index: t.index as i32,
        has_gps: t.has_gps,
        gps_lat: t.gps_lat,
        gps_lon: t.gps_lon,
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
        extract_page_label: locale.tr("viewer-pdf-extract-page").into(),
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
    let encodings: Vec<SharedString> = orchid_viewers::VIEWER_ENCODINGS
        .iter()
        .map(|e| SharedString::from(*e))
        .collect();
    let (
        mode_text_label,
        mode_hex_label,
        mode_bin_label,
        wrap_label,
        no_wrap_label,
        find_label,
        find_placeholder,
        find_no_match_label,
        replace_label,
        replace_placeholder,
        replace_one_label,
        replace_all_label,
        regex_label,
        multiline_label,
        print_label,
        undo_label,
    ) = text_chrome_labels(locale);
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
        display_mode: i32::from(s.display_mode),
        can_undo: s.can_undo,
        can_redo: s.can_redo,
        find_gen: s.find_gen,
        find_anchor: s.find_anchor,
        find_cursor: s.find_cursor,
        find_match_index: s.find_match_index,
        find_match_count: s.find_match_count,
        encodings: ModelRc::new(VecModel::from(encodings)),
        mode_text_label,
        mode_hex_label,
        mode_bin_label,
        wrap_label,
        no_wrap_label,
        find_label,
        find_placeholder,
        find_no_match_label,
        replace_label,
        replace_placeholder,
        replace_one_label,
        replace_all_label,
        regex_label,
        multiline_label,
        print_label,
        undo_label,
        redo_label: locale.tr("viewer-text-redo").into(),
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
