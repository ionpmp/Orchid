use orchid_i18n::LocaleManager;
use slint::{Model, ModelRc, VecModel};

use crate::slint_generated::{RecentFileItemEntry, RecentFilesModel};

pub(crate) fn empty_recent_files_model(locale: &LocaleManager) -> RecentFilesModel {
    RecentFilesModel {
        items: ModelRc::new(VecModel::default()),
        has_items: false,
        empty_state_text: locale.tr("recent-files-empty").into(),
    }
}

pub(crate) fn build_recent_files_model(
    p: &orchid_widgets::RecentFilesPayload,
    locale: &LocaleManager,
) -> RecentFilesModel {
    let mut model = empty_recent_files_model(locale);
    patch_recent_files_model(&mut model, p);
    model
}

/// Update an existing [`RecentFilesModel`] in place.
pub(crate) fn patch_recent_files_model(
    model: &mut RecentFilesModel,
    p: &orchid_widgets::RecentFilesPayload,
) {
    let items: Vec<RecentFileItemEntry> = p
        .items
        .iter()
        .map(|it| RecentFileItemEntry {
            id: it.id.clone().into(),
            name: it.name.clone().into(),
            path: it.path.clone().into(),
            opened: it.opened_text.clone().into(),
        })
        .collect();
    model.has_items = !items.is_empty();
    sync_rows(&model.items, items);
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
