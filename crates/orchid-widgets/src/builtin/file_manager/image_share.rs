//! Share / export: save-as quality, ICO, clipboard, wallpaper, email, screenshot.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use orchid_viewers::{
    encode_png, export_file, load_image_file, loaded_from_rgba, parse_export_line,
    parse_screenshot_line, prepare_mail_attachment, set_wallpaper, share_intent_url,
    unique_export_dest, write_mail_eml, write_screenshot, ExportFormat, ExportSpec, LoadedImage,
};

use super::image_edit::{path_strs, prompt, report_count, selected_images};
use super::{ActionOutcome, FileManagerInner};
use crate::error::Result as WidgetResult;
use crate::error::WidgetError;

const EXPORT_LINE: &str = "jpg | q=85 | max=1920";
const MAIL_LINE: &str = "max=1920";
const SHARE_LINE: &str = "twitter";
const SHOT_LINE: &str = "screen | delay=0";

pub(super) async fn run(
    inner: &Arc<FileManagerInner>,
    action_id: &str,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    match action_id {
        "fs.image-export" | "fs.image-save-as" => {
            export_line(inner, paths, input, EXPORT_LINE, None).await
        }
        "fs.image-ico" => {
            export_line(
                inner,
                paths,
                Some("ico"),
                "ico",
                Some(ExportSpec {
                    format: ExportFormat::Ico,
                    ..ExportSpec::default()
                }),
            )
            .await
        }
        "fs.image-favicon" => {
            export_line(
                inner,
                paths,
                Some("favicon"),
                "favicon",
                Some(ExportSpec {
                    format: ExportFormat::Favicon,
                    ..ExportSpec::default()
                }),
            )
            .await
        }
        "fs.image-email" => email_line(inner, paths, input).await,
        "fs.image-share" => share_line(inner, paths, input).await,
        "fs.image-copy" => copy_images(inner, paths).await,
        "fs.image-paste" => paste_image(inner, paths).await,
        "fs.image-wallpaper" => wallpaper(inner, paths).await,
        "fs.image-screenshot" => screenshot_line(inner, paths, input).await,
        _ => Ok(ActionOutcome::Done),
    }
}

async fn export_line(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
    proposed: &str,
    forced: Option<ExportSpec>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let spec = if let Some(spec) = forced {
        spec
    } else {
        let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
            return Ok(prompt(
                "fs.image-export",
                &path_strs(&images),
                proposed,
                locale.tr("fm-image-export-title"),
                locale.tr("fm-image-export-hint"),
            ));
        };
        parse_export_line(raw).ok_or_else(|| {
            WidgetError::InvalidStateForOperation(locale.tr("fm-image-export-bad-spec"))
        })?
    };
    Ok(report_count(
        locale,
        "fm-image-export-title",
        apply_each(&images, |os| export_file(os, &spec))?,
    ))
}

async fn email_line(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.image-email",
            &path_strs(&images),
            MAIL_LINE,
            locale.tr("fm-image-email-title"),
            locale.tr("fm-image-email-hint"),
        ));
    };
    let max = parse_max_edge(raw).ok_or_else(|| {
        WidgetError::InvalidStateForOperation(locale.tr("fm-image-export-bad-spec"))
    })?;
    super::image_batch::begin_batch();
    let mut n = 0u32;
    let mut last_eml = None;
    for (_, os) in &images {
        if super::image_batch::cancelled() {
            break;
        }
        let jpeg = prepare_mail_attachment(os, max)
            .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
        let eml = write_mail_eml(&jpeg)
            .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
        last_eml = Some(eml);
        n += 1;
    }
    if let Some(eml) = last_eml {
        let _ = opener::open(&eml);
    }
    Ok(report_count(locale, "fm-image-email-title", n))
}

async fn share_line(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.image-share",
            &path_strs(&images),
            SHARE_LINE,
            locale.tr("fm-image-share-title"),
            locale.tr("fm-image-share-hint"),
        ));
    };
    let network = raw
        .split('|')
        .next()
        .unwrap_or(raw)
        .trim()
        .to_ascii_lowercase();
    let (_, first) = &images[0];
    let img = load_image_file(first)
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    copy_loaded(&img)?;
    let label = first
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("image");
    if let Some(url) = share_intent_url(&network, label) {
        let _ = opener::open(url);
    } else {
        let _ = opener::open(first);
    }
    Ok(report_count(locale, "fm-image-share-title", 1))
}

async fn copy_images(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let img = load_image_file(&images[0].1)
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    copy_loaded(&img)?;
    Ok(report_count(locale, "fm-action-image-copy", 1))
}

async fn paste_image(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let dir = dest_folder(inner, paths);
    let img = paste_loaded()?;
    let hint = dir.join("clipboard.png");
    let dest = unique_export_dest(&hint, "paste", "png");
    let bytes =
        encode_png(&img).map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    std::fs::write(&dest, bytes)
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    Ok(report_count(locale, "fm-action-image-paste", 1))
}

async fn wallpaper(inner: &Arc<FileManagerInner>, paths: &[String]) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let wall = wallpaper_source(&images[0].1)?;
    set_wallpaper(&wall).map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    Ok(report_count(locale, "fm-action-image-wallpaper", 1))
}

async fn screenshot_line(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.image-screenshot",
            paths,
            SHOT_LINE,
            locale.tr("fm-image-screenshot-title"),
            locale.tr("fm-image-screenshot-hint"),
        ));
    };
    let spec = parse_screenshot_line(raw).ok_or_else(|| {
        WidgetError::InvalidStateForOperation(locale.tr("fm-image-export-bad-spec"))
    })?;
    let dir = dest_folder(inner, paths);
    let dest = tokio::task::spawn_blocking(move || write_screenshot(&dir, &spec))
        .await
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    let next = orchid_fs::FsPath::from_local(&dest)
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    Ok(ActionOutcome::OpenInViewer {
        path: next.as_str().to_string(),
    })
}

fn apply_each(
    images: &[(orchid_fs::FsPath, PathBuf)],
    f: impl Fn(&Path) -> orchid_viewers::Result<PathBuf>,
) -> WidgetResult<u32> {
    super::image_batch::begin_batch();
    let mut n = 0u32;
    for (_, os) in images {
        if super::image_batch::cancelled() {
            break;
        }
        f(os).map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
        n += 1;
    }
    Ok(n)
}

fn parse_max_edge(raw: &str) -> Option<u32> {
    for part in raw.split('|') {
        let t = part.trim();
        if let Some(v) = t.strip_prefix("max=") {
            return v.trim().parse().ok();
        }
        if let Ok(n) = t.parse::<u32>() {
            return Some(n);
        }
    }
    None
}

fn dest_folder(inner: &Arc<FileManagerInner>, paths: &[String]) -> PathBuf {
    for p in paths {
        if let Ok(fp) = orchid_fs::FsPath::new(p) {
            if let Ok(os) = fp.to_local() {
                if os.is_dir() {
                    return os;
                }
                if let Some(parent) = os.parent() {
                    return parent.to_path_buf();
                }
            }
        }
    }
    if let Ok(images) = selected_images(inner, paths) {
        if let Some(parent) = images[0].1.parent() {
            return parent.to_path_buf();
        }
    }
    pictures_dir()
}

fn pictures_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(|h| PathBuf::from(h).join("Pictures"))
        .filter(|p| p.is_dir())
        .unwrap_or_else(std::env::temp_dir)
}

fn wallpaper_source(path: &Path) -> WidgetResult<PathBuf> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(ext.as_str(), "jpg" | "jpeg" | "bmp") {
        return Ok(path.to_path_buf());
    }
    export_file(
        path,
        &ExportSpec {
            format: ExportFormat::Jpeg,
            quality: 92,
            png_level: 6,
            max_edge: None,
        },
    )
    .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))
}

pub(crate) fn copy_loaded(img: &LoadedImage) -> WidgetResult<()> {
    let mut cb = arboard::Clipboard::new()
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    cb.set_image(arboard::ImageData {
        width: img.width as usize,
        height: img.height as usize,
        bytes: Cow::Borrowed(img.rgba.as_slice()),
    })
    .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))
}

pub(crate) fn paste_loaded() -> WidgetResult<LoadedImage> {
    let mut cb = arboard::Clipboard::new()
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    let data = cb
        .get_image()
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    loaded_from_rgba(
        data.bytes.into_owned(),
        data.width as u32,
        data.height as u32,
    )
    .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))
}
