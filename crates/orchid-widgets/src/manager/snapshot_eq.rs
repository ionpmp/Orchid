//! Render-equality helpers for [`super::snapshot_renders_unchanged`].

use crate::widget::payloads::{
    AudioPlayerPayload, CalculatorPayload, CalendarPayload, ClockPayload, EntryPayload,
    FileManagerPayload, JyotishAntarRow, JyotishDayChip, JyotishFactorRow, JyotishMonthCell,
    JyotishMonthSummary, JyotishPayload, JyotishYearSummary, MediaPlayerPayload, MoonPayload,
    NotesPayload, PasswordEntryDetailView, PasswordEntryView, PasswordManagerPayload,
    ProcessRowView, ProcessesPayload, RecentFilesPayload, RssItemView, RssPayload,
    SearchCandidateView, ServiceRowView, StartupRowView, SystemIndicator, SystemPayload,
    UniversalSearchPayload, UserRowView, VideoPlayerPayload, ViewerPayload, WeatherForecastDay,
    WeatherPayload,
};
use crate::widget::snapshot::{TerminalPanePayload, TerminalPayload, WidgetPayload};

/// `true` when two payloads would draw the same in the UI.
pub(crate) fn payload_renders_equal(a: &WidgetPayload, b: &WidgetPayload) -> bool {
    match (a, b) {
        (WidgetPayload::Empty, WidgetPayload::Empty) => true,
        (WidgetPayload::Text { lines: la }, WidgetPayload::Text { lines: lb }) => la == lb,
        (
            WidgetPayload::KeyValueList { entries: ea },
            WidgetPayload::KeyValueList { entries: eb },
        ) => ea == eb,
        (WidgetPayload::Terminal(a), WidgetPayload::Terminal(b)) => terminal_payload_eq(a, b),
        (WidgetPayload::Weather(a), WidgetPayload::Weather(b)) => weather_payload_eq(a, b),
        (WidgetPayload::Moon(a), WidgetPayload::Moon(b)) => moon_payload_eq(a, b),
        (WidgetPayload::Jyotish(a), WidgetPayload::Jyotish(b)) => jyotish_payload_eq(a, b),
        (WidgetPayload::Clock(a), WidgetPayload::Clock(b)) => clock_payload_eq(a, b),
        (WidgetPayload::SystemIndicators(a), WidgetPayload::SystemIndicators(b)) => {
            system_payload_eq(a, b)
        }
        (WidgetPayload::Processes(a), WidgetPayload::Processes(b)) => processes_payload_eq(a, b),
        (WidgetPayload::Calculator(a), WidgetPayload::Calculator(b)) => calculator_payload_eq(a, b),
        (WidgetPayload::Notes(a), WidgetPayload::Notes(b)) => notes_payload_eq(a, b),
        (WidgetPayload::Calendar(a), WidgetPayload::Calendar(b)) => calendar_payload_eq(a, b),
        (WidgetPayload::RssFeed(a), WidgetPayload::RssFeed(b)) => rss_payload_eq(a, b),
        (WidgetPayload::UniversalSearch(a), WidgetPayload::UniversalSearch(b)) => {
            search_payload_eq(a, b)
        }
        (WidgetPayload::MediaPlayer(a), WidgetPayload::MediaPlayer(b)) => media_payload_eq(a, b),
        (WidgetPayload::AudioPlayer(a), WidgetPayload::AudioPlayer(b)) => {
            audio_player_payload_eq(a, b)
        }
        (WidgetPayload::VideoPlayer(a), WidgetPayload::VideoPlayer(b)) => {
            video_player_payload_eq(a, b)
        }
        (WidgetPayload::PasswordManager(a), WidgetPayload::PasswordManager(b)) => {
            password_payload_eq(a, b)
        }
        (WidgetPayload::RecentFiles(a), WidgetPayload::RecentFiles(b)) => {
            recent_files_payload_eq(a, b)
        }
        // Viewer / file-manager carry large trees; compare structurally when possible.
        (WidgetPayload::Viewer(a), WidgetPayload::Viewer(b)) => viewer_payload_eq(a, b),
        (WidgetPayload::FileManager(a), WidgetPayload::FileManager(b)) => {
            file_manager_payload_eq(a, b)
        }
        // Different payload kinds never render the same.
        _ => false,
    }
}

fn viewer_payload_eq(a: &ViewerPayload, b: &ViewerPayload) -> bool {
    use orchid_viewers::ViewerSnapshot as Vs;
    match (&a.snapshot, &b.snapshot) {
        (Vs::Loading { path_display: pa }, Vs::Loading { path_display: pb }) => pa == pb,
        (
            Vs::Error {
                path_display: pa,
                message: ma,
            },
            Vs::Error {
                path_display: pb,
                message: mb,
            },
        ) => pa == pb && ma == mb,
        (Vs::Image(a), Vs::Image(b)) => {
            a.path_display == b.path_display
                && a.width_px == b.width_px
                && a.height_px == b.height_px
                && std::sync::Arc::ptr_eq(&a.rgba_bytes, &b.rgba_bytes)
                && a.zoom.to_bits() == b.zoom.to_bits()
                && a.pan_x.to_bits() == b.pan_x.to_bits()
                && a.pan_y.to_bits() == b.pan_y.to_bits()
                && a.rotation_degrees.to_bits() == b.rotation_degrees.to_bits()
                && a.flipped_horizontal == b.flipped_horizontal
                && a.flipped_vertical == b.flipped_vertical
                && a.fit_mode == b.fit_mode
                && a.format_label == b.format_label
                && a.size_bytes == b.size_bytes
        }
        (Vs::Pdf(a), Vs::Pdf(b)) => {
            a.path_display == b.path_display
                && a.page_count == b.page_count
                && a.current_page == b.current_page
                && a.page_width_px == b.page_width_px
                && a.page_height_px == b.page_height_px
                && std::sync::Arc::ptr_eq(&a.page_rgba_bytes, &b.page_rgba_bytes)
                && a.zoom.to_bits() == b.zoom.to_bits()
                && a.fit_mode == b.fit_mode
        }
        (Vs::Text(a), Vs::Text(b)) => {
            a.path_display == b.path_display
                && a.language == b.language
                && a.encoding == b.encoding
                && a.line_ending == b.line_ending
                && a.dirty == b.dirty
                && a.read_only == b.read_only
                && a.total_lines == b.total_lines
                && a.first_visible_line == b.first_visible_line
                && a.cursor_line == b.cursor_line
                && a.cursor_column == b.cursor_column
                && selection_eq(a.selection, b.selection)
                && text_lines_eq(&a.visible_lines, &b.visible_lines)
                && (std::sync::Arc::ptr_eq(&a.plain_text, &b.plain_text)
                    || a.plain_text.as_ref() == b.plain_text.as_ref())
                && a.display_mode == b.display_mode
                && a.find_gen == b.find_gen
                && a.find_match_index == b.find_match_index
                && a.find_match_count == b.find_match_count
                && a.can_undo == b.can_undo
                && a.can_redo == b.can_redo
        }
        (Vs::Archive(a), Vs::Archive(b)) => {
            a.path_display == b.path_display
                && a.format == b.format
                && a.total_entries == b.total_entries
                && a.current_inner_path == b.current_inner_path
                && a.selected_path == b.selected_path
                && a.entries.len() == b.entries.len()
                && a.entries.iter().zip(b.entries.iter()).all(|(x, y)| {
                    x.path_in_archive == y.path_in_archive
                        && x.name == y.name
                        && x.is_dir == y.is_dir
                        && x.size == y.size
                        && x.modified_text == y.modified_text
                        && x.icon == y.icon
                })
                && archive_preview_eq(&a.preview, &b.preview)
                && archive_status_eq(&a.status, &b.status)
        }
        (Vs::Document(a), Vs::Document(b)) => {
            a.path_display == b.path_display
                && a.dirty == b.dirty
                && a.block_count == b.block_count
                && a.warnings == b.warnings
                && a.bold == b.bold
                && a.italic == b.italic
                && a.underline == b.underline
                && a.strikethrough == b.strikethrough
                && a.highlight == b.highlight
                && a.all_caps == b.all_caps
                && a.small_caps == b.small_caps
                && a.vanish == b.vanish
                && a.shadow == b.shadow
                && a.shade == b.shade
                && a.border_bottom == b.border_bottom
                && a.keep_next == b.keep_next
                && a.keep_lines == b.keep_lines
                && a.widow_control == b.widow_control
                && a.contextual_spacing == b.contextual_spacing
                && a.bidi == b.bidi
                && a.suppress_auto_hyphens == b.suppress_auto_hyphens
                && a.outline_level == b.outline_level
                && a.superscript == b.superscript
                && a.subscript == b.subscript
                && a.font_size_pt.to_bits() == b.font_size_pt.to_bits()
                && a.font_family == b.font_family
                && a.color_rgb == b.color_rgb
                && a.alignment == b.alignment
                && a.list_kind == b.list_kind
                && a.can_undo == b.can_undo
                && a.can_redo == b.can_redo
                && a.source_mode == b.source_mode
                && a.preview_width_px == b.preview_width_px
                && a.preview_height_px == b.preview_height_px
                && a.preview_render_scale == b.preview_render_scale
                && a.preview_zoom_percent == b.preview_zoom_percent
                && a.page_is_a4 == b.page_is_a4
                && a.page_landscape == b.page_landscape
                && (std::sync::Arc::ptr_eq(&a.plain_text, &b.plain_text)
                    || a.plain_text.as_ref() == b.plain_text.as_ref())
                && (std::sync::Arc::ptr_eq(&a.preview_rgba, &b.preview_rgba)
                    || a.preview_rgba.as_slice() == b.preview_rgba.as_slice())
        }
        (Vs::Media(a), Vs::Media(b)) => {
            a.path_display == b.path_display && a.kind_label == b.kind_label
        }
        (Vs::Html(a), Vs::Html(b)) => {
            a.path_display == b.path_display
                && (std::sync::Arc::ptr_eq(&a.source_preview, &b.source_preview)
                    || a.source_preview.as_ref() == b.source_preview.as_ref())
        }
        _ => false,
    }
}

fn file_manager_payload_eq(a: &FileManagerPayload, b: &FileManagerPayload) -> bool {
    a.active_pane == b.active_pane
        && a.dual_pane == b.dual_pane
        && a.clipboard_count == b.clipboard_count
        && a.clipboard_is_cut == b.clipboard_is_cut
        && a.ingest_in_flight == b.ingest_in_flight
        && a.transfer_active == b.transfer_active
        && a.transfer_progress.to_bits() == b.transfer_progress.to_bits()
        && a.transfer_is_copy == b.transfer_is_copy
        && a.transfer_current == b.transfer_current
        && a.transfer_error == b.transfer_error
        && a.passphrase_error == b.passphrase_error
        && a.ingest_error == b.ingest_error
        && a.activity_notice_key == b.activity_notice_key
        && a.activity_notice_name == b.activity_notice_name
        && a.activity_indicator == b.activity_indicator
        && a.visit_history.len() == b.visit_history.len()
        && a.visit_history
            .iter()
            .zip(b.visit_history.iter())
            .all(|(x, y)| {
                x.path == y.path && x.frequent == y.frequent && x.is_header == y.is_header
            })
        && a.managed_folders.len() == b.managed_folders.len()
        && a.managed_folders
            .iter()
            .zip(b.managed_folders.iter())
            .all(|(x, y)| {
                x.path == y.path
                    && x.files_tracked == y.files_tracked
                    && x.dedup_bytes == y.dedup_bytes
                    && x.policy_max_bytes == y.policy_max_bytes
                    && x.policy_retention_days == y.policy_retention_days
                    && x.policy_exclude_count == y.policy_exclude_count
            })
        && a.network_mounts.len() == b.network_mounts.len()
        && a.network_mounts
            .iter()
            .zip(b.network_mounts.iter())
            .all(|(x, y)| x.name == y.name && x.uri == y.uri)
        && a.panes.len() == b.panes.len()
        && a.panes.iter().zip(b.panes.iter()).all(|(pa, pb)| {
            pa.active_tab == pb.active_tab
                && pa.tabs.len() == pb.tabs.len()
                && pa.tabs.iter().zip(pb.tabs.iter()).all(|(ta, tb)| {
                    ta.tab_id == tb.tab_id
                        && ta.path_display == tb.path_display
                        && ta.can_go_back == tb.can_go_back
                        && ta.can_go_forward == tb.can_go_forward
                        && ta.view_mode == tb.view_mode
                        && ta.selection_count == tb.selection_count
                        && ta.item_count == tb.item_count
                        && ta.managed_files_tracked == tb.managed_files_tracked
                        && ta.managed_dedup_bytes == tb.managed_dedup_bytes
                        && ta.quick_filter == tb.quick_filter
                        && ta.is_loading == tb.is_loading
                        && ta.error == tb.error
                        && ta.sort_by == tb.sort_by
                        && ta.sort_descending == tb.sort_descending
                        && ta.breadcrumbs == tb.breadcrumbs
                        && ta.entries.len() == tb.entries.len()
                        && ta
                            .entries
                            .iter()
                            .zip(tb.entries.iter())
                            .all(|(ea, eb)| fm_entry_eq(ea, eb))
                })
        })
}

fn fm_entry_eq(a: &EntryPayload, b: &EntryPayload) -> bool {
    a.path == b.path
        && a.name == b.name
        && a.is_dir == b.is_dir
        && a.size_text == b.size_text
        && a.modified_text == b.modified_text
        && a.type_text == b.type_text
        && a.icon == b.icon
        && a.has_thumbnail == b.has_thumbnail
        && a.thumbnail_key == b.thumbnail_key
        && a.thumbnail_width == b.thumbnail_width
        && a.thumbnail_height == b.thumbnail_height
        && a.thumbnail_is_icon == b.thumbnail_is_icon
        && a.is_selected == b.is_selected
        && a.is_hidden == b.is_hidden
        && a.is_encrypted == b.is_encrypted
        && a.is_managed == b.is_managed
        && a.is_starred == b.is_starred
        && a.color_label == b.color_label
        && a.tags == b.tags
        && match (&a.thumbnail_rgba, &b.thumbnail_rgba) {
            (None, None) => true,
            (Some(ra), Some(rb)) => {
                std::sync::Arc::ptr_eq(ra, rb)
                    || (ra.len() == rb.len() && ra.as_slice() == rb.as_slice())
            }
            _ => false,
        }
}

fn selection_eq(
    a: Option<orchid_viewers::SelectionRange>,
    b: Option<orchid_viewers::SelectionRange>,
) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.start_line == b.start_line
                && a.start_column == b.start_column
                && a.end_line == b.end_line
                && a.end_column == b.end_column
        }
        _ => false,
    }
}

fn text_lines_eq(a: &[orchid_viewers::SyntaxLine], b: &[orchid_viewers::SyntaxLine]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b.iter()).all(|(la, lb)| {
            la.line_number == lb.line_number
                && la.segments.len() == lb.segments.len()
                && la
                    .segments
                    .iter()
                    .zip(lb.segments.iter())
                    .all(|(sa, sb)| sa.text == sb.text && sa.scope == sb.scope)
        })
}

fn archive_preview_eq(
    a: &Option<orchid_viewers::ArchivePreview>,
    b: &Option<orchid_viewers::ArchivePreview>,
) -> bool {
    match (a, b) {
        (None, None) => true,
        (
            Some(orchid_viewers::ArchivePreview::Text(ta)),
            Some(orchid_viewers::ArchivePreview::Text(tb)),
        ) => ta == tb,
        (
            Some(orchid_viewers::ArchivePreview::Binary { size: sa }),
            Some(orchid_viewers::ArchivePreview::Binary { size: sb }),
        ) => sa == sb,
        _ => false,
    }
}

fn archive_status_eq(a: &orchid_viewers::ArchiveStatus, b: &orchid_viewers::ArchiveStatus) -> bool {
    match (a, b) {
        (orchid_viewers::ArchiveStatus::Idle, orchid_viewers::ArchiveStatus::Idle) => true,
        (
            orchid_viewers::ArchiveStatus::ExtractedSelected { path: pa },
            orchid_viewers::ArchiveStatus::ExtractedSelected { path: pb },
        ) => pa == pb,
        (
            orchid_viewers::ArchiveStatus::ExtractedAll {
                count: ca,
                path: pa,
            },
            orchid_viewers::ArchiveStatus::ExtractedAll {
                count: cb,
                path: pb,
            },
        ) => ca == cb && pa == pb,
        _ => false,
    }
}

pub(crate) fn terminal_payload_eq(a: &TerminalPayload, b: &TerminalPayload) -> bool {
    a.cursor_col == b.cursor_col
        && a.cursor_row == b.cursor_row
        && a.cols == b.cols
        && a.rows == b.rows
        && a.cursor_visible == b.cursor_visible
        && a.content_generation == b.content_generation
        && a.active_tab == b.active_tab
        && a.tabs == b.tabs
        && a.dividers == b.dividers
        && panes_eq(&a.panes, &b.panes)
        // Prefer generation over full cell walks when present on both sides.
        && (a.content_generation != 0 && b.content_generation != 0 || a.cells == b.cells)
}

fn panes_eq(a: &[TerminalPanePayload], b: &[TerminalPanePayload]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b.iter()).all(|(x, y)| {
            x.session_id == y.session_id
                && x.left == y.left
                && x.top == y.top
                && x.right == y.right
                && x.bottom == y.bottom
                && x.is_focused == y.is_focused
                && x.show_close == y.show_close
                && x.cols == y.cols
                && x.rows == y.rows
                && x.cursor_col == y.cursor_col
                && x.cursor_row == y.cursor_row
                && x.cursor_visible == y.cursor_visible
                && x.content_generation == y.content_generation
                && (x.content_generation != 0 && y.content_generation != 0 || x.cells == y.cells)
        })
}

fn weather_payload_eq(a: &WeatherPayload, b: &WeatherPayload) -> bool {
    a.location_name == b.location_name
        && a.cities == b.cities
        && a.active_city_index == b.active_city_index
        && a.picker_open == b.picker_open
        && a.search_query == b.search_query
        && a.search_results == b.search_results
        && a.search_busy == b.search_busy
        && a.selected_day_index == b.selected_day_index
        && a.current_temp_text == b.current_temp_text
        && a.feels_like_temp == b.feels_like_temp
        && a.condition_key == b.condition_key
        && a.condition_icon == b.condition_icon
        && a.humidity_percent == b.humidity_percent
        && opt_f32_eq(a.wind_speed_kph, b.wind_speed_kph)
        && a.wind_direction == b.wind_direction
        && forecast_eq(&a.forecast, &b.forecast)
        && a.fetched_at == b.fetched_at
        && a.is_loading == b.is_loading
        && a.status == b.status
}

fn weather_forecast_day_eq(a: &WeatherForecastDay, b: &WeatherForecastDay) -> bool {
    a.day_index == b.day_index
        && a.weekday_label == b.weekday_label
        && a.high_text == b.high_text
        && a.low_text == b.low_text
        && a.condition_icon == b.condition_icon
        && a.precipitation_probability == b.precipitation_probability
        && a.selected == b.selected
        && a.sunrise_text == b.sunrise_text
        && a.sunset_text == b.sunset_text
}

fn forecast_eq(a: &[WeatherForecastDay], b: &[WeatherForecastDay]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| weather_forecast_day_eq(x, y))
}

fn moon_payload_eq(a: &MoonPayload, b: &MoonPayload) -> bool {
    a.phase_key == b.phase_key
        && a.phase_icon == b.phase_icon
        && opt_f32_eq(a.illumination_percent, b.illumination_percent)
        && opt_f32_eq(a.age_days, b.age_days)
        && opt_f64_eq(a.distance_km, b.distance_km)
        && a.next_full_date == b.next_full_date
        && a.next_new_date == b.next_new_date
        && a.moonrise_time == b.moonrise_time
        && a.moonset_time == b.moonset_time
        && a.sunrise_time == b.sunrise_time
        && a.sunset_time == b.sunset_time
        && opt_f64_eq(a.libration_lat_deg, b.libration_lat_deg)
        && opt_f64_eq(a.libration_lon_deg, b.libration_lon_deg)
        && a.is_loading == b.is_loading
}

fn jyotish_payload_eq(a: &JyotishPayload, b: &JyotishPayload) -> bool {
    a.date_text == b.date_text
        && a.location_name == b.location_name
        && a.cities == b.cities
        && a.active_city_index == b.active_city_index
        && a.picker_open == b.picker_open
        && a.search_query == b.search_query
        && a.search_results == b.search_results
        && a.search_busy == b.search_busy
        && a.current_locating == b.current_locating
        && a.current_failed == b.current_failed
        && a.profiles == b.profiles
        && a.active_profile_index == b.active_profile_index
        && a.profile_picker_open == b.profile_picker_open
        && a.profile_search_query == b.profile_search_query
        && a.profile_search_results == b.profile_search_results
        && a.profile_search_busy == b.profile_search_busy
        && a.profile_editing == b.profile_editing
        && a.profile_edit_index == b.profile_edit_index
        && a.profile_edit_name == b.profile_edit_name
        && a.profile_edit_gender == b.profile_edit_gender
        && a.profile_edit_date == b.profile_edit_date
        && a.profile_edit_time == b.profile_edit_time
        && a.profile_edit_offset == b.profile_edit_offset
        && a.profile_edit_place_name == b.profile_edit_place_name
        && a.ayanamsa_key == b.ayanamsa_key
        && a.ayanamsa_deg_text == b.ayanamsa_deg_text
        && a.day_offset == b.day_offset
        && a.is_today == b.is_today
        && a.tithi_key == b.tithi_key
        && a.paksha_key == b.paksha_key
        && a.tithi_end_text == b.tithi_end_text
        && a.nakshatra_key == b.nakshatra_key
        && a.pada == b.pada
        && a.nakshatra_end_text == b.nakshatra_end_text
        && a.yoga_key == b.yoga_key
        && a.yoga_end_text == b.yoga_end_text
        && a.karana_key == b.karana_key
        && a.karana_end_text == b.karana_end_text
        && a.vara_key == b.vara_key
        && a.sunrise_time == b.sunrise_time
        && a.sunset_time == b.sunset_time
        && a.rahukalam_text == b.rahukalam_text
        && a.yamagandam_text == b.yamagandam_text
        && a.gulika_text == b.gulika_text
        && a.in_rahukalam == b.in_rahukalam
        && a.show_planets == b.show_planets
        && a.is_loading == b.is_loading
        && a.planets.len() == b.planets.len()
        && a.planets.iter().zip(b.planets.iter()).all(|(x, y)| {
            x.graha_key == y.graha_key
                && x.rashi_key == y.rashi_key
                && x.degree_text == y.degree_text
                && x.is_retrograde == y.is_retrograde
        })
        && a.active_tab == b.active_tab
        && a.score_color == b.score_color
        && a.now_score_color == b.now_score_color
        && a.day_score_color == b.day_score_color
        && a.score_value == b.score_value
        && jyotish_factors_eq(&a.factors, &b.factors)
        && a.personal_mode == b.personal_mode
        && a.headline_key == b.headline_key
        && a.summary_key == b.summary_key
        && a.influence_keys == b.influence_keys
        && a.advice_keys == b.advice_keys
        && jyotish_week_strip_eq(&a.week_strip, &b.week_strip)
        && a.month_key == b.month_key
        && a.month_year == b.month_year
        && jyotish_month_cells_eq(&a.month_cells, &b.month_cells)
        && a.month_first_weekday == b.month_first_weekday
        && a.month_green == b.month_green
        && a.month_yellow == b.month_yellow
        && a.month_red == b.month_red
        && a.year_value == b.year_value
        && jyotish_year_months_eq(&a.year_months, &b.year_months)
        && jyotish_life_years_eq(&a.life_years, &b.life_years)
        && a.life_detail_year == b.life_detail_year
        && jyotish_life_antars_eq(&a.life_antars, &b.life_antars)
        && a.has_dasha_now == b.has_dasha_now
        && a.dasha_now == b.dasha_now
        && a.gochara_note_key == b.gochara_note_key
        && a.has_birth_data == b.has_birth_data
        && a.rectify == b.rectify
}

fn jyotish_factors_eq(a: &[JyotishFactorRow], b: &[JyotishFactorRow]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x == y)
}

fn jyotish_week_strip_eq(a: &[JyotishDayChip], b: &[JyotishDayChip]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x == y)
}

fn jyotish_month_cells_eq(a: &[JyotishMonthCell], b: &[JyotishMonthCell]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x == y)
}

fn jyotish_year_months_eq(a: &[JyotishMonthSummary], b: &[JyotishMonthSummary]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x == y)
}

fn jyotish_life_years_eq(a: &[JyotishYearSummary], b: &[JyotishYearSummary]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x == y)
}

fn jyotish_life_antars_eq(a: &[JyotishAntarRow], b: &[JyotishAntarRow]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x == y)
}

fn system_indicator_eq(a: &SystemIndicator, b: &SystemIndicator) -> bool {
    a.kind == b.kind
        && a.name_suffix == b.name_suffix
        && a.value_text == b.value_text
        && a.network_up == b.network_up
        && a.network_down == b.network_down
        && opt_f32_eq(a.percent, b.percent)
        && a.segments.len() == b.segments.len()
        && a.segments
            .iter()
            .zip(b.segments.iter())
            .all(|(x, y)| x.to_bits() == y.to_bits())
        && a.icon == b.icon
        && a.status == b.status
}

fn system_payload_eq(a: &SystemPayload, b: &SystemPayload) -> bool {
    a.is_loading == b.is_loading
        && a.indicators.len() == b.indicators.len()
        && a.indicators
            .iter()
            .zip(b.indicators.iter())
            .all(|(x, y)| system_indicator_eq(x, y))
}

fn clock_payload_eq(a: &ClockPayload, b: &ClockPayload) -> bool {
    a.local_time == b.local_time
        && a.local_date == b.local_date
        && a.local_timezone == b.local_timezone
        && a.picker_open == b.picker_open
        && a.search_query == b.search_query
        && a.search_busy == b.search_busy
        && a.cities.len() == b.cities.len()
        && a.cities.iter().zip(b.cities.iter()).all(|(x, y)| {
            x.name == y.name
                && x.timezone == y.timezone
                && x.time_text == y.time_text
                && x.date_text == y.date_text
                && x.offset_text == y.offset_text
                && x.day_offset == y.day_offset
                && x.is_local == y.is_local
        })
        && a.search_results.len() == b.search_results.len()
        && a.search_results
            .iter()
            .zip(b.search_results.iter())
            .all(|(x, y)| x.name == y.name && x.detail == y.detail && x.timezone == y.timezone)
}

fn process_row_eq(a: &ProcessRowView, b: &ProcessRowView) -> bool {
    a.pid == b.pid
        && a.name == b.name
        && a.status == b.status
        && a.cpu_percent.to_bits() == b.cpu_percent.to_bits()
        && a.memory_bytes == b.memory_bytes
        && a.memory_text == b.memory_text
        && a.io_read_bps == b.io_read_bps
        && a.io_write_bps == b.io_write_bps
        && a.io_text == b.io_text
        && a.user == b.user
        && a.path == b.path
        && a.group == b.group
        && a.parent_pid == b.parent_pid
        && a.session_id == b.session_id
        && a.is_group_header == b.is_group_header
        && a.group_label == b.group_label
}

fn service_row_eq(a: &ServiceRowView, b: &ServiceRowView) -> bool {
    a.name == b.name
        && a.display_name == b.display_name
        && a.status == b.status
        && a.status_code == b.status_code
        && a.start_type == b.start_type
        && a.pid == b.pid
        && a.can_start == b.can_start
        && a.can_stop == b.can_stop
}

fn startup_row_eq(a: &StartupRowView, b: &StartupRowView) -> bool {
    a.id == b.id
        && a.name == b.name
        && a.command == b.command
        && a.location == b.location
        && a.enabled == b.enabled
        && a.can_toggle == b.can_toggle
}

fn user_row_eq(a: &UserRowView, b: &UserRowView) -> bool {
    a.session_id == b.session_id
        && a.user_name == b.user_name
        && a.state == b.state
        && a.process_count == b.process_count
        && a.memory_bytes == b.memory_bytes
        && a.memory_text == b.memory_text
}

fn processes_payload_eq(a: &ProcessesPayload, b: &ProcessesPayload) -> bool {
    a.tab == b.tab
        && a.search_query == b.search_query
        && a.sort_column == b.sort_column
        && a.sort_descending == b.sort_descending
        && a.selected_pid == b.selected_pid
        && a.selected_service == b.selected_service
        && a.selected_startup == b.selected_startup
        && a.selected_session == b.selected_session
        && a.is_loading == b.is_loading
        && a.status_message == b.status_message
        && a.show_grouping == b.show_grouping
        && a.processes.len() == b.processes.len()
        && a.processes
            .iter()
            .zip(b.processes.iter())
            .all(|(x, y)| process_row_eq(x, y))
        && a.services.len() == b.services.len()
        && a.services
            .iter()
            .zip(b.services.iter())
            .all(|(x, y)| service_row_eq(x, y))
        && a.startups.len() == b.startups.len()
        && a.startups
            .iter()
            .zip(b.startups.iter())
            .all(|(x, y)| startup_row_eq(x, y))
        && a.users.len() == b.users.len()
        && a.users
            .iter()
            .zip(b.users.iter())
            .all(|(x, y)| user_row_eq(x, y))
}

fn calculator_payload_eq(a: &CalculatorPayload, b: &CalculatorPayload) -> bool {
    a.mode == b.mode
        && a.angle == b.angle
        && a.second == b.second
        && a.display == b.display
        && a.expression == b.expression
        && a.memory_set == b.memory_set
        && a.error_key == b.error_key
        && a.show_history == b.show_history
        && a.history == b.history
}

fn notes_payload_eq(a: &NotesPayload, b: &NotesPayload) -> bool {
    a.active_index == b.active_index
        && a.title == b.title
        && a.body == b.body
        && a.font_size == b.font_size
        && a.word_wrap == b.word_wrap
        && a.mono_font == b.mono_font
        && a.show_status_bar == b.show_status_bar
        && a.char_count == b.char_count
        && a.word_count == b.word_count
        && a.line_count == b.line_count
        && a.tabs.len() == b.tabs.len()
        && a.tabs
            .iter()
            .zip(b.tabs.iter())
            .all(|(x, y)| x.id == y.id && x.title == y.title && x.is_active == y.is_active)
        && a.find_gen == b.find_gen
        && a.find_cursor == b.find_cursor
        && a.find_anchor == b.find_anchor
}

fn calendar_payload_eq(a: &CalendarPayload, b: &CalendarPayload) -> bool {
    a.year == b.year
        && a.month == b.month
        && a.selected_date == b.selected_date
        && a.today_date == b.today_date
        && a.first_day_of_week == b.first_day_of_week
        && a.days == b.days
        && a.events == b.events
        && a.upcoming == b.upcoming
        && a.show_upcoming == b.show_upcoming
        && a.show_notes_preview == b.show_notes_preview
        && a.time_step_minutes == b.time_step_minutes
        && a.color_filter == b.color_filter
        && a.editor_open == b.editor_open
        && a.editor_event_id == b.editor_event_id
        && a.editor_is_new == b.editor_is_new
        && a.editor_title == b.editor_title
        && a.editor_date == b.editor_date
        && a.editor_all_day == b.editor_all_day
        && a.editor_start_hour == b.editor_start_hour
        && a.editor_start_min == b.editor_start_min
        && a.editor_end_hour == b.editor_end_hour
        && a.editor_end_min == b.editor_end_min
        && a.editor_notes == b.editor_notes
        && a.editor_color == b.editor_color
        && a.delete_confirm_open == b.delete_confirm_open
}

fn opt_f32_eq(a: Option<f32>, b: Option<f32>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => x.to_bits() == y.to_bits(),
        (None, None) => true,
        _ => false,
    }
}

fn opt_f64_eq(a: Option<f64>, b: Option<f64>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => x.to_bits() == y.to_bits(),
        (None, None) => true,
        _ => false,
    }
}

fn rss_item_eq(a: &RssItemView, b: &RssItemView) -> bool {
    a.id == b.id
        && a.title == b.title
        && a.source_name == b.source_name
        && a.published_text == b.published_text
        && a.summary_text == b.summary_text
        && a.link == b.link
}

fn rss_payload_eq(a: &RssPayload, b: &RssPayload) -> bool {
    a.last_updated_text == b.last_updated_text
        && a.is_loading == b.is_loading
        && a.enabled_feed_count == b.enabled_feed_count
        && a.failed_feed_count == b.failed_feed_count
        && a.items.len() == b.items.len()
        && a.items
            .iter()
            .zip(b.items.iter())
            .all(|(x, y)| rss_item_eq(x, y))
}

fn search_candidate_eq(a: &SearchCandidateView, b: &SearchCandidateView) -> bool {
    a.id == b.id
        && a.source_name == b.source_name
        && a.source_icon == b.source_icon
        && a.title == b.title
        && a.subtitle == b.subtitle
        && a.shortcut_hint == b.shortcut_hint
}

fn search_payload_eq(a: &UniversalSearchPayload, b: &UniversalSearchPayload) -> bool {
    a.query == b.query
        && a.is_searching == b.is_searching
        && a.error == b.error
        && a.candidates.len() == b.candidates.len()
        && a.candidates
            .iter()
            .zip(b.candidates.iter())
            .all(|(x, y)| search_candidate_eq(x, y))
}

fn media_payload_eq(a: &MediaPlayerPayload, b: &MediaPlayerPayload) -> bool {
    a.has_session == b.has_session
        && a.is_loading == b.is_loading
        && a.is_unsupported == b.is_unsupported
        && a.title == b.title
        && a.artist == b.artist
        && a.album == b.album
        && a.source_app == b.source_app
        && a.position_secs == b.position_secs
        && a.duration_secs == b.duration_secs
        && a.progress_fraction.to_bits() == b.progress_fraction.to_bits()
        && a.is_playing == b.is_playing
        && match (&a.thumbnail_bytes, &b.thumbnail_bytes) {
            (None, None) => true,
            (Some(x), Some(y)) => std::sync::Arc::ptr_eq(x, y) || x.as_ref() == y.as_ref(),
            _ => false,
        }
}

fn audio_player_payload_eq(a: &AudioPlayerPayload, b: &AudioPlayerPayload) -> bool {
    a.engine_available == b.engine_available
        && a.browse_tab == b.browse_tab
        && a.browse_filter == b.browse_filter
        && a.search_query == b.search_query
        && a.library_sort == b.library_sort
        && a.is_current_favorite == b.is_current_favorite
        && a.renaming_playlist == b.renaming_playlist
        && a.rename_playlist_draft == b.rename_playlist_draft
        && a.active_playlist_id == b.active_playlist_id
        && a.groups == b.groups
        && a.tracks == b.tracks
        && a.playlists == b.playlists
        && a.queue == b.queue
        && a.queue_index == b.queue_index
        && a.queue_count == b.queue_count
        && a.queue_duration_ms == b.queue_duration_ms
        && a.queue_remaining_count == b.queue_remaining_count
        && a.queue_remaining_ms / 1000 == b.queue_remaining_ms / 1000
        && a.has_track == b.has_track
        && a.title == b.title
        && a.artist == b.artist
        && a.album == b.album
        && a.is_playing == b.is_playing
        && a.position_ms / 100 == b.position_ms / 100
        && a.duration_ms == b.duration_ms
        && a.volume == b.volume
        && a.muted == b.muted
        && a.shuffle == b.shuffle
        && a.repeat == b.repeat
        && a.sleep_label == b.sleep_label
        && a.eq_label == b.eq_label
        && a.rg_label == b.rg_label
        && a.speed_label == b.speed_label
        && a.crossfade_secs == b.crossfade_secs
        && a.lyrics_line == b.lyrics_line
        && a.has_lyrics == b.has_lyrics
        && a.lyrics_open == b.lyrics_open
        && a.lyrics_lines == b.lyrics_lines
        && a.lyrics_active_index == b.lyrics_active_index
        && a.library_count == b.library_count
        && a.library_roots_count == b.library_roots_count
        && a.has_library_roots == b.has_library_roots
        && a.roots == b.roots
        && a.has_cover == b.has_cover
        && a.cover_width == b.cover_width
        && a.cover_height == b.cover_height
        && a.current_track_index == b.current_track_index
        && a.scroll_gen == b.scroll_gen
        && (std::sync::Arc::ptr_eq(&a.cover_rgba, &b.cover_rgba)
            || a.cover_rgba.as_ref() == b.cover_rgba.as_ref())
}

fn video_player_payload_eq(a: &VideoPlayerPayload, b: &VideoPlayerPayload) -> bool {
    a.engine_available == b.engine_available
        && a.browse_tab == b.browse_tab
        && a.search_query == b.search_query
        && a.roots == b.roots
        && a.items == b.items
        && a.queue == b.queue
        && a.queue_index == b.queue_index
        && a.queue_count == b.queue_count
        && a.has_track == b.has_track
        && a.title == b.title
        && a.is_playing == b.is_playing
        && a.position_ms / 100 == b.position_ms / 100
        && a.duration_ms == b.duration_ms
        && a.volume == b.volume
        && a.muted == b.muted
        && a.shuffle == b.shuffle
        && a.repeat == b.repeat
        && a.speed_label == b.speed_label
        && a.library_count == b.library_count
        && a.has_library_roots == b.has_library_roots
        && a.empty_hint == b.empty_hint
        && a.has_video == b.has_video
        && a.frame_width == b.frame_width
        && a.frame_height == b.frame_height
        && (std::sync::Arc::ptr_eq(&a.frame_rgba, &b.frame_rgba)
            || a.frame_rgba.as_ref() == b.frame_rgba.as_ref())
}

fn password_entry_eq(a: &PasswordEntryView, b: &PasswordEntryView) -> bool {
    a.id == b.id
        && a.title == b.title
        && a.username == b.username
        && a.url_host == b.url_host
        && a.has_totp == b.has_totp
        && a.tags == b.tags
        && a.color_label == b.color_label
        && a.modified_text == b.modified_text
}

fn password_detail_eq(a: &PasswordEntryDetailView, b: &PasswordEntryDetailView) -> bool {
    a.id == b.id
        && a.title == b.title
        && a.username == b.username
        && a.url == b.url
        && a.notes == b.notes
        && a.totp_code == b.totp_code
        && a.totp_remaining_seconds == b.totp_remaining_seconds
        && a.tags == b.tags
}

fn password_payload_eq(a: &PasswordManagerPayload, b: &PasswordManagerPayload) -> bool {
    a.is_unlocked == b.is_unlocked
        && a.lock_reason == b.lock_reason
        && a.biometric_available == b.biometric_available
        && a.unlock_error == b.unlock_error
        && a.search_query == b.search_query
        && a.entries.len() == b.entries.len()
        && a.entries
            .iter()
            .zip(b.entries.iter())
            .all(|(x, y)| password_entry_eq(x, y))
        && match (&a.selected, &b.selected) {
            (None, None) => true,
            (Some(sa), Some(sb)) => password_detail_eq(sa, sb),
            _ => false,
        }
}

fn recent_files_payload_eq(a: &RecentFilesPayload, b: &RecentFilesPayload) -> bool {
    a.items.len() == b.items.len()
        && a.items.iter().zip(b.items.iter()).all(|(x, y)| {
            x.id == y.id && x.name == y.name && x.path == y.path && x.opened_text == y.opened_text
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::payloads::WeatherStatusTag;
    use crate::widget::snapshot::WidgetPayload;

    #[test]
    fn weather_payload_detects_temp_change() {
        let a = WidgetPayload::Weather(WeatherPayload {
            location_name: "Home".into(),
            cities: vec![],
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
            humidity_percent: None,
            wind_speed_kph: None,
            wind_direction: None,
            forecast: Vec::new(),
            fetched_at: None,
            is_loading: false,
            status: WeatherStatusTag::Fresh,
        });
        let mut b = a.clone();
        if let WidgetPayload::Weather(ref mut w) = b {
            w.current_temp_text = "21°C".into();
        }
        assert!(!payload_renders_equal(&a, &b));
    }

    #[test]
    fn weather_payload_ignores_identical_snapshots() {
        let a = WidgetPayload::Weather(WeatherPayload {
            location_name: "Home".into(),
            cities: vec![],
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
            humidity_percent: None,
            wind_speed_kph: None,
            wind_direction: None,
            forecast: Vec::new(),
            fetched_at: None,
            is_loading: false,
            status: WeatherStatusTag::Fresh,
        });
        let b = a.clone();
        assert!(payload_renders_equal(&a, &b));
    }

    #[test]
    fn processes_payload_identical_snapshots_compare_equal() {
        use crate::widget::payloads::{ProcessGroup, ProcessSortColumn, ProcessesTab};
        let row = ProcessRowView {
            pid: 1,
            name: "orchid.exe".into(),
            status: "Run".into(),
            cpu_percent: 1.5,
            memory_bytes: 1024,
            memory_text: "1 KB".into(),
            io_read_bps: 0,
            io_write_bps: 0,
            io_text: String::new(),
            user: "me".into(),
            path: String::new(),
            group: ProcessGroup::Apps,
            parent_pid: None,
            session_id: None,
            is_group_header: false,
            group_label: String::new(),
        };
        let make = || {
            WidgetPayload::Processes(ProcessesPayload {
                tab: ProcessesTab::Processes,
                search_query: String::new(),
                sort_column: ProcessSortColumn::Cpu,
                sort_descending: true,
                selected_pid: 1,
                selected_service: String::new(),
                selected_startup: String::new(),
                selected_session: u32::MAX,
                processes: vec![row.clone()],
                services: Vec::new(),
                startups: Vec::new(),
                users: Vec::new(),
                is_loading: false,
                status_message: String::new(),
                show_grouping: true,
            })
        };
        let a = make();
        let b = make();
        assert!(payload_renders_equal(&a, &b));
        let mut c = make();
        if let WidgetPayload::Processes(ref mut p) = c {
            p.processes[0].cpu_percent = 2.0;
        }
        assert!(!payload_renders_equal(&a, &c));
    }

    #[test]
    fn text_viewer_read_only_still_compares_plain_text() {
        use orchid_viewers::{TextSnapshot, ViewerSnapshot};
        use std::sync::Arc;

        let make = |plain: &str| {
            WidgetPayload::Viewer(ViewerPayload {
                snapshot: ViewerSnapshot::Text(TextSnapshot {
                    path_display: "local:/a.rs".into(),
                    language: "rust".into(),
                    encoding: "UTF-8".into(),
                    line_ending: "LF".into(),
                    dirty: false,
                    read_only: true,
                    total_lines: 1,
                    visible_lines: Vec::new(),
                    first_visible_line: 0,
                    cursor_line: 0,
                    cursor_column: 0,
                    selection: None,
                    info_text: String::new(),
                    plain_text: Arc::from(plain),
                    display_mode: 0,
                    find_gen: 0,
                    find_anchor: 0,
                    find_cursor: 0,
                    find_match_index: 0,
                    find_match_count: 0,
                    can_undo: false,
                    can_redo: false,
                }),
            })
        };
        let a = make("old");
        let b = make("new");
        assert!(!payload_renders_equal(&a, &b));
        assert!(payload_renders_equal(&a, &make("old")));
    }
}
