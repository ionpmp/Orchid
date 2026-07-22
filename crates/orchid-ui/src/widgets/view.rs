//! Renderer-agnostic bridge between [`orchid_widgets::WidgetSnapshot`] and
//! the UI layer.
//!
//! Every built-in widget registers a [`WidgetView`] with the [`WidgetViewDispatcher`].
//! The Slint shell (added in a follow-up task) calls
//! [`WidgetViewDispatcher::render`] once per instance per frame to obtain the
//! concrete payload it should push into the corresponding Slint component.
//!
//! This module intentionally stops short of touching Slint types — the
//! dispatcher speaks only to [`SlintPayload`], a Slint-shaped mirror of
//! [`orchid_widgets::WidgetPayload`]. The final adapter that turns
//! [`SlintPayload::Terminal`] cells into a `slint::Model` lives alongside the
//! Slint component once the UI shell lands.

use std::collections::HashMap;

use orchid_widgets::{
    FileManagerPayload, JyotishPayload, MediaPlayerPayload, MoonPayload, PasswordManagerPayload,
    RssPayload, SystemPayload, UniversalSearchPayload, ViewerPayload, WeatherPayload,
    WidgetPayload, WidgetSnapshot,
};
use parking_lot::RwLock;

/// Rust-side mirror of the Slint struct emitted into the workspace model.
#[derive(Debug, Clone)]
pub struct SlintTerminalCell {
    /// Visible character.
    pub ch: char,
    /// Foreground RGBA.
    pub fg_rgba: [u8; 4],
    /// Background RGBA.
    pub bg_rgba: [u8; 4],
    /// Bold flag.
    pub bold: bool,
    /// Italic flag.
    pub italic: bool,
    /// Underline flag.
    pub underline: bool,
}

/// Renderer-facing payload. Exactly mirrors [`WidgetPayload`] but uses
/// owned `String`s and [`SlintTerminalCell`] so it is easy to adapt into
/// Slint models at the final step.
#[derive(Debug, Clone)]
pub enum SlintPayload {
    /// Nothing to render yet.
    Empty,
    /// A vertical list of text rows.
    Text(Vec<String>),
    /// Generic key / value rows.
    KeyValueList(Vec<(String, String)>),
    /// Terminal cells + cursor.
    Terminal {
        /// Columns.
        cols: i32,
        /// Rows.
        rows: i32,
        /// Cells in row-major order.
        cells: Vec<SlintTerminalCell>,
        /// Cursor column.
        cursor_col: i32,
        /// Cursor row.
        cursor_row: i32,
        /// Whether the cursor should be drawn.
        cursor_visible: bool,
    },
}

impl SlintPayload {
    /// Convert a framework payload into the renderer-friendly shape.
    #[must_use]
    pub fn from_widget(payload: &WidgetPayload) -> Self {
        match payload {
            WidgetPayload::Empty => Self::Empty,
            WidgetPayload::Text { lines } => Self::Text(lines.clone()),
            WidgetPayload::KeyValueList { entries } => Self::KeyValueList(entries.clone()),
            WidgetPayload::Terminal(payload) => Self::Terminal {
                cols: payload.cols as i32,
                rows: payload.rows as i32,
                cells: payload
                    .cells
                    .iter()
                    .map(|c| SlintTerminalCell {
                        ch: c.ch,
                        fg_rgba: c.fg_rgba,
                        bg_rgba: c.bg_rgba,
                        bold: c.bold,
                        italic: c.italic,
                        underline: c.underline,
                    })
                    .collect(),
                cursor_col: payload.cursor_col as i32,
                cursor_row: payload.cursor_row as i32,
                cursor_visible: payload.cursor_visible,
            },
            WidgetPayload::Weather(p) => Self::Text(weather_to_text_lines(p)),
            WidgetPayload::Moon(p) => Self::Text(moon_to_text_lines(p)),
            WidgetPayload::Jyotish(p) => Self::Text(jyotish_to_text_lines(p.as_ref())),
            WidgetPayload::Clock(p) => Self::Text(clock_to_text_lines(p)),
            WidgetPayload::SystemIndicators(p) => Self::Text(system_to_text_lines(p)),
            WidgetPayload::Processes(p) => Self::Text(processes_to_text_lines(p)),
            WidgetPayload::Calculator(p) => {
                Self::Text(vec![p.expression.clone(), p.display.clone()])
            }
            WidgetPayload::Notes(p) => Self::Text(vec![p.title.clone(), p.body.clone()]),
            WidgetPayload::Calendar(p) => Self::Text(
                p.events
                    .iter()
                    .flat_map(|e| [e.title.clone(), e.notes_preview.clone()])
                    .collect(),
            ),
            WidgetPayload::RssFeed(p) => Self::Text(rss_to_text_lines(p)),
            WidgetPayload::UniversalSearch(p) => Self::Text(search_to_text_lines(p)),
            WidgetPayload::MediaPlayer(p) => Self::Text(media_to_text_lines(p)),
            WidgetPayload::PasswordManager(p) => Self::Text(password_to_text_lines(p)),
            WidgetPayload::Viewer(p) => Self::Text(viewer_to_text_lines(p)),
            WidgetPayload::FileManager(p) => Self::Text(file_manager_to_text_lines(p)),
            WidgetPayload::RecentFiles(p) => Self::Text(recent_files_to_text_lines(p)),
        }
    }
}

fn recent_files_to_text_lines(p: &orchid_widgets::RecentFilesPayload) -> Vec<String> {
    if p.items.is_empty() {
        return vec!["No recent files".into()];
    }
    p.items
        .iter()
        .map(|it| format!("{} ({})", it.name, it.opened_text))
        .collect()
}

fn weather_to_text_lines(w: &WeatherPayload) -> Vec<String> {
    let mut lines = vec![
        w.location_name.clone(),
        format!("{} — {}", w.current_temp_text, w.condition_key),
    ];
    if let Some(ref f) = w.feels_like_temp {
        lines.push(format!("Feels {f}"));
    }
    if let Some(h) = w.humidity_percent {
        lines.push(format!("{h}%"));
    }
    if let Some(kph) = w.wind_speed_kph {
        let dir = w.wind_direction.as_deref().unwrap_or("");
        if dir.is_empty() {
            lines.push(format!("{kph:.0} km/h"));
        } else {
            lines.push(format!("{kph:.0} km/h {dir}"));
        }
    }
    for day in &w.forecast {
        let mut s = format!(
            "Day {}: {} / {}",
            day.day_index, day.high_text, day.low_text
        );
        if let Some(p) = day.precipitation_probability {
            s.push_str(" · ");
            s.push_str(&format!("{p}%"));
        }
        lines.push(s);
    }
    if let Some(at) = w.fetched_at {
        lines.push(format!("Fetched at {at}"));
    }
    lines
}

fn clock_to_text_lines(p: &orchid_widgets::ClockPayload) -> Vec<String> {
    let mut lines = vec![format!("{} {}", p.local_time, p.local_date)];
    for city in p.cities.iter().filter(|c| !c.is_local) {
        lines.push(format!("{}  {}", city.name, city.time_text));
    }
    lines
}

fn jyotish_to_text_lines(p: &JyotishPayload) -> Vec<String> {
    if p.is_loading {
        return vec!["Loading…".into()];
    }
    let mut lines = vec![
        p.date_text.clone(),
        p.location_name.clone(),
        p.vara_key.to_string(),
        format!("{} ({})", p.tithi_key, p.paksha_key),
        format!("{} pada {}", p.nakshatra_key, p.pada),
        p.yoga_key.to_string(),
        p.karana_key.to_string(),
    ];
    if let Some(ref t) = p.sunrise_time {
        lines.push(format!("Sunrise {t}"));
    }
    if let Some(ref t) = p.sunset_time {
        lines.push(format!("Sunset {t}"));
    }
    if let Some(ref t) = p.rahukalam_text {
        lines.push(format!("Rahu Kalam {t}"));
    }
    if let Some(ref t) = p.yamagandam_text {
        lines.push(format!("Yamagandam {t}"));
    }
    if let Some(ref t) = p.gulika_text {
        lines.push(format!("Gulika {t}"));
    }
    for g in &p.planets {
        let r = if g.is_retrograde { " R" } else { "" };
        lines.push(format!(
            "{} {} {}{r}",
            g.graha_key, g.rashi_key, g.degree_text
        ));
    }
    lines
}

fn moon_to_text_lines(m: &MoonPayload) -> Vec<String> {
    let mut lines = vec![m.phase_key.to_string()];
    if let Some(pct) = m.illumination_percent {
        lines.push(format!("{pct:.0}% illuminated"));
    }
    if let Some(days) = m.age_days {
        lines.push(format!("Age: {days:.1} days"));
    }
    if let Some(km) = m.distance_km {
        lines.push(format!("Distance: {km:.0} km"));
    }
    if let Some(ref d) = m.next_full_date {
        lines.push(format!("Next full: {d}"));
    }
    if let Some(ref d) = m.next_new_date {
        lines.push(format!("Next new: {d}"));
    }
    if let Some(ref t) = m.moonrise_time {
        lines.push(format!("Moonrise: {t}"));
    }
    if let Some(ref t) = m.moonset_time {
        lines.push(format!("Moonset: {t}"));
    }
    if let Some(ref t) = m.sunrise_time {
        lines.push(format!("Sunrise: {t}"));
    }
    if let Some(ref t) = m.sunset_time {
        lines.push(format!("Sunset: {t}"));
    }
    if let (Some(lat), Some(lon)) = (m.libration_lat_deg, m.libration_lon_deg) {
        lines.push(format!("Libration: {lat:.1}°, {lon:.1}°"));
    }
    lines
}

fn processes_to_text_lines(p: &orchid_widgets::ProcessesPayload) -> Vec<String> {
    if p.is_loading {
        return vec!["Loading processes…".into()];
    }
    p.processes
        .iter()
        .filter(|r| !r.is_group_header)
        .take(40)
        .map(|r| {
            format!(
                "{} [{}] {:.1}% {}",
                r.name, r.pid, r.cpu_percent, r.memory_text
            )
        })
        .collect()
}

fn system_to_text_lines(s: &SystemPayload) -> Vec<String> {
    use orchid_widgets::SystemIndicatorKind;
    s.indicators
        .iter()
        .map(|i| {
            if i.kind == SystemIndicatorKind::Network {
                format!(
                    "{:?}: ↑ {} ↓ {}",
                    i.name_suffix,
                    i.network_up.as_deref().unwrap_or("—"),
                    i.network_down.as_deref().unwrap_or("—")
                )
            } else {
                let mut line = format!("{:?}: {}", i.kind, i.value_text);
                if let Some(p) = i.percent {
                    line.push_str(&format!(" ({p:.0}%)"));
                }
                line
            }
        })
        .collect()
}

fn rss_to_text_lines(r: &RssPayload) -> Vec<String> {
    let mut lines = Vec::new();
    if r.is_loading {
        lines.push("Loading…".to_string());
    } else if !r.last_updated_text.is_empty() {
        lines.push(r.last_updated_text.clone());
    }
    if r.failed_feed_count > 0 {
        lines.push(format!(
            "{} of {} feeds failed to update",
            r.failed_feed_count, r.enabled_feed_count
        ));
    }
    for item in &r.items {
        let mut line = item.title.clone();
        if !item.source_name.is_empty() {
            line.push_str(" — ");
            line.push_str(&item.source_name);
        }
        lines.push(line);
        if let Some(s) = &item.summary_text {
            lines.push(format!("  {s}"));
        }
    }
    lines
}

fn search_to_text_lines(s: &UniversalSearchPayload) -> Vec<String> {
    let mut lines = vec![format!("Query: {}", s.query)];
    if s.is_searching {
        lines.push("Searching…".to_string());
    }
    if let Some(e) = &s.error {
        lines.push(e.clone());
    }
    for c in &s.candidates {
        let mut line = format!("· {} — {}", c.title, c.source_name);
        if let Some(sub) = &c.subtitle {
            line.push_str(" — ");
            line.push_str(sub);
        }
        lines.push(line);
    }
    lines
}

fn media_to_text_lines(m: &MediaPlayerPayload) -> Vec<String> {
    if m.is_unsupported {
        return vec!["Unsupported".to_string()];
    }
    if m.is_loading {
        return vec!["Loading".to_string()];
    }
    if !m.has_session {
        return vec!["No media session".to_string()];
    }
    let artist_album = format!("{} — {}", m.artist, m.album);
    vec![
        m.title.clone(),
        artist_album,
        m.source_app.clone(),
        format!(
            "{} / {} ({:.0}%)",
            format_media_duration(m.position_secs),
            format_media_duration(m.duration_secs),
            f64::from(m.progress_fraction) * 100.0
        ),
        if m.is_playing {
            "Playing".to_string()
        } else {
            "Paused".to_string()
        },
    ]
}

fn format_media_duration(secs: u64) -> String {
    let m = secs / 60;
    let s = secs % 60;
    format!("{m}:{s:02}")
}

/// Plain-text preview for the password widget: no secrets, no TOTP codes.
fn password_to_text_lines(p: &PasswordManagerPayload) -> Vec<String> {
    if !p.is_unlocked {
        let mut lines = vec!["Locked".to_string()];
        if let Some(r) = &p.lock_reason {
            lines.push(r.clone());
        }
        return lines;
    }
    let mut lines = vec![format!("Search: {}", p.search_query)];
    for e in &p.entries {
        lines.push(format!("· {} — {}", e.title, e.username));
    }
    if let Some(d) = &p.selected {
        lines.push("— — —".to_string());
        lines.push(d.title.clone());
        lines.push(format!("user: {}", d.username));
        if let Some(url) = &d.url {
            lines.push(url.clone());
        }
    }
    lines
}

fn viewer_to_text_lines(p: &ViewerPayload) -> Vec<String> {
    use orchid_viewers::ViewerSnapshot;
    match &p.snapshot {
        ViewerSnapshot::Loading { path_display } => {
            vec![format!("Loading {path_display}…")]
        }
        ViewerSnapshot::Error {
            path_display,
            message,
        } => vec![format!("{path_display}"), format!("Error: {message}")],
        ViewerSnapshot::Image(i) => vec![
            i.path_display.clone(),
            format!("Image {}×{}", i.width_px, i.height_px),
            i.info_text.clone(),
        ],
        ViewerSnapshot::Pdf(p) => vec![
            p.path_display.clone(),
            format!("PDF — page {} / {}", p.current_page, p.page_count),
            p.info_text.clone(),
        ],
        ViewerSnapshot::Text(t) => {
            let mut lines = vec![
                t.path_display.clone(),
                format!(
                    "{} · {} · {} · {} lines",
                    t.language, t.encoding, t.line_ending, t.total_lines
                ),
            ];
            for line in &t.visible_lines {
                let text: String = line.segments.iter().map(|s| s.text.as_str()).collect();
                lines.push(format!("{:>5}│ {}", line.line_number + 1, text));
            }
            lines
        }
        ViewerSnapshot::Archive(a) => {
            let mut lines = vec![
                format!("{} — {}", a.path_display, a.format),
                a.info_text.clone(),
                format!("{}/", a.current_inner_path),
            ];
            for e in &a.entries {
                lines.push(format!(
                    "{} {}{}",
                    if e.is_dir { "📁" } else { "📄" },
                    e.name,
                    if e.is_dir { "/" } else { "" }
                ));
            }
            if let Some(preview) = &a.preview {
                lines.push("— — —".into());
                match preview {
                    orchid_viewers::ArchivePreview::Text(t) => {
                        for row in t.lines().take(20) {
                            lines.push(row.to_string());
                        }
                    }
                    orchid_viewers::ArchivePreview::Binary { size } => {
                        lines.push(format!("Binary, {size} bytes"));
                    }
                }
            }
            lines
        }
    }
}

fn format_byte_size(n: u64) -> String {
    // Accessibility / text-export path has no live LocaleManager; use en-US.
    static LOCALE: std::sync::OnceLock<orchid_i18n::LocaleManager> = std::sync::OnceLock::new();
    let locale = LOCALE.get_or_init(|| {
        orchid_i18n::LocaleManager::new(orchid_i18n::default_language(), None)
            .expect("bundled en-US locale")
    });
    locale.format_byte_size(n)
}

fn file_manager_to_text_lines(p: &FileManagerPayload) -> Vec<String> {
    let mut lines = Vec::new();
    for (idx, pane) in p.panes.iter().enumerate() {
        if let Some(tab) = pane.tabs.first() {
            lines.push(format!(
                "Pane {}: {}",
                if idx == 0 { "L" } else { "R" },
                tab.path_display
            ));
            if let Some(tracked) = tab.managed_files_tracked {
                let dedup = tab
                    .managed_dedup_bytes
                    .map(|b| format_byte_size(b))
                    .unwrap_or_default();
                lines.push(format!(
                    "{} items, {} selected · {} ingested, {} deduped",
                    tab.item_count, tab.selection_count, tracked, dedup
                ));
            } else {
                lines.push(format!(
                    "{} items, {} selected",
                    tab.item_count, tab.selection_count
                ));
            }
            for entry in tab.entries.iter().take(20) {
                lines.push(format!(
                    "{} {} {} {}",
                    if entry.is_dir { "📁" } else { "📄" },
                    entry.name,
                    entry.size_text,
                    entry.modified_text
                ));
            }
        }
    }
    if p.clipboard_count > 0 {
        let op = if p.clipboard_is_cut { "cut" } else { "copy" };
        lines.push(format!(
            "{} entries ({op}) ready to paste",
            p.clipboard_count
        ));
    }
    lines
}

/// Implemented by each widget type so that the dispatcher can produce a
/// [`SlintPayload`] for it.
///
/// Most widget types share the same trivial implementation — the default
/// body just converts the framework payload with [`SlintPayload::from_widget`].
/// Widgets that need to massage their payload at render time (e.g. collapse
/// large terminal grids for previews) override it.
pub trait WidgetView: Send + Sync {
    /// Stable widget type id.
    fn type_id(&self) -> &'static str;

    /// Produce the renderer-facing payload for `snapshot`.
    fn render(&self, snapshot: &WidgetSnapshot) -> SlintPayload {
        SlintPayload::from_widget(&snapshot.payload)
    }
}

/// Directory of per-type [`WidgetView`]s. A single instance is held by the
/// workspace controller.
#[derive(Default)]
pub struct WidgetViewDispatcher {
    views: RwLock<HashMap<&'static str, Box<dyn WidgetView>>>,
}

impl std::fmt::Debug for WidgetViewDispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WidgetViewDispatcher")
            .field("registered", &self.views.read().len())
            .finish()
    }
}

impl WidgetViewDispatcher {
    /// New, empty dispatcher.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or replace) a view for `type_id`.
    pub fn register(&self, view: Box<dyn WidgetView>) {
        let type_id = view.type_id();
        self.views.write().insert(type_id, view);
    }

    /// Produce the payload for `snapshot`. Falls back to a generic conversion
    /// when the widget type has no registered view.
    #[must_use]
    pub fn render(&self, snapshot: &WidgetSnapshot) -> SlintPayload {
        let views = self.views.read();
        match views.get(snapshot.widget_type) {
            Some(v) => v.render(snapshot),
            None => SlintPayload::from_widget(&snapshot.payload),
        }
    }

    /// How many views are currently registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.views.read().len()
    }

    /// Whether no views are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.views.read().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchid_widgets::{WidgetSnapshot, WidgetStatus};

    struct DemoView;
    impl WidgetView for DemoView {
        fn type_id(&self) -> &'static str {
            "demo"
        }
    }

    fn snapshot_of(widget_type: &'static str, payload: WidgetPayload) -> WidgetSnapshot {
        WidgetSnapshot {
            instance_id: uuid::Uuid::nil(),
            widget_type,
            title: "t".into(),
            status: WidgetStatus::Ready,
            payload,
        }
    }

    #[test]
    fn dispatcher_uses_registered_view() {
        let d = WidgetViewDispatcher::new();
        d.register(Box::new(DemoView));
        assert_eq!(d.len(), 1);
        let out = d.render(&snapshot_of(
            "demo",
            WidgetPayload::Text {
                lines: vec!["hello".into()],
            },
        ));
        match out {
            SlintPayload::Text(rows) => assert_eq!(rows, vec!["hello".to_string()]),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn dispatcher_falls_back_when_no_view_registered() {
        let d = WidgetViewDispatcher::new();
        let out = d.render(&snapshot_of(
            "unknown",
            WidgetPayload::KeyValueList {
                entries: vec![("k".into(), "v".into())],
            },
        ));
        match out {
            SlintPayload::KeyValueList(kv) => {
                assert_eq!(kv, vec![("k".to_string(), "v".to_string())])
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
