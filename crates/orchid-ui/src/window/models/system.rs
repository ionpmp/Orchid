use orchid_i18n::LocaleManager;
use slint::{Model, ModelRc, SharedString, VecModel};

use crate::slint_generated::{SystemIndicatorEntry, SystemModel};

pub(crate) fn empty_system_model(locale: &LocaleManager) -> SystemModel {
    SystemModel {
        indicators: ModelRc::new(VecModel::from(vec![SystemIndicatorEntry {
            label: locale.tr("system-loading").into(),
            value_text: SharedString::new(),
            percent: -1.0,
            icon: SharedString::new(),
            status: 0,
            status_hint: SharedString::new(),
            segments: ModelRc::new(VecModel::from(Vec::<f32>::new())),
        }])),
    }
}

pub(crate) fn build_system_model(
    p: &orchid_widgets::SystemPayload,
    locale: &LocaleManager,
) -> SystemModel {
    SystemModel {
        indicators: ModelRc::new(VecModel::from(indicator_entries(p, locale))),
    }
}

/// Update an existing [`SystemModel`] in place (keep the same `ModelRc`s).
///
/// Replacing `indicators` with a fresh `ModelRc` on every sample leaves Slint
/// bound to the old list handle, so CPU/memory text looks frozen.
pub(crate) fn patch_system_model(
    model: &mut SystemModel,
    p: &orchid_widgets::SystemPayload,
    locale: &LocaleManager,
) {
    sync_system_indicators(&model.indicators, indicator_entries(p, locale));
}

fn indicator_entries(
    p: &orchid_widgets::SystemPayload,
    locale: &LocaleManager,
) -> Vec<SystemIndicatorEntry> {
    if p.is_loading {
        return vec![SystemIndicatorEntry {
            label: locale.tr("system-loading").into(),
            value_text: SharedString::new(),
            percent: -1.0,
            icon: SharedString::new(),
            status: 0,
            status_hint: SharedString::new(),
            segments: ModelRc::new(VecModel::from(Vec::<f32>::new())),
        }];
    }
    if p.indicators.is_empty() {
        return vec![SystemIndicatorEntry {
            label: locale.tr("system-empty").into(),
            value_text: SharedString::new(),
            percent: -1.0,
            icon: SharedString::new(),
            status: 0,
            status_hint: SharedString::new(),
            segments: ModelRc::new(VecModel::from(Vec::<f32>::new())),
        }];
    }
    p.indicators
        .iter()
        .map(|i| {
            let label = system_indicator_label(i, locale);
            let value_text = system_indicator_value(i, locale);
            let status = indicator_status_to_int(&i.status);
            let status_hint = system_indicator_status_hint(locale, &label, &value_text, status);
            let segments: Vec<f32> = i
                .segments
                .iter()
                .map(|pct| (*pct / 100.0).clamp(0.0, 1.0))
                .collect();
            SystemIndicatorEntry {
                label,
                value_text,
                percent: i
                    .percent
                    .map(|pct| (pct / 100.0).clamp(0.0, 1.0))
                    .unwrap_or(-1.0),
                icon: i.icon.into(),
                status,
                status_hint,
                segments: ModelRc::new(VecModel::from(segments)),
            }
        })
        .collect()
}

fn sync_system_indicators(
    model: &ModelRc<SystemIndicatorEntry>,
    new_rows: Vec<SystemIndicatorEntry>,
) {
    let Some(v) = model
        .as_any()
        .downcast_ref::<VecModel<SystemIndicatorEntry>>()
    else {
        return;
    };
    while v.row_count() > new_rows.len() {
        v.remove(v.row_count() - 1);
    }
    for (i, new_row) in new_rows.into_iter().enumerate() {
        if i < v.row_count() {
            let Some(mut old) = v.row_data(i) else {
                v.set_row_data(i, new_row);
                continue;
            };
            sync_f32_segments(&old.segments, segment_values(&new_row.segments));
            old.label = new_row.label;
            old.value_text = new_row.value_text;
            old.percent = new_row.percent;
            old.icon = new_row.icon;
            old.status = new_row.status;
            old.status_hint = new_row.status_hint;
            // Keep `old.segments` ModelRc — only its rows were synced above.
            v.set_row_data(i, old);
        } else {
            v.push(new_row);
        }
    }
}

fn segment_values(model: &ModelRc<f32>) -> Vec<f32> {
    let Some(v) = model.as_any().downcast_ref::<VecModel<f32>>() else {
        return Vec::new();
    };
    (0..v.row_count()).filter_map(|i| v.row_data(i)).collect()
}

fn sync_f32_segments(model: &ModelRc<f32>, new_rows: Vec<f32>) {
    let Some(v) = model.as_any().downcast_ref::<VecModel<f32>>() else {
        return;
    };
    while v.row_count() > new_rows.len() {
        v.remove(v.row_count() - 1);
    }
    for (i, row) in new_rows.into_iter().enumerate() {
        if i < v.row_count() {
            if let Some(old) = v.row_data(i) {
                if (old - row).abs() <= f32::EPSILON {
                    continue;
                }
            }
            v.set_row_data(i, row);
        } else {
            v.push(row);
        }
    }
}

fn system_indicator_status_hint(
    locale: &LocaleManager,
    label: &SharedString,
    value_text: &SharedString,
    status: i32,
) -> SharedString {
    let key = match status {
        2 => "system-status-critical",
        1 => "system-status-warning",
        _ => return SharedString::new(),
    };
    locale
        .tr_args(
            key,
            &orchid_i18n::FluentArgs::new()
                .with("label", label.as_str())
                .with("value", value_text.as_str()),
        )
        .into()
}

fn system_indicator_label(
    i: &orchid_widgets::SystemIndicator,
    locale: &LocaleManager,
) -> SharedString {
    use orchid_widgets::SystemIndicatorKind;
    match i.kind {
        SystemIndicatorKind::Cpu => locale.tr("system-cpu-label").into(),
        SystemIndicatorKind::Memory => locale.tr("system-memory-label").into(),
        SystemIndicatorKind::Disk => locale
            .tr_args(
                "system-disk-label",
                &orchid_i18n::FluentArgs::new()
                    .with("mount", i.name_suffix.clone().unwrap_or_default()),
            )
            .into(),
        SystemIndicatorKind::Network => match &i.name_suffix {
            Some(name) if !name.is_empty() => locale
                .tr_args(
                    "system-network-label",
                    &orchid_i18n::FluentArgs::new().with("name", name.clone()),
                )
                .into(),
            _ => locale.tr("system-network-total-label").into(),
        },
        SystemIndicatorKind::Battery => locale.tr("system-battery-label").into(),
        SystemIndicatorKind::Uptime => locale.tr("system-uptime-label").into(),
    }
}

fn system_indicator_value(
    i: &orchid_widgets::SystemIndicator,
    locale: &LocaleManager,
) -> SharedString {
    use orchid_widgets::SystemIndicatorKind;
    if i.kind == SystemIndicatorKind::Network {
        locale
            .tr_args(
                "system-network-rate",
                &orchid_i18n::FluentArgs::new()
                    .with("up", i.network_up.clone().unwrap_or_default())
                    .with("down", i.network_down.clone().unwrap_or_default()),
            )
            .into()
    } else {
        i.value_text.clone().into()
    }
}

fn indicator_status_to_int(s: &orchid_widgets::IndicatorStatus) -> i32 {
    use orchid_widgets::IndicatorStatus::*;
    match s {
        Normal => 0,
        Warning => 1,
        Critical => 2,
    }
}
