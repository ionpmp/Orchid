use orchid_i18n::LocaleManager;
use orchid_widgets::{JyotishRectifyView, JyotishYearSummary};
use slint::{ModelRc, SharedString, VecModel};

use crate::slint_generated::{
    JyotishDayChipModel, JyotishModel, JyotishMonthCellModel, JyotishMonthRowModel,
    JyotishPanchangaRow, JyotishPlanetEntry, JyotishRectifyModel, JyotishYearRowModel,
};

const WEEKDAY_KEYS: [&str; 7] = [
    "jyotish-wd-mon",
    "jyotish-wd-tue",
    "jyotish-wd-wed",
    "jyotish-wd-thu",
    "jyotish-wd-fri",
    "jyotish-wd-sat",
    "jyotish-wd-sun",
];

const WINDOW_KEYS: [&str; 4] = [
    "jyotish-rectify-window-30m",
    "jyotish-rectify-window-2h",
    "jyotish-rectify-window-6h",
    "jyotish-rectify-window-unknown",
];

pub(crate) fn empty_jyotish_model(locale: &LocaleManager) -> JyotishModel {
    JyotishModel {
        date_text: SharedString::new(),
        location_name: SharedString::new(),
        vara_label: SharedString::new(),
        ayanamsa_label: SharedString::new(),
        loading_label: locale.tr("jyotish-loading").into(),
        today_label: locale.tr("jyotish-today").into(),
        planets_title: locale.tr("jyotish-planets-title").into(),
        sunrise_label: locale.tr("jyotish-sunrise-label").into(),
        sunset_label: locale.tr("jyotish-sunset-label").into(),
        sunrise_text: SharedString::new(),
        sunset_text: SharedString::new(),
        is_today: true,
        is_loading: true,
        show_planets: false,
        panchanga: ModelRc::new(VecModel::default()),
        planets: ModelRc::new(VecModel::default()),
        active_tab: 0,
        tab_labels: ModelRc::new(VecModel::from(tab_labels(locale))),
        score_color: 0,
        headline: SharedString::new(),
        influences: ModelRc::new(VecModel::default()),
        advice: ModelRc::new(VecModel::default()),
        week_strip: ModelRc::new(VecModel::default()),
        month_title: SharedString::new(),
        month_cells: ModelRc::new(VecModel::default()),
        month_first_weekday: 0,
        month_green: 0,
        month_yellow: 0,
        month_red: 0,
        year_title: SharedString::new(),
        year_months: ModelRc::new(VecModel::default()),
        life_years: ModelRc::new(VecModel::default()),
        has_birth_data: false,
        birth_prompt: locale.tr("jyotish-birth-prompt").into(),
        rectify_button_label: locale.tr("jyotish-rectify-button").into(),
        legend_green: locale.tr("jyotish-legend-green").into(),
        legend_yellow: locale.tr("jyotish-legend-yellow").into(),
        legend_red: locale.tr("jyotish-legend-red").into(),
        weekday_headers: ModelRc::new(VecModel::from(weekday_headers(locale))),
        rectify: build_rectify_model(&JyotishRectifyView::default(), locale),
    }
}

pub(crate) fn build_jyotish_model(
    p: &orchid_widgets::JyotishPayload,
    locale: &LocaleManager,
) -> JyotishModel {
    if p.is_loading {
        let mut m = empty_jyotish_model(locale);
        m.location_name = p.location_name.clone().into();
        m.is_today = p.is_today;
        m.show_planets = p.show_planets;
        m.active_tab = i32::from(p.active_tab);
        m.has_birth_data = p.has_birth_data;
        return m;
    }

    let tithi = locale.tr(p.tithi_key);
    let paksha = locale.tr(p.paksha_key);
    let nakshatra = locale.tr(p.nakshatra_key);
    let pada_text = locale.tr_args(
        "jyotish-pada",
        &orchid_i18n::FluentArgs::new().with("n", p.pada.to_string()),
    );

    let panchanga = vec![
        JyotishPanchangaRow {
            label: locale.tr("jyotish-label-tithi").into(),
            value: format!("{tithi} ({paksha})").into(),
        },
        JyotishPanchangaRow {
            label: locale.tr("jyotish-label-nakshatra").into(),
            value: format!("{nakshatra} · {pada_text}").into(),
        },
        JyotishPanchangaRow {
            label: locale.tr("jyotish-label-yoga").into(),
            value: locale.tr(p.yoga_key).into(),
        },
        JyotishPanchangaRow {
            label: locale.tr("jyotish-label-karana").into(),
            value: locale.tr(p.karana_key).into(),
        },
        JyotishPanchangaRow {
            label: locale.tr("jyotish-label-vara").into(),
            value: locale.tr(p.vara_key).into(),
        },
    ];

    let planets: Vec<JyotishPlanetEntry> = p
        .planets
        .iter()
        .map(|row| JyotishPlanetEntry {
            graha: locale.tr(row.graha_key).into(),
            rashi: locale.tr(row.rashi_key).into(),
            degree_text: row.degree_text.clone().into(),
            is_retrograde: row.is_retrograde,
        })
        .collect();

    let ayanamsa_name = locale.tr(p.ayanamsa_key);
    let ayanamsa_label = locale.tr_args(
        "jyotish-ayanamsa-line",
        &orchid_i18n::FluentArgs::new()
            .with("name", ayanamsa_name)
            .with("deg", p.ayanamsa_deg_text.clone()),
    );

    let week_strip: Vec<JyotishDayChipModel> = p
        .week_strip
        .iter()
        .map(|chip| JyotishDayChipModel {
            weekday: locale.tr(chip.weekday_key).into(),
            day_num: i32::from(chip.day_num),
            color: i32::from(chip.color),
            offset: chip.offset,
            is_selected: chip.is_selected,
        })
        .collect();

    let month_cells: Vec<JyotishMonthCellModel> = p
        .month_cells
        .iter()
        .map(|cell| JyotishMonthCellModel {
            day: i32::from(cell.day),
            color: i32::from(cell.color),
            is_today: cell.is_today,
            offset: cell.offset,
        })
        .collect();

    let month_title = format!("{} {}", locale.tr(p.month_key), p.month_year);

    let year_months: Vec<JyotishMonthRowModel> = p
        .year_months
        .iter()
        .map(|row| JyotishMonthRowModel {
            name: locale.tr(row.month_key).into(),
            green: i32::from(row.green),
            yellow: i32::from(row.yellow),
            red: i32::from(row.red),
            month_offset: row.month_offset,
        })
        .collect();

    let life_years: Vec<JyotishYearRowModel> = p
        .life_years
        .iter()
        .map(|row: &JyotishYearSummary| JyotishYearRowModel {
            year: row.year,
            green: i32::from(row.green),
            yellow: i32::from(row.yellow),
            red: i32::from(row.red),
            dasha: locale.tr(row.dasha_key).into(),
            year_offset: row.year_offset,
        })
        .collect();

    JyotishModel {
        date_text: p.date_text.clone().into(),
        location_name: p.location_name.clone().into(),
        vara_label: locale.tr(p.vara_key).into(),
        ayanamsa_label: ayanamsa_label.into(),
        loading_label: locale.tr("jyotish-loading").into(),
        today_label: locale.tr("jyotish-today").into(),
        planets_title: locale.tr("jyotish-planets-title").into(),
        sunrise_label: locale.tr("jyotish-sunrise-label").into(),
        sunset_label: locale.tr("jyotish-sunset-label").into(),
        sunrise_text: p.sunrise_time.clone().unwrap_or_default().into(),
        sunset_text: p.sunset_time.clone().unwrap_or_default().into(),
        is_today: p.is_today,
        is_loading: false,
        show_planets: p.show_planets,
        panchanga: ModelRc::new(VecModel::from(panchanga)),
        planets: ModelRc::new(VecModel::from(planets)),
        active_tab: i32::from(p.active_tab),
        tab_labels: ModelRc::new(VecModel::from(tab_labels(locale))),
        score_color: i32::from(p.score_color),
        headline: locale.tr(p.headline_key).into(),
        influences: ModelRc::new(VecModel::from(
            p.influence_keys
                .iter()
                .map(|k| SharedString::from(locale.tr(k)))
                .collect::<Vec<_>>(),
        )),
        advice: ModelRc::new(VecModel::from(
            p.advice_keys
                .iter()
                .map(|k| SharedString::from(locale.tr(k)))
                .collect::<Vec<_>>(),
        )),
        week_strip: ModelRc::new(VecModel::from(week_strip)),
        month_title: month_title.into(),
        month_cells: ModelRc::new(VecModel::from(month_cells)),
        month_first_weekday: i32::from(p.month_first_weekday),
        month_green: i32::from(p.month_green),
        month_yellow: i32::from(p.month_yellow),
        month_red: i32::from(p.month_red),
        year_title: p.year_value.to_string().into(),
        year_months: ModelRc::new(VecModel::from(year_months)),
        life_years: ModelRc::new(VecModel::from(life_years)),
        has_birth_data: p.has_birth_data,
        birth_prompt: locale.tr("jyotish-birth-prompt").into(),
        rectify_button_label: locale.tr("jyotish-rectify-button").into(),
        legend_green: locale.tr("jyotish-legend-green").into(),
        legend_yellow: locale.tr("jyotish-legend-yellow").into(),
        legend_red: locale.tr("jyotish-legend-red").into(),
        weekday_headers: ModelRc::new(VecModel::from(weekday_headers(locale))),
        rectify: build_rectify_model(&p.rectify, locale),
    }
}

fn tab_labels(locale: &LocaleManager) -> Vec<SharedString> {
    [
        "jyotish-tab-day",
        "jyotish-tab-month",
        "jyotish-tab-year",
        "jyotish-tab-life",
    ]
    .iter()
    .map(|k| SharedString::from(locale.tr(k)))
    .collect()
}

fn weekday_headers(locale: &LocaleManager) -> Vec<SharedString> {
    WEEKDAY_KEYS
        .iter()
        .map(|k| SharedString::from(locale.tr(k)))
        .collect()
}

fn build_rectify_model(
    rectify: &JyotishRectifyView,
    locale: &LocaleManager,
) -> JyotishRectifyModel {
    let question = if rectify.question_key.is_empty() {
        String::new()
    } else {
        locale.tr(rectify.question_key)
    };
    let options: Vec<SharedString> = rectify
        .option_keys
        .iter()
        .map(|k| SharedString::from(locale.tr(k)))
        .collect();
    let events: Vec<SharedString> = rectify
        .events
        .iter()
        .map(|(kind_key, year)| SharedString::from(format!("{} · {}", locale.tr(kind_key), year)))
        .collect();
    let event_kinds: Vec<SharedString> = rectify
        .event_kind_keys
        .iter()
        .map(|k| SharedString::from(locale.tr(k)))
        .collect();
    let candidates: Vec<SharedString> = rectify
        .candidates
        .iter()
        .map(|(range, rashi_key, pct)| {
            SharedString::from(format!("{}  {}  {}%", range, locale.tr(rashi_key), pct))
        })
        .collect();
    let window_labels: Vec<SharedString> = WINDOW_KEYS
        .iter()
        .map(|k| SharedString::from(locale.tr(k)))
        .collect();

    JyotishRectifyModel {
        step: i32::from(rectify.step),
        question_idx: i32::from(rectify.question_idx),
        question_total: i32::from(rectify.question_total),
        question: question.into(),
        options: ModelRc::new(VecModel::from(options)),
        events: ModelRc::new(VecModel::from(events)),
        event_kinds: ModelRc::new(VecModel::from(event_kinds)),
        candidates: ModelRc::new(VecModel::from(candidates)),
        title: locale.tr("jyotish-rectify-title").into(),
        next_label: locale.tr("jyotish-rectify-next").into(),
        cancel_label: locale.tr("jyotish-rectify-cancel").into(),
        accept_label: locale.tr("jyotish-rectify-accept").into(),
        window_labels: ModelRc::new(VecModel::from(window_labels)),
        add_event_label: locale.tr("jyotish-rectify-add-event").into(),
        year_placeholder: locale.tr("jyotish-rectify-year").into(),
    }
}
