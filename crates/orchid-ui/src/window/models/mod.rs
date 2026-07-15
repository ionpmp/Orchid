//! Slint model builders for workspace widget frames.

mod file_manager;
mod media;
mod moon;
mod palette;
mod password;
mod recent;
mod rss;
mod search;
mod settings;
mod system;
mod terminal;
mod viewer;
mod weather;
mod widget_settings;

pub(crate) use file_manager::{
    build_context_menu, build_file_manager_model, build_managed_policy_state, empty_confirm_dialog,
    empty_context_menu, empty_file_manager_model, empty_managed_policy_state,
    empty_passphrase_state, empty_rename_state, empty_tag_state, fm_passphrase_dialog_labels,
    FileManagerOverlays,
};
pub(crate) use media::{build_media_model, empty_media_model};
pub(crate) use moon::{build_moon_model, empty_moon_model};
pub(crate) use palette::build_palette_candidates;
pub(crate) use password::{build_password_model, empty_password_model, PasswordAddDialogOverlay};
pub(crate) use recent::{build_recent_files_model, empty_recent_files_model};
pub(crate) use rss::{build_rss_model, empty_rss_model};
pub(crate) use search::{build_search_model, empty_search_model};
pub(crate) use settings::{
    build_settings_fields, build_settings_sections, locale_display_name, settings_section_id,
    settings_section_index, theme_display_name, SETTINGS_SECTION_IDS,
};
pub(crate) use system::{build_system_model, empty_system_model};
pub(crate) use terminal::{
    blank_terminal, build_terminal_divider_models, build_terminal_model, build_terminal_tab_models,
    default_terminal_divider_models, default_terminal_pane_models, default_terminal_tab_models,
    pane_payload_to_terminal,
};
pub(crate) use viewer::{build_viewer_model, empty_viewer_model};
pub(crate) use weather::{build_weather_model, empty_weather_model};
pub(crate) use widget_settings::{
    apply_widget_setting, build_widget_settings_fields, widget_has_settings,
};
