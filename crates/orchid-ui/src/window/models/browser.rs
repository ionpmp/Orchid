use orchid_i18n::LocaleManager;
use orchid_widgets::BrowserPayload;
use slint::{Model, ModelRc, VecModel};

use crate::html_webview::HtmlWebViewHost;
use crate::slint_generated::{BrowserBookmarkEntry, BrowserModel, BrowserTabEntry};

pub(crate) fn empty_browser_model(locale: &LocaleManager) -> BrowserModel {
    base_model(
        locale,
        &BrowserPayload {
            tabs: Vec::new(),
            active_index: 0,
            url: String::new(),
            title: String::new(),
            homepage: String::new(),
            bookmarks: Vec::new(),
            is_bookmarked: false,
        },
    )
}

pub(crate) fn build_browser_model(p: &BrowserPayload, locale: &LocaleManager) -> BrowserModel {
    base_model(locale, p)
}

/// Update an existing [`BrowserModel`] in place, keeping the tabs `ModelRc`.
pub(crate) fn patch_browser_model(
    model: &mut BrowserModel,
    p: &BrowserPayload,
    locale: &LocaleManager,
) {
    let tabs: Vec<BrowserTabEntry> = p
        .tabs
        .iter()
        .map(|t| BrowserTabEntry {
            id: t.id.clone().into(),
            title: t.title.clone().into(),
            url: t.url.clone().into(),
            is_active: t.is_active,
        })
        .collect();
    sync_rows(&model.tabs, tabs);
    sync_rows(&model.bookmarks, bookmark_rows(p));
    let tab_changed = model.active_index != p.active_index;
    model.active_index = p.active_index;
    model.url = p.url.clone().into();
    model.title = p.title.clone().into();
    model.is_bookmarked = p.is_bookmarked;
    model.webview_available = HtmlWebViewHost::runtime_available();
    apply_chrome_labels(model, locale);
    if tab_changed {
        model.can_go_back = false;
        model.can_go_forward = false;
        model.is_loading = false;
        model.zoom_percent = 100;
    }
}

fn bookmark_rows(p: &BrowserPayload) -> Vec<BrowserBookmarkEntry> {
    p.bookmarks
        .iter()
        .map(|b| BrowserBookmarkEntry {
            title: b.title.clone().into(),
            url: b.url.clone().into(),
        })
        .collect()
}

fn apply_chrome_labels(model: &mut BrowserModel, locale: &LocaleManager) {
    model.untitled_label = locale.tr("browser-untitled").into();
    model.address_placeholder = locale.tr("browser-address-placeholder").into();
    model.back_label = locale.tr("browser-back").into();
    model.forward_label = locale.tr("browser-forward").into();
    model.reload_label = locale.tr("browser-reload").into();
    model.stop_label = locale.tr("browser-stop").into();
    model.home_label = locale.tr("browser-home").into();
    model.go_label = locale.tr("browser-go").into();
    model.new_tab_tip = locale.tr("browser-new-tab").into();
    model.unavailable_label = locale.tr("browser-unavailable").into();
    model.open_label = locale.tr("browser-open-external").into();
    model.bookmark_tip = locale.tr("browser-bookmark").into();
    model.bookmarks_label = locale.tr("browser-bookmarks").into();
    model.bookmark_remove_tip = locale.tr("browser-bookmark-remove").into();
    model.bookmarks_empty = locale.tr("browser-bookmarks-empty").into();
    model.find_label = locale.tr("browser-find").into();
    model.find_placeholder = locale.tr("browser-find-placeholder").into();
    model.find_prev_tip = locale.tr("browser-find-prev").into();
    model.find_next_tip = locale.tr("browser-find-next").into();
    model.find_close_tip = locale.tr("browser-find-close").into();
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

fn base_model(locale: &LocaleManager, p: &BrowserPayload) -> BrowserModel {
    let tabs: Vec<BrowserTabEntry> = p
        .tabs
        .iter()
        .map(|t| BrowserTabEntry {
            id: t.id.clone().into(),
            title: t.title.clone().into(),
            url: t.url.clone().into(),
            is_active: t.is_active,
        })
        .collect();
    let mut model = BrowserModel {
        tabs: ModelRc::new(VecModel::from(tabs)),
        bookmarks: ModelRc::new(VecModel::from(bookmark_rows(p))),
        active_index: p.active_index,
        url: p.url.clone().into(),
        title: p.title.clone().into(),
        can_go_back: false,
        can_go_forward: false,
        is_loading: false,
        is_bookmarked: p.is_bookmarked,
        webview_available: HtmlWebViewHost::runtime_available(),
        untitled_label: Default::default(),
        address_placeholder: Default::default(),
        back_label: Default::default(),
        forward_label: Default::default(),
        reload_label: Default::default(),
        stop_label: Default::default(),
        home_label: Default::default(),
        go_label: Default::default(),
        new_tab_tip: Default::default(),
        unavailable_label: Default::default(),
        open_label: Default::default(),
        bookmark_tip: Default::default(),
        bookmarks_label: Default::default(),
        bookmark_remove_tip: Default::default(),
        bookmarks_empty: Default::default(),
        find_label: Default::default(),
        find_placeholder: Default::default(),
        find_prev_tip: Default::default(),
        find_next_tip: Default::default(),
        find_close_tip: Default::default(),
        focus_address_gen: 0,
        show_find_gen: 0,
        zoom_percent: 100,
    };
    apply_chrome_labels(&mut model, locale);
    model
}
