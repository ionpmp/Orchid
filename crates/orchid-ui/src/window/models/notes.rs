use orchid_i18n::LocaleManager;
use orchid_widgets::NotesPayload;
use slint::{ModelRc, VecModel};

use crate::slint_generated::{NotesModel, NotesTabEntry};

pub(crate) fn empty_notes_model(locale: &LocaleManager) -> NotesModel {
    base_model(
        locale,
        &NotesPayload {
            tabs: Vec::new(),
            active_index: 0,
            title: String::new(),
            body: String::new(),
            font_size: 14,
            word_wrap: true,
            mono_font: false,
            show_status_bar: true,
            char_count: 0,
            word_count: 0,
            line_count: 1,
            find_gen: 0,
            find_cursor: 0,
            find_anchor: 0,
        },
    )
}

pub(crate) fn build_notes_model(p: &NotesPayload, locale: &LocaleManager) -> NotesModel {
    base_model(locale, p)
}

fn base_model(locale: &LocaleManager, p: &NotesPayload) -> NotesModel {
    let tabs: Vec<NotesTabEntry> = p
        .tabs
        .iter()
        .map(|t| NotesTabEntry {
            id: t.id.clone().into(),
            title: t.title.clone().into(),
            is_active: t.is_active,
        })
        .collect();
    let stats_label = locale.tr_args(
        "notes-stats",
        &orchid_i18n::FluentArgs::new()
            .with("chars", p.char_count.to_string())
            .with("words", p.word_count.to_string())
            .with("lines", p.line_count.to_string()),
    );
    let font_size_label = locale.tr_args(
        "notes-font-size",
        &orchid_i18n::FluentArgs::new().with("size", p.font_size.to_string()),
    );
    NotesModel {
        tabs: ModelRc::new(VecModel::from(tabs)),
        active_index: p.active_index,
        title: p.title.clone().into(),
        body: p.body.clone().into(),
        font_size: p.font_size,
        word_wrap: p.word_wrap,
        mono_font: p.mono_font,
        show_status_bar: p.show_status_bar,
        char_count: p.char_count,
        word_count: p.word_count,
        line_count: p.line_count,
        find_gen: p.find_gen,
        find_cursor: p.find_cursor,
        find_anchor: p.find_anchor,
        stats_label: stats_label.into(),
        font_size_label: font_size_label.into(),
        untitled_label: locale.tr("notes-untitled").into(),
        title_placeholder: locale.tr("notes-title-placeholder").into(),
        wrap_label: locale.tr("notes-wrap").into(),
        mono_label: locale.tr("notes-mono").into(),
        find_label: locale.tr("notes-find").into(),
        find_placeholder: locale.tr("notes-find-placeholder").into(),
        clear_label: locale.tr("notes-clear").into(),
        autosave_label: locale.tr("notes-autosave").into(),
        tip_new_tab: locale.tr("notes-tip-new-tab").into(),
        tip_wrap: locale.tr("notes-tip-wrap").into(),
        tip_mono: locale.tr("notes-tip-mono").into(),
        tip_zoom_in: locale.tr("notes-tip-zoom-in").into(),
        tip_zoom_out: locale.tr("notes-tip-zoom-out").into(),
        tip_find: locale.tr("notes-tip-find").into(),
        tip_find_next: locale.tr("notes-tip-find-next").into(),
        tip_find_prev: locale.tr("notes-tip-find-prev").into(),
        tip_find_close: locale.tr("notes-tip-find-close").into(),
        tip_clear: locale.tr("notes-tip-clear").into(),
    }
}
