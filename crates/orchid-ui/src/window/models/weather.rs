use orchid_i18n::LocaleManager;
use slint::{ModelRc, SharedString, VecModel};

use crate::slint_generated::{WeatherForecastEntry, WeatherModel};

pub(crate) fn empty_weather_model(locale: &LocaleManager) -> WeatherModel {
    WeatherModel {
        location: SharedString::new(),
        current_temp: SharedString::new(),
        condition_label: locale.tr("weather-loading").into(),
        condition_icon: SharedString::new(),
        feels_like: SharedString::new(),
        humidity: SharedString::new(),
        wind: SharedString::new(),
        forecast: ModelRc::new(VecModel::default()),
        last_updated: locale.tr("weather-loading").into(),
        status: 2,
        status_label: locale.tr("weather-status-offline").into(),
    }
}

pub(crate) fn build_weather_model(p: &orchid_widgets::WeatherPayload, locale: &LocaleManager) -> WeatherModel {
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
                }
            })
            .collect()
    };

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
                    // Legacy payloads may still carry English compass abbreviations.
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
        forecast: ModelRc::new(VecModel::from(forecast)),
        last_updated: last_updated.into(),
        status,
        status_label: locale.tr(weather_status_i18n_key(status)).into(),
    }
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
    use orchid_i18n::{LocaleManager, default_language};

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
    }
}
