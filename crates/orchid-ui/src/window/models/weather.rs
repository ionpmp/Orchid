use orchid_i18n::LocaleManager;
use slint::{ModelRc, SharedString, VecModel};

use crate::slint_generated::{
    WeatherCityEntry, WeatherForecastEntry, WeatherModel, WeatherSearchHit,
};

pub(crate) fn empty_weather_model(locale: &LocaleManager) -> WeatherModel {
    WeatherModel {
        location: SharedString::new(),
        cities: ModelRc::new(VecModel::default()),
        active_city_index: 0,
        picker_open: false,
        search_query: SharedString::new(),
        search_results: ModelRc::new(VecModel::default()),
        search_busy: false,
        picker_title: locale.tr("weather-cities-title").into(),
        search_placeholder: locale.tr("weather-city-search-placeholder").into(),
        add_city_hint: locale.tr("weather-city-add").into(),
        remove_city_hint: locale.tr("weather-city-remove").into(),
        close_picker_label: locale.tr("weather-cities-close").into(),
        no_results_label: locale.tr("weather-city-no-results").into(),
        searching_label: locale.tr("weather-city-searching").into(),
        current_temp: SharedString::new(),
        condition_label: locale.tr("weather-loading").into(),
        condition_icon: SharedString::new(),
        feels_like: SharedString::new(),
        humidity: SharedString::new(),
        wind: SharedString::new(),
        selected_day_index: 0,
        day_detail: SharedString::new(),
        forecast: ModelRc::new(VecModel::default()),
        last_updated: locale.tr("weather-loading").into(),
        status: 2,
        status_label: locale.tr("weather-status-offline").into(),
    }
}

pub(crate) fn build_weather_model(
    p: &orchid_widgets::WeatherPayload,
    locale: &LocaleManager,
) -> WeatherModel {
    let condition_label = if p.is_loading {
        locale.tr("weather-loading")
    } else if p.status == orchid_widgets::WeatherStatusTag::Error {
        locale.tr("weather-status-error")
    } else {
        locale.tr(p.condition_key)
    };

    let forecast: Vec<WeatherForecastEntry> = if p.is_loading {
        Vec::new()
    } else {
        p.forecast
            .iter()
            .map(|d| {
                let day_label = match d.day_index {
                    0 => locale.tr("weather-day-today"),
                    1 => locale.tr("weather-day-tomorrow"),
                    _ => d.weekday_label.clone().unwrap_or_default(),
                };
                let range_text = locale.tr_args(
                    "weather-forecast-range",
                    &orchid_i18n::FluentArgs::new()
                        .with("high", d.high_text.as_str())
                        .with("low", d.low_text.as_str()),
                );
                let precip_text = d
                    .precipitation_probability
                    .map(|pct| {
                        locale.tr_args(
                            "weather-precip-chance",
                            &orchid_i18n::FluentArgs::new().with("pct", pct.to_string()),
                        )
                    })
                    .unwrap_or_default();
                WeatherForecastEntry {
                    day_label: day_label.into(),
                    range_text: range_text.into(),
                    icon: d.condition_icon.into(),
                    precip_text: precip_text.into(),
                    selected: d.selected,
                }
            })
            .collect()
    };

    let day_detail = if p.is_loading {
        String::new()
    } else {
        p.forecast
            .iter()
            .find(|d| d.selected)
            .or_else(|| p.forecast.first())
            .map(|d| format_day_detail(d, locale))
            .unwrap_or_default()
    };

    let cities: Vec<WeatherCityEntry> = p
        .cities
        .iter()
        .map(|c| WeatherCityEntry {
            name: c.name.clone().into(),
            active: c.active,
        })
        .collect();

    let search_results: Vec<WeatherSearchHit> = p
        .search_results
        .iter()
        .map(|h| WeatherSearchHit {
            name: h.name.clone().into(),
            detail: h.detail.clone().into(),
            latitude: h.latitude as f32,
            longitude: h.longitude as f32,
            timezone: h.timezone.clone().into(),
        })
        .collect();

    let feels_like = if p.is_loading {
        String::new()
    } else {
        p.feels_like_temp
            .as_ref()
            .map(|temp| {
                locale.tr_args(
                    "weather-feels-like",
                    &orchid_i18n::FluentArgs::new().with("temp", temp.clone()),
                )
            })
            .unwrap_or_default()
    };

    let humidity = if p.is_loading {
        String::new()
    } else {
        p.humidity_percent
            .map(|h| {
                locale.tr_args(
                    "weather-humidity-line",
                    &orchid_i18n::FluentArgs::new()
                        .with("label", locale.tr("weather-humidity-label"))
                        .with("h", h.to_string()),
                )
            })
            .unwrap_or_default()
    };

    let wind = if p.is_loading {
        String::new()
    } else {
        match (p.wind_speed_kph, p.wind_direction.as_deref()) {
            (Some(kph), Some(dir_key)) if !dir_key.is_empty() => {
                let dir = if dir_key.starts_with("weather-wind-") {
                    locale.tr(dir_key)
                } else {
                    dir_key.to_string()
                };
                locale.tr_args(
                    "weather-wind-line",
                    &orchid_i18n::FluentArgs::new()
                        .with("label", locale.tr("weather-wind-label"))
                        .with("speed", format!("{kph:.0}"))
                        .with("dir", dir),
                )
            }
            (Some(kph), _) => locale.tr_args(
                "weather-wind-line-no-dir",
                &orchid_i18n::FluentArgs::new()
                    .with("label", locale.tr("weather-wind-label"))
                    .with("speed", format!("{kph:.0}")),
            ),
            _ => String::new(),
        }
    };

    let last_updated = if p.is_loading {
        locale.tr("weather-loading")
    } else if p.status == orchid_widgets::WeatherStatusTag::Error {
        locale.tr("weather-status-error")
    } else {
        p.fetched_at
            .map(|at| format_weather_updated(at, locale))
            .unwrap_or_default()
    };

    let status = weather_status_to_int(&p.status);
    WeatherModel {
        location: p.location_name.clone().into(),
        cities: ModelRc::new(VecModel::from(cities)),
        active_city_index: p.active_city_index as i32,
        picker_open: p.picker_open,
        search_query: p.search_query.clone().into(),
        search_results: ModelRc::new(VecModel::from(search_results)),
        search_busy: p.search_busy,
        picker_title: locale.tr("weather-cities-title").into(),
        search_placeholder: locale.tr("weather-city-search-placeholder").into(),
        add_city_hint: locale.tr("weather-city-add").into(),
        remove_city_hint: locale.tr("weather-city-remove").into(),
        close_picker_label: locale.tr("weather-cities-close").into(),
        no_results_label: locale.tr("weather-city-no-results").into(),
        searching_label: locale.tr("weather-city-searching").into(),
        current_temp: if p.is_loading {
            SharedString::new()
        } else {
            p.current_temp_text.clone().into()
        },
        condition_label: condition_label.into(),
        condition_icon: if p.is_loading {
            SharedString::new()
        } else {
            p.condition_icon.into()
        },
        feels_like: feels_like.into(),
        humidity: humidity.into(),
        wind: wind.into(),
        selected_day_index: p.selected_day_index as i32,
        day_detail: day_detail.into(),
        forecast: ModelRc::new(VecModel::from(forecast)),
        last_updated: last_updated.into(),
        status,
        status_label: locale.tr(weather_status_i18n_key(status)).into(),
    }
}

fn format_day_detail(d: &orchid_widgets::WeatherForecastDay, locale: &LocaleManager) -> String {
    let range = locale.tr_args(
        "weather-forecast-range",
        &orchid_i18n::FluentArgs::new()
            .with("high", d.high_text.as_str())
            .with("low", d.low_text.as_str()),
    );
    let mut parts = vec![range];
    if let Some(pct) = d.precipitation_probability {
        parts.push(locale.tr_args(
            "weather-precip-chance",
            &orchid_i18n::FluentArgs::new().with("pct", pct.to_string()),
        ));
    }
    match (d.sunrise_text.as_deref(), d.sunset_text.as_deref()) {
        (Some(rise), Some(set)) => parts.push(locale.tr_args(
            "weather-sun-line",
            &orchid_i18n::FluentArgs::new()
                .with("rise", rise)
                .with("set", set),
        )),
        (Some(rise), None) => parts.push(locale.tr_args(
            "weather-sunrise-line",
            &orchid_i18n::FluentArgs::new().with("rise", rise),
        )),
        (None, Some(set)) => parts.push(locale.tr_args(
            "weather-sunset-line",
            &orchid_i18n::FluentArgs::new().with("set", set),
        )),
        (None, None) => {}
    }
    parts.join(" · ")
}

fn format_weather_updated(at: chrono::DateTime<chrono::Utc>, locale: &LocaleManager) -> String {
    let secs = (chrono::Utc::now() - at).num_seconds().max(0);
    if secs < 60 {
        locale.tr("weather-updated-just-now")
    } else if secs < 3600 {
        locale.tr_args(
            "weather-updated-minutes",
            &orchid_i18n::FluentArgs::new().with("m", (secs / 60).to_string()),
        )
    } else if secs < 86400 {
        locale.tr_args(
            "weather-updated-hours",
            &orchid_i18n::FluentArgs::new().with("h", (secs / 3600).to_string()),
        )
    } else {
        locale.tr_args(
            "weather-updated-days",
            &orchid_i18n::FluentArgs::new().with("d", (secs / 86400).to_string()),
        )
    }
}

fn weather_status_to_int(s: &orchid_widgets::WeatherStatusTag) -> i32 {
    use orchid_widgets::WeatherStatusTag::*;
    match s {
        Fresh => 0,
        Stale => 1,
        Offline => 2,
        Error => 3,
    }
}

fn weather_status_i18n_key(status: i32) -> &'static str {
    match status {
        0 => "weather-status-fresh",
        1 => "weather-status-stale",
        2 => "weather-status-offline",
        _ => "weather-status-error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchid_i18n::{default_language, LocaleManager};
    use slint::Model;

    fn test_locale() -> LocaleManager {
        LocaleManager::new(default_language(), None).expect("locale")
    }

    #[test]
    fn format_weather_updated_uses_locale_keys() {
        let locale = test_locale();
        let at = chrono::Utc::now() - chrono::Duration::seconds(30);
        assert_eq!(
            format_weather_updated(at, &locale),
            locale.tr("weather-updated-just-now")
        );
    }

    #[test]
    fn build_weather_model_localizes_humidity_and_wind() {
        let locale = test_locale();
        let model = build_weather_model(
            &orchid_widgets::WeatherPayload {
                location_name: "Home".into(),
                cities: vec![orchid_widgets::WeatherCityEntry {
                    name: "Home".into(),
                    active: true,
                }],
                active_city_index: 0,
                picker_open: false,
                search_query: String::new(),
                search_results: vec![],
                search_busy: false,
                selected_day_index: 0,
                current_temp_text: "20°C".into(),
                feels_like_temp: None,
                condition_key: "weather-condition-clear",
                condition_icon: "weather-clear",
                humidity_percent: Some(45),
                wind_speed_kph: Some(12.0),
                wind_direction: Some("weather-wind-ne".into()),
                forecast: vec![],
                fetched_at: None,
                is_loading: false,
                status: orchid_widgets::WeatherStatusTag::Fresh,
            },
            &locale,
        );
        assert_eq!(
            model.humidity.as_str(),
            locale.tr_args(
                "weather-humidity-line",
                &orchid_i18n::FluentArgs::new()
                    .with("label", locale.tr("weather-humidity-label"))
                    .with("h", "45"),
            )
        );
        assert!(model.wind.as_str().contains("NE"));
        assert!(model.wind.as_str().contains("12"));
        assert_eq!(model.cities.row_count(), 1);
    }

    #[test]
    fn format_day_detail_joins_range_precip_and_sun() {
        let locale = test_locale();
        let detail = format_day_detail(
            &orchid_widgets::WeatherForecastDay {
                day_index: 2,
                weekday_label: Some("Wed".into()),
                high_text: "28°C".into(),
                low_text: "22°C".into(),
                condition_icon: "weather-rain",
                precipitation_probability: Some(40),
                selected: true,
                sunrise_text: Some("06:12".into()),
                sunset_text: Some("18:04".into()),
            },
            &locale,
        );
        assert!(detail.contains("28°C"));
        assert!(detail.contains("22°C"));
        assert!(detail.contains("40"));
        assert!(detail.contains("06:12"));
        assert!(detail.contains("18:04"));
    }
}
