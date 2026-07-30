use orchid_i18n::LocaleManager;
use slint::{Model, ModelRc, SharedString, VecModel};

use super::super::errors::search_localized_error;
use crate::slint_generated::{SearchCandidateEntry, SearchModel};

pub(crate) fn empty_search_model(locale: &LocaleManager) -> SearchModel {
    SearchModel {
        query: SharedString::new(),
        candidates: ModelRc::new(VecModel::default()),
        is_searching: false,
        error: SharedString::new(),
        selected_index: -1,
        placeholder_text: locale.tr("search-placeholder").into(),
        empty_state_text: locale.tr("search-empty-state").into(),
        no_results_text: locale.tr("search-no-results-short").into(),
        searching_text: locale.tr("search-searching").into(),
        request_autofocus: false,
    }
}

pub(crate) fn build_search_model(
    p: &orchid_widgets::UniversalSearchPayload,
    locale: &LocaleManager,
    selected: i32,
    request_autofocus: bool,
) -> SearchModel {
    let mut model = empty_search_model(locale);
    patch_search_model(&mut model, p, locale, selected, request_autofocus);
    model
}

/// Update an existing [`SearchModel`] in place, keeping the candidates `ModelRc`.
pub(crate) fn patch_search_model(
    model: &mut SearchModel,
    p: &orchid_widgets::UniversalSearchPayload,
    locale: &LocaleManager,
    selected: i32,
    request_autofocus: bool,
) {
    let candidates = candidate_entries(p, locale);
    let max = candidates.len() as i32;
    let clamped = if candidates.is_empty() {
        -1
    } else {
        selected.clamp(0, max - 1)
    };
    sync_rows(&model.candidates, candidates);
    let error = p
        .error
        .as_deref()
        .map(|e| search_localized_error(locale, e))
        .unwrap_or_default();
    let no_results_text = if !p.query.trim().is_empty() {
        locale.tr_args(
            "search-no-results",
            &orchid_i18n::FluentArgs::new().with("query", p.query.clone()),
        )
    } else {
        locale.tr("search-no-results-short")
    };
    model.query = p.query.clone().into();
    model.is_searching = p.is_searching;
    model.error = error.into();
    model.selected_index = clamped;
    model.no_results_text = no_results_text.into();
    model.request_autofocus = request_autofocus;
}

fn candidate_entries(
    p: &orchid_widgets::UniversalSearchPayload,
    locale: &LocaleManager,
) -> Vec<SearchCandidateEntry> {
    p.candidates
        .iter()
        .map(|c| {
            let title: SharedString = match c.source_name.as_str() {
                "commands" | "settings" => locale.tr(c.title.as_str()).into(),
                _ => c.title.clone().into(),
            };
            let source_label = match c.source_name.as_str() {
                "files" => locale.tr("search-source-files"),
                "commands" => locale.tr("search-source-commands"),
                "settings" => locale.tr("search-source-settings"),
                "calculator" => locale.tr("search-source-calculator"),
                _ => c.source_name.clone(),
            };
            let subtitle: SharedString = match &c.subtitle {
                Some(s) => s.clone().into(),
                None => source_label.clone().into(),
            };
            SearchCandidateEntry {
                id: c.id.clone().into(),
                source_name: source_label.into(),
                source_icon: c.source_name.as_str().into(),
                title,
                subtitle,
                shortcut: c.shortcut_hint.clone().unwrap_or_default().into(),
            }
        })
        .collect()
}

fn sync_rows<T: Clone + PartialEq + 'static>(model: &ModelRc<T>, new_rows: Vec<T>) {
    let Some(v) = model.as_any().downcast_ref::<VecModel<T>>() else {
        return;
    };
    while v.row_count() > new_rows.len() {
        v.remove(v.row_count() - 1);
    }
    for (i, row) in new_rows.into_iter().enumerate() {
        if i < v.row_count() {
            if let Some(old) = v.row_data(i) {
                if old == row {
                    continue;
                }
            }
            v.set_row_data(i, row);
        } else {
            v.push(row);
        }
    }
}
