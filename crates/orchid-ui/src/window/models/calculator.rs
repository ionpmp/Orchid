use orchid_i18n::LocaleManager;
use orchid_widgets::CalculatorPayload;
use slint::{ModelRc, VecModel};

use crate::slint_generated::{CalcHistoryEntry, CalculatorModel};

pub(crate) fn empty_calculator_model(locale: &LocaleManager) -> CalculatorModel {
    base_model(
        locale,
        &CalculatorPayload {
            mode: 0,
            angle: 0,
            second: false,
            display: "0".into(),
            expression: String::new(),
            memory_set: false,
            error_key: None,
            history: Vec::new(),
            show_history: true,
        },
    )
}

pub(crate) fn build_calculator_model(
    p: &CalculatorPayload,
    locale: &LocaleManager,
) -> CalculatorModel {
    base_model(locale, p)
}

fn base_model(locale: &LocaleManager, p: &CalculatorPayload) -> CalculatorModel {
    let display_text = if let Some(key) = p.error_key {
        locale.tr(key)
    } else {
        p.display.clone()
    };
    let angle_label = match p.angle {
        1 => "RAD",
        2 => "GRAD",
        _ => "DEG",
    };
    let history: Vec<CalcHistoryEntry> = p
        .history
        .iter()
        .map(|h| CalcHistoryEntry {
            expression: h.expression.clone().into(),
            result: h.result.clone().into(),
        })
        .collect();
    CalculatorModel {
        mode: p.mode,
        angle: p.angle,
        second: p.second,
        display_text: display_text.into(),
        expression: p.expression.clone().into(),
        memory_set: p.memory_set,
        has_error: p.error_key.is_some(),
        show_history: p.show_history,
        history: ModelRc::new(VecModel::from(history)),
        mode_standard_label: locale.tr("calc-mode-standard").into(),
        mode_scientific_label: locale.tr("calc-mode-scientific").into(),
        angle_label: angle_label.into(),
        history_title: locale.tr("calc-history-title").into(),
        history_clear_label: locale.tr("calc-history-clear").into(),
        history_empty_label: locale.tr("calc-history-empty").into(),
    }
}
