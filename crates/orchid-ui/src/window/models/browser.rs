use orchid_i18n::LocaleManager;
use orchid_widgets::BrowserPayload;
use slint::{Model, ModelRc, VecModel};

use crate::html_webview::HtmlWebViewHost;
use crate::slint_generated::{BrowserModel, BrowserTabEntry};

pub(crate) fn empty_browser_model(locale: &LocaleManager) -> BrowserModel {
    base_model(
        locale,
        &BrowserPayload {
            tabs: Vec::new(),
            active_index: 0,
            url: String::new(),
            title: String::new(),
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
    model.active_index = p.active_index;
    model.url = p.url.clone().into();
    model.title = p.title.clone().into();
    model.webview_available = HtmlWebViewHost::runtime_available();
    model.untitled_label = locale.tr("browser-untitled").into();
    model.address_placeholder = locale.tr("browser-address-placeholder").into();
    model.back_label = locale.tr("browser-back").into();
    model.forward_label = locale.tr("browser-forward").into();
    model.reload_label = locale.tr("browser-reload").into();
    model.go_label = locale.tr("browser-go").into();
    model.new_tab_tip = locale.tr("browser-new-tab").into();
    model.unavailable_label = locale.tr("browser-unavailable").into();
    model.open_label = locale.tr("browser-open-external").into();
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
    BrowserModel {
        tabs: ModelRc::new(VecModel::from(tabs)),
        active_index: p.active_index,
        url: p.url.clone().into(),
        title: p.title.clone().into(),
        can_go_back: false,
        can_go_forward: false,
        webview_available: HtmlWebViewHost::runtime_available(),
        untitled_label: locale.tr("browser-untitled").into(),
        address_placeholder: locale.tr("browser-address-placeholder").into(),
        back_label: locale.tr("browser-back").into(),
        forward_label: locale.tr("browser-forward").into(),
        reload_label: locale.tr("browser-reload").into(),
        go_label: locale.tr("browser-go").into(),
        new_tab_tip: locale.tr("browser-new-tab").into(),
        unavailable_label: locale.tr("browser-unavailable").into(),
        open_label: locale.tr("browser-open-external").into(),
    }
}
