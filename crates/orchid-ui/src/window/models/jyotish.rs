use orchid_i18n::LocaleManager;
use slint::{ModelRc, SharedString, VecModel};

use crate::slint_generated::{JyotishModel, JyotishPanchangaRow, JyotishPlanetEntry};

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
    }
}
