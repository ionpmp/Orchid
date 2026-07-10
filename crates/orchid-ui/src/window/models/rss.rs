use orchid_i18n::LocaleManager;
use slint::{ModelRc, SharedString, VecModel};

use crate::slint_generated::{RssItemEntry, RssModel};

pub(crate) fn empty_rss_model(locale: &LocaleManager) -> RssModel {
    RssModel {
        items: ModelRc::new(VecModel::default()),
        last_updated: SharedString::new(),
        error_summary: SharedString::new(),
        has_items: false,
        empty_state_text: locale.tr("rss-loading").into(),
    }
}

pub(crate) fn build_rss_model(p: &orchid_widgets::RssPayload, locale: &LocaleManager) -> RssModel {
    let items: Vec<RssItemEntry> = p
        .items
        .iter()
        .map(|it| RssItemEntry {
            id: it.id.clone().into(),
            title: it.title.clone().into(),
            source: it.source_name.clone().into(),
            published: it.published_text.clone().into(),
            summary: it.summary_text.clone().unwrap_or_default().into(),
            link: it.link.clone().unwrap_or_default().into(),
        })
        .collect();
    let has_items = !items.is_empty();
    let error_summary = if p.failed_feed_count > 0 {
        locale
            .tr_args(
                "rss-error-summary",
                &orchid_i18n::FluentArgs::new()
                    .with("n", p.failed_feed_count.to_string())
                    .with("total", p.enabled_feed_count.to_string()),
            )
            .into()
    } else {
        SharedString::new()
    };
    let empty_state_text = if p.is_loading {
        locale.tr("rss-loading").into()
    } else if p.enabled_feed_count == 0 {
        locale.tr("rss-no-feeds").into()
    } else if p.failed_feed_count > 0 && !has_items {
        locale.tr("rss-fetch-failed").into()
    } else if !has_items {
        locale.tr("rss-empty").into()
    } else {
        locale.tr("rss-no-feeds").into()
    };
    RssModel {
        items: ModelRc::new(VecModel::from(items)),
        last_updated: p.last_updated_text.clone().into(),
        error_summary,
        has_items,
        empty_state_text,
    }
}
