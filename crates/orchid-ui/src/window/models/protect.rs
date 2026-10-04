//! Slint model for the Protection widget.

use orchid_i18n::LocaleManager;
use orchid_widgets::ProtectPayload;
use slint::{ModelRc, SharedString, VecModel};

use crate::slint_generated::{ProtectDrive, ProtectModel, ProtectRow};

pub(crate) fn empty_protect_model(locale: &LocaleManager) -> ProtectModel {
    base_model(
        locale,
        &ProtectPayload {
            tab: 0,
            rows: Vec::new(),
            drives: Vec::new(),
            passes: 1,
            wipe_percent: -1,
            status_key: String::new(),
            status_files: 0,
            status_bytes: 0,
            status_detail: String::new(),
            busy: false,
            scanned: false,
            clean_bytes: 0,
            free_bytes: 0,
        },
    )
}

pub(crate) fn build_protect_model(
    payload: &ProtectPayload,
    locale: &LocaleManager,
) -> ProtectModel {
    base_model(locale, payload)
}

pub(crate) fn patch_protect_model(
    model: &mut ProtectModel,
    payload: &ProtectPayload,
    locale: &LocaleManager,
) {
    *model = base_model(locale, payload);
}

fn base_model(locale: &LocaleManager, payload: &ProtectPayload) -> ProtectModel {
    let rows: Vec<ProtectRow> = payload
        .rows
        .iter()
        .map(|row| ProtectRow {
            id: row.id.clone().into(),
            title: label(locale, &row.title_key, &row.title),
            detail: label(locale, &row.detail_key, &row.detail),
            size: size_text(locale, &row.size_key, row.files, row.bytes),
            checked: row.checked,
        })
        .collect();
    let drives: Vec<ProtectDrive> = payload
        .drives
        .iter()
        .map(|drive| ProtectDrive {
            label: drive.label.clone().into(),
            selected: drive.selected,
        })
        .collect();
    let hint_key = match payload.tab {
        1 => "protect-hint-histories",
        2 => "protect-hint-wipe",
        3 => "protect-hint-network",
        _ => "protect-hint-traces",
    };
    ProtectModel {
        tab: payload.tab,
        tab_traces: locale.tr("protect-tab-traces").into(),
        tab_histories: locale.tr("protect-tab-histories").into(),
        tab_wipe: locale.tr("protect-tab-wipe").into(),
        tab_network: locale.tr("protect-tab-network").into(),
        hint: locale.tr(hint_key).into(),
        rows: ModelRc::new(VecModel::from(rows)),
        drives: ModelRc::new(VecModel::from(drives)),
        passes: payload.passes,
        wipe_progress: if payload.wipe_percent < 0 {
            -1.0
        } else {
            (payload.wipe_percent as f32 / 100.0).clamp(0.0, 1.0)
        },
        status: status_line(locale, payload),
        busy: payload.busy,
        scanned: payload.scanned,
        free_label: locale.format_byte_size(payload.free_bytes).into(),
        free_caption: locale.tr("protect-free-label").into(),
        scan_label: locale.tr("protect-scan").into(),
        clean_label: if payload.clean_bytes > 0 {
            locale
                .tr_args(
                    "protect-clean-sized",
                    &orchid_i18n::FluentArgs::new()
                        .with("bytes", locale.format_byte_size(payload.clean_bytes)),
                )
                .into()
        } else {
            locale.tr("protect-clean").into()
        },
        wipe_label: locale.tr("protect-wipe-start").into(),
        cancel_label: locale.tr("protect-cancel").into(),
        refresh_label: locale.tr("protect-refresh").into(),
        add_label: locale.tr("protect-add").into(),
        passes_label: locale.tr("protect-passes").into(),
    }
}

fn label(locale: &LocaleManager, key: &str, literal: &str) -> SharedString {
    if key.is_empty() {
        literal.to_string().into()
    } else {
        locale.tr(key).into()
    }
}

fn size_text(locale: &LocaleManager, key: &str, files: u64, bytes: u64) -> SharedString {
    if key.is_empty() {
        locale.format_byte_size(bytes).into()
    } else if key == "protect-files" {
        locale
            .tr_args(
                "protect-files",
                &orchid_i18n::FluentArgs::new().with("count", files.to_string()),
            )
            .into()
    } else {
        locale.tr(key).into()
    }
}

fn status_line(locale: &LocaleManager, payload: &ProtectPayload) -> SharedString {
    match payload.status_key.as_str() {
        "" => SharedString::default(),
        "protect-status-done" | "protect-status-browser" => locale
            .tr_args(
                &payload.status_key,
                &orchid_i18n::FluentArgs::new()
                    .with("files", payload.status_files.to_string())
                    .with("bytes", locale.format_byte_size(payload.status_bytes)),
            )
            .into(),
        "protect-status-error" => locale
            .tr_args(
                "protect-status-error",
                &orchid_i18n::FluentArgs::new().with("detail", payload.status_detail.as_str()),
            )
            .into(),
        key => locale.tr(key).into(),
    }
}
