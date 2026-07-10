use orchid_i18n::LocaleManager;
use slint::{ModelRc, SharedString, VecModel};

use crate::slint_generated::{MoonModel, MoonValueEntry};

pub(crate) fn empty_moon_model(locale: &LocaleManager) -> MoonModel {
    MoonModel {
        phase_label: locale.tr("moon-loading").into(),
        phase_icon: SharedString::new(),
        illumination: SharedString::new(),
        values: ModelRc::new(VecModel::default()),
    }
}

pub(crate) fn build_moon_model(p: &orchid_widgets::MoonPayload, locale: &LocaleManager) -> MoonModel {
    let phase_label = if p.is_loading {
        locale.tr("moon-loading")
    } else {
        locale.tr(p.phase_key)
    };

    let illumination = p
        .illumination_percent
        .map(|pct| {
            locale.tr_args(
                "moon-illumination",
                &orchid_i18n::FluentArgs::new().with("pct", format!("{pct:.0}")),
            )
        })
        .unwrap_or_default();

    let mut values = Vec::new();
    if let Some(days) = p.age_days {
        values.push(MoonValueEntry {
            label: locale.tr("moon-age-label").into(),
            value: locale
                .tr_args(
                    "moon-age",
                    &orchid_i18n::FluentArgs::new().with("days", format!("{days:.1}")),
                )
                .into(),
        });
    }
    if let Some(km) = p.distance_km {
        values.push(MoonValueEntry {
            label: locale.tr("moon-distance-label").into(),
            value: locale
                .tr_args(
                    "moon-distance",
                    &orchid_i18n::FluentArgs::new().with("km", format!("{km:.0}")),
                )
                .into(),
        });
    }
    if let Some(ref date) = p.next_full_date {
        values.push(MoonValueEntry {
            label: locale.tr("moon-next-full-label").into(),
            value: locale
                .tr_args(
                    "moon-next-full",
                    &orchid_i18n::FluentArgs::new().with("date", date.clone()),
                )
                .into(),
        });
    }
    if let Some(ref date) = p.next_new_date {
        values.push(MoonValueEntry {
            label: locale.tr("moon-next-new-label").into(),
            value: locale
                .tr_args(
                    "moon-next-new",
                    &orchid_i18n::FluentArgs::new().with("date", date.clone()),
                )
                .into(),
        });
    }
    if let Some(ref time) = p.moonrise_time {
        values.push(MoonValueEntry {
            label: locale.tr("moon-moonrise-label").into(),
            value: locale
                .tr_args(
                    "moon-moonrise",
                    &orchid_i18n::FluentArgs::new().with("time", time.clone()),
                )
                .into(),
        });
    }
    if let Some(ref time) = p.moonset_time {
        values.push(MoonValueEntry {
            label: locale.tr("moon-moonset-label").into(),
            value: locale
                .tr_args(
                    "moon-moonset",
                    &orchid_i18n::FluentArgs::new().with("time", time.clone()),
                )
                .into(),
        });
    }
    if let Some(ref time) = p.sunrise_time {
        values.push(MoonValueEntry {
            label: locale.tr("moon-sunrise-label").into(),
            value: locale
                .tr_args(
                    "moon-sunrise",
                    &orchid_i18n::FluentArgs::new().with("time", time.clone()),
                )
                .into(),
        });
    }
    if let Some(ref time) = p.sunset_time {
        values.push(MoonValueEntry {
            label: locale.tr("moon-sunset-label").into(),
            value: locale
                .tr_args(
                    "moon-sunset",
                    &orchid_i18n::FluentArgs::new().with("time", time.clone()),
                )
                .into(),
        });
    }
    if let (Some(lat), Some(lon)) = (p.libration_lat_deg, p.libration_lon_deg) {
        values.push(MoonValueEntry {
            label: locale.tr("moon-libration-label").into(),
            value: locale
                .tr_args(
                    "moon-libration",
                    &orchid_i18n::FluentArgs::new()
                        .with("lat", format!("{lat:.1}"))
                        .with("lon", format!("{lon:.1}")),
                )
                .into(),
        });
    }

    MoonModel {
        phase_label: phase_label.into(),
        phase_icon: p.phase_icon.into(),
        illumination: illumination.into(),
        values: ModelRc::new(VecModel::from(values)),
    }
}
