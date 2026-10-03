use orchid_i18n::LocaleManager;
use orchid_widgets::AgentPayload;
use slint::{Model, ModelRc, VecModel};

use super::sync_eq_rows;
use crate::slint_generated::{AgentLine, AgentModel};

pub(crate) fn empty_agent_model(locale: &LocaleManager) -> AgentModel {
    base_model(
        locale,
        &AgentPayload {
            lines: Vec::new(),
            pending_path: String::new(),
            pending_preview: String::new(),
            status: String::new(),
        },
    )
}

pub(crate) fn build_agent_model(payload: &AgentPayload, locale: &LocaleManager) -> AgentModel {
    base_model(locale, payload)
}

/// Update an existing [`AgentModel`] in place, keeping the lines `ModelRc`.
pub(crate) fn patch_agent_model(
    model: &mut AgentModel,
    payload: &AgentPayload,
    locale: &LocaleManager,
) {
    let fresh = base_model(locale, payload);
    sync_eq_rows(&model.lines, rows_of(&fresh.lines));
    model.pending_path = fresh.pending_path;
    model.pending_preview = fresh.pending_preview;
    model.status = fresh.status;
    model.empty_label = fresh.empty_label;
    model.placeholder = fresh.placeholder;
    model.send_label = fresh.send_label;
    model.confirm_label = fresh.confirm_label;
    model.dismiss_label = fresh.dismiss_label;
    model.clear_label = fresh.clear_label;
    model.pending_label = fresh.pending_label;
}

fn base_model(locale: &LocaleManager, payload: &AgentPayload) -> AgentModel {
    let lines: Vec<AgentLine> = payload
        .lines
        .iter()
        .map(|line| AgentLine {
            role: role_label(locale, &line.role).into(),
            text: line.text.clone().into(),
        })
        .collect();
    AgentModel {
        lines: ModelRc::new(VecModel::from(lines)),
        pending_path: payload.pending_path.clone().into(),
        pending_preview: payload.pending_preview.clone().into(),
        status: status_label(locale, &payload.status).into(),
        empty_label: locale.tr("agent-empty").into(),
        placeholder: locale.tr("agent-placeholder").into(),
        send_label: locale.tr("agent-send").into(),
        confirm_label: locale.tr("agent-confirm-write").into(),
        dismiss_label: locale.tr("agent-dismiss-write").into(),
        clear_label: locale.tr("agent-clear").into(),
        pending_label: locale.tr("agent-pending").into(),
    }
}

fn rows_of(model: &ModelRc<AgentLine>) -> Vec<AgentLine> {
    let Some(rows) = model.as_any().downcast_ref::<VecModel<AgentLine>>() else {
        return Vec::new();
    };
    (0..rows.row_count())
        .filter_map(|index| rows.row_data(index))
        .collect()
}

fn role_label(locale: &LocaleManager, role: &str) -> String {
    match role {
        "user" => locale.tr("agent-role-user"),
        "tool" => locale.tr("agent-role-tool"),
        _ => locale.tr("agent-role-assistant"),
    }
}

fn status_label(locale: &LocaleManager, status: &str) -> String {
    match status {
        "" => String::new(),
        "working" => locale.tr("agent-working"),
        "closed" => locale.tr("agent-store-closed"),
        other => other.to_string(),
    }
}
