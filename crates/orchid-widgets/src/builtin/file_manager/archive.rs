//! Archive browse / extract / create / mutate actions.

use std::sync::Arc;

use super::map_fs_error;
use super::selection::parse_byte_size;
use super::ActionOutcome;
use super::FileManagerInner;
use super::PassphrasePurpose;
use crate::error::Result as WidgetResult;
use crate::error::WidgetError;

use orchid_fs::{
    add_to_archive, create_archive, default_extract_dir, delete_from_archive, extract_archive,
    is_archive_file, looks_like_archive_name, test_archive, CreateArchiveOptions, FsPath,
};

const VOL_PREFIX: &str = "orchid:vol:";
const OPEN_VIEW: &str = "orchid:view";
const OPEN_BROWSE: &str = "orchid:browse";
const OPEN_TEST: &str = "orchid:test";

/// Dispatch an archive action.
pub(super) async fn run(
    inner: &Arc<FileManagerInner>,
    action_id: &str,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    match action_id {
        "fs.archive-open" => open_as_folder(inner, paths).await,
        "fs.archive-extract" => extract(inner, paths, false, None).await,
        "fs.archive-extract-selected" => extract(inner, paths, true, None).await,
        "fs.archive-create" => create(inner, paths, input, CreateKind::Plain).await,
        "fs.archive-create-password" => create(inner, paths, input, CreateKind::Password).await,
        "fs.archive-create-sfx" => create(inner, paths, input, CreateKind::Sfx).await,
        "fs.archive-create-volume" => create_volume(inner, paths, input).await,
        "fs.archive-add" => add(inner, paths, input).await,
        "fs.archive-delete" => delete_inner(inner, paths).await,
        "fs.archive-test" => test(inner, paths, None).await,
        _ => Ok(ActionOutcome::Done),
    }
}

/// True for archive context-menu / tool-prompt action ids.
#[must_use]
pub(super) fn is_archive_action(id: &str) -> bool {
    matches!(
        id,
        "fs.archive-open"
            | "fs.archive-extract"
            | "fs.archive-extract-selected"
            | "fs.archive-create"
            | "fs.archive-create-password"
            | "fs.archive-create-sfx"
            | "fs.archive-create-volume"
            | "fs.archive-add"
            | "fs.archive-delete"
            | "fs.archive-test"
    )
}

/// Finish encrypt / open after the passphrase dialog.
pub(super) async fn apply_passphrase(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    passphrase: &str,
    purpose: PassphrasePurpose,
) -> WidgetResult<ActionOutcome> {
    match purpose {
        PassphrasePurpose::ArchiveCreate => {
            let Some(dest) = paths.first().and_then(|p| FsPath::new(p).ok()) else {
                return Ok(ActionOutcome::Done);
            };
            let sources: Vec<FsPath> = paths
                .iter()
                .skip(1)
                .filter_map(|p| FsPath::new(p).ok())
                .collect();
            create_archive(
                &dest,
                &sources,
                CreateArchiveOptions {
                    password: Some(passphrase.to_string()),
                    volume_bytes: None,
                    sfx: false,
                },
            )
            .await
            .map_err(map_fs_error)?;
            inner.refresh_all_tabs().await;
            Ok(report(
                inner.deps.locale.tr("fm-archive-create-title"),
                inner.deps.locale.tr_args(
                    "fm-archive-created",
                    &orchid_i18n::FluentArgs::new()
                        .with("name", dest.file_name().unwrap_or("archive").to_string()),
                ),
            ))
        }
        PassphrasePurpose::ArchiveOpen => finish_open_with_password(inner, paths, passphrase).await,
        _ => Ok(ActionOutcome::Done),
    }
}

/// Double-click: browse a local archive, or open/extract an inner entry.
pub(super) async fn open_if_archive(
    inner: &Arc<FileManagerInner>,
    instance_id: uuid::Uuid,
    pane: u8,
    fp: &FsPath,
) -> Option<WidgetResult<ActionOutcome>> {
    if fp.is_archive() {
        return Some(open_inner(instance_id, pane, fp).await);
    }
    if is_archive_file(fp) {
        return Some(browse_local(inner, instance_id, pane, fp).await);
    }
    None
}

async fn browse_local(
    inner: &Arc<FileManagerInner>,
    instance_id: uuid::Uuid,
    pane: u8,
    fp: &FsPath,
) -> WidgetResult<ActionOutcome> {
    let dest = FsPath::archive_from_file(fp, "").map_err(map_fs_error)?;
    inner.record_recent(fp);
    super::navigate(instance_id, pane, dest)
        .await
        .map(|()| ActionOutcome::Done)
}

async fn open_inner(instance_id: uuid::Uuid, pane: u8, fp: &FsPath) -> WidgetResult<ActionOutcome> {
    let name = fp.file_name().unwrap_or("file");
    match materialize_inner(fp, None).await {
        Ok(local) => {
            if looks_like_archive_name(name) {
                let dest = FsPath::archive_from_file(&local, "").map_err(map_fs_error)?;
                super::navigate(instance_id, pane, dest)
                    .await
                    .map(|()| ActionOutcome::Done)
            } else {
                Ok(ActionOutcome::OpenInViewer {
                    path: local.as_str().to_string(),
                })
            }
        }
        Err(e) if needs_password(&e) => {
            let container = fp
                .archive_container()
                .ok_or_else(|| WidgetError::InvalidStateForOperation(e.clone()))?;
            let inner_path = fp.archive_inner().unwrap_or("").to_string();
            let dest = if looks_like_archive_name(name) {
                OPEN_BROWSE
            } else {
                OPEN_VIEW
            };
            Ok(ActionOutcome::NeedsPassphrase {
                paths: vec![container.as_str().to_string(), dest.into(), inner_path],
                purpose: PassphrasePurpose::ArchiveOpen,
            })
        }
        Err(e) => Err(WidgetError::InvalidStateForOperation(e)),
    }
}

async fn open_as_folder(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> WidgetResult<ActionOutcome> {
    let Some(p) = paths.first() else {
        return Ok(ActionOutcome::Done);
    };
    let fp = FsPath::new(p).map_err(map_fs_error)?;
    let file = if fp.is_archive() {
        match materialize_inner(&fp, None).await {
            Ok(local) => local,
            Err(e) if needs_password(&e) => {
                return Ok(ActionOutcome::NeedsPassphrase {
                    paths: vec![
                        fp.archive_container()
                            .map(|c| c.as_str().to_string())
                            .unwrap_or_else(|| p.clone()),
                        OPEN_BROWSE.into(),
                        fp.archive_inner().unwrap_or("").to_string(),
                    ],
                    purpose: PassphrasePurpose::ArchiveOpen,
                });
            }
            Err(e) => return Err(WidgetError::InvalidStateForOperation(e)),
        }
    } else {
        fp
    };
    let dest = FsPath::archive_from_file(&file, "").map_err(map_fs_error)?;
    navigate_active(inner, dest).await?;
    Ok(ActionOutcome::Done)
}

async fn extract(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    selected_only: bool,
    password: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let (archive, inners) = resolve_archive_targets(inner, paths, selected_only)?;
    let dest = default_extract_dir(&archive).map_err(map_fs_error)?;
    match extract_archive(&archive, &dest, &inners, password).await {
        Ok(n) => {
            inner.refresh_all_tabs().await;
            Ok(report(
                inner.deps.locale.tr("fm-archive-extract-title"),
                inner.deps.locale.tr_args(
                    "fm-archive-done",
                    &orchid_i18n::FluentArgs::new().with("n", n.to_string()),
                ),
            ))
        }
        Err(e) if password.is_none() && needs_password(&e.to_string()) => {
            let mut p = vec![archive.as_str().to_string(), dest.as_str().to_string()];
            p.extend(inners);
            Ok(ActionOutcome::NeedsPassphrase {
                paths: p,
                purpose: PassphrasePurpose::ArchiveOpen,
            })
        }
        Err(e) => Err(map_archive_error(inner, e)),
    }
}

#[derive(Clone, Copy)]
enum CreateKind {
    Plain,
    Password,
    Sfx,
}

async fn create(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
    kind: CreateKind,
) -> WidgetResult<ActionOutcome> {
    let sources: Vec<FsPath> = paths
        .iter()
        .filter(|p| !p.starts_with(VOL_PREFIX))
        .filter_map(|p| FsPath::new(p).ok())
        .filter(|p| p.is_local())
        .collect();
    if sources.is_empty() {
        return Ok(ActionOutcome::Done);
    }
    let default_name = match kind {
        CreateKind::Sfx => "Archive.exe".to_string(),
        _ => default_archive_name(&sources),
    };
    let Some(name) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        let (title, hint) = (
            inner.deps.locale.tr("fm-archive-create-title"),
            inner.deps.locale.tr("fm-archive-create-hint"),
        );
        let action = match kind {
            CreateKind::Plain => "fs.archive-create",
            CreateKind::Password => "fs.archive-create-password",
            CreateKind::Sfx => "fs.archive-create-sfx",
        };
        return Ok(prompt(action, paths, &default_name, title, hint));
    };
    let folder = sources[0]
        .parent()
        .ok_or_else(|| WidgetError::InvalidStateForOperation("fm-archive-create-title".into()))?;
    let dest = folder.join(name);
    if matches!(kind, CreateKind::Password) {
        let mut p = vec![dest.as_str().to_string()];
        p.extend(sources.iter().map(|s| s.as_str().to_string()));
        return Ok(ActionOutcome::NeedsPassphrase {
            paths: p,
            purpose: PassphrasePurpose::ArchiveCreate,
        });
    }
    create_archive(
        &dest,
        &sources,
        CreateArchiveOptions {
            password: None,
            volume_bytes: None,
            sfx: matches!(kind, CreateKind::Sfx),
        },
    )
    .await
    .map_err(|e| map_archive_error(inner, e))?;
    inner.refresh_all_tabs().await;
    Ok(report(
        inner.deps.locale.tr("fm-archive-create-title"),
        inner.deps.locale.tr_args(
            "fm-archive-created",
            &orchid_i18n::FluentArgs::new().with("name", name.to_string()),
        ),
    ))
}

async fn create_volume(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let vol = paths.iter().find_map(|p| p.strip_prefix(VOL_PREFIX));
    match (vol, input.map(str::trim).filter(|s| !s.is_empty())) {
        (None, None) => Ok(prompt(
            "fs.archive-create-volume",
            paths,
            "100MB",
            inner.deps.locale.tr("fm-archive-volume-title"),
            inner.deps.locale.tr("fm-archive-volume-hint"),
        )),
        (None, Some(size)) => {
            let n = parse_byte_size(size)
                .ok_or_else(|| WidgetError::InvalidStateForOperation("fm-split-bad-size".into()))?;
            let mut next = vec![format!("{VOL_PREFIX}{n}")];
            next.extend(paths.iter().cloned());
            Ok(prompt(
                "fs.archive-create-volume",
                &next,
                "Archive.7z",
                inner.deps.locale.tr("fm-archive-create-title"),
                inner.deps.locale.tr("fm-archive-create-hint"),
            ))
        }
        (Some(n), Some(name)) => {
            let volume_bytes = n.parse::<u64>().ok().or_else(|| parse_byte_size(n));
            let sources: Vec<FsPath> = paths
                .iter()
                .filter(|p| !p.starts_with(VOL_PREFIX))
                .filter_map(|p| FsPath::new(p).ok())
                .collect();
            if sources.is_empty() {
                return Ok(ActionOutcome::Done);
            }
            let dest = sources[0]
                .parent()
                .ok_or_else(|| {
                    WidgetError::InvalidStateForOperation("fm-archive-create-title".into())
                })?
                .join(name);
            create_archive(
                &dest,
                &sources,
                CreateArchiveOptions {
                    password: None,
                    volume_bytes,
                    sfx: false,
                },
            )
            .await
            .map_err(|e| map_archive_error(inner, e))?;
            inner.refresh_all_tabs().await;
            Ok(report(
                inner.deps.locale.tr("fm-archive-create-title"),
                inner.deps.locale.tr_args(
                    "fm-archive-created",
                    &orchid_i18n::FluentArgs::new().with("name", name.to_string()),
                ),
            ))
        }
        (Some(_), None) => Ok(prompt(
            "fs.archive-create-volume",
            paths,
            "Archive.7z",
            inner.deps.locale.tr("fm-archive-create-title"),
            inner.deps.locale.tr("fm-archive-create-hint"),
        )),
    }
}

async fn add(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let tab_path = {
        let state = inner.state.lock();
        state.active_pane().active_tab().path.clone()
    };
    if tab_path.is_archive() {
        let container = tab_path
            .archive_container()
            .ok_or_else(|| WidgetError::InvalidStateForOperation("fm-archive-add".into()))?;
        let sources = if paths.iter().any(|p| {
            FsPath::new(p)
                .ok()
                .is_some_and(|fp| fp.is_local() && !fp.is_archive())
        }) {
            paths
                .iter()
                .filter_map(|p| FsPath::new(p).ok())
                .filter(|p| p.is_local())
                .collect()
        } else {
            other_pane_local_selection(inner)
        };
        if sources.is_empty() {
            return Ok(ActionOutcome::Done);
        }
        add_to_archive(&container, &sources)
            .await
            .map_err(|e| map_archive_error(inner, e))?;
        inner.refresh_all_tabs().await;
        return Ok(report(
            inner.deps.locale.tr("fm-action-archive-add"),
            inner.deps.locale.tr_args(
                "fm-archive-added",
                &orchid_i18n::FluentArgs::new().with("n", sources.len().to_string()),
            ),
        ));
    }
    let fps: Vec<FsPath> = paths.iter().filter_map(|p| FsPath::new(p).ok()).collect();
    let archives: Vec<_> = fps.iter().filter(|p| is_archive_file(p)).cloned().collect();
    let locals: Vec<_> = fps
        .iter()
        .filter(|p| p.is_local() && !is_archive_file(p))
        .cloned()
        .collect();
    if archives.len() == 1 && !locals.is_empty() {
        add_to_archive(&archives[0], &locals)
            .await
            .map_err(|e| map_archive_error(inner, e))?;
        inner.refresh_all_tabs().await;
        return Ok(report(
            inner.deps.locale.tr("fm-action-archive-add"),
            inner.deps.locale.tr_args(
                "fm-archive-added",
                &orchid_i18n::FluentArgs::new().with("n", locals.len().to_string()),
            ),
        ));
    }
    let Some(name) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.archive-add",
            paths,
            "Archive.zip",
            inner.deps.locale.tr("fm-action-archive-add"),
            inner.deps.locale.tr("fm-archive-create-hint"),
        ));
    };
    let dest = fps
        .first()
        .and_then(|p| p.parent())
        .ok_or_else(|| WidgetError::InvalidStateForOperation("fm-action-archive-add".into()))?
        .join(name);
    if !is_archive_file(&dest) {
        return Err(WidgetError::InvalidStateForOperation(
            dest.as_str().to_string(),
        ));
    }
    add_to_archive(&dest, &locals)
        .await
        .map_err(|e| map_archive_error(inner, e))?;
    inner.refresh_all_tabs().await;
    Ok(report(
        inner.deps.locale.tr("fm-action-archive-add"),
        inner.deps.locale.tr_args(
            "fm-archive-added",
            &orchid_i18n::FluentArgs::new().with("n", locals.len().to_string()),
        ),
    ))
}

async fn delete_inner(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> WidgetResult<ActionOutcome> {
    let mut by_archive: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for p in paths {
        let Ok(fp) = FsPath::new(p) else {
            continue;
        };
        if let (Some(c), Some(inner_path)) = (fp.archive_container(), fp.archive_inner()) {
            if !inner_path.is_empty() {
                by_archive
                    .entry(c.as_str().to_string())
                    .or_default()
                    .push(inner_path.to_string());
            }
        }
    }
    let mut n = 0_usize;
    for (archive, inners) in by_archive {
        let dest = FsPath::new(&archive).map_err(map_fs_error)?;
        n += inners.len();
        delete_from_archive(&dest, &inners)
            .await
            .map_err(|e| map_archive_error(inner, e))?;
    }
    inner.refresh_all_tabs().await;
    Ok(report(
        inner.deps.locale.tr("fm-action-archive-delete"),
        inner.deps.locale.tr_args(
            "fm-archive-deleted",
            &orchid_i18n::FluentArgs::new().with("n", n.to_string()),
        ),
    ))
}

async fn test(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    password: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let archive = resolve_test_target(inner, paths)?;
    match test_archive(&archive, password).await {
        Ok(report_data) => Ok(report(
            inner.deps.locale.tr("fm-archive-test-title"),
            report_data.summary,
        )),
        Err(e) if password.is_none() && needs_password(&e.to_string()) => {
            Ok(ActionOutcome::NeedsPassphrase {
                paths: vec![archive.as_str().to_string(), OPEN_TEST.into()],
                purpose: PassphrasePurpose::ArchiveOpen,
            })
        }
        Err(e) => Err(map_archive_error(inner, e)),
    }
}

async fn finish_open_with_password(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    password: &str,
) -> WidgetResult<ActionOutcome> {
    let Some(archive) = paths.first().and_then(|p| FsPath::new(p).ok()) else {
        return Ok(ActionOutcome::Done);
    };
    let dest_token = paths.get(1).map(String::as_str).unwrap_or("");
    if dest_token == OPEN_TEST {
        return test(inner, &[archive.as_str().to_string()], Some(password)).await;
    }
    if dest_token == OPEN_VIEW || dest_token == OPEN_BROWSE {
        let inner_path = paths.get(2).cloned().unwrap_or_default();
        let fp = FsPath::archive_from_file(&archive, &inner_path).map_err(map_fs_error)?;
        let local = materialize_inner(&fp, Some(password))
            .await
            .map_err(WidgetError::InvalidStateForOperation)?;
        if dest_token == OPEN_BROWSE || looks_like_archive_name(fp.file_name().unwrap_or("")) {
            let dest = FsPath::archive_from_file(&local, "").map_err(map_fs_error)?;
            navigate_active(inner, dest).await?;
            return Ok(ActionOutcome::Done);
        }
        return Ok(ActionOutcome::OpenInViewer {
            path: local.as_str().to_string(),
        });
    }
    let dest = FsPath::new(dest_token).map_err(map_fs_error)?;
    let inners: Vec<String> = paths.iter().skip(2).cloned().collect();
    let n = extract_archive(&archive, &dest, &inners, Some(password))
        .await
        .map_err(|e| map_archive_error(inner, e))?;
    inner.refresh_all_tabs().await;
    Ok(report(
        inner.deps.locale.tr("fm-archive-extract-title"),
        inner.deps.locale.tr_args(
            "fm-archive-done",
            &orchid_i18n::FluentArgs::new().with("n", n.to_string()),
        ),
    ))
}

async fn materialize_inner(fp: &FsPath, password: Option<&str>) -> Result<FsPath, String> {
    let container = fp
        .archive_container()
        .ok_or_else(|| "not an archive path".to_string())?;
    let inner = fp.archive_inner().unwrap_or("").trim_matches('/');
    if inner.is_empty() {
        return Err("empty archive entry".into());
    }
    let stamp = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        container.as_str().hash(&mut h);
        inner.hash(&mut h);
        h.finish()
    };
    let dest_dir = std::env::temp_dir()
        .join("orchid-archive-cache")
        .join(format!("{stamp:x}"));
    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let dest = FsPath::from_local(&dest_dir).map_err(|e| e.to_string())?;
    extract_archive(&container, &dest, &[inner.to_string()], password)
        .await
        .map_err(|e| e.to_string())?;
    let extracted = dest_dir.join(inner.replace('/', std::path::MAIN_SEPARATOR_STR));
    if extracted.is_file() || extracted.is_dir() {
        return FsPath::from_local(&extracted).map_err(|e| e.to_string());
    }
    let name = fp.file_name().unwrap_or("file");
    let fallback = dest_dir.join(name);
    if fallback.is_file() {
        return FsPath::from_local(&fallback).map_err(|e| e.to_string());
    }
    Err(format!("extracted entry not found: {inner}"))
}

fn resolve_archive_targets(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    selected_only: bool,
) -> WidgetResult<(FsPath, Vec<String>)> {
    let tab_archive = {
        let state = inner.state.lock();
        let p = state.active_pane().active_tab().path.clone();
        p.is_archive().then_some(p)
    };
    if selected_only {
        let mut inners = Vec::new();
        let mut container = None;
        for p in paths {
            let Ok(fp) = FsPath::new(p) else {
                continue;
            };
            if let (Some(c), Some(i)) = (fp.archive_container(), fp.archive_inner()) {
                if !i.is_empty() {
                    container = Some(c);
                    inners.push(i.to_string());
                }
            }
        }
        if let Some(c) = container {
            return Ok((c, inners));
        }
    }
    for p in paths {
        let Ok(fp) = FsPath::new(p) else {
            continue;
        };
        if is_archive_file(&fp) {
            return Ok((fp, Vec::new()));
        }
        if let Some(c) = fp.archive_container() {
            return Ok((c, Vec::new()));
        }
    }
    if let Some(p) = tab_archive {
        if let Some(c) = p.archive_container() {
            return Ok((c, Vec::new()));
        }
    }
    Err(WidgetError::InvalidStateForOperation(
        "fm-action-archive-extract".into(),
    ))
}

fn resolve_test_target(inner: &Arc<FileManagerInner>, paths: &[String]) -> WidgetResult<FsPath> {
    for p in paths {
        let Ok(fp) = FsPath::new(p) else {
            continue;
        };
        if is_archive_file(&fp) {
            return Ok(fp);
        }
        if let Some(c) = fp.archive_container() {
            return Ok(c);
        }
    }
    let state = inner.state.lock();
    let p = state.active_pane().active_tab().path.clone();
    p.archive_container()
        .ok_or_else(|| WidgetError::InvalidStateForOperation("fm-archive-test-title".into()))
}

fn default_archive_name(sources: &[FsPath]) -> String {
    if sources.len() == 1 {
        let name = sources[0].file_name().unwrap_or("Archive");
        let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
        return format!("{stem}.zip");
    }
    "Archive.zip".into()
}

fn other_pane_local_selection(inner: &Arc<FileManagerInner>) -> Vec<FsPath> {
    let state = inner.state.lock();
    let other = match state.active_pane {
        super::ActivePane::Left => state.right_pane.as_ref().map(|p| p.active_tab()),
        super::ActivePane::Right => Some(state.left_pane.active_tab()),
    };
    other
        .map(|t| t.selection.selected_paths())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| FsPath::new(&p).ok())
        .filter(|p| p.is_local())
        .collect()
}

fn active_pane_idx(inner: &Arc<FileManagerInner>) -> u8 {
    match inner.state.lock().active_pane {
        super::ActivePane::Left => 0,
        super::ActivePane::Right => 1,
    }
}

async fn navigate_active(inner: &Arc<FileManagerInner>, path: FsPath) -> WidgetResult<()> {
    super::navigate(inner.instance_id, active_pane_idx(inner), path).await
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

fn needs_password(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    m.contains("password") || m.contains("encrypted") || m.contains("wrong password")
}

fn map_archive_error(inner: &Arc<FileManagerInner>, e: orchid_fs::FsError) -> WidgetError {
    let s = e.to_string();
    if s.contains("7-Zip") || s.contains("ORCHID_7Z") {
        return WidgetError::InvalidStateForOperation(
            inner.deps.locale.tr("fm-archive-7z-missing"),
        );
    }
    map_fs_error(e)
}
