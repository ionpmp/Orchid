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
        "fs.properties" => file_properties(inner, paths).await,
        "fs.share" | "fs.share-view" => share_report(inner, paths).await,
        "fs.share-add" => share_add(inner, paths, input).await,
        "fs.share-remove" => share_remove(inner, paths, opts).await,
        "fs.share-os" => share_os_tab(inner, paths).await,
        "fs.versions" | "fs.versions-view" => versions_report(inner, paths).await,
        "fs.versions-restore" => versions_restore(inner, paths, opts, input).await,
        "fs.versions-copy" => versions_copy(inner, paths, input).await,
        "fs.versions-os" => versions_os_tab(inner, paths).await,
        "fs.bitlocker" | "fs.bitlocker-view" => bitlocker_report(inner, paths).await,
        "fs.bitlocker-lock" => bitlocker_lock(inner, paths, opts).await,
        "fs.bitlocker-unlock" => bitlocker_unlock(inner, paths, input).await,
        "fs.bitlocker-os" => bitlocker_os_panel().await,
        "fs.exif" => exif_report(inner, paths).await,
        "fs.id3" => id3_report(inner, paths).await,
        "fs.office-meta" => office_meta(inner, paths, input).await,
        "fs.signature" => signature_report(inner, paths).await,
        _ => Ok(ActionOutcome::Done),
    }
}

fn report(title: String, body: String) -> ActionOutcome {
    ActionOutcome::NeedsReport { title, body }
}

fn file_ext(path: &orchid_fs::FsPath) -> String {
    path.file_name()
        .and_then(|n| n.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

fn first_path(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> Result<orchid_fs::FsPath, WidgetError> {
    if let Some(raw) = paths.first().filter(|s| !s.is_empty()) {
        return orchid_fs::FsPath::new(raw).map_err(map_fs_error);
    }
    orchid_fs::FsPath::new(active_pane_path(inner)).map_err(map_fs_error)
}

fn dash(s: Option<String>) -> String {
    s.filter(|v| !v.is_empty()).unwrap_or_else(|| "—".into())
}

async fn file_properties(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let fmt = inner.deps.orchid_config.read().locale.clone();
    let fp = first_path(inner, paths)?;
    let props = orchid_fs::inspect_path(&inner.deps.registry, &fp)
        .await
        .map_err(map_fs_error)?;
    let kind = match props.kind {
        orchid_fs::FsEntryKind::Directory => locale.tr("fm-properties-kind-folder"),
        orchid_fs::FsEntryKind::File => locale.tr("fm-properties-kind-file"),
        orchid_fs::FsEntryKind::Symlink => locale.tr("fm-properties-kind-symlink"),
        orchid_fs::FsEntryKind::Other => locale.tr("fm-properties-kind-file"),
    };
    let attrs = [
        props
            .readonly
            .then_some(locale.tr("fm-properties-attr-readonly")),
        props
            .hidden
            .then_some(locale.tr("fm-properties-attr-hidden")),
        props
            .system
            .then_some(locale.tr("fm-properties-attr-system")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(", ");
    let mut body = String::new();
    for (key, value) in [
        ("fm-prop-name", props.name),
        ("fm-prop-path", props.path),
        ("fm-prop-type", kind),
        ("fm-prop-size", locale.format_byte_size(props.size)),
        (
            "fm-prop-created",
            dash(props.created.map(|t| fmt.format_datetime(t))),
        ),
        (
            "fm-prop-modified",
            dash(props.modified.map(|t| fmt.format_datetime(t))),
        ),
        (
            "fm-prop-accessed",
            dash(props.accessed.map(|t| fmt.format_datetime(t))),
        ),
        (
            "fm-prop-attributes",
            if attrs.is_empty() {
                "—".into()
            } else {
                attrs
            },
        ),
        ("fm-prop-mime", dash(props.mime)),
    ] {
        body.push_str(&locale.tr_args(key, &orchid_i18n::FluentArgs::new().with("value", value)));
        body.push('\n');
    }
    append_content_metadata(&mut body, locale, &fp);
    append_sharing(&mut body, locale, &fp).await;
    append_previous_versions(&mut body, locale, &fmt, &fp).await;
    append_bitlocker(&mut body, locale, &fp).await;
    Ok(report(locale.tr("fm-properties-title"), body))
}

async fn append_sharing(
    body: &mut String,
    locale: &orchid_i18n::LocaleManager,
    fp: &orchid_fs::FsPath,
) {
    body.push('\n');
    body.push_str(&locale.tr("fm-share-title"));
    body.push('\n');
    match orchid_fs::shares_for_path(fp).await {
        Ok(shares) if shares.is_empty() => {
            body.push_str(&locale.tr("fm-share-not-shared"));
            body.push('\n');
        }
        Ok(shares) => {
            for share in shares {
                push_share_lines(body, locale, &share);
            }
        }
        Err(_) => {
            body.push_str(&locale.tr("fm-share-unsupported"));
            body.push('\n');
        }
    }
}

fn push_share_lines(
    body: &mut String,
    locale: &orchid_i18n::LocaleManager,
    share: &orchid_fs::FolderShare,
) {
    let args = |key: &str, value: String| {
        locale.tr_args(key, &orchid_i18n::FluentArgs::new().with("value", value))
    };
    body.push_str(&args("fm-share-name", share.name.clone()));
    body.push('\n');
    body.push_str(&args("fm-share-unc", share.unc.clone()));
    body.push('\n');
    if !share.remark.is_empty() {
        body.push_str(&args("fm-share-remark", share.remark.clone()));
        body.push('\n');
    }
    if !share.exact {
        body.push_str(&locale.tr("fm-share-via-parent"));
        body.push('\n');
    }
    if share.administrative {
        body.push_str(&locale.tr("fm-share-admin"));
        body.push('\n');
    }
    let users = if let Some(max) = share.max_uses {
        locale.tr_args(
            "fm-share-users",
            &orchid_i18n::FluentArgs::new()
                .with("current", share.current_uses.to_string())
                .with("max", max.to_string()),
        )
    } else {
        locale.tr_args(
            "fm-share-users-unlimited",
            &orchid_i18n::FluentArgs::new().with("current", share.current_uses.to_string()),
        )
    };
    body.push_str(&users);
    body.push('\n');
}

async fn share_report(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let fp = first_path(inner, paths)?;
    let mut body = String::new();
    append_sharing(&mut body, locale, &fp).await;
    Ok(report(locale.tr("fm-share-title"), body))
}

async fn share_add(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let fp = first_path(inner, paths)?;
    let default_name = fp.file_name().unwrap_or("share").to_string();
    let Some(raw) = input else {
        return Ok(prompt(
            "fs.share-add",
            paths,
            &default_name,
            inner.deps.locale.tr("fm-share-add-title"),
            inner.deps.locale.tr("fm-share-add-hint"),
        ));
    };
    let (name, remark) = parse_share_spec(raw, &default_name);
    let share = orchid_fs::add_folder_share(&fp, name, remark)
        .await
        .map_err(map_fs_error)?;
    Ok(report(
        inner.deps.locale.tr("fm-share-title"),
        inner.deps.locale.tr_args(
            "fm-share-add-done",
            &orchid_i18n::FluentArgs::new().with("name", share.name),
        ),
    ))
}

fn parse_share_spec<'a>(raw: &'a str, fallback: &'a str) -> (&'a str, &'a str) {
    let t = raw.trim();
    if t.is_empty() {
        return (fallback, "");
    }
    if let Some((name, remark)) = t.split_once('|') {
        let name = name.trim();
        (if name.is_empty() { fallback } else { name }, remark.trim())
    } else {
        (t, "")
    }
}

async fn share_remove(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    opts: RunActionOpts,
) -> WidgetResult<ActionOutcome> {
    let fp = first_path(inner, paths)?;
    let shares = orchid_fs::shares_for_path(&fp)
        .await
        .map_err(map_fs_error)?;
    let Some(share) = orchid_fs::exact_user_share(&shares) else {
        return Err(WidgetError::InvalidStateForOperation(
            "fm-error-share-not-shared".into(),
        ));
    };
    if !opts.skip_confirm {
        return Ok(ActionOutcome::NeedsConfirmation {
            message: inner.deps.locale.tr_args(
                "fm-confirm-share-remove",
                &orchid_i18n::FluentArgs::new().with("name", share.name.clone()),
            ),
            action_id: "fs.share-remove".into(),
            paths: vec![fp.as_str().to_string()],
        });
    }
    let name = share.name.clone();
    orchid_fs::remove_folder_share(&name)
        .await
        .map_err(map_fs_error)?;
    Ok(report(
        inner.deps.locale.tr("fm-share-title"),
        inner.deps.locale.tr_args(
            "fm-share-remove-done",
            &orchid_i18n::FluentArgs::new().with("name", name),
        ),
    ))
}

async fn share_os_tab(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> WidgetResult<ActionOutcome> {
    let fp = first_path(inner, paths)?;
    orchid_fs::open_sharing_tab(&fp)
        .await
        .map_err(map_fs_error)?;
    Ok(ActionOutcome::Done)
}

async fn append_previous_versions(
    body: &mut String,
    locale: &orchid_i18n::LocaleManager,
    fmt: &orchid_storage::LocaleConfig,
    fp: &orchid_fs::FsPath,
) {
    body.push('\n');
    body.push_str(&locale.tr("fm-versions-title"));
    body.push('\n');
    match orchid_fs::previous_versions(fp).await {
        Ok(versions) if versions.is_empty() => {
            body.push_str(&locale.tr("fm-versions-none"));
            body.push('\n');
        }
        Ok(versions) => push_version_lines(body, locale, fmt, &versions),
        Err(_) => {
            body.push_str(&locale.tr("fm-versions-unsupported"));
            body.push('\n');
        }
    }
}

async fn versions_report(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let fmt = inner.deps.orchid_config.read().locale.clone();
    let fp = first_path(inner, paths)?;
    let versions = orchid_fs::previous_versions(&fp)
        .await
        .map_err(map_fs_error)?;
    let mut body = String::new();
    body.push_str(&locale.tr("fm-versions-title"));
    body.push('\n');
    if versions.is_empty() {
        body.push_str(&locale.tr("fm-versions-none"));
        body.push('\n');
    } else {
        push_version_lines(&mut body, locale, &fmt, &versions);
    }
    Ok(report(locale.tr("fm-versions-title"), body))
}

fn push_version_lines(
    body: &mut String,
    locale: &orchid_i18n::LocaleManager,
    fmt: &orchid_storage::LocaleConfig,
    versions: &[orchid_fs::PreviousVersion],
) {
    for (i, ver) in versions.iter().enumerate() {
        let when = fmt.format_datetime(ver.created);
        let line = if ver.is_dir {
            locale.tr_args(
                "fm-versions-item-dir",
                &orchid_i18n::FluentArgs::new()
                    .with("n", (i + 1).to_string())
                    .with("when", when),
            )
        } else {
            locale.tr_args(
                "fm-versions-item",
                &orchid_i18n::FluentArgs::new()
                    .with("n", (i + 1).to_string())
                    .with("when", when)
                    .with("size", locale.format_byte_size(ver.size)),
            )
        };
        body.push_str(&line);
        body.push('\n');
    }
}

fn versions_spec(paths: &[String], input: Option<&str>) -> Option<String> {
    if let Some(s) = input.map(str::trim).filter(|s| !s.is_empty()) {
        return Some(s.to_string());
    }
    paths
        .get(1)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

async fn versions_restore(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    opts: RunActionOpts,
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let fp = first_path(inner, paths)?;
    let Some(spec) = versions_spec(paths, input) else {
        return Ok(prompt(
            "fs.versions-restore",
            paths,
            "1",
            inner.deps.locale.tr("fm-versions-restore-title"),
            inner.deps.locale.tr("fm-versions-restore-hint"),
        ));
    };
    let versions = orchid_fs::previous_versions(&fp)
        .await
        .map_err(map_fs_error)?;
    let ver = orchid_fs::pick_previous_version(&versions, &spec).map_err(map_fs_error)?;
    let confirmed = opts.skip_confirm && paths.get(1).is_some();
    if !confirmed {
        let fmt = inner.deps.orchid_config.read().locale.clone();
        return Ok(ActionOutcome::NeedsConfirmation {
            message: inner.deps.locale.tr_args(
                "fm-confirm-versions-restore",
                &orchid_i18n::FluentArgs::new().with("when", fmt.format_datetime(ver.created)),
            ),
            action_id: "fs.versions-restore".into(),
            paths: vec![fp.as_str().to_string(), spec],
        });
    }
    let restored = orchid_fs::restore_previous_version(&fp, &spec)
        .await
        .map_err(map_fs_error)?;
    let fmt = inner.deps.orchid_config.read().locale.clone();
    Ok(report(
        inner.deps.locale.tr("fm-versions-title"),
        inner.deps.locale.tr_args(
            "fm-versions-restore-done",
            &orchid_i18n::FluentArgs::new().with("when", fmt.format_datetime(restored.created)),
        ),
    ))
}

async fn versions_copy(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let fp = first_path(inner, paths)?;
    let Some(spec) = versions_spec(paths, input) else {
        return Ok(prompt(
            "fs.versions-copy",
            paths,
            "1",
            inner.deps.locale.tr("fm-versions-copy-title"),
            inner.deps.locale.tr("fm-versions-copy-hint"),
        ));
    };
    let (ver, dest) = orchid_fs::copy_previous_version(&fp, &spec)
        .await
        .map_err(map_fs_error)?;
    let fmt = inner.deps.orchid_config.read().locale.clone();
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dest.display().to_string());
    Ok(report(
        inner.deps.locale.tr("fm-versions-title"),
        inner.deps.locale.tr_args(
            "fm-versions-copy-done",
            &orchid_i18n::FluentArgs::new()
                .with("when", fmt.format_datetime(ver.created))
                .with("name", name),
        ),
    ))
}

async fn versions_os_tab(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> WidgetResult<ActionOutcome> {
    let fp = first_path(inner, paths)?;
    orchid_fs::open_previous_versions_tab(&fp)
        .await
        .map_err(map_fs_error)?;
    Ok(ActionOutcome::Done)
}

async fn append_bitlocker(
    body: &mut String,
    locale: &orchid_i18n::LocaleManager,
    fp: &orchid_fs::FsPath,
) {
    body.push('\n');
    body.push_str(&locale.tr("fm-bitlocker-title"));
    body.push('\n');
    match orchid_fs::bitlocker_status(fp).await {
        Ok(status) => push_bitlocker_lines(body, locale, &status),
        Err(_) => {
            body.push_str(&locale.tr("fm-bitlocker-unsupported"));
            body.push('\n');
        }
    }
}

fn push_bitlocker_lines(
    body: &mut String,
    locale: &orchid_i18n::LocaleManager,
    status: &orchid_fs::BitLockerStatus,
) {
    let args = |key: &str, value: String| {
        locale.tr_args(key, &orchid_i18n::FluentArgs::new().with("value", value))
    };
    body.push_str(&args("fm-bitlocker-drive", status.letter.clone()));
    body.push('\n');
    body.push_str(&args(
        "fm-bitlocker-protection",
        locale.tr(bitlocker_protection_key(status.protection)),
    ));
    body.push('\n');
    body.push_str(&args(
        "fm-bitlocker-lock",
        locale.tr(bitlocker_lock_key(status.lock)),
    ));
    body.push('\n');
    body.push_str(&args(
        "fm-bitlocker-conversion",
        locale.tr(bitlocker_conversion_key(status.conversion)),
    ));
    body.push('\n');
    if matches!(
        status.conversion,
        orchid_fs::BitLockerConversion::Encrypting
            | orchid_fs::BitLockerConversion::Decrypting
            | orchid_fs::BitLockerConversion::EncryptionPaused
            | orchid_fs::BitLockerConversion::DecryptionPaused
    ) {
        body.push_str(&args("fm-bitlocker-percent", status.percent.to_string()));
        body.push('\n');
    }
    if !status.method.is_empty() {
        body.push_str(&args("fm-bitlocker-method", status.method.clone()));
        body.push('\n');
    }
}

fn bitlocker_protection_key(v: orchid_fs::BitLockerProtection) -> &'static str {
    match v {
        orchid_fs::BitLockerProtection::On => "fm-bitlocker-protection-on",
        orchid_fs::BitLockerProtection::Off => "fm-bitlocker-protection-off",
        orchid_fs::BitLockerProtection::Unknown => "fm-bitlocker-protection-unknown",
    }
}

fn bitlocker_lock_key(v: orchid_fs::BitLockerLock) -> &'static str {
    match v {
        orchid_fs::BitLockerLock::Unlocked => "fm-bitlocker-lock-unlocked",
        orchid_fs::BitLockerLock::Locked => "fm-bitlocker-lock-locked",
        orchid_fs::BitLockerLock::Unknown => "fm-bitlocker-lock-unknown",
    }
}

fn bitlocker_conversion_key(v: orchid_fs::BitLockerConversion) -> &'static str {
    match v {
        orchid_fs::BitLockerConversion::FullyDecrypted => "fm-bitlocker-conv-decrypted",
        orchid_fs::BitLockerConversion::FullyEncrypted => "fm-bitlocker-conv-encrypted",
        orchid_fs::BitLockerConversion::Encrypting => "fm-bitlocker-conv-encrypting",
        orchid_fs::BitLockerConversion::Decrypting => "fm-bitlocker-conv-decrypting",
        orchid_fs::BitLockerConversion::EncryptionPaused => "fm-bitlocker-conv-enc-paused",
        orchid_fs::BitLockerConversion::DecryptionPaused => "fm-bitlocker-conv-dec-paused",
        orchid_fs::BitLockerConversion::Unknown => "fm-bitlocker-conv-unknown",
    }
}

async fn bitlocker_report(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let fp = first_path(inner, paths)?;
    let status = orchid_fs::bitlocker_status(&fp)
        .await
        .map_err(map_fs_error)?;
    let mut body = String::new();
    body.push_str(&locale.tr("fm-bitlocker-title"));
    body.push('\n');
    push_bitlocker_lines(&mut body, locale, &status);
    Ok(report(locale.tr("fm-bitlocker-title"), body))
}

async fn bitlocker_lock(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    opts: RunActionOpts,
) -> WidgetResult<ActionOutcome> {
    let fp = first_path(inner, paths)?;
    let status = orchid_fs::bitlocker_status(&fp)
        .await
        .map_err(map_fs_error)?;
    if !opts.skip_confirm {
        return Ok(ActionOutcome::NeedsConfirmation {
            message: inner.deps.locale.tr_args(
                "fm-confirm-bitlocker-lock",
                &orchid_i18n::FluentArgs::new().with("drive", status.letter.clone()),
            ),
            action_id: "fs.bitlocker-lock".into(),
            paths: vec![fp.as_str().to_string()],
        });
    }
    let locked = orchid_fs::bitlocker_lock(&fp).await.map_err(map_fs_error)?;
    Ok(report(
        inner.deps.locale.tr("fm-bitlocker-title"),
        inner.deps.locale.tr_args(
            "fm-bitlocker-lock-done",
            &orchid_i18n::FluentArgs::new().with("drive", locked.letter),
        ),
    ))
}

async fn bitlocker_unlock(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let fp = first_path(inner, paths)?;
    let Some(secret) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.bitlocker-unlock",
            paths,
            "",
            inner.deps.locale.tr("fm-bitlocker-unlock-title"),
            inner.deps.locale.tr("fm-bitlocker-unlock-hint"),
        ));
    };
    let unlocked = orchid_fs::bitlocker_unlock(&fp, secret)
        .await
        .map_err(map_fs_error)?;
    Ok(report(
        inner.deps.locale.tr("fm-bitlocker-title"),
        inner.deps.locale.tr_args(
            "fm-bitlocker-unlock-done",
            &orchid_i18n::FluentArgs::new().with("drive", unlocked.letter),
        ),
    ))
}

async fn bitlocker_os_panel() -> WidgetResult<ActionOutcome> {
    orchid_fs::open_bitlocker_os().await.map_err(map_fs_error)?;
    Ok(ActionOutcome::Done)
}

fn append_content_metadata(
    body: &mut String,
    locale: &orchid_i18n::LocaleManager,
    fp: &orchid_fs::FsPath,
) {
    let Ok(os) = fp.to_local() else {
        return;
    };
    let ext = file_ext(fp);
    if orchid_viewers::is_exif_extension(&ext) {
        match orchid_viewers::format_exif_report(&os) {
            Ok(text) if !text.is_empty() => {
                body.push('\n');
                body.push_str(&locale.tr("fm-exif-title"));
                body.push('\n');
                body.push_str(&text);
            }
            _ => {}
        }
        let sidecar = orchid_viewers::format_sidecar_report(&os);
        if !sidecar.is_empty() {
            body.push('\n');
            body.push_str(&sidecar);
        }
    }
    if orchid_viewers::is_id3_extension(&ext) {
        match orchid_viewers::format_id3_report(&os) {
            Ok(text) if !text.is_empty() => {
                body.push('\n');
                body.push_str(&locale.tr("fm-id3-title"));
                body.push('\n');
                body.push_str(&text);
            }
            _ => {}
        }
    }
    if orchid_viewers::is_office_extension(&ext) {
        if let Ok(props) = orchid_viewers::read_office_core_props(&os) {
            let text = orchid_viewers::format_office_report(&props);
            if !text.is_empty() {
                body.push('\n');
                body.push_str(&locale.tr("fm-office-meta-title"));
                body.push('\n');
                body.push_str(&text);
            }
        }
    }
    if let Ok(sig) = orchid_fs::inspect_signature(fp) {
        if sig
            .lines
            .iter()
            .any(|l| l.contains("certificate table: yes") || l.contains("Authenticode: trusted"))
        {
            body.push('\n');
            body.push_str(&locale.tr("fm-signature-title"));
            body.push('\n');
            for line in sig.lines {
                body.push_str(&line);
                body.push('\n');
            }
        }
    }
}

async fn exif_report(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let fp = first_path(inner, paths)?;
    let os = fp.to_local().map_err(map_fs_error)?;
    let text = orchid_viewers::format_exif_report(&os)
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    let body = if text.is_empty() {
        locale.tr("fm-exif-empty")
    } else {
        text
    };
    Ok(report(locale.tr("fm-exif-title"), body))
}

async fn id3_report(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let fp = first_path(inner, paths)?;
    let os = fp.to_local().map_err(map_fs_error)?;
    let text = orchid_viewers::format_id3_report(&os)
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    let body = if text.is_empty() {
        locale.tr("fm-id3-empty")
    } else {
        text
    };
    Ok(report(locale.tr("fm-id3-title"), body))
}

async fn office_meta(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
    input: Option<&str>,
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let fp = first_path(inner, paths)?;
    let os = fp.to_local().map_err(map_fs_error)?;
    let current = orchid_viewers::read_office_core_props(&os)
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    let Some(raw) = input.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(prompt(
            "fs.office-meta",
            &[fp.as_str().to_string()],
            &orchid_viewers::pack_office_props(&current),
            locale.tr("fm-office-meta-title"),
            locale.tr("fm-office-meta-hint"),
        ));
    };
    let next = orchid_viewers::unpack_office_props(raw, current);
    orchid_viewers::write_office_core_props(&os, &next)
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    Ok(report(
        locale.tr("fm-office-meta-title"),
        orchid_viewers::format_office_report(&next),
    ))
}

async fn signature_report(
    inner: &Arc<FileManagerInner>,
    paths: &[String],
) -> WidgetResult<ActionOutcome> {
    let locale = inner.deps.locale.as_ref();
    let fp = first_path(inner, paths)?;
    let sig = orchid_fs::inspect_signature(&fp).map_err(map_fs_error)?;
    Ok(report(
        locale.tr("fm-signature-title"),
        sig.lines.join("\n"),
    ))
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
        if let Some(existing) = marks.iter_mut().find(|m| m.uri.trim() == mount.uri.trim()) {
            *existing = mount.clone();
        } else {
            marks.push(mount.clone());
        }
        orchid_storage::save_network_bookmarks(path, &marks).map_err(|e| {
            WidgetError::InvalidStateForOperation(format!("network bookmark save: {e}"))
        })?;
    }
    let mut live = inner.deps.network_mounts.write();
    if let Some(existing) = live.iter_mut().find(|m| m.uri.trim() == mount.uri.trim()) {
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
    let mount = parse_connect_line(raw)
        .ok_or_else(|| WidgetError::InvalidStateForOperation("fm-network-connect-bad".into()))?;
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
