//! Image IPTC / XMP / GPS / date edit, strip, copy, CSV, and templates.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use orchid_viewers::{
    apply_editable_meta, copy_image_metadata, export_metadata_csv, export_metadata_xml,
    import_metadata_csv, inspect_image_file, inspect_to_edit, is_image_file_extension,
    load_templates, pack_editable_meta, save_template, unpack_editable_meta, EditableMeta,
    MetaTemplate,
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
        "fs.meta-edit" => edit(inner, paths, input, EditKind::All).await,
        "fs.meta-gps" => edit(inner, paths, input, EditKind::Gps).await,
        "fs.meta-date" => edit(inner, paths, input, EditKind::Date).await,
        "fs.meta-date-shift" => edit(inner, paths, input, EditKind::Shift).await,
        "fs.meta-strip" => apply_flag(inner, paths, true, false).await,
        "fs.meta-strip-gps" => apply_flag(inner, paths, false, true).await,
        "fs.meta-copy" => copy_meta(inner, paths).await,
        "fs.meta-export-csv" => export(inner, paths, false).await,
        "fs.meta-export-xml" => export(inner, paths, true).await,
        "fs.meta-import-csv" => import_csv(inner, paths, input).await,
        "fs.meta-template-save" => template_save(inner, paths, input).await,
        "fs.meta-template-apply" => template_apply(inner, paths, input).await,
        _ => Ok(ActionOutcome::Done),
    }
}

#[derive(Clone, Copy)]
enum EditKind {
    All,
    Gps,
    Date,
    Shift,
}

async fn edit(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
    kind: EditKind,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let first = &images[0].1;
    let current = inspect_image_file(first).unwrap_or_default();
    let packed = match kind {
        EditKind::All => pack_editable_meta(&inspect_to_edit(&current)),
        EditKind::Gps => current
            .gps
            .map(|g| format!("gps={},{}", g.lat, g.lon))
            .unwrap_or_else(|| "gps=".into()),
        EditKind::Date => inspect_to_edit(&current)
            .date
            .flatten()
            .map(|d| format!("date={d}"))
            .unwrap_or_else(|| "date=".into()),
        EditKind::Shift => "shift=+1h".into(),
    };
    let (action, title, hint) = match kind {
        EditKind::All => (
            "fs.meta-edit",
            locale.tr("fm-meta-edit-title"),
            locale.tr("fm-meta-edit-hint"),
        ),
        EditKind::Gps => (
            "fs.meta-gps",
            locale.tr("fm-meta-gps-title"),
            locale.tr("fm-meta-gps-hint"),
        ),
        EditKind::Date => (
            "fs.meta-date",
            locale.tr("fm-meta-date-title"),
            locale.tr("fm-meta-date-hint"),
        ),
        EditKind::Shift => (
            "fs.meta-date-shift",
            locale.tr("fm-meta-shift-title"),
            locale.tr("fm-meta-shift-hint"),
        ),
    };
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            action,
            &images
                .iter()
                .map(|(fp, _)| fp.as_str().to_string())
                .collect::<Vec<_>>(),
            &packed,
            title,
            hint,
        ));
    };
    let edit = unpack_editable_meta(raw);
    let n = apply_all(&images, &edit)?;
    Ok(report(
        title,
        locale.tr_args(
            "fm-meta-applied",
            &orchid_i18n::FluentArgs::new().with("count", n.to_string()),
        ),
    ))
}

async fn apply_flag(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    strip_all: bool,
    strip_gps: bool,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let n = apply_all(
        &images,
        &EditableMeta {
            strip_all,
            strip_gps,
            ..EditableMeta::default()
        },
    )?;
    Ok(report(
        locale.tr(if strip_all {
            "fm-action-meta-strip"
        } else {
            "fm-action-meta-strip-gps"
        }),
        locale.tr_args(
            "fm-meta-applied",
            &orchid_i18n::FluentArgs::new().with("count", n.to_string()),
        ),
    ))
}

async fn copy_meta(inner: &Arc<FileManagerInner>, paths: &[String]) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    if paths.len() < 2 {
        return Err(WidgetError::InvalidStateForOperation(
            "fm-tools-need-two-files".into(),
        ));
    }
    let from = orchid_fs::FsPath::new(&paths[0]).map_err(map_fs_error)?;
    let to = orchid_fs::FsPath::new(&paths[1]).map_err(map_fs_error)?;
    let src = from.to_local().map_err(map_fs_error)?;
    let dest = to.to_local().map_err(map_fs_error)?;
    copy_image_metadata(&src, &dest)
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    Ok(report(
        locale.tr("fm-action-meta-copy"),
        locale.tr("fm-meta-copied"),
    ))
}

async fn export(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    xml: bool,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let os_paths: Vec<PathBuf> = images.iter().map(|(_, p)| p.clone()).collect();
    let body = if xml {
        export_metadata_xml(&os_paths)
    } else {
        export_metadata_csv(&os_paths)
    }
    .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    let name = if xml {
        "orchid-metadata.xml"
    } else {
        "orchid-metadata.csv"
    };
    let dest = images[0]
        .1
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(name);
    std::fs::write(&dest, body)
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    let _ = opener::open(&dest);
    Ok(report(
        locale.tr(if xml {
            "fm-action-meta-export-xml"
        } else {
            "fm-action-meta-export-csv"
        }),
        locale.tr_args(
            "fm-meta-exported",
            &orchid_i18n::FluentArgs::new().with("path", dest.display().to_string()),
        ),
    ))
}

async fn import_csv(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.meta-import-csv",
            &images
                .iter()
                .map(|(fp, _)| fp.as_str().to_string())
                .collect::<Vec<_>>(),
            "",
            locale.tr("fm-meta-import-title"),
            locale.tr("fm-meta-import-hint"),
        ));
    };
    let csv = if Path::new(raw).is_file() {
        std::fs::read_to_string(raw)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
    } else {
        raw.to_string()
    };
    let rows = import_metadata_csv(&csv)
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    let folder = images[0].1.parent().map(Path::to_path_buf);
    let mut n = 0u32;
    for (path_str, edit) in rows {
        let p = PathBuf::from(path_str.trim());
        let target = if p.is_file() {
            p
        } else if let (Some(folder), Some(name)) = (folder.as_ref(), p.file_name()) {
            folder.join(name)
        } else {
            continue;
        };
        if target.is_file() {
            apply_editable_meta(&target, &edit)
                .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
            n += 1;
        }
    }
    Ok(report(
        locale.tr("fm-meta-import-title"),
        locale.tr_args(
            "fm-meta-applied",
            &orchid_i18n::FluentArgs::new().with("count", n.to_string()),
        ),
    ))
}

async fn template_save(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let dir = images[0].1.parent().unwrap_or_else(|| Path::new("."));
    let current = inspect_to_edit(&inspect_image_file(&images[0].1).unwrap_or_default());
    let proposed = format!("name=\n{}", pack_editable_meta(&current));
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.meta-template-save",
            &images
                .iter()
                .map(|(fp, _)| fp.as_str().to_string())
                .collect::<Vec<_>>(),
            &proposed,
            locale.tr("fm-meta-template-save-title"),
            locale.tr("fm-meta-template-save-hint"),
        ));
    };
    let (name, rest) = split_name(raw);
    let edit = unpack_editable_meta(&rest);
    save_template(
        dir,
        MetaTemplate {
            name: if name.is_empty() {
                "default".into()
            } else {
                name
            },
            title: edit.title.unwrap_or_default(),
            headline: edit.headline.unwrap_or_default(),
            description: edit.description.unwrap_or_default(),
            creator: edit.creator.unwrap_or_default(),
            copyright: edit.copyright.unwrap_or_default(),
            keywords: edit.keywords.unwrap_or_default(),
            credit: edit.credit.unwrap_or_default(),
        },
    )
    .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    Ok(report(
        locale.tr("fm-meta-template-save-title"),
        locale.tr_args(
            "fm-meta-applied",
            &orchid_i18n::FluentArgs::new().with("count", "1"),
        ),
    ))
}

async fn template_apply(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let images = selected_images(inner, paths)?;
    let dir = images[0].1.parent().unwrap_or_else(|| Path::new("."));
    let tmpls = load_templates(dir);
    let names = tmpls
        .iter()
        .map(|t| t.name.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.meta-template-apply",
            &images
                .iter()
                .map(|(fp, _)| fp.as_str().to_string())
                .collect::<Vec<_>>(),
            &names,
            locale.tr("fm-meta-template-apply-title"),
            locale.tr("fm-meta-template-apply-hint"),
        ));
    };
    let name = raw.lines().next().unwrap_or(raw).trim();
    let Some(tmpl) = tmpls.iter().find(|t| t.name.eq_ignore_ascii_case(name)) else {
        return Err(WidgetError::InvalidStateForOperation(
            locale.tr("fm-meta-template-missing"),
        ));
    };
    let n = apply_all(&images, &tmpl.to_edit())?;
    Ok(report(
        locale.tr("fm-meta-template-apply-title"),
        locale.tr_args(
            "fm-meta-applied",
            &orchid_i18n::FluentArgs::new().with("count", n.to_string()),
        ),
    ))
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

fn apply_all(images: &[(orchid_fs::FsPath, PathBuf)], edit: &EditableMeta) -> WidgetResult<u32> {
    let mut n = 0u32;
    for (_, os) in images {
        apply_editable_meta(os, edit)
            .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
        n += 1;
    }
    Ok(n)
}

fn split_name(raw: &str) -> (String, String) {
    let mut name = String::new();
    let mut rest = String::new();
    for line in raw.lines() {
        if name.is_empty() {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim().eq_ignore_ascii_case("name") {
                    name = v.trim().to_string();
                    continue;
                }
            }
        }
        rest.push_str(line);
        rest.push('\n');
    }
    (name, rest)
}

fn report(title: String, body: String) -> ActionOutcome {
    ActionOutcome::NeedsReport { title, body }
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
