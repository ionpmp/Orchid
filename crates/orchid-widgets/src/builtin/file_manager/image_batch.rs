//! Multi-image convert, compare, merge, preview, cancel, and recipes.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use orchid_viewers::{
    apply_adjust_file, apply_annotate_file, apply_edit_file, apply_lossless, compare_files,
    composite_files, convert_file, diff_files, export_thumb_file, format_from_extension,
    image_date_token, load_batch_recipes, load_image_file, merge_hdr_files, parse_adjust_line,
    parse_annotate_line, parse_resize_line, pick_best_file, planned_sibling, save_batch_recipe,
    stitch_panorama_files, BatchRecipe, CompositeMode, EncodeFormat, LosslessOp,
};

use super::batch_rename::apply_rename_pattern;
use super::image_edit::{path_strs, prompt, report_count, selected_images};
use super::map_fs_error;
use super::{ActionOutcome, FileManagerInner};
use crate::error::Result as WidgetResult;
use crate::error::WidgetError;

static BATCH_CANCEL: AtomicBool = AtomicBool::new(false);

pub(super) fn begin_batch() {
    BATCH_CANCEL.store(false, Ordering::SeqCst);
}

pub(super) fn cancelled() -> bool {
    BATCH_CANCEL.load(Ordering::SeqCst)
}

pub(super) fn request_cancel() {
    BATCH_CANCEL.store(true, Ordering::SeqCst);
}

pub(super) async fn run(
    inner: &Arc<FileManagerInner>,
    action_id: &str,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    match action_id {
        "fs.image-convert" => convert(inner, paths, input).await,
        "fs.image-rotate" => rotate(inner, paths, input).await,
        "fs.image-thumbs" => thumbs(inner, paths, input).await,
        "fs.image-rename-tpl" => rename_tpl(inner, paths, input).await,
        "fs.image-batch" => recipe_line(inner, paths, input, false).await,
        "fs.image-batch-preview" => recipe_line(inner, paths, input, true).await,
        "fs.image-batch-save" => save_recipe(inner, paths, input).await,
        "fs.image-batch-cancel" => {
            request_cancel();
            Ok(ActionOutcome::NeedsReport {
                title: inner.deps.locale.tr("fm-image-batch-cancel-title"),
                body: inner.deps.locale.tr("fm-image-batch-cancel-body"),
            })
        }
        "fs.image-compare" => compare(inner, paths).await,
        "fs.image-pick" => pick(inner, paths, input).await,
        "fs.image-diff" => diff(inner, paths).await,
        "fs.image-composite" => composite(inner, paths, input).await,
        "fs.image-pano" => pano(inner, paths).await,
        "fs.image-hdr" => hdr(inner, paths).await,
        _ => Ok(ActionOutcome::Done),
    }
}

async fn convert(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.image-convert",
            &path_strs(&images),
            "jpg",
            locale.tr("fm-image-convert-title"),
            locale.tr("fm-image-convert-hint"),
        ));
    };
    let fmt = EncodeFormat::parse(raw).ok_or_else(|| {
        WidgetError::InvalidStateForOperation(locale.tr("fm-image-batch-bad-spec"))
    })?;
    Ok(report_count(
        locale,
        "fm-image-convert-title",
        apply_each(&images, |os| convert_file(os, fmt))?,
    ))
}

async fn rotate(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.image-rotate",
            &path_strs(&images),
            "cw",
            locale.tr("fm-image-rotate-title"),
            locale.tr("fm-image-rotate-hint"),
        ));
    };
    let op = LosslessOp::from_token(raw.trim()).ok_or_else(|| {
        WidgetError::InvalidStateForOperation(locale.tr("fm-image-batch-bad-spec"))
    })?;
    begin_batch();
    let mut n = 0u32;
    for (_, os) in &images {
        if cancelled() {
            break;
        }
        let bytes =
            std::fs::read(os).map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        let fmt = format_from_extension(os.extension().and_then(|s| s.to_str()));
        let out = apply_lossless(&bytes, fmt, op)
            .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
        std::fs::write(os, out)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        n += 1;
    }
    Ok(report_count(locale, "fm-image-rotate-title", n))
}

async fn thumbs(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.image-thumbs",
            &path_strs(&images),
            "256 jpg",
            locale.tr("fm-image-thumbs-title"),
            locale.tr("fm-image-thumbs-hint"),
        ));
    };
    let (edge, fmt) = parse_thumb(raw).ok_or_else(|| {
        WidgetError::InvalidStateForOperation(locale.tr("fm-image-batch-bad-spec"))
    })?;
    Ok(report_count(
        locale,
        "fm-image-thumbs-title",
        apply_each(&images, |os| export_thumb_file(os, edge, fmt))?,
    ))
}

async fn rename_tpl(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.image-rename-tpl",
            &path_strs(&images),
            "{name}_{date}_{n}{ext}",
            locale.tr("fm-image-rename-title"),
            locale.tr("fm-image-rename-hint"),
        ));
    };
    begin_batch();
    let mut n = 0u32;
    for (i, (_, os)) in images.iter().enumerate() {
        if cancelled() {
            break;
        }
        let name = os.file_name().and_then(|s| s.to_str()).unwrap_or("image");
        let mut next = apply_rename_pattern(name, i, raw, "", "");
        if next.contains("{date}") {
            next = next.replace("{date}", &image_date_token(os));
        }
        if next.contains("{w}") || next.contains("{h}") {
            if let Ok(img) = load_image_file(os) {
                next = next
                    .replace("{w}", &img.width.to_string())
                    .replace("{h}", &img.height.to_string());
            }
        }
        if next == name {
            continue;
        }
        let dest = os.with_file_name(next);
        if dest.exists() {
            continue;
        }
        std::fs::rename(os, dest)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        n += 1;
    }
    Ok(report_count(locale, "fm-image-rename-title", n))
}

async fn recipe_line(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
    force_preview: bool,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let action = if force_preview {
        "fs.image-batch-preview"
    } else {
        "fs.image-batch"
    };
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            action,
            &path_strs(&images),
            "resize=50% | convert=jpg",
            locale.tr(if force_preview {
                "fm-image-batch-preview-title"
            } else {
                "fm-image-batch-title"
            }),
            locale.tr("fm-image-batch-hint"),
        ));
    };
    let (preview, ops) = split_preview(raw, force_preview);
    let ops = resolve_recipe(&images, &ops);
    if ops.trim().is_empty() {
        return Err(WidgetError::InvalidStateForOperation(
            locale.tr("fm-image-batch-bad-spec"),
        ));
    }
    if preview {
        let body = preview_ops(&images, &ops);
        return Ok(ActionOutcome::NeedsReport {
            title: locale.tr("fm-image-batch-preview-title"),
            body,
        });
    }
    let n = apply_recipe(&images, &ops)?;
    Ok(report_count(locale, "fm-image-batch-title", n))
}

async fn save_recipe(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.image-batch-save",
            &path_strs(&images),
            "name=web | resize=50% | convert=jpg",
            locale.tr("fm-image-batch-save-title"),
            locale.tr("fm-image-batch-save-hint"),
        ));
    };
    let (name, ops) = parse_save(raw).ok_or_else(|| {
        WidgetError::InvalidStateForOperation(locale.tr("fm-image-batch-bad-spec"))
    })?;
    let dir = images[0]
        .1
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    save_batch_recipe(&dir, BatchRecipe { name, ops })
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    Ok(report_count(locale, "fm-image-batch-save-title", 1))
}

async fn compare(inner: &Arc<FileManagerInner>, paths: &[String]) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    if images.len() < 2 || images.len() > 4 {
        return Err(WidgetError::InvalidStateForOperation(
            locale.tr("fm-image-compare-need"),
        ));
    }
    let refs: Vec<&std::path::Path> = images.iter().map(|(_, p)| p.as_path()).collect();
    let dest =
        compare_files(&refs).map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    let next = orchid_fs::FsPath::from_local(&dest).map_err(map_fs_error)?;
    Ok(ActionOutcome::OpenInViewer {
        path: next.as_str().to_string(),
    })
}

async fn pick(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.image-pick",
            &path_strs(&images),
            "keep=1",
            locale.tr("fm-image-pick-title"),
            locale.tr("fm-image-pick-hint"),
        ));
    };
    let idx = parse_keep(raw).ok_or_else(|| {
        WidgetError::InvalidStateForOperation(locale.tr("fm-image-batch-bad-spec"))
    })?;
    let refs: Vec<&std::path::Path> = images.iter().map(|(_, p)| p.as_path()).collect();
    pick_best_file(&refs, idx)
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    Ok(report_count(locale, "fm-image-pick-title", 1))
}

async fn diff(inner: &Arc<FileManagerInner>, paths: &[String]) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    if images.len() < 2 {
        return Err(WidgetError::InvalidStateForOperation(
            locale.tr("fm-image-compare-need"),
        ));
    }
    let (dest, stats) = diff_files(&images[0].1, &images[1].1)
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    Ok(ActionOutcome::NeedsReport {
        title: locale.tr("fm-image-diff-title"),
        body: locale.tr_args(
            "fm-image-diff-body",
            &orchid_i18n::FluentArgs::new()
                .with("changed", stats.changed.to_string())
                .with("total", stats.total.to_string())
                .with("mean", format!("{:.1}", stats.mean))
                .with("path", dest.display().to_string()),
        ),
    })
}

async fn composite(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.image-composite",
            &path_strs(&images),
            "avg",
            locale.tr("fm-image-composite-title"),
            locale.tr("fm-image-composite-hint"),
        ));
    };
    let mode = CompositeMode::parse(raw).ok_or_else(|| {
        WidgetError::InvalidStateForOperation(locale.tr("fm-image-batch-bad-spec"))
    })?;
    let refs: Vec<&std::path::Path> = images.iter().map(|(_, p)| p.as_path()).collect();
    let dest = composite_files(&refs, mode)
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    let next = orchid_fs::FsPath::from_local(&dest).map_err(map_fs_error)?;
    Ok(ActionOutcome::OpenInViewer {
        path: next.as_str().to_string(),
    })
}

async fn pano(inner: &Arc<FileManagerInner>, paths: &[String]) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    if images.len() < 2 {
        return Err(WidgetError::InvalidStateForOperation(
            locale.tr("fm-image-compare-need"),
        ));
    }
    let refs: Vec<&std::path::Path> = images.iter().map(|(_, p)| p.as_path()).collect();
    let dest = stitch_panorama_files(&refs)
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    let next = orchid_fs::FsPath::from_local(&dest).map_err(map_fs_error)?;
    Ok(ActionOutcome::OpenInViewer {
        path: next.as_str().to_string(),
    })
}

async fn hdr(inner: &Arc<FileManagerInner>, paths: &[String]) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    if images.len() < 2 {
        return Err(WidgetError::InvalidStateForOperation(
            locale.tr("fm-image-compare-need"),
        ));
    }
    let refs: Vec<&std::path::Path> = images.iter().map(|(_, p)| p.as_path()).collect();
    let dest = merge_hdr_files(&refs)
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    let next = orchid_fs::FsPath::from_local(&dest).map_err(map_fs_error)?;
    Ok(ActionOutcome::OpenInViewer {
        path: next.as_str().to_string(),
    })
}

fn apply_each(
    images: &[(orchid_fs::FsPath, PathBuf)],
    f: impl Fn(&PathBuf) -> orchid_viewers::Result<PathBuf>,
) -> WidgetResult<u32> {
    begin_batch();
    let mut n = 0u32;
    for (_, os) in images {
        if cancelled() {
            break;
        }
        f(os).map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
        n += 1;
    }
    Ok(n)
}

fn apply_recipe(images: &[(orchid_fs::FsPath, PathBuf)], raw: &str) -> WidgetResult<u32> {
    begin_batch();
    let mut n = 0u32;
    for (_, os) in images {
        if cancelled() {
            break;
        }
        apply_one(os, raw)?;
        n += 1;
    }
    Ok(n)
}

fn apply_one(os: &PathBuf, raw: &str) -> WidgetResult<()> {
    for part in raw.split(" | ") {
        let part = part.trim();
        if part.is_empty() || part.eq_ignore_ascii_case("preview") {
            continue;
        }
        if let Some(fmt) = part
            .strip_prefix("convert=")
            .or_else(|| part.strip_prefix("to="))
            .and_then(EncodeFormat::parse)
        {
            convert_file(os, fmt)
                .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
            continue;
        }
        if let Some(rest) = part.strip_prefix("thumb=") {
            let (edge, fmt) = parse_thumb(rest)
                .ok_or_else(|| WidgetError::InvalidStateForOperation("thumb=".into()))?;
            export_thumb_file(os, edge, fmt)
                .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
            continue;
        }
        if let Some(tok) = part.strip_prefix("rotate=") {
            let op = LosslessOp::from_token(tok)
                .ok_or_else(|| WidgetError::InvalidStateForOperation(tok.to_string()))?;
            let bytes = std::fs::read(os)
                .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
            let fmt = format_from_extension(os.extension().and_then(|s| s.to_str()));
            let out = apply_lossless(&bytes, fmt, op)
                .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
            std::fs::write(os, out)
                .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
            continue;
        }
        if part.starts_with("watermark=")
            || part.starts_with("wm-image=")
            || part == "stamp"
            || part.starts_with("stamp")
        {
            if let Some(op) = parse_annotate_line(part) {
                apply_annotate_file(os, &op)
                    .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
                continue;
            }
        }
        if let Some((spec, filter)) =
            parse_resize_line(part.strip_prefix("resize=").unwrap_or(part))
        {
            apply_edit_file(os, &orchid_viewers::EditOp::Resize { spec, filter })
                .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
            continue;
        }
        if let Some(op) = parse_adjust_line(part) {
            apply_adjust_file(os, &op)
                .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
            continue;
        }
    }
    Ok(())
}

fn preview_ops(images: &[(orchid_fs::FsPath, PathBuf)], raw: &str) -> String {
    let mut lines = Vec::new();
    for (_, os) in images {
        let name = os.file_name().and_then(|s| s.to_str()).unwrap_or("image");
        let mut planned = Vec::new();
        for part in raw.split(" | ") {
            let part = part.trim();
            if let Some(fmt) = part
                .strip_prefix("convert=")
                .or_else(|| part.strip_prefix("to="))
                .and_then(EncodeFormat::parse)
            {
                planned.push(
                    planned_sibling(os, "convert", fmt.ext())
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("convert")
                        .to_string(),
                );
            } else if part.starts_with("thumb=") {
                planned.push(
                    planned_sibling(os, "thumb", "jpg")
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("thumb")
                        .to_string(),
                );
            } else if part.starts_with("resize=") || parse_resize_line(part).is_some() {
                planned.push(
                    planned_sibling(os, "resize", "png")
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("resize")
                        .to_string(),
                );
            } else if part.starts_with("rotate=") {
                planned.push(format!("{name} (lossless overwrite)"));
            }
        }
        if planned.is_empty() {
            planned.push(name.to_string());
        }
        lines.push(format!("{name} → {}", planned.join(", ")));
    }
    lines.join("\n")
}

fn resolve_recipe(images: &[(orchid_fs::FsPath, PathBuf)], raw: &str) -> String {
    let mut out = Vec::new();
    for part in raw.split(" | ") {
        let part = part.trim();
        if let Some(name) = part
            .strip_prefix("recipe=")
            .or_else(|| part.strip_prefix("look="))
        {
            if let Some(dir) = images.first().and_then(|(_, p)| p.parent()) {
                if let Some(found) = load_batch_recipes(dir)
                    .into_iter()
                    .find(|r| r.name.eq_ignore_ascii_case(name))
                {
                    out.push(found.ops);
                    continue;
                }
            }
        }
        if !part.is_empty() {
            out.push(part.to_string());
        }
    }
    out.join(" | ")
}

fn split_preview(raw: &str, force: bool) -> (bool, String) {
    let t = raw.trim();
    if let Some(rest) = t.strip_prefix("preview | ") {
        return (true, rest.to_string());
    }
    if t.eq_ignore_ascii_case("preview") {
        return (true, String::new());
    }
    (force, t.to_string())
}

fn parse_save(raw: &str) -> Option<(String, String)> {
    let mut name = None;
    let mut ops = Vec::new();
    for part in raw.split(" | ") {
        let part = part.trim();
        if let Some(v) = part.strip_prefix("name=") {
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

fn parse_thumb(raw: &str) -> Option<(u32, EncodeFormat)> {
    let mut edge = 256u32;
    let mut fmt = EncodeFormat::Jpeg;
    for part in raw.split([',', ' ', '|']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Ok(n) = part.parse::<u32>() {
            edge = n;
        } else if let Some(f) = EncodeFormat::parse(part) {
            fmt = f;
        } else {
            return None;
        }
    }
    Some((edge, fmt))
}

fn parse_keep(raw: &str) -> Option<usize> {
    let v = raw
        .strip_prefix("keep=")
        .or_else(|| raw.strip_prefix("best="))
        .unwrap_or(raw)
        .trim();
    let n: usize = v.parse().ok()?;
    n.checked_sub(1)
}
