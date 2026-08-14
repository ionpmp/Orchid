//! Advanced file-manager tools: compare/sync, hash, split, encode, attributes.

use std::sync::Arc;

use super::map_fs_error;
use super::selection::parse_byte_size;
use super::ActionOutcome;
use super::FileManagerInner;
use super::RunActionOpts;
use crate::error::Result as WidgetResult;
use crate::error::WidgetError;

use orchid_fs::{HashAlgo, NameCase, SyncMode};

/// Dispatch an advanced-tools action.
pub(super) async fn run(
    inner: &Arc<FileManagerInner>,
    action_id: &str,
    paths: &[String],
    opts: RunActionOpts,
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    match action_id {
        "fs.compare-dirs" => compare_dirs(inner, false).await,
        "fs.compare-dirs-bytes" => compare_dirs(inner, true).await,
        "fs.compare-files" => compare_files(inner, paths).await,
        "fs.sync-to-other" => sync_dirs(inner, SyncMode::ToRight, opts).await,
        "fs.sync-from-other" => sync_dirs(inner, SyncMode::ToLeft, opts).await,
        "fs.sync-both" => sync_dirs(inner, SyncMode::Both, opts).await,
        "fs.cloud-sync" => cloud_sync(inner, opts).await,
        "fs.network-bookmark" => network_bookmark(inner, input).await,
        "fs.network-connect" => network_connect(inner, input).await,
        "fs.merge-to-other" => sync_dirs(inner, SyncMode::MergeToRight, opts).await,
        "fs.split" => split_file(inner, paths, input).await,
        "fs.join" => join_files(inner, paths).await,
        "fs.hash-md5" => hash_files(inner, paths, HashAlgo::Md5).await,
        "fs.hash-sha1" => hash_files(inner, paths, HashAlgo::Sha1).await,
        "fs.hash-sha256" => hash_files(inner, paths, HashAlgo::Sha256).await,
        "fs.hash-blake3" => hash_files(inner, paths, HashAlgo::Blake3).await,
        "fs.hash-crc32" => hash_files(inner, paths, HashAlgo::Crc32).await,
        "fs.hash-verify" => verify_hashes(inner, paths).await,
        "fs.encode-base64" => encode(inner, paths, true, true).await,
        "fs.decode-base64" => encode(inner, paths, true, false).await,
        "fs.encode-uue" => encode(inner, paths, false, true).await,
        "fs.decode-uue" => encode(inner, paths, false, false).await,
        "fs.attr-readonly-on" => attrs(inner, paths, Some(true), None, None, None).await,
        "fs.attr-readonly-off" => attrs(inner, paths, Some(false), None, None, None).await,
        "fs.attr-hidden-on" => attrs(inner, paths, None, Some(true), None, None).await,
        "fs.attr-hidden-off" => attrs(inner, paths, None, Some(false), None, None).await,
        "fs.attr-system-on" => attrs(inner, paths, None, None, Some(true), None).await,
        "fs.attr-system-off" => attrs(inner, paths, None, None, Some(false), None).await,
        "fs.attr-archive-on" => attrs(inner, paths, None, None, None, Some(true)).await,
        "fs.attr-archive-off" => attrs(inner, paths, None, None, None, Some(false)).await,
        "fs.touch-now" => touch(inner, paths, None).await,
        "fs.touch-set" => touch_set(inner, paths, input).await,
        "fs.case-lower" => rename_case(inner, paths, NameCase::Lower).await,
        "fs.case-upper" => rename_case(inner, paths, NameCase::Upper).await,
        "fs.case-title" => rename_case(inner, paths, NameCase::Title).await,
        "fs.chmod" => chmod_set(inner, paths, input).await,
        "fs.chown" => chown_set(inner, paths, input).await,
        "fs.acl-view" => acl_view(inner, paths).await,
        "fs.acl-grant" => acl_grant_set(inner, paths, input).await,
        "fs.acl-reset" => acl_reset(inner, paths).await,
        _ => Ok(ActionOutcome::Done),
    }
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

fn pane_dirs(
    inner: &Arc<FileManagerInner>,
) -> Result<(orchid_fs::FsPath, orchid_fs::FsPath), WidgetError> {
    let state = inner.state.lock();
    let left = state.left_pane.active_tab().path.clone();
    let Some(right) = state
        .right_pane
        .as_ref()
        .map(|p| p.active_tab().path.clone())
    else {
        return Err(WidgetError::InvalidStateForOperation(
            "fm-tools-need-dual-pane".into(),
        ));
    };
    Ok((left, right))
}

async fn compare_dirs(
    inner: &Arc<FileManagerInner>,
    byte_level: bool,
) -> WidgetResult<ActionOutcome> {
    let (left, right) = pane_dirs(inner)?;
    let diff = orchid_fs::compare_dirs(&inner.deps.registry, &left, &right, byte_level)
        .await
        .map_err(map_fs_error)?;
    Ok(report(
        inner.deps.locale.tr("fm-compare-title"),
        diff.format_report(24),
    ))
}

async fn compare_files(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> WidgetResult<ActionOutcome> {
    let (a, b) = file_pair(inner, paths)?;
    let r = orchid_fs::compare_files(&inner.deps.registry, &a, &b)
        .await
        .map_err(map_fs_error)?;
    let body = if r.equal {
        inner.deps.locale.tr("fm-compare-files-equal")
    } else {
        inner.deps.locale.tr_args(
            "fm-compare-files-differ",
            &orchid_i18n::FluentArgs::new()
                .with("offset", r.mismatch_at.unwrap_or(0).to_string())
                .with("left", r.left_size.to_string())
                .with("right", r.right_size.to_string()),
        )
    };
    Ok(report(inner.deps.locale.tr("fm-compare-files-title"), body))
}

fn file_pair(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> WidgetResult<(orchid_fs::FsPath, orchid_fs::FsPath)> {
    let fps: Vec<orchid_fs::FsPath> = paths
        .iter()
        .filter_map(|p| orchid_fs::FsPath::new(p).ok())
        .collect();
    if fps.len() >= 2 {
        return Ok((fps[0].clone(), fps[1].clone()));
    }
    if fps.len() == 1 {
        if let Ok((_, other)) = pane_dirs(inner) {
            if let Some(name) = fps[0].file_name() {
                return Ok((fps[0].clone(), other.join(name)));
            }
        }
    }
    Err(WidgetError::InvalidStateForOperation(
        "fm-tools-need-two-files".into(),
    ))
}

async fn sync_dirs(
    inner: &Arc<FileManagerInner>,
    mode: SyncMode,
    opts: RunActionOpts,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let (left, right) = pane_dirs(inner)?;
    if !opts.skip_confirm {
        let diff = orchid_fs::compare_dirs(&inner.deps.registry, &left, &right, false)
            .await
            .map_err(map_fs_error)?;
        let c = diff.counts();
        let key = match mode {
            SyncMode::ToRight => "fm-confirm-sync-to",
            SyncMode::ToLeft => "fm-confirm-sync-from",
            SyncMode::Both => "fm-confirm-sync-both",
            SyncMode::MergeToRight => "fm-confirm-merge",
        };
        let msg = locale.tr_args(
            key,
            &orchid_i18n::FluentArgs::new()
                .with("left", c.left_only.to_string())
                .with("right", c.right_only.to_string())
                .with("diff", c.different.to_string()),
        );
        let action_id = match mode {
            SyncMode::ToRight => "fs.sync-to-other",
            SyncMode::ToLeft => "fs.sync-from-other",
            SyncMode::Both => "fs.sync-both",
            SyncMode::MergeToRight => "fs.merge-to-other",
        };
        return Ok(ActionOutcome::NeedsConfirmation {
            message: msg,
            action_id: action_id.into(),
            paths: Vec::new(),
        });
    }
    let stats = orchid_fs::sync_dirs(&inner.deps.registry, &left, &right, mode, false)
        .await
        .map_err(map_fs_error)?;
    inner.refresh_all_tabs().await;
    Ok(report(
        locale.tr("fm-sync-title"),
        locale.tr_args(
            "fm-sync-done",
            &orchid_i18n::FluentArgs::new()
                .with("right", stats.to_right.to_string())
                .with("left", stats.to_left.to_string()),
        ),
    ))
}

fn is_network_place(path: &orchid_fs::FsPath) -> bool {
    orchid_fs::is_rclone_scheme(path.scheme())
        || (path.is_local() && path.without_scheme().starts_with("//"))
}

fn persist_network_place(
    inner: &Arc<FileManagerInner>,
    mount: orchid_storage::NetworkMountConfig,
) -> Result<(), WidgetError> {
    if let Some(path) = inner.deps.network_bookmarks_file.as_ref() {
        let mut marks = orchid_storage::load_network_bookmarks(path);
        if let Some(existing) = marks
            .iter_mut()
            .find(|m| m.uri.trim() == mount.uri.trim())
        {
            *existing = mount.clone();
        } else {
            marks.push(mount.clone());
        }
        orchid_storage::save_network_bookmarks(path, &marks).map_err(|e| {
            WidgetError::InvalidStateForOperation(format!("network bookmark save: {e}"))
        })?;
    }
    let mut live = inner.deps.network_mounts.write();
    if let Some(existing) = live
        .iter_mut()
        .find(|m| m.uri.trim() == mount.uri.trim())
    {
        *existing = mount;
    } else {
        live.push(mount);
    }
    Ok(())
}

async fn cloud_sync(
    inner: &Arc<FileManagerInner>,
    opts: RunActionOpts,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let (left, right) = pane_dirs(inner)?;
    if !is_network_place(&left) && !is_network_place(&right) {
        return Err(WidgetError::InvalidStateForOperation(
            "fm-cloud-sync-need-remote".into(),
        ));
    }
    if !opts.skip_confirm {
        return Ok(ActionOutcome::NeedsConfirmation {
            message: locale.tr("fm-confirm-cloud-sync"),
            action_id: "fs.cloud-sync".into(),
            paths: Vec::new(),
        });
    }
    orchid_fs::rclone_sync(&inner.deps.registry, &left, &right, None)
        .await
        .map_err(map_fs_error)?;
    inner.refresh_all_tabs().await;
    Ok(report(
        locale.tr("fm-cloud-sync-title"),
        locale.tr("fm-cloud-sync-done"),
    ))
}

async fn network_bookmark(
    inner: &Arc<FileManagerInner>,
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let path = orchid_fs::FsPath::new(active_pane_path(inner)).map_err(map_fs_error)?;
    if !is_network_place(&path) {
        return Err(WidgetError::InvalidStateForOperation(
            "fm-network-bookmark-need-remote".into(),
        ));
    }
    let proposed = path
        .file_name()
        .map(String::from)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| path.as_str().to_string());
    let Some(name) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.network-bookmark",
            &[],
            &proposed,
            inner.deps.locale.tr("fm-network-bookmark-title"),
            inner.deps.locale.tr("fm-network-bookmark-hint"),
        ));
    };
    persist_network_place(
        inner,
        orchid_storage::NetworkMountConfig {
            name: name.to_string(),
            uri: path.as_str().to_string(),
            ..orchid_storage::NetworkMountConfig::default()
        },
    )?;
    inner.refresh_all_tabs().await;
    Ok(ActionOutcome::Done)
}

async fn network_connect(
    inner: &Arc<FileManagerInner>,
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.network-connect",
            &[],
            "Name | sftp:host/path | user | rclone-remote",
            inner.deps.locale.tr("fm-network-connect-title"),
            inner.deps.locale.tr("fm-network-connect-hint"),
        ));
    };
    let mount = parse_connect_line(raw).ok_or_else(|| {
        WidgetError::InvalidStateForOperation("fm-network-connect-bad".into())
    })?;
    persist_network_place(inner, mount)?;
    inner.refresh_all_tabs().await;
    Ok(ActionOutcome::Done)
}

fn parse_connect_line(input: &str) -> Option<orchid_storage::NetworkMountConfig> {
    let parts: Vec<&str> = input.split('|').map(str::trim).collect();
    if parts.is_empty() || parts.iter().all(|p| p.is_empty()) {
        return None;
    }
    let (name, uri, user, remote) = match parts.as_slice() {
        [uri] => (String::new(), *uri, None, None),
        [name, uri] => ((*name).to_string(), *uri, None, None),
        [name, uri, user] => ((*name).to_string(), *uri, nonempty(user), None),
        [name, uri, user, remote, ..] => {
            ((*name).to_string(), *uri, nonempty(user), nonempty(remote))
        }
        _ => return None,
    };
    if uri.is_empty() {
        return None;
    }
    let name = if name.is_empty() {
        uri.to_string()
    } else {
        name
    };
    Some(orchid_storage::NetworkMountConfig {
        name,
        uri: uri.to_string(),
        user: user.map(str::to_string),
        rclone_remote: remote.map(str::to_string),
        ..orchid_storage::NetworkMountConfig::default()
    })
}

fn nonempty(s: &str) -> Option<&str> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

fn active_pane_path(inner: &Arc<FileManagerInner>) -> String {
    let state = inner.state.lock();
    let pane = match state.active_pane {
        super::ActivePane::Left => &state.left_pane,
        super::ActivePane::Right => state.right_pane.as_ref().unwrap_or(&state.left_pane),
    };
    pane.active_tab().path.as_str().to_string()
}

async fn split_file(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    if paths.is_empty() {
        return Ok(ActionOutcome::Done);
    }
    let Some(size_s) = input else {
        return Ok(prompt(
            "fs.split",
            paths,
            "10M",
            inner.deps.locale.tr("fm-split-title"),
            inner.deps.locale.tr("fm-split-hint"),
        ));
    };
    let size = parse_byte_size(size_s)
        .filter(|n| *n > 0)
        .ok_or_else(|| WidgetError::InvalidStateForOperation("fm-split-bad-size".into()))?;
    let src = orchid_fs::FsPath::new(&paths[0]).map_err(map_fs_error)?;
    let parts = orchid_fs::split_file(&inner.deps.registry, &src, size)
        .await
        .map_err(map_fs_error)?;
    inner.refresh_all_tabs().await;
    Ok(report(
        inner.deps.locale.tr("fm-split-title"),
        inner.deps.locale.tr_args(
            "fm-split-done",
            &orchid_i18n::FluentArgs::new().with("n", parts.len().to_string()),
        ),
    ))
}

async fn join_files(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> WidgetResult<ActionOutcome> {
    let fps: Vec<orchid_fs::FsPath> = paths
        .iter()
        .filter_map(|p| orchid_fs::FsPath::new(p).ok())
        .collect();
    if fps.is_empty() {
        return Ok(ActionOutcome::Done);
    }
    let parts = if fps.len() == 1 {
        orchid_fs::discover_parts(&inner.deps.registry, &fps[0])
            .await
            .map_err(map_fs_error)?
    } else {
        let mut v = fps;
        v.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        v
    };
    let dest_name = orchid_fs::join_output_name(&parts[0]);
    let dest = parts[0]
        .parent()
        .map(|p| p.join(&dest_name))
        .ok_or_else(|| WidgetError::InvalidStateForOperation("fm-join-failed".into()))?;
    orchid_fs::join_files(&inner.deps.registry, &parts, &dest)
        .await
        .map_err(map_fs_error)?;
    inner.refresh_all_tabs().await;
    Ok(report(inner.deps.locale.tr("fm-join-title"), dest_name))
}

async fn hash_files(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    algo: HashAlgo,
) -> WidgetResult<ActionOutcome> {
    let fps: Vec<orchid_fs::FsPath> = paths
        .iter()
        .filter_map(|p| orchid_fs::FsPath::new(p).ok())
        .collect();
    let recs = orchid_fs::hash_paths(&inner.deps.registry, &fps, algo)
        .await
        .map_err(map_fs_error)?;
    if recs.is_empty() {
        return Ok(ActionOutcome::Done);
    }
    if let Some(parent) = recs[0].path.parent() {
        let dest = if recs.len() == 1 {
            parent.join(&format!("{}.{}", recs[0].name, algo.ext()))
        } else {
            parent.join(&format!("checksums.{}", algo.ext()))
        };
        let _ = orchid_fs::write_hash_file(&inner.deps.registry, &dest, algo, &recs).await;
    }
    inner.refresh_all_tabs().await;
    Ok(report(
        inner.deps.locale.tr("fm-hash-title"),
        orchid_fs::format_hash_report(algo, &recs),
    ))
}

async fn verify_hashes(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> WidgetResult<ActionOutcome> {
    let sidecar = paths.iter().find_map(|p| {
        let fp = orchid_fs::FsPath::new(p).ok()?;
        orchid_fs::is_hash_sidecar(&fp).then_some(fp)
    });
    let Some(side) = sidecar else {
        return Err(WidgetError::InvalidStateForOperation(
            "fm-hash-no-sidecar".into(),
        ));
    };
    let rows = orchid_fs::verify_hash_file(&inner.deps.registry, &side)
        .await
        .map_err(map_fs_error)?;
    Ok(report(
        inner.deps.locale.tr("fm-hash-verify-title"),
        orchid_fs::format_verify_report(&rows),
    ))
}

async fn encode(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    base64: bool,
    do_encode: bool,
) -> WidgetResult<ActionOutcome> {
    for p in paths {
        let src = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
        if do_encode {
            let dest = orchid_fs::sidecar_path(&src, if base64 { "b64" } else { "uue" });
            if base64 {
                orchid_fs::encode_base64(&inner.deps.registry, &src, &dest)
                    .await
                    .map_err(map_fs_error)?;
            } else {
                orchid_fs::encode_uue(&inner.deps.registry, &src, &dest)
                    .await
                    .map_err(map_fs_error)?;
            }
        } else {
            let dest = orchid_fs::decoded_path(&src);
            if base64 {
                orchid_fs::decode_base64(&inner.deps.registry, &src, &dest)
                    .await
                    .map_err(map_fs_error)?;
            } else {
                orchid_fs::decode_uue(&inner.deps.registry, &src, &dest)
                    .await
                    .map_err(map_fs_error)?;
            }
        }
    }
    inner.refresh_all_tabs().await;
    Ok(ActionOutcome::Done)
}

async fn attrs(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    readonly: Option<bool>,
    hidden: Option<bool>,
    system: Option<bool>,
    archive: Option<bool>,
) -> WidgetResult<ActionOutcome> {
    let patch = orchid_fs::AttrPatch {
        readonly,
        hidden,
        system,
        archive,
    };
    for p in paths {
        let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
        orchid_fs::apply_attr_patch(&fp, patch)
            .await
            .map_err(map_fs_error)?;
    }
    inner.refresh_all_tabs().await;
    Ok(ActionOutcome::Done)
}

async fn touch(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    when: Option<chrono::DateTime<chrono::Utc>>,
) -> WidgetResult<ActionOutcome> {
    for p in paths {
        let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
        orchid_fs::set_mtime(&fp, when)
            .await
            .map_err(map_fs_error)?;
    }
    inner.refresh_all_tabs().await;
    Ok(ActionOutcome::Done)
}

async fn touch_set(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let Some(s) = input else {
        return Ok(prompt(
            "fs.touch-set",
            paths,
            &chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            inner.deps.locale.tr("fm-touch-title"),
            inner.deps.locale.tr("fm-touch-hint"),
        ));
    };
    let when = orchid_fs::parse_timestamp(s);
    touch(inner, paths, when).await
}

async fn rename_case(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    mode: NameCase,
) -> WidgetResult<ActionOutcome> {
    for p in paths {
        let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
        orchid_fs::apply_name_case(&inner.deps.registry, &fp, mode)
            .await
            .map_err(map_fs_error)?;
    }
    inner.refresh_all_tabs().await;
    Ok(ActionOutcome::Done)
}

async fn chmod_set(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let Some(s) = input else {
        return Ok(prompt(
            "fs.chmod",
            paths,
            "755",
            inner.deps.locale.tr("fm-chmod-title"),
            inner.deps.locale.tr("fm-chmod-hint"),
        ));
    };
    let mode = orchid_fs::parse_mode(s)
        .ok_or_else(|| WidgetError::InvalidStateForOperation("fm-chmod-bad-mode".into()))?;
    for p in paths {
        let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
        orchid_fs::chmod(&fp, mode).await.map_err(map_fs_error)?;
    }
    inner.refresh_all_tabs().await;
    Ok(ActionOutcome::Done)
}

async fn chown_set(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let Some(s) = input else {
        return Ok(prompt(
            "fs.chown",
            paths,
            "user:group",
            inner.deps.locale.tr("fm-chown-title"),
            inner.deps.locale.tr("fm-chown-hint"),
        ));
    };
    for p in paths {
        let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
        orchid_fs::chown(&fp, s).await.map_err(map_fs_error)?;
    }
    inner.refresh_all_tabs().await;
    Ok(ActionOutcome::Done)
}

async fn acl_view(inner: &Arc<FileManagerInner>, paths: &[String]) -> WidgetResult<ActionOutcome> {
    let mut body = String::new();
    for p in paths {
        let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
        let text = orchid_fs::acl_text(&fp).await.map_err(map_fs_error)?;
        body.push_str(&text);
        if !body.ends_with('\n') {
            body.push('\n');
        }
    }
    Ok(report(inner.deps.locale.tr("fm-acl-title"), body))
}

async fn acl_grant_set(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let Some(s) = input else {
        return Ok(prompt(
            "fs.acl-grant",
            paths,
            "Users:(M)",
            inner.deps.locale.tr("fm-acl-grant-title"),
            inner.deps.locale.tr("fm-acl-grant-hint"),
        ));
    };
    let (account, rights) = parse_acl_grant(s);
    let mut body = String::new();
    for p in paths {
        let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
        let text = orchid_fs::acl_grant(&fp, account, rights)
            .await
            .map_err(map_fs_error)?;
        body.push_str(&text);
    }
    inner.refresh_all_tabs().await;
    Ok(report(inner.deps.locale.tr("fm-acl-title"), body))
}

fn parse_acl_grant(s: &str) -> (&str, &str) {
    let t = s.trim();
    if let Some((a, r)) = t.split_once(":(") {
        let rights = r.trim_end_matches(')');
        (a.trim(), if rights.is_empty() { "M" } else { rights })
    } else if let Some((a, r)) = t.split_once(':') {
        (a.trim(), r.trim())
    } else {
        (t, "M")
    }
}

async fn acl_reset(inner: &Arc<FileManagerInner>, paths: &[String]) -> WidgetResult<ActionOutcome> {
    let mut body = String::new();
    for p in paths {
        let fp = orchid_fs::FsPath::new(p).map_err(map_fs_error)?;
        let text = orchid_fs::acl_reset(&fp).await.map_err(map_fs_error)?;
        body.push_str(&text);
    }
    inner.refresh_all_tabs().await;
    Ok(report(inner.deps.locale.tr("fm-acl-title"), body))
}

#[cfg(test)]
mod connect_parse_tests {
    use super::parse_connect_line;

    #[test]
    fn parses_uri_only_and_packed() {
        let a = parse_connect_line("sftp:host/home").unwrap();
        assert_eq!(a.uri, "sftp:host/home");
        assert_eq!(a.name, "sftp:host/home");
        let b = parse_connect_line("Lab | sftp:host/tmp | alice | myserver").unwrap();
        assert_eq!(b.name, "Lab");
        assert_eq!(b.uri, "sftp:host/tmp");
        assert_eq!(b.user.as_deref(), Some("alice"));
        assert_eq!(b.rclone_remote.as_deref(), Some("myserver"));
        assert!(parse_connect_line("").is_none());
        assert!(parse_connect_line("Name |").is_none());
    }
}
