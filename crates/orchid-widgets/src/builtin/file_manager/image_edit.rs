//! Batch destructive image edits that write sibling files.

use std::path::PathBuf;
use std::sync::Arc;

use orchid_viewers::{
    apply_edit_file, is_image_file_extension, load_image_file, parse_canvas_line,
    parse_resize_line, EditOp,
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
