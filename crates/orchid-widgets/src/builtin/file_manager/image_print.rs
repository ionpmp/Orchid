//! Print selected images: preview, n-up, contact sheet, batch.

use std::sync::Arc;

use orchid_viewers::{parse_print_line, send_to_printer, write_print_preview, write_print_temps};

use super::image_edit::{path_strs, prompt, report_count, selected_images};
use super::map_fs_error;
use super::{ActionOutcome, FileManagerInner};
use crate::error::Result as WidgetResult;
use crate::error::WidgetError;

const DEFAULT: &str = "paper=a4 | margin=12 | nup=1 | footer={name} {date} | icc=srgb";
const SHEET: &str = "sheet | paper=a4 | cols=4 | margin=10 | footer={date} | icc=srgb";
const NUP: &str = "paper=a4 | nup=4 | margin=10 | footer={name} {date} | icc=srgb";

pub(super) async fn run(
    inner: &Arc<FileManagerInner>,
    action_id: &str,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    match action_id {
        "fs.image-print" => {
            print_line(
                inner,
                paths,
                input,
                DEFAULT,
                "fs.image-print",
                "fm-image-print-title",
                "fm-image-print-hint",
                false,
            )
            .await
        }
        "fs.image-print-preview" => {
            print_line(
                inner,
                paths,
                input,
                DEFAULT,
                "fs.image-print-preview",
                "fm-image-print-preview-title",
                "fm-image-print-hint",
                true,
            )
            .await
        }
        "fs.image-print-sheet" => {
            print_line(
                inner,
                paths,
                input,
                SHEET,
                "fs.image-print-sheet",
                "fm-image-print-sheet-title",
                "fm-image-print-hint",
                false,
            )
            .await
        }
        "fs.image-print-nup" => {
            print_line(
                inner,
                paths,
                input,
                NUP,
                "fs.image-print-nup",
                "fm-image-print-nup-title",
                "fm-image-print-hint",
                false,
            )
            .await
        }
        "fs.image-print-batch" => {
            print_line(
                inner,
                paths,
                input,
                DEFAULT,
                "fs.image-print-batch",
                "fm-image-print-batch-title",
                "fm-image-print-hint",
                false,
            )
            .await
        }
        _ => Ok(ActionOutcome::Done),
    }
}

#[allow(clippy::too_many_arguments)]
async fn print_line(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
    proposed: &str,
    action_id: &str,
    title_key: &str,
    hint_key: &str,
    preview: bool,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            action_id,
            &path_strs(&images),
            proposed,
            locale.tr(title_key),
            locale.tr(hint_key),
        ));
    };
    let raw = if action_id.ends_with("sheet") && !raw.contains("sheet") {
        format!("sheet | {raw}")
    } else {
        raw.to_string()
    };
    let spec = parse_print_line(&raw).ok_or_else(|| {
        WidgetError::InvalidStateForOperation(locale.tr("fm-image-print-bad-spec"))
    })?;
    let os: Vec<std::path::PathBuf> = images.iter().map(|(_, p)| p.clone()).collect();
    let hint = os[0].clone();
    if preview {
        let dest = tokio::task::spawn_blocking(move || {
            let refs: Vec<&std::path::Path> = os.iter().map(std::path::PathBuf::as_path).collect();
            write_print_preview(&refs, &spec, &hint)
        })
        .await
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
        let next = orchid_fs::FsPath::from_local(&dest).map_err(map_fs_error)?;
        return Ok(ActionOutcome::OpenInViewer {
            path: next.as_str().to_string(),
        });
    }
    let dests = tokio::task::spawn_blocking(move || {
        let refs: Vec<&std::path::Path> = os.iter().map(std::path::PathBuf::as_path).collect();
        write_print_temps(&refs, &spec)
    })
    .await
    .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
    .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    super::image_batch::begin_batch();
    let mut n = 0u32;
    for dest in dests {
        if super::image_batch::cancelled() {
            break;
        }
        send_to_printer(&dest)
            .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
        n += 1;
    }
    Ok(report_count(locale, title_key, n))
}
