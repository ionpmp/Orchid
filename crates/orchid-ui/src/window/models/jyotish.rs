use orchid_i18n::LocaleManager;
use orchid_widgets::{JyotishRectifyView, JyotishYearSummary};
use slint::{ModelRc, SharedString, VecModel};

use crate::slint_generated::{
    JyotishAntarRowModel, JyotishDayChipModel, JyotishFactorEntry, JyotishModel,
    JyotishMonthCellModel, JyotishMonthRowModel, JyotishPanchangaRow, JyotishPlanetEntry,
    JyotishRectifyCandidateModel, JyotishRectifyModel, JyotishYearRowModel,
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
        rahukalam_label: locale.tr("jyotish-label-rahukalam").into(),
        yamagandam_label: locale.tr("jyotish-label-yamagandam").into(),
        gulika_label: locale.tr("jyotish-label-gulika").into(),
        rahukalam_text: SharedString::new(),
        yamagandam_text: SharedString::new(),
        gulika_text: SharedString::new(),
        in_rahukalam: false,
        is_today: true,
        is_loading: true,
        show_planets: false,
        panchanga: ModelRc::new(VecModel::default()),
        planets: ModelRc::new(VecModel::default()),
        active_tab: 0,
        tab_labels: ModelRc::new(VecModel::from(tab_labels(locale))),
        score_color: 0,
        now_score_color: 0,
        day_score_color: 0,
        score_value: 0,
        score_label: locale.tr("jyotish-score-label").into(),
        score_now_label: locale.tr("jyotish-score-now").into(),
        score_day_label: locale.tr("jyotish-score-day").into(),
        factors: ModelRc::new(VecModel::default()),
        personal_mode: false,
        personal_badge: locale.tr("jyotish-badge-panchanga").into(),
        headline: SharedString::new(),
        influences: ModelRc::new(VecModel::default()),
        advice: ModelRc::new(VecModel::default()),
        disclaimer: locale.tr("jyotish-disclaimer").into(),
        week_strip: ModelRc::new(VecModel::default()),
        month_title: SharedString::new(),
        month_cells: ModelRc::new(VecModel::default()),
        month_first_weekday: 0,
        month_green: 0,
        month_yellow: 0,
        month_red: 0,
        year_title: SharedString::new(),
        year_months: ModelRc::new(VecModel::default()),
        gochara_note: SharedString::new(),
        life_years: ModelRc::new(VecModel::default()),
        life_antars: ModelRc::new(VecModel::default()),
        life_antars_title: locale.tr("jyotish-life-antars-title").into(),
        has_dasha_now: false,
        dasha_now_title: locale.tr("jyotish-dasha-now-title").into(),
        dasha_maha_label: locale.tr("jyotish-dasha-maha").into(),
        dasha_antar_label: locale.tr("jyotish-dasha-antar").into(),
        dasha_pratyantar_label: locale.tr("jyotish-dasha-pratyantar").into(),
        dasha_maha_text: SharedString::new(),
        dasha_antar_text: SharedString::new(),
        dasha_pratyantar_text: SharedString::new(),
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
    let with_until = |base: String, end: &Option<String>| -> String {
        match end {
            Some(t) if !t.is_empty() => format!(
                "{base} · {}",
                locale.tr_args(
                    "jyotish-until",
                    &orchid_i18n::FluentArgs::new().with("time", t.clone()),
                )
            ),
            _ => base,
        }
    };

    let panchanga = vec![
        JyotishPanchangaRow {
            label: locale.tr("jyotish-label-tithi").into(),
            value: with_until(format!("{tithi} ({paksha})"), &p.tithi_end_text).into(),
        },
        JyotishPanchangaRow {
            label: locale.tr("jyotish-label-nakshatra").into(),
            value: with_until(format!("{nakshatra} · {pada_text}"), &p.nakshatra_end_text).into(),
        },
        JyotishPanchangaRow {
            label: locale.tr("jyotish-label-yoga").into(),
            value: with_until(locale.tr(p.yoga_key), &p.yoga_end_text).into(),
        },
        JyotishPanchangaRow {
            label: locale.tr("jyotish-label-karana").into(),
            value: with_until(locale.tr(p.karana_key), &p.karana_end_text).into(),
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
            dasha: if row.dasha_key.is_empty() {
                SharedString::new()
            } else {
                locale.tr(row.dasha_key).into()
            },
            year_offset: row.year_offset,
            is_selected: row.is_selected,
            is_current: row.is_current,
        })
        .collect();

    let life_antars: Vec<JyotishAntarRowModel> = p
        .life_antars
        .iter()
        .map(|row| JyotishAntarRowModel {
            lord: locale.tr(row.lord_key).into(),
            range: format!("{} – {}", row.from_text, row.to_text).into(),
            is_current: row.is_current,
        })
        .collect();

    let dasha_line = |lord_key: &str, range: &str| -> SharedString {
        if lord_key.is_empty() {
            SharedString::new()
        } else {
            format!("{} · {}", locale.tr(lord_key), range).into()
        }
    };

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
        rahukalam_label: locale.tr("jyotish-label-rahukalam").into(),
        yamagandam_label: locale.tr("jyotish-label-yamagandam").into(),
        gulika_label: locale.tr("jyotish-label-gulika").into(),
        rahukalam_text: p.rahukalam_text.clone().unwrap_or_default().into(),
        yamagandam_text: p.yamagandam_text.clone().unwrap_or_default().into(),
        gulika_text: p.gulika_text.clone().unwrap_or_default().into(),
        in_rahukalam: p.in_rahukalam,
        is_today: p.is_today,
        is_loading: false,
        show_planets: p.show_planets,
        panchanga: ModelRc::new(VecModel::from(panchanga)),
        planets: ModelRc::new(VecModel::from(planets)),
        active_tab: i32::from(p.active_tab),
        tab_labels: ModelRc::new(VecModel::from(tab_labels(locale))),
        score_color: i32::from(p.score_color),
        now_score_color: i32::from(p.now_score_color),
        day_score_color: i32::from(p.day_score_color),
        score_value: i32::from(p.score_value),
        score_label: locale.tr("jyotish-score-label").into(),
        score_now_label: locale.tr("jyotish-score-now").into(),
        score_day_label: locale.tr("jyotish-score-day").into(),
        factors: ModelRc::new(VecModel::from(
            p.factors
                .iter()
                .map(|f| JyotishFactorEntry {
                    label: locale.tr(f.label_key).into(),
                    delta_text: format!("{:+}", f.delta).into(),
                    strength: i32::from(f.strength),
                    valence: i32::from(f.valence),
                })
                .collect::<Vec<_>>(),
        )),
        personal_mode: p.personal_mode,
        personal_badge: locale
            .tr(if p.personal_mode {
                "jyotish-badge-personal"
            } else {
                "jyotish-badge-panchanga"
            })
            .into(),
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
        disclaimer: locale.tr("jyotish-disclaimer").into(),
        week_strip: ModelRc::new(VecModel::from(week_strip)),
        month_title: month_title.into(),
        month_cells: ModelRc::new(VecModel::from(month_cells)),
        month_first_weekday: i32::from(p.month_first_weekday),
        month_green: i32::from(p.month_green),
        month_yellow: i32::from(p.month_yellow),
        month_red: i32::from(p.month_red),
        year_title: p.year_value.to_string().into(),
        year_months: ModelRc::new(VecModel::from(year_months)),
        gochara_note: if p.gochara_note_key.is_empty() {
            SharedString::new()
        } else {
            locale.tr(p.gochara_note_key).into()
        },
        life_years: ModelRc::new(VecModel::from(life_years)),
        life_antars: ModelRc::new(VecModel::from(life_antars)),
        life_antars_title: locale.tr("jyotish-life-antars-title").into(),
        has_dasha_now: p.has_dasha_now,
        dasha_now_title: locale.tr("jyotish-dasha-now-title").into(),
        dasha_maha_label: locale.tr("jyotish-dasha-maha").into(),
        dasha_antar_label: locale.tr("jyotish-dasha-antar").into(),
        dasha_pratyantar_label: locale.tr("jyotish-dasha-pratyantar").into(),
        dasha_maha_text: dasha_line(p.dasha_now.maha_key, &p.dasha_now.maha_range),
        dasha_antar_text: dasha_line(p.dasha_now.antar_key, &p.dasha_now.antar_range),
        dasha_pratyantar_text: dasha_line(p.dasha_now.pratyantar_key, &p.dasha_now.pratyantar_range),
        has_birth_data: p.has_birth_data,
        birth_prompt: locale.tr("jyotish-birth-prompt").into(),
        rectify_button_label: rectify_button_label(locale, p.rectify.has_draft),
        legend_green: locale.tr("jyotish-legend-green").into(),
        legend_yellow: locale.tr("jyotish-legend-yellow").into(),
        legend_red: locale.tr("jyotish-legend-red").into(),
        weekday_headers: ModelRc::new(VecModel::from(weekday_headers(locale))),
        rectify: build_rectify_model(&p.rectify, locale),
    }
}

fn rectify_button_label(locale: &LocaleManager, has_draft: bool) -> SharedString {
    locale
        .tr(if has_draft {
            "jyotish-rectify-resume"
        } else {
            "jyotish-rectify-button"
        })
        .into()
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
    let candidates: Vec<JyotishRectifyCandidateModel> = rectify
        .candidates
        .iter()
        .map(|c| {
            let breakdown = locale.tr_args(
                "jyotish-rectify-breakdown",
                &orchid_i18n::FluentArgs::new()
                    .with("quiz", c.quiz_score.to_string())
                    .with("events", c.event_score.to_string())
                    .with("total", c.total_score.to_string()),
            );
            JyotishRectifyCandidateModel {
                range: c.range.clone().into(),
                rashi: locale.tr(c.rashi_key).into(),
                confidence: i32::from(c.confidence_pct),
                quiz_score: i32::from(c.quiz_score),
                event_score: i32::from(c.event_score),
                total_score: i32::from(c.total_score),
                breakdown: breakdown.into(),
                is_top: c.is_top,
            }
        })
        .collect();
    let window_labels: Vec<SharedString> = WINDOW_KEYS
        .iter()
        .map(|k| SharedString::from(locale.tr(k)))
        .collect();

    let step_title_key = match rectify.step {
        1 => "jyotish-rectify-step-window",
        2 => "jyotish-rectify-step-quiz",
        3 => "jyotish-rectify-step-events",
        4 => "jyotish-rectify-step-results",
        _ => "",
    };
    let progress_text = if rectify.step == 0 {
        String::new()
    } else {
        locale.tr_args(
            "jyotish-rectify-progress",
            &orchid_i18n::FluentArgs::new()
                .with("n", rectify.step.to_string())
                .with("total", "4"),
        )
    };
    let error_text = if rectify.error_key.is_empty() {
        String::new()
    } else {
        locale.tr(rectify.error_key)
    };

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
        back_label: locale.tr("jyotish-rectify-back").into(),
        refine_label: locale.tr("jyotish-rectify-refine").into(),
        window_labels: ModelRc::new(VecModel::from(window_labels)),
        add_event_label: locale.tr("jyotish-rectify-add-event").into(),
        year_placeholder: locale.tr("jyotish-rectify-year").into(),
        progress_text: progress_text.into(),
        step_title: if step_title_key.is_empty() {
            SharedString::new()
        } else {
            locale.tr(step_title_key).into()
        },
        error_text: error_text.into(),
        can_go_back: rectify.can_go_back || rectify.step > 1,
        can_refine: rectify.can_refine,
        has_draft: rectify.has_draft,
        event_year_min: rectify.event_year_min,
        event_year_max: rectify.event_year_max,
    }
}
