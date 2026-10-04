//! Slint model for the Optimize widget.

use orchid_i18n::LocaleManager;
use orchid_widgets::OptimizePayload;
use slint::{ModelRc, SharedString, VecModel};

use crate::slint_generated::{OptimizeModel, OptimizeRow};

pub(crate) fn empty_optimize_model(locale: &LocaleManager) -> OptimizeModel {
    base_model(
        locale,
        &OptimizePayload {
            tab: 0,
            rows: Vec::new(),
            query: String::new(),
            status_key: String::new(),
            unsupported: !cfg!(windows),
        },
    )
}

pub(crate) fn build_optimize_model(
    payload: &OptimizePayload,
    locale: &LocaleManager,
) -> OptimizeModel {
    base_model(locale, payload)
}

pub(crate) fn patch_optimize_model(
    model: &mut OptimizeModel,
    payload: &OptimizePayload,
    locale: &LocaleManager,
) {
    let next = base_model(locale, payload);
    model.tab = next.tab;
    model.rows = next.rows;
    model.query = next.query;
    model.hint = next.hint;
    model.tab_startup = next.tab_startup.clone();
    model.search_placeholder = next.search_placeholder.clone();
    model.search_empty = next.search_empty.clone();
    model.changed_label = next.changed_label.clone();
    model.restart_row_label = next.restart_row_label.clone();
    model.preset_windows = next.preset_windows.clone();
    model.preset_noreboot = next.preset_noreboot.clone();
    model.preset_quiet = next.preset_quiet.clone();
    model.status = next.status;
    model.refresh_label = next.refresh_label;
    model.restart_label = next.restart_label;
    model.admin_label = next.admin_label;
    model.unsupported = next.unsupported;
    model.unsupported_label = next.unsupported_label;
    model.tab_updates = next.tab_updates;
    model.tab_privacy = next.tab_privacy;
    model.tab_explorer = next.tab_explorer;
    model.tab_shell = next.tab_shell;
    model.tab_quiet = next.tab_quiet;
    model.tab_performance = next.tab_performance;
}

fn base_model(locale: &LocaleManager, payload: &OptimizePayload) -> OptimizeModel {
    let rows: Vec<OptimizeRow> = payload
        .rows
        .iter()
        .map(|row| {
            let mut choices = [
                SharedString::default(),
                SharedString::default(),
                SharedString::default(),
                SharedString::default(),
            ];
            for (index, key) in row.option_keys.iter().take(4).enumerate() {
                choices[index] = locale.tr(key).into();
            }
            let count = i32::try_from(row.option_keys.len().min(4)).unwrap_or(0);
            let title: SharedString = if row.title_text.is_empty() {
                locale.tr(&row.title_key).into()
            } else {
                row.title_text.clone().into()
            };
            let detail: SharedString = if row.detail_text.is_empty() {
                locale.tr(&row.detail_key).into()
            } else {
                row.detail_text.clone().into()
            };
            OptimizeRow {
                id: row.id.clone().into(),
                title,
                detail,
                choice_count: count,
                checked: row.selected == 1 && count == 0,
                choice: i32::from(row.selected),
                choice_0: choices[0].clone(),
                choice_1: choices[1].clone(),
                choice_2: choices[2].clone(),
                choice_3: choices[3].clone(),
                needs_admin: row.needs_admin,
                changed: row.changed,
                needs_restart: row.needs_restart,
                can_toggle: row.can_toggle,
            }
        })
        .collect();
    let status = if payload.status_key.is_empty() {
        SharedString::default()
    } else {
        locale.tr(&payload.status_key).into()
    };
    let hint_key = if payload.tab == 6 {
        "optimize-startup-hint"
    } else {
        "optimize-hint"
    };
    OptimizeModel {
        tab: payload.tab,
        tab_updates: locale.tr("optimize-tab-updates").into(),
        tab_privacy: locale.tr("optimize-tab-privacy").into(),
        tab_explorer: locale.tr("optimize-tab-explorer").into(),
        tab_shell: locale.tr("optimize-tab-shell").into(),
        tab_quiet: locale.tr("optimize-tab-quiet").into(),
        tab_performance: locale.tr("optimize-tab-performance").into(),
        tab_startup: locale.tr("optimize-tab-startup").into(),
        rows: ModelRc::new(VecModel::from(rows)),
        query: payload.query.clone().into(),
        hint: locale.tr(hint_key).into(),
        search_placeholder: locale.tr("optimize-search").into(),
        search_empty: locale.tr("optimize-search-empty").into(),
        changed_label: locale.tr("optimize-changed").into(),
        restart_row_label: locale.tr("optimize-needs-explorer").into(),
        preset_windows: locale.tr("optimize-preset-windows").into(),
        preset_noreboot: locale.tr("optimize-preset-noreboot").into(),
        preset_quiet: locale.tr("optimize-preset-quiet").into(),
        status,
        refresh_label: locale.tr("optimize-refresh").into(),
        restart_label: locale.tr("optimize-restart-explorer").into(),
        admin_label: locale.tr("optimize-admin").into(),
        unsupported: payload.unsupported,
        unsupported_label: locale.tr("optimize-unsupported").into(),
    }
}
