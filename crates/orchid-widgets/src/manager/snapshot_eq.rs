//! Render-equality helpers for [`super::snapshot_renders_unchanged`].

use crate::widget::payloads::{
    MediaPlayerPayload, MoonPayload, PasswordEntryDetailView, PasswordEntryView,
    PasswordManagerPayload, RssItemView, RssPayload, SearchCandidateView, SystemIndicator,
    SystemPayload, UniversalSearchPayload, WeatherForecastDay, WeatherPayload,
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
        // Viewer / file-manager carry large trees; treat inequality as the safe default.
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
        && a.feels_like_text == b.feels_like_text
        && a.condition_label == b.condition_label
        && a.condition_icon == b.condition_icon
        && a.humidity_text == b.humidity_text
        && a.wind_text == b.wind_text
        && forecast_eq(&a.forecast, &b.forecast)
        && a.last_updated_text == b.last_updated_text
        && a.status == b.status
}

fn weather_forecast_day_eq(a: &WeatherForecastDay, b: &WeatherForecastDay) -> bool {
    a.day_label == b.day_label
        && a.high_text == b.high_text
        && a.low_text == b.low_text
        && a.condition_icon == b.condition_icon
        && a.precipitation_probability_text == b.precipitation_probability_text
}

fn forecast_eq(a: &[WeatherForecastDay], b: &[WeatherForecastDay]) -> bool {
    a.len() == b.len()
        && a
            .iter()
            .zip(b.iter())
            .all(|(x, y)| weather_forecast_day_eq(x, y))
}

fn moon_payload_eq(a: &MoonPayload, b: &MoonPayload) -> bool {
    a.phase_label == b.phase_label
        && a.phase_icon == b.phase_icon
        && a.illumination_text == b.illumination_text
        && a.age_text == b.age_text
        && a.distance_text == b.distance_text
        && a.next_full_text == b.next_full_text
        && a.next_new_text == b.next_new_text
        && a.moonrise_text == b.moonrise_text
        && a.moonset_text == b.moonset_text
        && a.sunrise_text == b.sunrise_text
        && a.sunset_text == b.sunset_text
        && a.libration_text == b.libration_text
}

fn system_indicator_eq(a: &SystemIndicator, b: &SystemIndicator) -> bool {
    a.label == b.label
        && a.value_text == b.value_text
        && a.percent == b.percent
        && a.icon == b.icon
        && a.status == b.status
}

fn system_payload_eq(a: &SystemPayload, b: &SystemPayload) -> bool {
    a.indicators.len() == b.indicators.len()
        && a
            .indicators
            .iter()
            .zip(b.indicators.iter())
            .all(|(x, y)| system_indicator_eq(x, y))
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
        && a.error_summary == b.error_summary
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
        && a.title == b.title
        && a.artist == b.artist
        && a.album == b.album
        && a.source_app == b.source_app
        && a.position_text == b.position_text
        && a.duration_text == b.duration_text
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
            feels_like_text: None,
            condition_label: "Clear".into(),
            condition_icon: "weather-clear",
            humidity_text: None,
            wind_text: None,
            forecast: Vec::new(),
            last_updated_text: "Updated just now".into(),
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
            feels_like_text: None,
            condition_label: "Clear".into(),
            condition_icon: "weather-clear",
            humidity_text: None,
            wind_text: None,
            forecast: Vec::new(),
            last_updated_text: "Updated just now".into(),
            status: WeatherStatusTag::Fresh,
        });
        let b = a.clone();
        assert!(payload_renders_equal(&a, &b));
    }
}
