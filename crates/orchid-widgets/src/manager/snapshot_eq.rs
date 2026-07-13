//! Render-equality helpers for [`super::snapshot_renders_unchanged`].

use crate::widget::payloads::{
    MediaPlayerPayload, MoonPayload, PasswordEntryDetailView, PasswordEntryView,
    PasswordManagerPayload, RecentFilesPayload, RssItemView, RssPayload, SearchCandidateView,
    SystemIndicator, SystemPayload, UniversalSearchPayload, ViewerPayload, WeatherForecastDay,
    WeatherPayload,
};
use crate::widget::snapshot::{TerminalPayload, WidgetPayload};

/// `true` when two payloads would draw the same in the UI.
pub(crate) fn payload_renders_equal(a: &WidgetPayload, b: &WidgetPayload) -> bool {
    match (a, b) {
        (WidgetPayload::Empty, WidgetPayload::Empty) => true,
        (
            WidgetPayload::Text { lines: la },
            WidgetPayload::Text { lines: lb },
        ) => la == lb,
        (
            WidgetPayload::KeyValueList { entries: ea },
            WidgetPayload::KeyValueList { entries: eb },
        ) => ea == eb,
        (WidgetPayload::Terminal(a), WidgetPayload::Terminal(b)) => terminal_payload_eq(a, b),
        (WidgetPayload::Weather(a), WidgetPayload::Weather(b)) => weather_payload_eq(a, b),
        (WidgetPayload::Moon(a), WidgetPayload::Moon(b)) => moon_payload_eq(a, b),
        (WidgetPayload::SystemIndicators(a), WidgetPayload::SystemIndicators(b)) => {
            system_payload_eq(a, b)
        }
        (WidgetPayload::RssFeed(a), WidgetPayload::RssFeed(b)) => rss_payload_eq(a, b),
        (
            WidgetPayload::UniversalSearch(a),
            WidgetPayload::UniversalSearch(b),
        ) => search_payload_eq(a, b),
        (WidgetPayload::MediaPlayer(a), WidgetPayload::MediaPlayer(b)) => media_payload_eq(a, b),
        (WidgetPayload::PasswordManager(a), WidgetPayload::PasswordManager(b)) => {
            password_payload_eq(a, b)
        }
        (WidgetPayload::RecentFiles(a), WidgetPayload::RecentFiles(b)) => {
            recent_files_payload_eq(a, b)
        }
        // Viewer / file-manager carry large trees; compare structurally when possible.
        (WidgetPayload::Viewer(a), WidgetPayload::Viewer(b)) => viewer_payload_eq(a, b),
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
                && a.rotation_degrees == b.rotation_degrees
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
                && (a.read_only && b.read_only || a.plain_text == b.plain_text)
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
                && la.segments.iter().zip(lb.segments.iter()).all(|(sa, sb)| {
                    sa.text == sb.text && sa.scope == sb.scope
                })
        })
}

fn archive_preview_eq(
    a: &Option<orchid_viewers::ArchivePreview>,
    b: &Option<orchid_viewers::ArchivePreview>,
) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(orchid_viewers::ArchivePreview::Text(ta)), Some(orchid_viewers::ArchivePreview::Text(tb))) => {
            ta == tb
        }
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
        && a.active_tab == b.active_tab
        && a.tabs == b.tabs
        && a.panes == b.panes
        && a.dividers == b.dividers
        && a.cells == b.cells
}

fn weather_payload_eq(a: &WeatherPayload, b: &WeatherPayload) -> bool {
    a.location_name == b.location_name
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
}

fn forecast_eq(a: &[WeatherForecastDay], b: &[WeatherForecastDay]) -> bool {
    a.len() == b.len()
        && a
            .iter()
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

fn system_indicator_eq(a: &SystemIndicator, b: &SystemIndicator) -> bool {
    a.kind == b.kind
        && a.name_suffix == b.name_suffix
        && a.value_text == b.value_text
        && a.network_up == b.network_up
        && a.network_down == b.network_down
        && opt_f32_eq(a.percent, b.percent)
        && a.icon == b.icon
        && a.status == b.status
}

fn system_payload_eq(a: &SystemPayload, b: &SystemPayload) -> bool {
    a.is_loading == b.is_loading
        && a.indicators.len() == b.indicators.len()
        && a
            .indicators
            .iter()
            .zip(b.indicators.iter())
            .all(|(x, y)| system_indicator_eq(x, y))
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
        && a
            .items
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
        && a
            .candidates
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
        && a.thumbnail_base64 == b.thumbnail_base64
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
        && a
            .entries
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
        && a
            .items
            .iter()
            .zip(b.items.iter())
            .all(|(x, y)| {
                x.id == y.id
                    && x.name == y.name
                    && x.path == y.path
                    && x.opened_text == y.opened_text
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
}
