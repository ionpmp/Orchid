//! Slint model builders for workspace widget frames.

use slint::{Model, ModelRc, VecModel};

mod audio_player;
mod calculator;
mod calendar;
mod clock;
mod file_manager;
mod jyotish;
mod media;
mod moon;
mod notes;
mod palette;
mod password;
mod processes;
mod recent;
mod rss;
mod search;
mod settings;
mod system;
mod terminal;
mod video_player;
mod viewer;
mod weather;
mod widget_settings;

pub(crate) use audio_player::{
    build_audio_player_model, empty_audio_player_model, patch_audio_player_model,
};
pub(crate) use calculator::{
    build_calculator_model, empty_calculator_model, patch_calculator_model,
};
pub(crate) use calendar::{build_calendar_model, empty_calendar_model, patch_calendar_model};
pub(crate) use clock::{build_clock_model, empty_clock_model, patch_clock_model};
pub(crate) use file_manager::{
    build_context_menu, build_file_manager_model, build_managed_policy_state, empty_confirm_dialog,
    empty_conflict_dialog, empty_context_menu, empty_file_manager_model, empty_find_state,
    empty_fm_overlays, empty_managed_policy_state, empty_passphrase_state, empty_rename_state,
    empty_tag_state, fm_grid_rebase_slack, fm_grid_visible_range, fm_grid_window,
    fm_list_visible_range, fm_list_window, fm_passphrase_dialog_labels, fm_window_covers,
    patch_file_manager_model, patch_fm_selection, sync_fm_path_suggestions, FileManagerOverlays,
    FmViewport, FM_LIST_REBASE_SLACK,
};
pub(crate) use jyotish::{build_jyotish_model, empty_jyotish_model, patch_jyotish_model};
pub(crate) use media::{build_media_model, empty_media_model, patch_media_model};
pub(crate) use moon::{build_moon_model, empty_moon_model, patch_moon_model};
pub(crate) use notes::{build_notes_model, empty_notes_model, patch_notes_model};
pub(crate) use palette::build_palette_candidates;
pub(crate) use password::{
    build_password_model, empty_password_model, patch_password_model, PasswordAddDialogOverlay,
};
pub(crate) use processes::{
    build_processes_model, empty_processes_confirm, empty_processes_model, patch_processes_model,
};
pub(crate) use recent::{
    build_recent_files_model, empty_recent_files_model, patch_recent_files_model,
};
pub(crate) use rss::{build_rss_model, empty_rss_model, patch_rss_model};
pub(crate) use search::{build_search_model, empty_search_model, patch_search_model};
pub(crate) use settings::{
    build_settings_fields, build_settings_sections, locale_display_name, settings_section_id,
    settings_section_index, theme_display_name, SETTINGS_SECTION_IDS,
};
pub(crate) use system::{build_system_model, empty_system_model, patch_system_model};
pub(crate) use terminal::{
    blank_terminal, build_terminal_divider_models, build_terminal_tab_models,
    default_terminal_divider_models, default_terminal_pane_models, default_terminal_tab_models,
    empty_terminal_cells,
};
pub(crate) use video_player::{
    build_video_player_model, empty_video_player_model, patch_video_player_model,
};
pub(crate) use viewer::{build_viewer_model, empty_viewer_model, patch_viewer_model};
pub(crate) use weather::{build_weather_model, empty_weather_model, patch_weather_model};
pub(crate) use widget_settings::{
    apply_widget_setting, build_widget_settings_fields, widget_has_settings,
};

/// Sync a `VecModel` in place, skipping rows that already compare equal.
pub(crate) fn sync_eq_rows<T: Clone + PartialEq + 'static>(model: &ModelRc<T>, new_rows: Vec<T>) {
    let Some(v) = model.as_any().downcast_ref::<VecModel<T>>() else {
        return;
    };
    while v.row_count() > new_rows.len() {
        v.remove(v.row_count() - 1);
    }
    for (i, row) in new_rows.into_iter().enumerate() {
        if i < v.row_count() {
            if let Some(old) = v.row_data(i) {
                if old == row {
                    continue;
                }
            }
            v.set_row_data(i, row);
        } else {
            v.push(row);
        }
    }
}

/// Copy rows from a freshly built `VecModel` into a kept `ModelRc` identity.
pub(crate) fn adopt_eq_rows<T: Clone + PartialEq + 'static>(keep: &ModelRc<T>, built: &ModelRc<T>) {
    let Some(src) = built.as_any().downcast_ref::<VecModel<T>>() else {
        return;
    };
    let rows: Vec<T> = (0..src.row_count())
        .filter_map(|i| src.row_data(i))
        .collect();
    sync_eq_rows(keep, rows);
}
