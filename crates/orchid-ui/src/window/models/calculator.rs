use orchid_i18n::LocaleManager;
use orchid_widgets::CalculatorPayload;
use slint::{Model, ModelRc, VecModel};

use crate::slint_generated::{CalcHistoryEntry, CalculatorModel};

pub(crate) fn empty_calculator_model(locale: &LocaleManager) -> CalculatorModel {
    CalculatorModel {
        mode: 0,
        angle: 0,
        second: false,
        display_text: "0".into(),
        expression: "".into(),
        memory_set: false,
        has_error: false,
        show_history: true,
        history: ModelRc::new(VecModel::default()),
        mode_standard_label: locale.tr("calc-mode-standard").into(),
        mode_scientific_label: locale.tr("calc-mode-scientific").into(),
        angle_label: locale.tr("calc-angle-short-deg").into(),
        history_title: locale.tr("calc-history-title").into(),
        history_clear_label: locale.tr("calc-history-clear").into(),
        history_empty_label: locale.tr("calc-history-empty").into(),
        tip_mc: locale.tr("calc-tip-mc").into(),
        tip_mr: locale.tr("calc-tip-mr").into(),
        tip_mplus: locale.tr("calc-tip-mplus").into(),
        tip_mminus: locale.tr("calc-tip-mminus").into(),
        tip_ms: locale.tr("calc-tip-ms").into(),
        tip_ce: locale.tr("calc-tip-ce").into(),
        tip_c: locale.tr("calc-tip-c").into(),
        tip_bs: locale.tr("calc-tip-bs").into(),
        tip_neg: locale.tr("calc-tip-neg").into(),
        tip_eq: locale.tr("calc-tip-eq").into(),
        tip_2nd: locale.tr("calc-tip-2nd").into(),
        tip_angle: locale.tr("calc-tip-angle").into(),
    }
}

pub(crate) fn build_calculator_model(
    p: &CalculatorPayload,
    locale: &LocaleManager,
) -> CalculatorModel {
    let mut model = empty_calculator_model(locale);
    patch_calculator_model(&mut model, p, locale);
    model
}

/// Update an existing [`CalculatorModel`] in place, keeping the history `ModelRc`.
pub(crate) fn patch_calculator_model(
    model: &mut CalculatorModel,
    p: &CalculatorPayload,
    locale: &LocaleManager,
) {
    let display_text = if let Some(key) = p.error_key {
        locale.tr(key)
    } else {
        p.display.clone()
    };
    let angle_label = match p.angle {
        1 => locale.tr("calc-angle-short-rad"),
        2 => locale.tr("calc-angle-short-grad"),
        _ => locale.tr("calc-angle-short-deg"),
    };
    let history: Vec<CalcHistoryEntry> = p
        .history
        .iter()
        .map(|h| CalcHistoryEntry {
            expression: h.expression.clone().into(),
            result: h.result.clone().into(),
        })
        .collect();
    sync_history(&model.history, history);
    model.mode = p.mode;
    model.angle = p.angle;
    model.second = p.second;
    model.display_text = display_text.into();
    model.expression = p.expression.clone().into();
    model.memory_set = p.memory_set;
    model.has_error = p.error_key.is_some();
    model.show_history = p.show_history;
    model.angle_label = angle_label.into();
}

fn sync_history(model: &ModelRc<CalcHistoryEntry>, new_rows: Vec<CalcHistoryEntry>) {
    let Some(v) = model.as_any().downcast_ref::<VecModel<CalcHistoryEntry>>() else {
        return;
    };
    while v.row_count() > new_rows.len() {
        v.remove(v.row_count() - 1);
    }
    for (i, row) in new_rows.into_iter().enumerate() {
        if i < v.row_count() {
            if let Some(old) = v.row_data(i) {
                if old.expression == row.expression && old.result == row.result {
                    continue;
                }
            }
            v.set_row_data(i, row);
        } else {
            v.push(row);
        }
    }
}
