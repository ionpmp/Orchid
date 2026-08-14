//! Batch destructive image edits that write sibling files.

use std::path::PathBuf;
use std::sync::Arc;

use orchid_viewers::{
    apply_adjust_file, apply_annotate_file, apply_edit_file, apply_filter_file,
    is_image_file_extension, load_image_file, parse_adjust_line, parse_annotate_line,
    parse_canvas_line, parse_filter_line_in, parse_resize_line, save_filter_preset, AdjustOp,
    AnnotateOp, EditOp, FilterOp, FilterPreset,
};

use super::map_fs_error;
use super::{ActionOutcome, FileManagerInner};
use crate::error::Result as WidgetResult;
use crate::error::WidgetError;

pub(super) async fn run(
    inner: &Arc<FileManagerInner>,
    action_id: &str,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    match action_id {
        "fs.image-resize" => resize(inner, paths, input).await,
        "fs.image-canvas" => canvas(inner, paths, input).await,
        "fs.image-auto-straighten" => auto_straighten(inner, paths).await,
        "fs.image-adjust" => adjust(inner, paths, input).await,
        "fs.image-auto-levels" => adjust_token(inner, paths, AdjustOp::AutoLevels).await,
        "fs.image-auto-contrast" => adjust_token(inner, paths, AdjustOp::AutoContrast).await,
        "fs.image-auto-color" => adjust_token(inner, paths, AdjustOp::AutoColor).await,
        "fs.image-gray" => adjust_token(inner, paths, AdjustOp::Grayscale).await,
        "fs.image-sepia" => adjust_token(inner, paths, AdjustOp::Sepia).await,
        "fs.image-invert" => adjust_token(inner, paths, AdjustOp::Invert).await,
        "fs.image-filter" => filter_line(inner, paths, input).await,
        "fs.image-sharpen" => filter_token(inner, paths, "sharpen").await,
        "fs.image-blur" => filter_token(inner, paths, "blur=1.5").await,
        "fs.image-despeckle" => filter_token(inner, paths, "despeckle").await,
        "fs.image-cartoon" => filter_token(inner, paths, "cartoon").await,
        "fs.image-sketch" => filter_token(inner, paths, "sketch").await,
        "fs.image-vignette" => filter_token(inner, paths, "vignette=40").await,
        "fs.image-redeye" => filter_token(inner, paths, "redeye").await,
        "fs.image-filter-save-look" => save_look(inner, paths, input).await,
        "fs.image-annotate" => annotate_line(inner, paths, input).await,
        "fs.image-watermark" => {
            watermark_line(
                inner,
                paths,
                input,
                "watermark=© Orchid | pos=br | opacity=40 | size=18",
                "fm-image-watermark-title",
                "fm-image-watermark-hint",
            )
            .await
        }
        "fs.image-wm-image" => {
            watermark_line(
                inner,
                paths,
                input,
                "wm-image=logo.png | pos=br | opacity=30 | scale=0.22",
                "fm-image-wm-image-title",
                "fm-image-wm-image-hint",
            )
            .await
        }
        "fs.image-stamp" => {
            annotate_token(
                inner,
                paths,
                "stamp | pos=br | opacity=50 | size=16",
                "fm-action-image-stamp",
            )
            .await
        }
        _ => Ok(ActionOutcome::Done),
    }
}

async fn resize(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.image-resize",
            &path_strs(&images),
            "50% filter=lanczos",
            locale.tr("fm-image-resize-title"),
            locale.tr("fm-image-resize-hint"),
        ));
    };
    let (spec, filter) = parse_resize_line(raw).ok_or_else(|| {
        WidgetError::InvalidStateForOperation(locale.tr("fm-image-edit-bad-spec"))
    })?;
    let n = apply_all(&images, &EditOp::Resize { spec, filter })?;
    Ok(report_count(locale, "fm-image-resize-title", n))
}

async fn canvas(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.image-canvas",
            &path_strs(&images),
            "+40+40",
            locale.tr("fm-image-canvas-title"),
            locale.tr("fm-image-canvas-hint"),
        ));
    };
    let mut n = 0u32;
    for (_, os) in &images {
        let src = load_image_file(os)
            .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
        let (w, h) = parse_canvas_line(raw, src.width, src.height).ok_or_else(|| {
            WidgetError::InvalidStateForOperation(locale.tr("fm-image-edit-bad-spec"))
        })?;
        apply_edit_file(
            os,
            &EditOp::Canvas {
                width: w,
                height: h,
                fill: [0, 0, 0, 255],
            },
        )
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
        n += 1;
    }
    Ok(report_count(locale, "fm-image-canvas-title", n))
}

async fn auto_straighten(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let n = apply_all(&images, &EditOp::AutoStraighten)?;
    Ok(report_count(locale, "fm-action-image-auto-straighten", n))
}

async fn adjust(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.image-adjust",
            &path_strs(&images),
            "brightness=0 | contrast=0 | saturation=0 | temp=0",
            locale.tr("fm-image-adjust-title"),
            locale.tr("fm-image-adjust-hint"),
        ));
    };
    let op = parse_adjust_line(raw).ok_or_else(|| {
        WidgetError::InvalidStateForOperation(locale.tr("fm-image-adjust-bad-spec"))
    })?;
    let n = apply_adjust_all(&images, &op)?;
    Ok(report_count(locale, "fm-image-adjust-title", n))
}

async fn adjust_token(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    op: AdjustOp,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let title = match op {
        AdjustOp::AutoLevels => "fm-action-image-auto-levels",
        AdjustOp::AutoContrast => "fm-action-image-auto-contrast",
        AdjustOp::AutoColor => "fm-action-image-auto-color",
        AdjustOp::Grayscale => "fm-action-image-gray",
        AdjustOp::Sepia => "fm-action-image-sepia",
        AdjustOp::Invert => "fm-action-image-invert",
        _ => "fm-image-adjust-title",
    };
    let n = apply_adjust_all(&images, &op)?;
    Ok(report_count(locale, title, n))
}

async fn filter_line(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.image-filter",
            &path_strs(&images),
            "sharpen=1 | vignette=20",
            locale.tr("fm-image-filter-title"),
            locale.tr("fm-image-filter-hint"),
        ));
    };
    let dir = images.first().and_then(|(_, os)| os.parent());
    let op = parse_filter_line_in(raw, dir).ok_or_else(|| {
        WidgetError::InvalidStateForOperation(locale.tr("fm-image-filter-bad-spec"))
    })?;
    let n = apply_filter_all(&images, &op)?;
    Ok(report_count(locale, "fm-image-filter-title", n))
}

async fn filter_token(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    raw: &str,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let dir = images.first().and_then(|(_, os)| os.parent());
    let op = parse_filter_line_in(raw, dir).ok_or_else(|| {
        WidgetError::InvalidStateForOperation(locale.tr("fm-image-filter-bad-spec"))
    })?;
    let title = match &op {
        FilterOp::Sharpen { .. } => "fm-action-image-sharpen",
        FilterOp::Blur { .. } => "fm-action-image-blur",
        FilterOp::Despeckle => "fm-action-image-despeckle",
        FilterOp::Cartoon => "fm-action-image-cartoon",
        FilterOp::Sketch => "fm-action-image-sketch",
        FilterOp::Vignette { .. } => "fm-action-image-vignette",
        FilterOp::RedEye { .. } => "fm-action-image-redeye",
        _ => "fm-image-filter-title",
    };
    let n = apply_filter_all(&images, &op)?;
    Ok(report_count(locale, title, n))
}

async fn save_look(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.image-filter-save-look",
            &path_strs(&images),
            "name=portrait | skin=40 | vignette=15",
            locale.tr("fm-image-filter-save-title"),
            locale.tr("fm-image-filter-save-hint"),
        ));
    };
    let (name, ops) = parse_save_look(raw).ok_or_else(|| {
        WidgetError::InvalidStateForOperation(locale.tr("fm-image-filter-bad-spec"))
    })?;
    let dir = images
        .first()
        .and_then(|(_, os)| os.parent())
        .ok_or_else(|| {
            WidgetError::InvalidStateForOperation(locale.tr("fm-image-filter-bad-spec"))
        })?;
    save_filter_preset(dir, FilterPreset { name, ops })
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    Ok(report_count(locale, "fm-image-filter-save-title", 1))
}

fn parse_save_look(raw: &str) -> Option<(String, String)> {
    let mut name = None;
    let mut ops = Vec::new();
    for part in raw.split(" | ") {
        let part = part.trim();
        if let Some(v) = part
            .strip_prefix("name=")
            .or_else(|| part.strip_prefix("look="))
        {
            name = Some(v.trim().to_string());
        } else if !part.is_empty() {
            ops.push(part);
        }
    }
    let name = name.filter(|s| !s.is_empty())?;
    if ops.is_empty() {
        return None;
    }
    Some((name, ops.join(" | ")))
}

async fn annotate_line(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.image-annotate",
            &path_strs(&images),
            "watermark=© Orchid | pos=br | opacity=40 | size=18",
            locale.tr("fm-image-annotate-title"),
            locale.tr("fm-image-annotate-hint"),
        ));
    };
    let op = parse_annotate_line(raw).ok_or_else(|| {
        WidgetError::InvalidStateForOperation(locale.tr("fm-image-annotate-bad-spec"))
    })?;
    let n = apply_annotate_all(&images, &op)?;
    Ok(report_count(locale, "fm-image-annotate-title", n))
}

async fn watermark_line(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
    proposed: &str,
    title_key: &str,
    hint_key: &str,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            if title_key.contains("wm-image") {
                "fs.image-wm-image"
            } else {
                "fs.image-watermark"
            },
            &path_strs(&images),
            proposed,
            locale.tr(title_key),
            locale.tr(hint_key),
        ));
    };
    let op = parse_annotate_line(raw).ok_or_else(|| {
        WidgetError::InvalidStateForOperation(locale.tr("fm-image-annotate-bad-spec"))
    })?;
    let n = apply_annotate_all(&images, &op)?;
    Ok(report_count(locale, title_key, n))
}

async fn annotate_token(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    raw: &str,
    title_key: &str,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let op = parse_annotate_line(raw).ok_or_else(|| {
        WidgetError::InvalidStateForOperation(locale.tr("fm-image-annotate-bad-spec"))
    })?;
    let n = apply_annotate_all(&images, &op)?;
    Ok(report_count(locale, title_key, n))
}

fn apply_annotate_all(
    images: &[(orchid_fs::FsPath, PathBuf)],
    op: &AnnotateOp,
) -> WidgetResult<u32> {
    let mut n = 0u32;
    for (_, os) in images {
        apply_annotate_file(os, op)
            .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
        n += 1;
    }
    Ok(n)
}

fn selected_images(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> WidgetResult<Vec<(orchid_fs::FsPath, PathBuf)>> {
    let mut out = Vec::new();
    for p in paths {
        let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
        let ext = fp
            .file_name()
            .and_then(|n| n.rsplit_once('.'))
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        if !is_image_file_extension(&ext) {
            continue;
        }
        let os = fp.to_local().map_err(map_fs_error)?;
        out.push((fp, os));
    }
    if out.is_empty() {
        return Err(WidgetError::InvalidStateForOperation(
            inner.deps.locale.tr("fm-meta-none"),
        ));
    }
    Ok(out)
}

fn apply_filter_all(images: &[(orchid_fs::FsPath, PathBuf)], op: &FilterOp) -> WidgetResult<u32> {
    let mut n = 0u32;
    for (_, os) in images {
        apply_filter_file(os, op)
            .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
        n += 1;
    }
    Ok(n)
}

fn apply_adjust_all(images: &[(orchid_fs::FsPath, PathBuf)], op: &AdjustOp) -> WidgetResult<u32> {
    let mut n = 0u32;
    for (_, os) in images {
        apply_adjust_file(os, op)
            .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
        n += 1;
    }
    Ok(n)
}

fn apply_all(images: &[(orchid_fs::FsPath, PathBuf)], op: &EditOp) -> WidgetResult<u32> {
    let mut n = 0u32;
    for (_, os) in images {
        apply_edit_file(os, op)
            .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
        n += 1;
    }
    Ok(n)
}

fn path_strs(images: &[(orchid_fs::FsPath, PathBuf)]) -> Vec<String> {
    images
        .iter()
        .map(|(fp, _)| fp.as_str().to_string())
        .collect()
}

fn report_count(locale: &orchid_i18n::LocaleManager, title_key: &str, n: u32) -> ActionOutcome {
    ActionOutcome::NeedsReport {
        title: locale.tr(title_key),
        body: locale.tr_args(
            "fm-meta-applied",
            &orchid_i18n::FluentArgs::new().with("count", n.to_string()),
        ),
    }
}

fn prompt(
    action_id: &str,
    paths: &[String],
    proposed: &str,
    title: String,
    hint: String,
) -> ActionOutcome {
    ActionOutcome::NeedsToolPrompt {
        action_id: action_id.into(),
        paths: paths.to_vec(),
        proposed: proposed.into(),
        title,
        hint,
    }
}
