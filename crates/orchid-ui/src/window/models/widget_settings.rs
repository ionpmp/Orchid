//! Per-widget settings dialog field builders and apply helpers.

use orchid_i18n::LocaleManager;
use orchid_widgets::builtin::file_manager::{
    ClickBehavior, FileManagerConfig, FmThumbnailSize as ThumbnailSize,
};
use orchid_widgets::builtin::moon::MoonConfig;
use orchid_widgets::builtin::rss::RssConfig;
use orchid_widgets::builtin::system::SystemConfig;
use orchid_widgets::builtin::weather::{TemperatureUnit, WeatherConfig};
use slint::{ModelRc, SharedString, VecModel};
use uuid::Uuid;

use crate::slint_generated::SettingsFieldRow;

const FIELD_BOOL: i32 = 1;
const FIELD_TEXT: i32 = 2;
const FIELD_COMBO: i32 = 3;

fn strings_model(values: Vec<SharedString>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(values))
}

fn push_bool(
    rows: &mut Vec<SettingsFieldRow>,
    locale: &LocaleManager,
    key: &str,
    label_key: &str,
    value: bool,
) {
    rows.push(SettingsFieldRow {
        key: key.into(),
        label: locale.tr(label_key).into(),
        kind: FIELD_BOOL,
        value: SharedString::default(),
        bool_value: value,
        combo_options: strings_model(vec![]),
        combo_values: strings_model(vec![]),
        combo_index: -1,
    });
}

fn push_text(
    rows: &mut Vec<SettingsFieldRow>,
    locale: &LocaleManager,
    key: &str,
    label_key: &str,
    value: impl Into<SharedString>,
) {
    rows.push(SettingsFieldRow {
        key: key.into(),
        label: locale.tr(label_key).into(),
        kind: FIELD_TEXT,
        value: value.into(),
        bool_value: false,
        combo_options: strings_model(vec![]),
        combo_values: strings_model(vec![]),
        combo_index: -1,
    });
}

fn push_combo(
    rows: &mut Vec<SettingsFieldRow>,
    locale: &LocaleManager,
    key: &str,
    label_key: &str,
    options: &[(SharedString, SharedString)],
    current: &str,
) {
    let combo_values: Vec<SharedString> = options.iter().map(|(v, _)| v.clone()).collect();
    let combo_options: Vec<SharedString> = options.iter().map(|(_, l)| l.clone()).collect();
    let combo_index = combo_values
        .iter()
        .position(|v| v.as_str() == current)
        .map_or(0, |i| i as i32);
    rows.push(SettingsFieldRow {
        key: key.into(),
        label: locale.tr(label_key).into(),
        kind: FIELD_COMBO,
        value: SharedString::default(),
        bool_value: false,
        combo_options: strings_model(combo_options),
        combo_values: strings_model(combo_values),
        combo_index,
    });
}

/// Whether this widget type shows the header settings gear.
#[must_use]
pub(crate) fn widget_has_settings(type_id: &str) -> bool {
    matches!(
        type_id,
        "weather" | "moon" | "clock" | "system" | "processes" | "calculator" | "rss"
            | "file-manager"
    )
}

/// Build settings fields for a live widget instance.
#[must_use]
pub(crate) fn build_widget_settings_fields(
    type_id: &str,
    instance_id: Uuid,
    locale: &LocaleManager,
) -> Vec<SettingsFieldRow> {
    match type_id {
        "weather" => orchid_widgets::builtin::weather::current_config(instance_id)
            .map(|cfg| weather_fields(&cfg, locale))
            .unwrap_or_default(),
        "moon" => orchid_widgets::builtin::moon::current_config(instance_id)
            .map(|cfg| moon_fields(&cfg, locale))
            .unwrap_or_default(),
        "clock" => orchid_widgets::builtin::clock::current_config(instance_id)
            .map(|cfg| clock_fields(&cfg, locale))
            .unwrap_or_default(),
        "system" => orchid_widgets::builtin::system::current_config(instance_id)
            .map(|cfg| system_fields(&cfg, locale))
            .unwrap_or_default(),
        "processes" => orchid_widgets::builtin::processes::current_config(instance_id)
            .map(|cfg| processes_fields(&cfg, locale))
            .unwrap_or_default(),
        "calculator" => orchid_widgets::builtin::calculator::current_config(instance_id)
            .map(|cfg| calculator_fields(&cfg, locale))
            .unwrap_or_default(),
        "rss" => orchid_widgets::builtin::rss::current_config(instance_id)
            .map(|cfg| rss_fields(&cfg, locale))
            .unwrap_or_default(),
        "file-manager" => orchid_widgets::builtin::file_manager::current_config(instance_id)
            .map(|cfg| fm_fields(&cfg, locale))
            .unwrap_or_default(),
        _ => vec![],
    }
}

fn weather_fields(cfg: &WeatherConfig, locale: &LocaleManager) -> Vec<SettingsFieldRow> {
    let mut rows = Vec::new();
    let units = match cfg.units {
        TemperatureUnit::Celsius => "celsius",
        TemperatureUnit::Fahrenheit => "fahrenheit",
    };
    push_combo(
        &mut rows,
        locale,
        "units",
        "widget-settings.weather.units",
        &[
            (
                "celsius".into(),
                locale
                    .tr("widget-settings.weather.units.celsius")
                    .into(),
            ),
            (
                "fahrenheit".into(),
                locale
                    .tr("widget-settings.weather.units.fahrenheit")
                    .into(),
            ),
        ],
        units,
    );
    push_text(
        &mut rows,
        locale,
        "refresh_minutes",
        "widget-settings.weather.refresh",
        cfg.refresh_interval_minutes.to_string(),
    );
    rows
}

fn moon_fields(cfg: &MoonConfig, locale: &LocaleManager) -> Vec<SettingsFieldRow> {
    let mut rows = Vec::new();
    push_text(
        &mut rows,
        locale,
        "location_name",
        "widget-settings.moon.location-name",
        cfg.location_name.clone(),
    );
    push_text(
        &mut rows,
        locale,
        "latitude",
        "widget-settings.moon.latitude",
        format!("{:.4}", cfg.latitude),
    );
    push_text(
        &mut rows,
        locale,
        "longitude",
        "widget-settings.moon.longitude",
        format!("{:.4}", cfg.longitude),
    );
    push_bool(
        &mut rows,
        locale,
        "show_sunrise_sunset",
        "widget-settings.moon.show-sunrise-sunset",
        cfg.show_sunrise_sunset,
    );
    push_bool(
        &mut rows,
        locale,
        "show_libration",
        "widget-settings.moon.show-libration",
        cfg.show_libration,
    );
    rows
}

fn clock_fields(
    cfg: &orchid_widgets::builtin::clock::ClockConfig,
    locale: &LocaleManager,
) -> Vec<SettingsFieldRow> {
    let mut rows = Vec::new();
    push_bool(
        &mut rows,
        locale,
        "show_seconds",
        "widget-settings.clock.show-seconds",
        cfg.show_seconds,
    );
    push_bool(
        &mut rows,
        locale,
        "show_dates",
        "widget-settings.clock.show-dates",
        cfg.show_dates,
    );
    push_bool(
        &mut rows,
        locale,
        "show_offsets",
        "widget-settings.clock.show-offsets",
        cfg.show_offsets,
    );
    rows
}


fn calculator_fields(
    cfg: &orchid_widgets::builtin::calculator::CalculatorConfig,
    locale: &LocaleManager,
) -> Vec<SettingsFieldRow> {
    let mut rows = Vec::new();
    push_combo(
        &mut rows,
        locale,
        "mode",
        "calc-settings-mode",
        &[
            (
                "0".into(),
                locale.tr("calc-settings-mode-standard").into(),
            ),
            (
                "1".into(),
                locale.tr("calc-settings-mode-scientific").into(),
            ),
        ],
        &cfg.mode.to_string(),
    );
    push_combo(
        &mut rows,
        locale,
        "angle_mode",
        "calc-settings-angle",
        &[
            ("0".into(), locale.tr("calc-settings-angle-deg").into()),
            ("1".into(), locale.tr("calc-settings-angle-rad").into()),
            ("2".into(), locale.tr("calc-settings-angle-grad").into()),
        ],
        &cfg.angle_mode.to_string(),
    );
    push_bool(
        &mut rows,
        locale,
        "show_history",
        "calc-settings-show-history",
        cfg.show_history,
    );
    rows
}

fn processes_fields(
    cfg: &orchid_widgets::builtin::processes::ProcessesConfig,
    locale: &LocaleManager,
) -> Vec<SettingsFieldRow> {
    let mut rows = Vec::new();
    push_text(
        &mut rows,
        locale,
        "refresh_seconds",
        "processes-settings-refresh",
        cfg.refresh_interval_seconds.to_string(),
    );
    push_bool(
        &mut rows,
        locale,
        "show_grouping",
        "processes-settings-grouping",
        cfg.show_grouping,
    );
    rows
}

fn system_fields(cfg: &SystemConfig, locale: &LocaleManager) -> Vec<SettingsFieldRow> {
    let mut rows = Vec::new();
    push_bool(
        &mut rows,
        locale,
        "show_cpu",
        "widget-settings.system.show-cpu",
        cfg.show_cpu,
    );
    push_bool(
        &mut rows,
        locale,
        "show_cpu_cores",
        "widget-settings.system.show-cpu-cores",
        cfg.show_cpu_cores,
    );
    push_bool(
        &mut rows,
        locale,
        "show_memory",
        "widget-settings.system.show-memory",
        cfg.show_memory,
    );
    push_bool(
        &mut rows,
        locale,
        "show_disks",
        "widget-settings.system.show-disks",
        cfg.show_disks,
    );
    push_bool(
        &mut rows,
        locale,
        "show_removable_disks",
        "widget-settings.system.show-removable-disks",
        cfg.show_removable_disks,
    );
    push_bool(
        &mut rows,
        locale,
        "show_network",
        "widget-settings.system.show-network",
        cfg.show_network,
    );
    push_bool(
        &mut rows,
        locale,
        "aggregate_network",
        "widget-settings.system.aggregate-network",
        cfg.aggregate_network,
    );
    push_bool(
        &mut rows,
        locale,
        "show_battery",
        "widget-settings.system.show-battery",
        cfg.show_battery,
    );
    push_bool(
        &mut rows,
        locale,
        "show_uptime",
        "widget-settings.system.show-uptime",
        cfg.show_uptime,
    );
    push_text(
        &mut rows,
        locale,
        "refresh_seconds",
        "widget-settings.system.refresh",
        cfg.refresh_interval_seconds.to_string(),
    );
    rows
}

fn rss_fields(cfg: &RssConfig, locale: &LocaleManager) -> Vec<SettingsFieldRow> {
    let mut rows = Vec::new();
    let (name, url) = cfg
        .feeds
        .first()
        .map(|f| (f.name.clone(), f.url.clone()))
        .unwrap_or_default();
    push_text(
        &mut rows,
        locale,
        "feed_name",
        "widget-settings.rss.feed-name",
        name,
    );
    push_text(
        &mut rows,
        locale,
        "feed_url",
        "widget-settings.rss.feed-url",
        url,
    );
    push_text(
        &mut rows,
        locale,
        "max_items",
        "widget-settings.rss.max-items",
        cfg.max_items_displayed.to_string(),
    );
    push_text(
        &mut rows,
        locale,
        "refresh_minutes",
        "widget-settings.rss.refresh",
        cfg.refresh_interval_minutes.to_string(),
    );
    push_bool(
        &mut rows,
        locale,
        "open_in_browser",
        "widget-settings.rss.open-in-browser",
        cfg.open_in_browser,
    );
    rows
}

fn fm_fields(cfg: &FileManagerConfig, locale: &LocaleManager) -> Vec<SettingsFieldRow> {
    let mut rows = Vec::new();
    push_bool(
        &mut rows,
        locale,
        "dual_pane",
        "widget-settings.fm.dual-pane",
        cfg.dual_pane,
    );
    push_bool(
        &mut rows,
        locale,
        "show_hidden",
        "widget-settings.fm.show-hidden",
        cfg.show_hidden,
    );
    push_bool(
        &mut rows,
        locale,
        "single_click_open",
        "widget-settings.fm.single-click-open",
        matches!(cfg.click_behavior, ClickBehavior::SingleToOpen),
    );
    push_bool(
        &mut rows,
        locale,
        "show_extensions",
        "widget-settings.fm.show-extensions",
        cfg.show_extensions,
    );
    push_bool(
        &mut rows,
        locale,
        "confirm_delete",
        "widget-settings.fm.confirm-delete",
        cfg.confirm_delete,
    );
    push_bool(
        &mut rows,
        locale,
        "delete_to_recycle",
        "widget-settings.fm.delete-to-recycle",
        cfg.delete_to_recycle,
    );
    let thumb = match cfg.thumbnail_size {
        ThumbnailSize::Small => "small",
        ThumbnailSize::Medium => "medium",
        ThumbnailSize::Large => "large",
    };
    push_combo(
        &mut rows,
        locale,
        "thumbnail_size",
        "widget-settings.fm.thumbnail-size",
        &[
            (
                "small".into(),
                locale
                    .tr("widget-settings.fm.thumbnail-size.small")
                    .into(),
            ),
            (
                "medium".into(),
                locale
                    .tr("widget-settings.fm.thumbnail-size.medium")
                    .into(),
            ),
            (
                "large".into(),
                locale
                    .tr("widget-settings.fm.thumbnail-size.large")
                    .into(),
            ),
        ],
        thumb,
    );
    rows
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Apply one settings-dialog field change to the live widget.
pub(crate) async fn apply_widget_setting(type_id: &str, instance_id: Uuid, key: &str, value: &str) {
    match type_id {
        "weather" => apply_weather(instance_id, key, value),
        "moon" => apply_moon(instance_id, key, value),
        "clock" => apply_clock(instance_id, key, value),
        "system" => apply_system(instance_id, key, value),
        "processes" => apply_processes(instance_id, key, value),
        "calculator" => apply_calculator(instance_id, key, value),
        "rss" => apply_rss(instance_id, key, value),
        "file-manager" => {
            let _ = apply_fm(instance_id, key, value).await;
        }
        _ => {}
    }
}

fn apply_weather(instance_id: Uuid, key: &str, value: &str) {
    orchid_widgets::builtin::weather::update_config(instance_id, |cfg| match key {
        "units" => {
            cfg.units = match value {
                "fahrenheit" => TemperatureUnit::Fahrenheit,
                _ => TemperatureUnit::Celsius,
            };
        }
        "refresh_minutes" => {
            if let Ok(n) = value.parse::<u32>() {
                cfg.refresh_interval_minutes = n.max(1);
            }
        }
        _ => {}
    });
}

fn apply_clock(instance_id: Uuid, key: &str, value: &str) {
    orchid_widgets::builtin::clock::update_config(instance_id, |cfg| match key {
        "show_seconds" => {
            if let Some(b) = parse_bool(value) {
                cfg.show_seconds = b;
            }
        }
        "show_dates" => {
            if let Some(b) = parse_bool(value) {
                cfg.show_dates = b;
            }
        }
        "show_offsets" => {
            if let Some(b) = parse_bool(value) {
                cfg.show_offsets = b;
            }
        }
        _ => {}
    });
}

fn apply_moon(instance_id: Uuid, key: &str, value: &str) {
    orchid_widgets::builtin::moon::update_config(instance_id, |cfg| match key {
        "location_name" => cfg.location_name = value.to_string(),
        "latitude" => {
            if let Ok(n) = value.parse::<f64>() {
                cfg.latitude = n.clamp(-90.0, 90.0);
            }
        }
        "longitude" => {
            if let Ok(n) = value.parse::<f64>() {
                cfg.longitude = n.clamp(-180.0, 180.0);
            }
        }
        "show_sunrise_sunset" => {
            if let Some(b) = parse_bool(value) {
                cfg.show_sunrise_sunset = b;
            }
        }
        "show_libration" => {
            if let Some(b) = parse_bool(value) {
                cfg.show_libration = b;
            }
        }
        _ => {}
    });
}

fn apply_system(instance_id: Uuid, key: &str, value: &str) {
    orchid_widgets::builtin::system::update_config(instance_id, |cfg| match key {
        "show_cpu" => {
            if let Some(b) = parse_bool(value) {
                cfg.show_cpu = b;
            }
        }
        "show_cpu_cores" => {
            if let Some(b) = parse_bool(value) {
                cfg.show_cpu_cores = b;
            }
        }
        "show_memory" => {
            if let Some(b) = parse_bool(value) {
                cfg.show_memory = b;
            }
        }
        "show_disks" => {
            if let Some(b) = parse_bool(value) {
                cfg.show_disks = b;
            }
        }
        "show_removable_disks" => {
            if let Some(b) = parse_bool(value) {
                cfg.show_removable_disks = b;
            }
        }
        "show_network" => {
            if let Some(b) = parse_bool(value) {
                cfg.show_network = b;
            }
        }
        "aggregate_network" => {
            if let Some(b) = parse_bool(value) {
                cfg.aggregate_network = b;
            }
        }
        "show_battery" => {
            if let Some(b) = parse_bool(value) {
                cfg.show_battery = b;
            }
        }
        "show_uptime" => {
            if let Some(b) = parse_bool(value) {
                cfg.show_uptime = b;
            }
        }
        "refresh_seconds" => {
            if let Ok(n) = value.parse::<u32>() {
                cfg.refresh_interval_seconds = n.max(1);
            }
        }
        _ => {}
    });
}

fn apply_processes(instance_id: Uuid, key: &str, value: &str) {
    orchid_widgets::builtin::processes::update_config(instance_id, |cfg| match key {
        "refresh_seconds" => {
            if let Ok(n) = value.parse::<u32>() {
                cfg.refresh_interval_seconds = n.max(1);
            }
        }
        "show_grouping" => {
            if let Some(b) = parse_bool(value) {
                cfg.show_grouping = b;
            }
        }
        _ => {}
    });
}

fn apply_calculator(instance_id: Uuid, key: &str, value: &str) {
    orchid_widgets::builtin::calculator::update_config(instance_id, |cfg| match key {
        "mode" => {
            if let Ok(n) = value.parse::<u8>() {
                cfg.mode = n;
            }
        }
        "angle_mode" => {
            if let Ok(n) = value.parse::<u8>() {
                cfg.angle_mode = n;
            }
        }
        "show_history" => {
            if let Some(b) = parse_bool(value) {
                cfg.show_history = b;
            }
        }
        _ => {}
    });
}

fn apply_rss(instance_id: Uuid, key: &str, value: &str) {
    orchid_widgets::builtin::rss::update_config(instance_id, |cfg| match key {
        "feed_name" => {
            if let Some(feed) = cfg.feeds.first_mut() {
                feed.name = value.to_string();
            }
        }
        "feed_url" => {
            if let Some(feed) = cfg.feeds.first_mut() {
                feed.url = value.to_string();
            }
        }
        "max_items" => {
            if let Ok(n) = value.parse::<u32>() {
                cfg.max_items_displayed = n.max(1);
            }
        }
        "refresh_minutes" => {
            if let Ok(n) = value.parse::<u32>() {
                cfg.refresh_interval_minutes = n.max(1);
            }
        }
        "open_in_browser" => {
            if let Some(b) = parse_bool(value) {
                cfg.open_in_browser = b;
            }
        }
        _ => {}
    });
}

async fn apply_fm(instance_id: Uuid, key: &str, value: &str) -> orchid_widgets::Result<()> {
    orchid_widgets::builtin::file_manager::update_config(instance_id, |cfg| match key {
        "dual_pane" => {
            if let Some(b) = parse_bool(value) {
                cfg.dual_pane = b;
            }
        }
        "show_hidden" => {
            if let Some(b) = parse_bool(value) {
                cfg.show_hidden = b;
            }
        }
        "single_click_open" => {
            if let Some(b) = parse_bool(value) {
                cfg.click_behavior = if b {
                    ClickBehavior::SingleToOpen
                } else {
                    ClickBehavior::DoubleToOpen
                };
            }
        }
        "show_extensions" => {
            if let Some(b) = parse_bool(value) {
                cfg.show_extensions = b;
            }
        }
        "confirm_delete" => {
            if let Some(b) = parse_bool(value) {
                cfg.confirm_delete = b;
            }
        }
        "delete_to_recycle" => {
            if let Some(b) = parse_bool(value) {
                cfg.delete_to_recycle = b;
            }
        }
        "thumbnail_size" => {
            cfg.thumbnail_size = match value {
                "small" => ThumbnailSize::Small,
                "large" => ThumbnailSize::Large,
                _ => ThumbnailSize::Medium,
            };
        }
        _ => {}
    })
    .await
}
