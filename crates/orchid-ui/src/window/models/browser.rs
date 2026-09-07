use orchid_i18n::LocaleManager;
use orchid_widgets::BrowserPayload;
use slint::{Image, Model, ModelRc, VecModel};

use crate::html_webview::HtmlWebViewHost;
use crate::slint_generated::{
    BrowserBookmarkEntry, BrowserDownloadEntry, BrowserModel, BrowserTabEntry,
};

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
            downloads: Vec::new(),
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
        .map(|t| tab_entry(t, icon_for(&model.tabs, t.id.as_str())))
        .collect();
    sync_tab_rows(&model.tabs, tabs);
    sync_rows(&model.bookmarks, bookmark_rows(p));
    sync_rows(&model.downloads, download_rows(p, locale));
    let tab_changed = model.active_index != p.active_index;
    model.active_index = p.active_index;
    model.url = p.url.clone().into();
    model.title = p.title.clone().into();
    model.is_bookmarked = p.is_bookmarked;
    model.has_active_download = p.downloads.iter().any(|d| d.in_progress);
    model.webview_available = HtmlWebViewHost::runtime_available();
    apply_chrome_labels(model, locale);
    if tab_changed {
        model.can_go_back = false;
        model.can_go_forward = false;
        model.is_loading = false;
        model.zoom_percent = 100;
    }
}

fn download_rows(p: &BrowserPayload, locale: &LocaleManager) -> Vec<BrowserDownloadEntry> {
    p.downloads
        .iter()
        .map(|d| {
            let status = if d.in_progress {
                locale.tr("browser-download-progress")
            } else if d.state == 1 {
                locale.tr("browser-download-done")
            } else {
                locale.tr("browser-download-failed")
            };
            BrowserDownloadEntry {
                id: d.id.clone().into(),
                filename: d.filename.clone().into(),
                path: d.path.clone().into(),
                progress: d.progress,
                state: d.state,
                in_progress: d.in_progress,
                status_label: status.into(),
            }
        })
        .collect()
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
    model.downloads_label = locale.tr("browser-downloads").into();
    model.downloads_empty = locale.tr("browser-downloads-empty").into();
    model.download_open_label = locale.tr("browser-download-open").into();
    model.download_show_label = locale.tr("browser-download-show").into();
    model.download_cancel_label = locale.tr("browser-download-cancel").into();
    model.download_remove_tip = locale.tr("browser-download-remove").into();
    model.download_progress_label = locale.tr("browser-download-progress").into();
    model.download_done_label = locale.tr("browser-download-done").into();
    model.download_failed_label = locale.tr("browser-download-failed").into();
}

fn sync_tab_rows(model: &ModelRc<BrowserTabEntry>, new_rows: Vec<BrowserTabEntry>) {
    let Some(v) = model.as_any().downcast_ref::<VecModel<BrowserTabEntry>>() else {
        return;
    };
    while v.row_count() > new_rows.len() {
        v.remove(v.row_count() - 1);
    }
    for (i, row) in new_rows.into_iter().enumerate() {
        if i < v.row_count() {
            v.set_row_data(i, row);
        } else {
            v.push(row);
        }
    }
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
    let tabs: Vec<BrowserTabEntry> = p.tabs.iter().map(|t| tab_entry(t, None)).collect();
    let mut model = BrowserModel {
        tabs: ModelRc::new(VecModel::from(tabs)),
        bookmarks: ModelRc::new(VecModel::from(bookmark_rows(p))),
        downloads: ModelRc::new(VecModel::from(download_rows(p, locale))),
        active_index: p.active_index,
        url: p.url.clone().into(),
        title: p.title.clone().into(),
        can_go_back: false,
        can_go_forward: false,
        is_loading: false,
        is_bookmarked: p.is_bookmarked,
        webview_available: HtmlWebViewHost::runtime_available(),
        has_active_download: p.downloads.iter().any(|d| d.in_progress),
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
        downloads_label: Default::default(),
        downloads_empty: Default::default(),
        download_open_label: Default::default(),
        download_show_label: Default::default(),
        download_cancel_label: Default::default(),
        download_remove_tip: Default::default(),
        download_progress_label: Default::default(),
        download_done_label: Default::default(),
        download_failed_label: Default::default(),
        focus_address_gen: 0,
        show_find_gen: 0,
        show_downloads_gen: 0,
        zoom_percent: 100,
    };
    apply_chrome_labels(&mut model, locale);
    model
}

pub(crate) fn apply_browser_favicon(model: &mut BrowserModel, tab_id: &str, png: &[u8]) {
    let Some(icon) = decode_favicon(png) else {
        return;
    };
    let Some(v) = model
        .tabs
        .as_any()
        .downcast_ref::<VecModel<BrowserTabEntry>>()
    else {
        return;
    };
    for i in 0..v.row_count() {
        let Some(mut row) = v.row_data(i) else {
            continue;
        };
        if row.id.as_str() != tab_id {
            continue;
        }
        row.has_icon = true;
        row.icon = icon;
        v.set_row_data(i, row);
        return;
    }
}

fn tab_entry(t: &orchid_widgets::BrowserTabRow, icon: Option<(bool, Image)>) -> BrowserTabEntry {
    let (has_icon, icon) = icon.unwrap_or((false, Image::default()));
    BrowserTabEntry {
        id: t.id.clone().into(),
        title: t.title.clone().into(),
        url: t.url.clone().into(),
        is_active: t.is_active,
        has_icon,
        icon,
    }
}

fn icon_for(model: &ModelRc<BrowserTabEntry>, id: &str) -> Option<(bool, Image)> {
    let v = model.as_any().downcast_ref::<VecModel<BrowserTabEntry>>()?;
    for i in 0..v.row_count() {
        let Some(row) = v.row_data(i) else {
            continue;
        };
        if row.id.as_str() == id {
            return Some((row.has_icon, row.icon));
        }
    }
    None
}

fn decode_favicon(png: &[u8]) -> Option<Image> {
    let dyn_img = image::load_from_memory(png).ok()?;
    let resized = dyn_img.resize_exact(16, 16, image::imageops::FilterType::Triangle);
    let rgba = resized.into_rgba8();
    let buf =
        slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(rgba.as_raw(), 16, 16);
    Some(Image::from_rgba8(buf))
}
