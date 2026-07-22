//! Settings panel Slint model builders.

use orchid_core::CommandRegistry;
use orchid_i18n::{LocaleId, LocaleManager};
use orchid_storage::OrchidConfig;
use slint::{ModelRc, SharedString, VecModel};

use crate::slint_generated::{SettingsFieldRow, SettingsSectionEntry};
use crate::theme::ThemeManager;

pub(crate) const SETTINGS_SECTION_IDS: &[&str] = &[
    "general",
    "appearance",
    "input",
    "shortcuts",
    "locale",
    "privacy",
];

pub(crate) fn build_settings_sections(locale: &LocaleManager) -> Vec<SettingsSectionEntry> {
    SETTINGS_SECTION_IDS
        .iter()
        .map(|id| {
            let key = format!("settings-section-{id}");
            SettingsSectionEntry {
                id: (*id).into(),
                label: locale.tr(&key).into(),
            }
        })
        .collect()
}

pub(crate) fn settings_section_index(section: &str) -> i32 {
    SETTINGS_SECTION_IDS
        .iter()
        .position(|&id| id == section)
        .map_or(0, |i| i as i32)
}

pub(crate) fn settings_section_id(index: i32) -> &'static str {
    SETTINGS_SECTION_IDS
        .get(index as usize)
        .copied()
        .unwrap_or(SETTINGS_SECTION_IDS[0])
}

const SETTINGS_FIELD_READONLY: i32 = 0;
const SETTINGS_FIELD_BOOL: i32 = 1;
const SETTINGS_FIELD_TEXT: i32 = 2;
const SETTINGS_FIELD_COMBO: i32 = 3;

fn settings_strings_model(values: Vec<SharedString>) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(values))
}

fn push_settings_readonly(
    rows: &mut Vec<SettingsFieldRow>,
    locale: &LocaleManager,
    key: &str,
    label_key: &str,
    value: SharedString,
) {
    rows.push(SettingsFieldRow {
        key: key.into(),
        label: locale.tr(label_key).into(),
        kind: SETTINGS_FIELD_READONLY,
        value,
        bool_value: false,
        combo_options: settings_strings_model(vec![]),
        combo_values: settings_strings_model(vec![]),
        combo_index: -1,
    });
}

fn push_settings_bool(
    rows: &mut Vec<SettingsFieldRow>,
    locale: &LocaleManager,
    key: &str,
    label_key: &str,
    value: bool,
) {
    rows.push(SettingsFieldRow {
        key: key.into(),
        label: locale.tr(label_key).into(),
        kind: SETTINGS_FIELD_BOOL,
        value: SharedString::default(),
        bool_value: value,
        combo_options: settings_strings_model(vec![]),
        combo_values: settings_strings_model(vec![]),
        combo_index: -1,
    });
}

fn push_settings_text(
    rows: &mut Vec<SettingsFieldRow>,
    locale: &LocaleManager,
    key: &str,
    label_key: &str,
    value: impl Into<SharedString>,
) {
    rows.push(SettingsFieldRow {
        key: key.into(),
        label: locale.tr(label_key).into(),
        kind: SETTINGS_FIELD_TEXT,
        value: value.into(),
        bool_value: false,
        combo_options: settings_strings_model(vec![]),
        combo_values: settings_strings_model(vec![]),
        combo_index: -1,
    });
}

fn push_settings_combo(
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
        kind: SETTINGS_FIELD_COMBO,
        value: SharedString::default(),
        bool_value: false,
        combo_options: settings_strings_model(combo_options),
        combo_values: settings_strings_model(combo_values),
        combo_index,
    });
}

fn density_combo_options(locale: &LocaleManager) -> Vec<(SharedString, SharedString)> {
    vec![
        ("touch".into(), locale.tr("density-touch").into()),
        ("mouse".into(), locale.tr("density-mouse").into()),
        ("hybrid".into(), locale.tr("density-hybrid").into()),
    ]
}

fn density_storage_key(density: orchid_storage::Density) -> &'static str {
    match density {
        orchid_storage::Density::Touch => "touch",
        orchid_storage::Density::Mouse => "mouse",
        orchid_storage::Density::Hybrid => "hybrid",
    }
}

fn theme_combo_options(
    themes: &ThemeManager,
    locale: &LocaleManager,
) -> Vec<(SharedString, SharedString)> {
    themes
        .list()
        .into_iter()
        .map(|meta| {
            (
                meta.id.clone().into(),
                theme_display_name(locale, &meta.id, &meta.display_name).into(),
            )
        })
        .collect()
}

pub(crate) fn theme_display_name(locale: &LocaleManager, id: &str, fallback: &str) -> String {
    let key = format!("theme-name-{id}");
    let name = locale.tr(&key);
    if name == key {
        fallback.to_string()
    } else {
        name
    }
}

fn locale_combo_options(locale_mgr: &LocaleManager) -> Vec<(SharedString, SharedString)> {
    locale_mgr
        .available_locales()
        .into_iter()
        .map(|id| {
            let tag = id.to_string();
            let label = locale_display_name(locale_mgr, &id);
            (tag.into(), label.into())
        })
        .collect()
}

pub(crate) fn locale_display_name(locale_mgr: &LocaleManager, id: &LocaleId) -> String {
    let key = format!("locale-name-{id}");
    let name = locale_mgr.tr(&key);
    if name == key {
        id.to_string()
    } else {
        name
    }
}


fn command_display_label(
    registry: &CommandRegistry,
    locale: &LocaleManager,
    cmd_id: &str,
) -> String {
    registry
        .get(cmd_id)
        .map(|d| locale.tr(&d.display_name_key))
        .unwrap_or_else(|| cmd_id.to_string())
}

pub(crate) fn build_settings_fields(
    section: &str,
    cfg: &OrchidConfig,
    locale: &LocaleManager,
    themes: &ThemeManager,
    registry: &CommandRegistry,
) -> Vec<SettingsFieldRow> {
    let mut rows = Vec::new();

    match section {
        "general" => {
            // Auto-update / telemetry are reserved for a later release — keep
            // them visible but read-only so toggles do not pretend to work.
            push_settings_readonly(
                &mut rows,
                locale,
                "auto-update",
                "settings-field-auto-update",
                locale.tr("settings-value-disabled").into(),
            );
            push_settings_readonly(
                &mut rows,
                locale,
                "telemetry",
                "settings-field-telemetry",
                locale.tr("settings-value-disabled").into(),
            );
            push_settings_bool(
                &mut rows,
                locale,
                "open-on-startup",
                "settings-field-open-on-startup",
                cfg.general.open_on_startup,
            );
        }
        "appearance" => {
            let theme_options = theme_combo_options(themes, locale);
            push_settings_combo(
                &mut rows,
                locale,
                "theme",
                "settings-field-theme",
                &theme_options,
                &cfg.appearance.theme,
            );
            push_settings_combo(
                &mut rows,
                locale,
                "density",
                "settings-field-density",
                &density_combo_options(locale),
                density_storage_key(cfg.appearance.density),
            );
            push_settings_text(
                &mut rows,
                locale,
                "font-family",
                "settings-field-font-family",
                cfg.appearance
                    .font_family
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| locale.tr("settings-value-system-default")),
            );
            push_settings_text(
                &mut rows,
                locale,
                "font-scale",
                "settings-field-font-scale",
                format!("{:.2}", cfg.appearance.font_scale),
            );
            push_settings_bool(
                &mut rows,
                locale,
                "reduce-motion",
                "settings-field-reduce-motion",
                cfg.appearance.reduce_motion,
            );
            push_settings_bool(
                &mut rows,
                locale,
                "follow-system-theme",
                "settings-field-follow-system-theme",
                cfg.appearance.follow_system_theme,
            );
            push_settings_combo(
                &mut rows,
                locale,
                "dark-theme",
                "settings-field-dark-theme",
                &theme_options,
                &cfg.appearance.dark_theme,
            );
            push_settings_combo(
                &mut rows,
                locale,
                "light-theme",
                "settings-field-light-theme",
                &theme_options,
                &cfg.appearance.light_theme,
            );
        }
        "input" => {
            push_settings_combo(
                &mut rows,
                locale,
                "primary-hand",
                "settings-field-primary-hand",
                &[
                    ("left".into(), locale.tr("settings-value-hand-left").into()),
                    ("right".into(), locale.tr("settings-value-hand-right").into()),
                ],
                match cfg.input.primary_hand {
                    orchid_storage::Hand::Left => "left",
                    orchid_storage::Hand::Right => "right",
                },
            );
            push_settings_bool(
                &mut rows,
                locale,
                "mirror-edge-swipes",
                "settings-field-mirror-edge-swipes",
                cfg.input.mirror_edge_swipes,
            );
            // Haptics / palm rejection / pen double-tap need platform pen+haptic
            // plumbing — mark bools as unavailable rather than Yes/No.
            push_settings_readonly(
                &mut rows,
                locale,
                "haptic-feedback",
                "settings-field-haptic-feedback",
                locale.tr("settings-value-disabled").into(),
            );
            push_settings_readonly(
                &mut rows,
                locale,
                "palm-rejection",
                "settings-field-palm-rejection",
                locale.tr("settings-value-disabled").into(),
            );
            let pen_label = match cfg.input.pen_double_tap_action {
                orchid_storage::PenDoubleTapAction::None => {
                    locale.tr("settings-value-pen-double-tap-none")
                }
                orchid_storage::PenDoubleTapAction::SwitchTool => {
                    locale.tr("settings-value-pen-double-tap-switch-tool")
                }
                orchid_storage::PenDoubleTapAction::Erase => {
                    locale.tr("settings-value-pen-double-tap-erase")
                }
            };
            push_settings_readonly(
                &mut rows,
                locale,
                "pen-double-tap",
                "settings-field-pen-double-tap",
                pen_label.into(),
            );
        }
        "shortcuts" => {
            push_settings_readonly(
                &mut rows,
                locale,
                "leader-key",
                "settings-field-leader-key",
                cfg
                    .shortcuts
                    .leader_key
                    .clone()
                    .unwrap_or_else(|| locale.tr("settings-value-none").into())
                    .into(),
            );
            push_settings_readonly(
                &mut rows,
                locale,
                "leader-timeout",
                "settings-field-leader-timeout",
                locale
                    .tr_args(
                        "settings-value-leader-timeout",
                        &orchid_i18n::FluentArgs::new()
                            .with("ms", cfg.shortcuts.leader_timeout_ms.to_string()),
                    )
                    .into(),
            );
            push_settings_readonly(
                &mut rows,
                locale,
                "leader-bindings",
                "settings-field-leader-bindings",
                if cfg.shortcuts.leader_bindings.is_empty() {
                    locale.tr("settings-value-none").into()
                } else {
                    let mut pairs: Vec<_> = cfg.shortcuts.leader_bindings.iter().collect();
                    pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
                    let sep = locale.tr("settings-shortcut-list-separator");
                    pairs
                        .into_iter()
                        .map(|(key, cmd)| {
                            let cmd_label = command_display_label(registry, locale, cmd);
                            locale.tr_args(
                                "settings-shortcut-binding",
                                &orchid_i18n::FluentArgs::new()
                                    .with("key", key.as_str())
                                    .with("cmd", cmd_label),
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(&sep)
                        .into()
                },
            );
            if cfg.shortcuts.overrides.is_empty() {
                push_settings_readonly(
                    &mut rows,
                    locale,
                    "shortcut-overrides",
                    "settings-field-shortcut-overrides",
                    locale.tr("settings-value-none").into(),
                );
            } else {
                let mut pairs: Vec<_> = cfg.shortcuts.overrides.iter().collect();
                pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
                for (cmd, shortcut) in pairs {
                    rows.push(SettingsFieldRow {
                        key: format!("override:{cmd}").into(),
                        label: command_display_label(registry, locale, cmd).into(),
                        kind: SETTINGS_FIELD_READONLY,
                        value: shortcut.clone().into(),
                        bool_value: false,
                        combo_options: settings_strings_model(vec![]),
                        combo_values: settings_strings_model(vec![]),
                        combo_index: -1,
                    });
                }
            }
        }
        "locale" => {
            push_settings_combo(
                &mut rows,
                locale,
                "language",
                "settings-field-language",
                &locale_combo_options(locale),
                &cfg.locale.language,
            );
            push_settings_text(
                &mut rows,
                locale,
                "date-format",
                "settings-field-date-format",
                cfg.locale
                    .date_format
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| locale.tr("settings-value-default")),
            );
            push_settings_text(
                &mut rows,
                locale,
                "time-format",
                "settings-field-time-format",
                cfg.locale
                    .time_format
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| locale.tr("settings-value-default")),
            );
            push_settings_readonly(
                &mut rows,
                locale,
                "first-day-of-week",
                "settings-field-first-day-of-week",
                if cfg.locale.first_day_of_week == 0 {
                    locale.tr("settings-value-sunday").into()
                } else {
                    locale.tr("settings-value-monday").into()
                },
            );
        }
        "privacy" => {
            push_settings_bool(
                &mut rows,
                locale,
                "record-action-history",
                "settings-field-record-action-history",
                cfg.privacy.record_action_history,
            );
            push_settings_text(
                &mut rows,
                locale,
                "history-retention-days",
                "settings-field-history-retention-days",
                format!("{}", cfg.privacy.history_retention_days),
            );
            push_settings_text(
                &mut rows,
                locale,
                "clear-clipboard-seconds",
                "settings-field-clear-clipboard-seconds",
                format!("{}", cfg.privacy.clear_clipboard_seconds),
            );
            push_settings_text(
                &mut rows,
                locale,
                "vault-auto-lock-seconds",
                "settings-field-vault-auto-lock",
                format!("{}", cfg.privacy.vault_auto_lock_seconds),
            );
        }
        _ => {}
    }
    rows
}
