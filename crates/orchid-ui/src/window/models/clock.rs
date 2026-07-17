use orchid_i18n::LocaleManager;
use slint::{ModelRc, SharedString, VecModel};

use crate::slint_generated::{ClockCityEntry, ClockModel, ClockSearchHit};

pub(crate) fn empty_clock_model(locale: &LocaleManager) -> ClockModel {
    ClockModel {
        cities: ModelRc::new(VecModel::default()),
        picker_open: false,
        search_query: SharedString::new(),
        search_results: ModelRc::new(VecModel::default()),
        search_busy: false,
        local_label: locale.tr("clock-local-label").into(),
        add_cities_label: locale.tr("clock-add-cities").into(),
        cities_hint: locale.tr("clock-cities-hint").into(),
        picker_title: locale.tr("clock-picker-title").into(),
        search_placeholder: locale.tr("clock-search-placeholder").into(),
        add_city_hint: locale.tr("clock-add-city-hint").into(),
        remove_city_hint: locale.tr("clock-remove-city-hint").into(),
        close_picker_label: locale.tr("clock-close-picker").into(),
        no_results_label: locale.tr("clock-no-results").into(),
        searching_label: locale.tr("clock-searching").into(),
    }
}

pub(crate) fn build_clock_model(
    p: &orchid_widgets::ClockPayload,
    locale: &LocaleManager,
) -> ClockModel {
    let cities: Vec<ClockCityEntry> = p
        .cities
        .iter()
        .map(|c| {
            let name = if c.is_local {
                locale.tr("clock-local-label")
            } else {
                c.name.clone()
            };
            let day_label = match c.day_offset {
                -1 => locale.tr("clock-day-yesterday"),
                1 => locale.tr("clock-day-tomorrow"),
                _ => String::new(),
            };
            ClockCityEntry {
                name: name.into(),
                timezone: c.timezone.clone().into(),
                time_text: c.time_text.clone().into(),
                date_text: c.date_text.clone().into(),
                offset_text: c.offset_text.clone().into(),
                day_label: day_label.into(),
                is_local: c.is_local,
            }
        })
        .collect();

    let search_results: Vec<ClockSearchHit> = p
        .search_results
        .iter()
        .map(|h| ClockSearchHit {
            name: h.name.clone().into(),
            detail: h.detail.clone().into(),
            timezone: h.timezone.clone().into(),
        })
        .collect();

    ClockModel {
        cities: ModelRc::new(VecModel::from(cities)),
        picker_open: p.picker_open,
        search_query: p.search_query.clone().into(),
        search_results: ModelRc::new(VecModel::from(search_results)),
        search_busy: p.search_busy,
        local_label: locale.tr("clock-local-label").into(),
        add_cities_label: locale.tr("clock-add-cities").into(),
        cities_hint: locale.tr("clock-cities-hint").into(),
        picker_title: locale.tr("clock-picker-title").into(),
        search_placeholder: locale.tr("clock-search-placeholder").into(),
        add_city_hint: locale.tr("clock-add-city-hint").into(),
        remove_city_hint: locale.tr("clock-remove-city-hint").into(),
        close_picker_label: locale.tr("clock-close-picker").into(),
        no_results_label: locale.tr("clock-no-results").into(),
        searching_label: locale.tr("clock-searching").into(),
    }
}
