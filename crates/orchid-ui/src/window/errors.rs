//! Localized error messages for widget and storage failures.

use orchid_i18n::LocaleManager;

use crate::error::UiError;

pub(crate) fn viewer_localized_error(locale: &LocaleManager, err: &str) -> String {
    // WidgetError wraps ViewerError Display; peel common prefixes first.
    let mut msg = err;
    if let Some(rest) = msg.strip_prefix("widget is in an invalid state for this operation: ") {
        msg = rest;
    }
    match msg {
        "viewer-image-heic-unsupported"
        | "viewer-image-raw-unsupported"
        | "viewer-archive-nothing-selected"
        | "viewer-archive-cannot-extract-folder" => locale.tr(msg),
        _ if msg.starts_with("unsupported file type") => locale.tr("viewer-unsupported"),
        _ if msg.contains("edit outside buffer bounds") => locale.tr("viewer-text-read-only"),
        _ if msg.contains("PDF support unavailable") => locale.tr("viewer-pdf-unavailable"),
        _ if msg.starts_with("file too large:") => locale.tr("viewer-error-file-too-large"),
        _ if msg.starts_with("failed to decode image:") => locale.tr("viewer-error-image-decode"),
        _ if msg.starts_with("failed to render PDF page") => locale.tr("viewer-error-pdf-render"),
        _ if msg == "PDF has no pages" => locale.tr("viewer-error-pdf-empty"),
        _ if msg.starts_with("failed to parse text:") => locale.tr("viewer-error-parse-text"),
        _ if msg.starts_with("syntax grammar not found") => locale.tr("viewer-error-syntax-grammar"),
        _ if msg.starts_with("archive entry not found:") => {
            locale.tr("viewer-error-archive-entry-not-found")
        }
        _ if msg.starts_with("thumbnail generation failed:") => {
            locale.tr("viewer-error-thumbnail")
        }
        "no viewer" | "viewer widget not live" => locale.tr("viewer-error-unavailable"),
        "not an archive" | "no archive open" => locale.tr("viewer-error-no-archive"),
        _ if lower_contains_io(msg) => locale.tr("viewer-error-io"),
        // Callers wrap the result in `viewer-error-with-reason`.
        _ => locale.tr("viewer-error-unknown"),
    }
}

fn lower_contains_io(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("os error")
        || lower.contains("i/o error")
        || lower.contains("io error")
        || lower.contains("permission denied")
        || lower.contains("access is denied")
        || lower.contains("no such file")
        || lower.contains("failed to read")
        || lower.contains("failed to write")
        || lower.contains("failed to open")
}

pub(crate) fn ui_localized_error(locale: &LocaleManager, err: &UiError) -> String {
    match err {
        UiError::ThemeNotFound(id) => locale.tr_args(
            "settings-error-theme-not-found",
            &orchid_i18n::FluentArgs::new().with("id", id.as_str()),
        ),
        UiError::Io(e) => storage_io_reason(locale, &e.to_string()),
        UiError::Storage(e) => storage_localized_error(locale, e),
        other => {
            let text = other.to_string();
            let mapped = fm_localized_error(locale, &text);
            if mapped != text {
                mapped
            } else {
                text
            }
        }
    }
}

pub(crate) fn storage_localized_error(
    locale: &LocaleManager,
    err: &orchid_storage::StorageError,
) -> String {
    storage_io_reason(locale, &err.to_string())
}

fn storage_io_reason(locale: &LocaleManager, text: &str) -> String {
    let mapped = fm_localized_error(locale, text);
    if mapped != text {
        mapped
    } else {
        locale.tr("viewer-error-io")
    }
}

pub(crate) fn media_localized_error(
    locale: &LocaleManager,
    err: &orchid_widgets::builtin::media::MediaError,
) -> String {
    use orchid_widgets::builtin::media::MediaError;
    match err {
        MediaError::NoSession => locale.tr("media-no-session"),
        MediaError::Unsupported => locale.tr("media-unsupported"),
        MediaError::ControlFailed(reason)
            if reason.to_ascii_lowercase().contains("transport command rejected") =>
        {
            locale.tr("media-control-rejected")
        }
        MediaError::ControlFailed(_) => locale.tr("media-control-failed"),
    }
}

pub(crate) fn password_localized_error(locale: &LocaleManager, err: &str) -> String {
    let lower = err.to_ascii_lowercase();
    match err {
        "title required" => locale.tr("password-add-error-title"),
        "vault locked" => locale.tr("password-error-vault-locked"),
        "password widget not live" => locale.tr("password-error-unavailable"),
        _ if lower.contains("invalid master password") => {
            locale.tr("password-error-invalid-master")
        }
        _ if lower.contains("biometric verification cancelled") => {
            locale.tr("password-error-biometric-cancelled")
        }
        _ if lower.contains("biometric verification is unavailable") => {
            locale.tr("password-error-biometric-unavailable")
        }
        _ if lower.contains("biometric verification failed") => {
            locale.tr("password-error-biometric-failed")
        }
        _ if lower.contains("no stored master key for biometric") => {
            locale.tr("password-error-no-master-key")
        }
        _ if lower.starts_with("failed to open password database:") => {
            locale.tr("password-error-db-open")
        }
        _ if lower.starts_with("duplicate entry title in same group:") => {
            locale.tr("password-add-error-duplicate")
        }
        _ if lower.starts_with("entry not found:") => locale.tr("password-error-entry-not-found"),
        _ => locale.tr_args(
            "password-error-with-reason",
            &orchid_i18n::FluentArgs::new().with("reason", err),
        ),
    }
}

pub(crate) fn fm_localized_error(locale: &LocaleManager, err: &str) -> String {
    let mut msg = err;
    if let Some(rest) = msg.strip_prefix("widget is in an invalid state for this operation: ") {
        msg = rest;
    }
    // Fluent-key sentinels from orchid-widgets file manager.
    match msg {
        "network-placeholder" => return locale.tr("fm-network-placeholder"),
        "virtual-empty-recent" => return locale.tr("fm-virtual-recent-empty"),
        "virtual-empty-starred" => return locale.tr("fm-virtual-starred-empty"),
        "virtual-empty-tags" => return locale.tr("fm-virtual-tags-empty"),
        "virtual-empty-category" => return locale.tr("fm-virtual-category-empty"),
        "fm-transfer-virtual-dest"
        | "fm-virtual-create-denied"
        | "fm-encryption-unavailable"
        | "fm-managed-unavailable"
        | "fm-managed-no-selection"
        | "fm-not-managed-folder"
        | "fm-managed-conflict"
        | "fm-invalid-folder-name"
        | "fm-no-provider-parent"
        | "fm-no-parent-folder"
        | "fm-selection-multiple-folders"
        | "fm-invalid-rename-target"
        | "fm-cannot-rename-root"
        | "fm-no-provider-path"
        | "fm-empty-tag"
        | "fm-drop-not-directory"
        | "fm-drop-unavailable" => return locale.tr(msg),
        "invalid tab id" => return locale.tr("fm-error-invalid-tab"),
        "invalid sort column" => return locale.tr("fm-error-invalid-sort"),
        "file-manager widget not live" => return locale.tr("fm-error-unavailable"),
        _ => {}
    }
    let lower = msg.to_ascii_lowercase();
    match msg {
        _ if msg.contains("no provider for scheme") => locale.tr("fm-network-no-provider"),
        _ if msg.contains("not found; install rclone") => locale.tr("fm-network-rclone-missing"),
        _ if lower.contains("invalid mount uri") => locale.tr("fm-network-invalid-mount"),
        // Local FS access failures (Windows "Access is denied. (os error 5)").
        _ if lower.contains("os error 5") || lower.contains("access is denied") => {
            locale.tr("fm-error-access")
        }
        _ if lower.contains("os error 2")
            || lower.contains("no such file")
            || lower.contains("the system cannot find the file")
            || lower.contains("the system cannot find the path") =>
        {
            locale.tr("fm-error-not-found")
        }
        _ if lower.contains("os error 112")
            || lower.contains("no space")
            || lower.contains("disk full")
            || lower.contains("not enough space") =>
        {
            locale.tr("fm-error-disk-full")
        }
        _ if lower.contains("os error 32")
            || lower.contains("sharing violation")
            || lower.contains("being used by another process") =>
        {
            locale.tr("fm-error-in-use")
        }
        _ if lower.contains("authentication")
            || lower.contains("access denied")
            || lower.contains("login failed")
            || lower.contains("401") =>
        {
            locale.tr("fm-network-auth-failed")
        }
        _ if lower.contains("permission denied") || lower.contains("forbidden") || lower.contains("403") => {
            // Prefer a generic access message for local FS; keep network wording for HTTP 403.
            if lower.contains("403") || lower.contains("forbidden") {
                locale.tr("fm-network-permission-denied")
            } else {
                locale.tr("fm-error-access")
            }
        }
        _ if lower.contains("connection refused")
            || lower.contains("timed out")
            || lower.contains("timeout")
            || lower.contains("no such host")
            || lower.contains("network is unreachable")
            || lower.contains("could not connect") =>
        {
            locale.tr("fm-network-connection-failed")
        }
        _ if lower.contains("already exists") => locale.tr("fm-transfer-already-exists"),
        _ if lower.contains("cannot drop into virtual folder") => locale.tr("fm-transfer-virtual-dest"),
        _ if lower.contains("cannot create folder in virtual location") => {
            locale.tr("fm-virtual-create-denied")
        }
        _ if lower.contains("encryption unavailable") => locale.tr("fm-encryption-unavailable"),
        _ if lower.contains("managed folders unavailable") => locale.tr("fm-managed-unavailable"),
        _ if lower.contains("no selection for managed folder") => locale.tr("fm-managed-no-selection"),
        _ if lower.contains("not a managed folder") => locale.tr("fm-not-managed-folder"),
        _ if lower.contains("managed folder conflict") => locale.tr("fm-managed-conflict"),
        _ if lower.contains("invalid passphrase") => locale.tr("fm-passphrase-invalid"),
        _ if lower.contains("passphrase required") => locale.tr("fm-passphrase-required"),
        _ if lower.contains("age decryption failed") => locale.tr("fm-decryption-failed"),
        _ => locale.tr_args(
            "fm-error-io",
            &orchid_i18n::FluentArgs::new().with("reason", msg),
        ),
    }
}

pub(crate) fn search_localized_error(locale: &LocaleManager, err: &str) -> String {
    match err {
        "search-sources-unconfigured" => locale.tr("search-sources-unconfigured"),
        _ => locale.tr_args(
            "search-error-with-reason",
            &orchid_i18n::FluentArgs::new().with("reason", err),
        ),
    }
}

pub(crate) fn is_passphrase_retryable(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("invalid passphrase") || lower.contains("passphrase required")
}
