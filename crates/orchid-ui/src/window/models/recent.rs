use orchid_i18n::LocaleManager;
use slint::{ModelRc, VecModel};

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
    let has_items = !items.is_empty();
    RecentFilesModel {
        items: ModelRc::new(VecModel::from(items)),
        has_items,
        empty_state_text: locale.tr("recent-files-empty").into(),
    }
}
