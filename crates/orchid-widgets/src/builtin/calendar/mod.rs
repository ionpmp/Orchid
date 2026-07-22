//! Local calendar / agenda — month grid, day list, CRUD events.

pub mod config;

use std::sync::{Arc, LazyLock};
use std::time::Duration as StdDuration;

use async_trait::async_trait;
use chrono::{Datelike, Duration, Local, NaiveDate, Weekday};
use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::{
    CalendarDayCell, CalendarEventRow, CalendarPayload, CalendarUpcomingRow,
};
use crate::widget::refresh::PeriodicRefresh;
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};
use orchid_storage::{LifecycleState, WidgetSize};

pub use config::{
    decode_config, format_date, format_minutes, parse_date, parse_date_input, CalendarConfig,
    CalendarEvent,
};

/// Stable type id.
pub const TYPE_ID: &str = "calendar";

static CALENDAR_LIVE: LazyLock<DashMap<Uuid, Arc<CalendarHandle>>> = LazyLock::new(DashMap::new);

#[derive(Debug, Clone)]
struct EventDraft {
    title: String,
    date: String,
    all_day: bool,
    start_minutes: u16,
    end_minutes: u16,
    notes: String,
    color: u8,
}

impl EventDraft {
    fn from_event(ev: &CalendarEvent) -> Self {
        Self {
            title: ev.title.clone(),
            date: ev.date.clone(),
            all_day: ev.all_day,
            start_minutes: ev.start_minutes,
            end_minutes: ev.end_minutes,
            notes: ev.notes.clone(),
            color: ev.color,
        }
    }

    fn blank_on(date: &str) -> Self {
        let ev = CalendarEvent::blank_on(date);
        Self::from_event(&ev)
    }
}

#[derive(Debug, Clone, Default)]
struct UiState {
    editor_open: bool,
    editing_id: Option<String>,
    draft: Option<EventDraft>,
    delete_confirm_open: bool,
    /// When set, agenda / upcoming / month dots show only this color (0..=5).
    color_filter: Option<u8>,
}

struct CalendarHandle {
    instance_id: Uuid,
    config: Arc<RwLock<CalendarConfig>>,
    ui: Arc<RwLock<UiState>>,
    orchid_config: Arc<RwLock<orchid_storage::OrchidConfig>>,
    bus: Arc<orchid_core::EventBus>,
    refresh: Mutex<PeriodicRefresh>,
}

impl CalendarHandle {
    fn publish(&self) {
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    fn schedule_refresh(self: &Arc<Self>) {
        let mut refresh = self.refresh.lock();
        refresh.set_interval(StdDuration::from_secs(60 * 30));
        let handle = Arc::clone(self);
        refresh.start(move || {
            let handle = Arc::clone(&handle);
            async move {
                handle.publish();
            }
        });
    }

    fn stop_refresh(&self) {
        self.refresh.lock().stop();
    }
}

/// Snapshot the live config for the settings dialog.
#[must_use]
pub fn current_config(instance_id: Uuid) -> Option<CalendarConfig> {
    CALENDAR_LIVE
        .get(&instance_id)
        .map(|h| h.config.read().clone())
}

/// Apply a settings-dialog mutation to the live config.
pub fn update_config(instance_id: Uuid, mutate: impl FnOnce(&mut CalendarConfig)) {
    let Some(h) = CALENDAR_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        mutate(&mut cfg);
        cfg.normalize();
    }
    h.publish();
}

/// Select a day (`YYYY-MM-DD`) and align the month view.
///
/// When the editor is open, also moves the draft onto that day (date picker).
pub fn select_date(instance_id: Uuid, date_key: &str) {
    let Some(d) = parse_date_input(date_key) else {
        return;
    };
    let Some(h) = CALENDAR_LIVE.get(&instance_id) else {
        return;
    };
    let key = format_date(d);
    {
        let mut cfg = h.config.write();
        cfg.selected_date = key.clone();
        cfg.view_year = d.year();
        cfg.view_month = d.month() as u8;
        cfg.normalize();
    }
    {
        let mut ui = h.ui.write();
        if ui.editor_open {
            if let Some(draft) = ui.draft.as_mut() {
                draft.date = key;
            }
        }
    }
    h.publish();
}

/// Select a day and open the new-event editor (double-click shortcut).
pub fn activate_day(instance_id: Uuid, date_key: &str) {
    select_date(instance_id, date_key);
    open_new_editor(instance_id);
}

/// Shift the visible month by `delta` (−1 / +1).
pub fn shift_month(instance_id: Uuid, delta: i32) {
    if delta == 0 {
        return;
    }
    let Some(h) = CALENDAR_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        let mut y = cfg.view_year;
        let mut m = i32::from(cfg.view_month) + delta;
        while m < 1 {
            m += 12;
            y -= 1;
        }
        while m > 12 {
            m -= 12;
            y += 1;
        }
        if y < 1 {
            y = 1;
            m = 1;
        }
        cfg.view_year = y;
        cfg.view_month = m as u8;
        // Keep selection inside the new month when possible.
        if let Some(sel) = parse_date(&cfg.selected_date) {
            let day = sel.day().min(days_in_month(y, m as u32));
            if let Some(nd) = NaiveDate::from_ymd_opt(y, m as u32, day) {
                cfg.selected_date = format_date(nd);
            }
        }
        cfg.normalize();
    }
    h.publish();
}

/// Jump view + selection to today.
pub fn goto_today(instance_id: Uuid) {
    let today = Local::now().date_naive();
    select_date(instance_id, &format_date(today));
}

/// Open the editor for a new event on the selected day.
pub fn open_new_editor(instance_id: Uuid) {
    let Some(h) = CALENDAR_LIVE.get(&instance_id) else {
        return;
    };
    let (date, default_all_day, duration, color_filter) = {
        let cfg = h.config.read();
        let ui = h.ui.read();
        (
            cfg.selected_date.clone(),
            cfg.default_all_day,
            cfg.default_duration_minutes,
            ui.color_filter,
        )
    };
    {
        let mut ui = h.ui.write();
        ui.editor_open = true;
        ui.editing_id = None;
        let mut draft = EventDraft::blank_on(&date);
        draft.all_day = default_all_day;
        if !default_all_day {
            let start = draft.start_minutes;
            let end = start.saturating_add(duration).min(23 * 60 + 59);
            draft.end_minutes = end.max(start);
        }
        if let Some(c) = color_filter {
            draft.color = c;
        }
        ui.draft = Some(draft);
        ui.delete_confirm_open = false;
    }
    h.publish();
}

/// Toggle agenda/month color filter (`color` 0..=5). Pass a negative value to clear.
pub fn set_color_filter(instance_id: Uuid, color: i32) {
    let Some(h) = CALENDAR_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut ui = h.ui.write();
        if color < 0 {
            ui.color_filter = None;
        } else {
            let c = color.clamp(0, 5) as u8;
            ui.color_filter = match ui.color_filter {
                Some(cur) if cur == c => None,
                _ => Some(c),
            };
        }
    }
    h.publish();
}

/// Open the editor for an existing event.
pub fn open_edit_editor(instance_id: Uuid, event_id: &str) {
    let Some(h) = CALENDAR_LIVE.get(&instance_id) else {
        return;
    };
    let ev = {
        let cfg = h.config.read();
        cfg.events.iter().find(|e| e.id == event_id).cloned()
    };
    let Some(ev) = ev else {
        return;
    };
    {
        let mut ui = h.ui.write();
        ui.editor_open = true;
        ui.editing_id = Some(ev.id.clone());
        ui.draft = Some(EventDraft::from_event(&ev));
        ui.delete_confirm_open = false;
    }
    h.publish();
}

/// Close the editor without saving.
pub fn close_editor(instance_id: Uuid) {
    let Some(h) = CALENDAR_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut ui = h.ui.write();
        ui.editor_open = false;
        ui.editing_id = None;
        ui.draft = None;
        ui.delete_confirm_open = false;
    }
    h.publish();
}

/// Shift the draft date by whole days (−1 / +1) and keep the grid selection in sync.
pub fn shift_editor_date(instance_id: Uuid, delta_days: i32) {
    if delta_days == 0 {
        return;
    }
    let Some(h) = CALENDAR_LIVE.get(&instance_id) else {
        return;
    };
    let next_key = {
        let ui = h.ui.read();
        let Some(draft) = ui.draft.as_ref() else {
            return;
        };
        let Some(cur) = parse_date(&draft.date) else {
            return;
        };
        cur.checked_add_signed(Duration::days(i64::from(delta_days)))
            .map(format_date)
    };
    let Some(next_key) = next_key else {
        return;
    };
    {
        let mut ui = h.ui.write();
        if let Some(draft) = ui.draft.as_mut() {
            draft.date = next_key.clone();
        }
    }
    if let Some(d) = parse_date(&next_key) {
        let mut cfg = h.config.write();
        cfg.selected_date = next_key;
        cfg.view_year = d.year();
        cfg.view_month = d.month() as u8;
        cfg.normalize();
    }
    h.publish();
}

/// Nudge draft start time by minutes (can be ±15 / ±60).
pub fn nudge_editor_start(instance_id: Uuid, delta_minutes: i32) {
    mutate_draft(instance_id, |d| {
        let next = i32::from(d.start_minutes).saturating_add(delta_minutes);
        d.start_minutes = next.clamp(0, 23 * 60 + 59) as u16;
        if d.end_minutes < d.start_minutes {
            d.end_minutes = d.start_minutes;
        }
    });
}

/// Nudge draft end time by minutes (can be ±15 / ±60).
pub fn nudge_editor_end(instance_id: Uuid, delta_minutes: i32) {
    mutate_draft(instance_id, |d| {
        let next = i32::from(d.end_minutes).saturating_add(delta_minutes);
        d.end_minutes = next.clamp(0, 23 * 60 + 59) as u16;
        if d.end_minutes < d.start_minutes {
            d.end_minutes = d.start_minutes;
        }
    });
}

/// Show the delete confirmation sheet (edit mode only).
pub fn request_delete(instance_id: Uuid) {
    let Some(h) = CALENDAR_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut ui = h.ui.write();
        if ui.editing_id.is_none() || !ui.editor_open {
            return;
        }
        ui.delete_confirm_open = true;
    }
    h.publish();
}

/// Dismiss the delete confirmation sheet.
pub fn cancel_delete(instance_id: Uuid) {
    let Some(h) = CALENDAR_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut ui = h.ui.write();
        ui.delete_confirm_open = false;
    }
    h.publish();
}

/// Confirm deletion of the event currently being edited.
pub fn confirm_delete(instance_id: Uuid) {
    let Some(h) = CALENDAR_LIVE.get(&instance_id) else {
        return;
    };
    let event_id = h.ui.read().editing_id.clone();
    let Some(event_id) = event_id else {
        return;
    };
    delete_event(instance_id, &event_id);
}

/// Turn an open edit session into a create session with the same draft fields.
///
/// The original event is left unchanged until the user saves the duplicate.
pub fn duplicate_editor(instance_id: Uuid) {
    let Some(h) = CALENDAR_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut ui = h.ui.write();
        if !ui.editor_open || ui.draft.is_none() || ui.editing_id.is_none() {
            return;
        }
        ui.editing_id = None;
        ui.delete_confirm_open = false;
    }
    h.publish();
}

/// Update a draft field while the editor is open.
pub fn set_editor_title(instance_id: Uuid, title: String) {
    mutate_draft(instance_id, |d| d.title = title);
}

/// Update the draft date (`YYYY-MM-DD`).
pub fn set_editor_date(instance_id: Uuid, date: String) {
    if parse_date(&date).is_none() {
        return;
    }
    mutate_draft(instance_id, |d| d.date = date);
}

/// Toggle / set all-day on the draft.
pub fn set_editor_all_day(instance_id: Uuid, all_day: bool) {
    mutate_draft(instance_id, |d| d.all_day = all_day);
}

/// Set draft start time from hour/minute (minutes may overflow/underflow).
pub fn set_editor_start(instance_id: Uuid, hour: i32, minute: i32) {
    mutate_draft(instance_id, |d| {
        d.start_minutes = total_minutes(hour, minute);
        if d.end_minutes < d.start_minutes {
            d.end_minutes = d.start_minutes;
        }
    });
}

/// Set draft end time from hour/minute (minutes may overflow/underflow).
pub fn set_editor_end(instance_id: Uuid, hour: i32, minute: i32) {
    mutate_draft(instance_id, |d| {
        d.end_minutes = total_minutes(hour, minute);
        if d.end_minutes < d.start_minutes {
            d.end_minutes = d.start_minutes;
        }
    });
}

/// Set draft notes body.
pub fn set_editor_notes(instance_id: Uuid, notes: String) {
    mutate_draft(instance_id, |d| d.notes = notes);
}

/// Set draft accent color (0..=5).
pub fn set_editor_color(instance_id: Uuid, color: i32) {
    mutate_draft(instance_id, |d| d.color = color.clamp(0, 5) as u8);
}

/// Persist the open draft (create or update).
pub fn save_editor(instance_id: Uuid) {
    let Some(h) = CALENDAR_LIVE.get(&instance_id) else {
        return;
    };
    let (editing_id, draft) = {
        let ui = h.ui.read();
        (ui.editing_id.clone(), ui.draft.clone())
    };
    let Some(draft) = draft else {
        return;
    };
    if parse_date(&draft.date).is_none() {
        return;
    }
    {
        let mut cfg = h.config.write();
        let title = draft.title.trim().to_string();
        if let Some(id) = editing_id {
            if let Some(ev) = cfg.events.iter_mut().find(|e| e.id == id) {
                ev.title = title;
                ev.date = draft.date.clone();
                ev.all_day = draft.all_day;
                ev.start_minutes = draft.start_minutes;
                ev.end_minutes = draft.end_minutes;
                ev.notes = draft.notes;
                ev.color = draft.color.min(5);
            }
        } else {
            cfg.events.push(CalendarEvent {
                id: Uuid::new_v4().to_string(),
                title,
                date: draft.date.clone(),
                all_day: draft.all_day,
                start_minutes: draft.start_minutes,
                end_minutes: draft.end_minutes,
                notes: draft.notes,
                color: draft.color.min(5),
            });
        }
        cfg.selected_date = draft.date;
        if let Some(d) = parse_date(&cfg.selected_date) {
            cfg.view_year = d.year();
            cfg.view_month = d.month() as u8;
        }
        cfg.normalize();
    }
    {
        let mut ui = h.ui.write();
        ui.editor_open = false;
        ui.editing_id = None;
        ui.draft = None;
        ui.delete_confirm_open = false;
    }
    h.publish();
}

/// Delete an event by id (also closes editor if it was editing that event).
pub fn delete_event(instance_id: Uuid, event_id: &str) {
    let Some(h) = CALENDAR_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        cfg.events.retain(|e| e.id != event_id);
    }
    {
        let mut ui = h.ui.write();
        if ui.editing_id.as_deref() == Some(event_id) {
            ui.editor_open = false;
            ui.editing_id = None;
            ui.draft = None;
        }
        ui.delete_confirm_open = false;
    }
    h.publish();
}

fn mutate_draft(instance_id: Uuid, f: impl FnOnce(&mut EventDraft)) {
    let Some(h) = CALENDAR_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut ui = h.ui.write();
        if let Some(d) = ui.draft.as_mut() {
            f(d);
        } else {
            return;
        }
    }
    h.publish();
}

fn total_minutes(hour: i32, minute: i32) -> u16 {
    let total = hour.saturating_mul(60).saturating_add(minute);
    total.clamp(0, 23 * 60 + 59) as u16
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let first =
        NaiveDate::from_ymd_opt(year, month, 1).unwrap_or_else(|| Local::now().date_naive());
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    next.and_then(|n| n.signed_duration_since(first).num_days().try_into().ok())
        .unwrap_or(30)
}

/// Hit produced by [`search_all_events`].
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct CalendarSearchHit {
    pub instance_id: Uuid,
    pub event_id: String,
    pub title: String,
    pub date: String,
    pub subtitle: String,
    pub score: i32,
}

/// Search events across all live calendar instances.
#[must_use]
pub fn search_all_events(query: &str, limit: usize) -> Vec<CalendarSearchHit> {
    let q = query.trim().to_lowercase();
    if q.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for entry in CALENDAR_LIVE.iter() {
        let instance_id = *entry.key();
        let cfg = entry.value().config.read();
        for ev in &cfg.events {
            let title = ev.title.trim();
            let title_l = title.to_lowercase();
            let notes_l = ev.notes.to_lowercase();
            let score = if title_l == q {
                100
            } else if title_l.starts_with(&q) {
                90
            } else if title_l.contains(&q) {
                75
            } else if notes_l.contains(&q) {
                55
            } else if ev.date.contains(&q) {
                40
            } else {
                continue;
            };
            let time = if ev.all_day {
                String::new()
            } else {
                format_minutes(ev.start_minutes)
            };
            let subtitle = if time.is_empty() {
                ev.date.clone()
            } else {
                format!("{} · {}", ev.date, time)
            };
            hits.push(CalendarSearchHit {
                instance_id,
                event_id: ev.id.clone(),
                title: if title.is_empty() {
                    String::new()
                } else {
                    title.to_string()
                },
                date: ev.date.clone(),
                subtitle,
                score,
            });
        }
    }
    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.date.cmp(&b.date)));
    hits.truncate(limit);
    hits
}

/// Build the 42-cell month grid for a view month.
#[must_use]
pub fn build_month_grid(
    year: i32,
    month: u8,
    selected: &str,
    today: &str,
    first_day_of_week: u8,
    events: &[CalendarEvent],
) -> Vec<CalendarDayCell> {
    let month = month.clamp(1, 12);
    let first = NaiveDate::from_ymd_opt(year, u32::from(month), 1)
        .unwrap_or_else(|| Local::now().date_naive());
    let start_offset = weekday_offset(first.weekday(), first_day_of_week);
    let grid_start = first - Duration::days(i64::from(start_offset));

    let mut cells = Vec::with_capacity(42);
    for i in 0..42 {
        let d = grid_start + Duration::days(i);
        let key = format_date(d);
        let mut colors: Vec<i32> = Vec::new();
        let mut count = 0i32;
        for ev in events.iter().filter(|e| e.date == key) {
            count += 1;
            if colors.len() < 3 && !colors.contains(&i32::from(ev.color)) {
                colors.push(i32::from(ev.color));
            }
        }
        cells.push(CalendarDayCell {
            date_key: key.clone(),
            day: d.day() as i32,
            in_month: d.month() == u32::from(month) && d.year() == year,
            is_today: key == today,
            is_selected: key == selected,
            dot_colors: colors,
            event_count: count,
        });
    }
    cells
}

/// Weekday column offset for the 1st of the month.
///
/// `first_day_of_week`: 0 = Sunday, 1 = Monday.
#[must_use]
pub fn weekday_offset(weekday: Weekday, first_day_of_week: u8) -> u32 {
    let from_sunday = weekday.num_days_from_sunday();
    if first_day_of_week == 0 {
        from_sunday
    } else {
        weekday.num_days_from_monday()
    }
}

fn notes_preview(notes: &str) -> String {
    let line = notes
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    if line.is_empty() {
        return String::new();
    }
    const MAX: usize = 48;
    let mut out = String::new();
    for (i, ch) in line.chars().enumerate() {
        if i >= MAX {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

fn sort_events_for_day(events: &mut [CalendarEvent]) {
    events.sort_by(|a, b| match (a.all_day, b.all_day) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        (true, true) => a.title.cmp(&b.title),
        (false, false) => a
            .start_minutes
            .cmp(&b.start_minutes)
            .then_with(|| a.title.cmp(&b.title)),
    });
}

/// Calendar widget implementation.
pub struct CalendarWidget {
    instance_id: Uuid,
    handle: Arc<CalendarHandle>,
}

impl std::fmt::Debug for CalendarWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CalendarWidget")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl CalendarWidget {
    /// Construct with config.
    pub fn new(
        instance_id: Uuid,
        mut config: CalendarConfig,
        bus: Arc<orchid_core::EventBus>,
        orchid_config: Arc<RwLock<orchid_storage::OrchidConfig>>,
    ) -> Self {
        config.normalize();
        let handle = Arc::new(CalendarHandle {
            instance_id,
            config: Arc::new(RwLock::new(config)),
            ui: Arc::new(RwLock::new(UiState::default())),
            orchid_config,
            bus,
            refresh: Mutex::new(PeriodicRefresh::new(StdDuration::from_secs(60 * 30))),
        });
        CALENDAR_LIVE.insert(instance_id, Arc::clone(&handle));
        Self {
            instance_id,
            handle,
        }
    }

    fn build_payload(&self) -> CalendarPayload {
        let cfg = self.handle.config.read().clone();
        let ui = self.handle.ui.read().clone();
        let first_day = self
            .handle
            .orchid_config
            .read()
            .locale
            .first_day_of_week
            .min(1);
        let today = format_date(Local::now().date_naive());
        let visible_events: Vec<CalendarEvent> = match ui.color_filter {
            Some(c) => cfg.events.iter().filter(|e| e.color == c).cloned().collect(),
            None => cfg.events.clone(),
        };
        let days = build_month_grid(
            cfg.view_year,
            cfg.view_month,
            &cfg.selected_date,
            &today,
            first_day,
            &visible_events,
        );

        let mut day_events: Vec<CalendarEvent> = visible_events
            .iter()
            .filter(|e| e.date == cfg.selected_date)
            .cloned()
            .collect();
        sort_events_for_day(&mut day_events);
        let events: Vec<CalendarEventRow> = day_events
            .iter()
            .map(|e| CalendarEventRow {
                id: e.id.clone(),
                title: e.title.clone(),
                time_label: if e.all_day {
                    String::new()
                } else if e.end_minutes > e.start_minutes {
                    format!(
                        "{}–{}",
                        format_minutes(e.start_minutes),
                        format_minutes(e.end_minutes)
                    )
                } else {
                    format_minutes(e.start_minutes)
                },
                notes_preview: notes_preview(&e.notes),
                all_day: e.all_day,
                color: i32::from(e.color),
            })
            .collect();

        let draft = ui
            .draft
            .clone()
            .unwrap_or_else(|| EventDraft::blank_on(&cfg.selected_date));

        let upcoming = if cfg.show_upcoming {
            build_upcoming(&visible_events, &today, 7)
        } else {
            Vec::new()
        };

        let events = if cfg.show_notes_preview {
            events
        } else {
            events
                .into_iter()
                .map(|mut e| {
                    e.notes_preview.clear();
                    e
                })
                .collect()
        };

        CalendarPayload {
            year: cfg.view_year,
            month: i32::from(cfg.view_month),
            selected_date: cfg.selected_date,
            today_date: today,
            first_day_of_week: i32::from(first_day),
            days,
            events,
            upcoming,
            show_upcoming: cfg.show_upcoming,
            show_notes_preview: cfg.show_notes_preview,
            time_step_minutes: i32::from(cfg.time_step_minutes),
            color_filter: ui.color_filter.map(i32::from).unwrap_or(-1),
            editor_open: ui.editor_open,
            editor_event_id: ui.editing_id.clone().unwrap_or_default(),
            editor_is_new: ui.editing_id.is_none(),
            editor_title: draft.title,
            editor_date: draft.date,
            editor_all_day: draft.all_day,
            editor_start_hour: i32::from(draft.start_minutes / 60),
            editor_start_min: i32::from(draft.start_minutes % 60),
            editor_end_hour: i32::from(draft.end_minutes / 60),
            editor_end_min: i32::from(draft.end_minutes % 60),
            editor_notes: draft.notes,
            editor_color: i32::from(draft.color),
            delete_confirm_open: ui.delete_confirm_open,
        }
    }
}

fn build_upcoming(
    events: &[CalendarEvent],
    today_key: &str,
    days: i64,
) -> Vec<CalendarUpcomingRow> {
    let Some(today) = parse_date(today_key) else {
        return Vec::new();
    };
    let end = today + Duration::days(days);
    let mut rows: Vec<(String, CalendarUpcomingRow)> = Vec::new();
    for ev in events {
        let Some(d) = parse_date(&ev.date) else {
            continue;
        };
        if d < today || d >= end {
            continue;
        }
        let time_label = if ev.all_day {
            String::new()
        } else if ev.end_minutes > ev.start_minutes {
            format!(
                "{}–{}",
                format_minutes(ev.start_minutes),
                format_minutes(ev.end_minutes)
            )
        } else {
            format_minutes(ev.start_minutes)
        };
        rows.push((
            format!(
                "{}-{:04}-{}",
                ev.date,
                if ev.all_day { 0 } else { ev.start_minutes },
                ev.title
            ),
            CalendarUpcomingRow {
                id: ev.id.clone(),
                title: ev.title.clone(),
                date_key: ev.date.clone(),
                time_label,
                all_day: ev.all_day,
                color: i32::from(ev.color),
            },
        ));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows.into_iter().map(|(_, r)| r).take(12).collect()
}

#[async_trait]
impl Widget for CalendarWidget {
    fn type_id(&self) -> &'static str {
        TYPE_ID
    }

    fn instance_id(&self) -> Uuid {
        self.instance_id
    }

    async fn on_create(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_activate(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.handle.schedule_refresh();
        Ok(())
    }

    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.handle.stop_refresh();
        Ok(())
    }

    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.handle.stop_refresh();
        Ok(())
    }

    async fn on_close(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.handle.stop_refresh();
        CALENDAR_LIVE.remove(&self.instance_id);
        Ok(())
    }

    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }

    fn snapshot(&self) -> Option<WidgetSnapshot> {
        let payload = self.build_payload();
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title: String::new(),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::Calendar(payload),
        })
    }

    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        let cfg = self.handle.config.read().clone();
        state_codec::save_state(&cfg)
    }

    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let cfg = decode_config(bytes);
        *self.handle.config.write() = cfg;
        Ok(())
    }

    fn capabilities(&self) -> WidgetCapabilities {
        WidgetCapabilities {
            supports_resize: true,
            min_size: Some(WidgetSize::Small),
            max_size: None,
            preferred_size: Some(WidgetSize::Large),
            allows_grouping: true,
            keeps_state_when_unloaded: true,
            has_settings_panel: true,
        }
    }
}

/// Descriptor ready to register on a widget registry.
#[must_use]
pub fn descriptor() -> WidgetDescriptor {
    let factory: WidgetFactory = Arc::new(|ctx: WidgetContext, state_bytes| {
        let cfg = match state_bytes {
            Some(bytes) => decode_config(bytes),
            None => {
                let mut c = CalendarConfig::default();
                c.normalize();
                c
            }
        };
        Ok(Box::new(CalendarWidget::new(
            ctx.instance_id,
            cfg,
            ctx.bus.clone(),
            ctx.config.clone(),
        )) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-calendar-name",
        description_key: "widget-calendar-desc",
        icon_name: "calendar",
        category: WidgetCategory::Productivity,
        default_size: WidgetSize::Large,
        min_size: Some(WidgetSize::Small),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: true,
        factory,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekday_offset_sunday_start() {
        // 2026-07-01 is Wednesday → offset 3 from Sunday
        let d = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
        assert_eq!(weekday_offset(d.weekday(), 0), 3);
    }

    #[test]
    fn weekday_offset_monday_start() {
        // 2026-07-01 is Wednesday → offset 2 from Monday
        let d = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
        assert_eq!(weekday_offset(d.weekday(), 1), 2);
    }

    #[test]
    fn month_grid_has_42_cells() {
        let cells = build_month_grid(2026, 7, "2026-07-21", "2026-07-21", 1, &[]);
        assert_eq!(cells.len(), 42);
        let in_month: Vec<_> = cells.iter().filter(|c| c.in_month).collect();
        assert_eq!(in_month.len(), 31);
        let selected = cells.iter().find(|c| c.is_selected).unwrap();
        assert_eq!(selected.date_key, "2026-07-21");
        assert!(selected.is_today);
    }

    #[test]
    fn month_grid_dots_from_events() {
        let events = vec![
            CalendarEvent {
                id: "a".into(),
                title: "A".into(),
                date: "2026-07-21".into(),
                all_day: true,
                start_minutes: 0,
                end_minutes: 0,
                notes: String::new(),
                color: 2,
            },
            CalendarEvent {
                id: "b".into(),
                title: "B".into(),
                date: "2026-07-21".into(),
                all_day: false,
                start_minutes: 600,
                end_minutes: 660,
                notes: String::new(),
                color: 4,
            },
        ];
        let cells = build_month_grid(2026, 7, "2026-07-21", "2026-07-15", 1, &events);
        let cell = cells.iter().find(|c| c.date_key == "2026-07-21").unwrap();
        assert_eq!(cell.event_count, 2);
        assert_eq!(cell.dot_colors, vec![2, 4]);
    }

    #[test]
    fn normalize_repairs_bad_selected_date() {
        let mut cfg = CalendarConfig {
            events: vec![],
            view_year: 2026,
            view_month: 7,
            selected_date: "not-a-date".into(),
            ..CalendarConfig::default()
        };
        cfg.normalize();
        assert_eq!(cfg.selected_date, "2026-07-01");
    }

    #[test]
    fn format_minutes_pads() {
        assert_eq!(format_minutes(0), "00:00");
        assert_eq!(format_minutes(9 * 60 + 5), "09:05");
    }

    #[test]
    fn parse_date_input_accepts_compact_and_dashed() {
        assert_eq!(
            parse_date_input("2026-07-21"),
            NaiveDate::from_ymd_opt(2026, 7, 21)
        );
        assert_eq!(
            parse_date_input("20260721"),
            NaiveDate::from_ymd_opt(2026, 7, 21)
        );
        assert_eq!(parse_date_input(" 2026-07-21 "), NaiveDate::from_ymd_opt(2026, 7, 21));
        assert!(parse_date_input("2026-13-01").is_none());
        assert!(parse_date_input("not-a-date").is_none());
    }

    #[test]
    fn select_date_accepts_compact_input() {
        let bus = Arc::new(orchid_core::EventBus::new(
            orchid_core::EventBusConfig::default(),
        ));
        let orchid_config = Arc::new(RwLock::new(orchid_storage::OrchidConfig::default()));
        let id = Uuid::new_v4();
        let mut cfg = CalendarConfig::default();
        cfg.selected_date = "2026-01-01".into();
        cfg.view_year = 2026;
        cfg.view_month = 1;
        let widget = CalendarWidget::new(id, cfg, bus, orchid_config);
        select_date(id, "20260721");
        let snap = widget.snapshot().expect("snapshot");
        let WidgetPayload::Calendar(p) = snap.payload else {
            panic!("expected calendar");
        };
        assert_eq!(p.selected_date, "2026-07-21");
        assert_eq!(p.month, 7);
    }

    #[test]
    fn shift_editor_date_and_confirm_delete() {
        let bus = Arc::new(orchid_core::EventBus::new(
            orchid_core::EventBusConfig::default(),
        ));
        let orchid_config = Arc::new(RwLock::new(orchid_storage::OrchidConfig::default()));
        let id = Uuid::new_v4();
        let mut cfg = CalendarConfig::default();
        cfg.selected_date = "2026-07-21".into();
        cfg.view_year = 2026;
        cfg.view_month = 7;
        let widget = CalendarWidget::new(id, cfg, bus, orchid_config);

        open_new_editor(id);
        set_editor_title(id, "Trip".into());
        shift_editor_date(id, 1);
        save_editor(id);

        let snap = widget.snapshot().expect("snapshot");
        let WidgetPayload::Calendar(p) = snap.payload else {
            panic!("expected calendar");
        };
        assert_eq!(p.selected_date, "2026-07-22");
        // Agenda is for selected day; event was saved on 22nd.
        assert_eq!(p.events.len(), 1);

        open_edit_editor(id, &p.events[0].id);
        request_delete(id);
        let snap2 = widget.snapshot().expect("snapshot");
        let WidgetPayload::Calendar(p2) = snap2.payload else {
            panic!("expected calendar");
        };
        assert!(p2.delete_confirm_open);
        confirm_delete(id);
        let snap3 = widget.snapshot().expect("snapshot");
        let WidgetPayload::Calendar(p3) = snap3.payload else {
            panic!("expected calendar");
        };
        assert!(!p3.editor_open);
        assert!(p3.events.is_empty());
    }

    #[test]
    fn build_upcoming_lists_next_days_only() {
        let events = vec![
            CalendarEvent {
                id: "past".into(),
                title: "Past".into(),
                date: "2026-07-20".into(),
                all_day: true,
                start_minutes: 0,
                end_minutes: 0,
                notes: String::new(),
                color: 0,
            },
            CalendarEvent {
                id: "today".into(),
                title: "Today".into(),
                date: "2026-07-21".into(),
                all_day: true,
                start_minutes: 0,
                end_minutes: 0,
                notes: String::new(),
                color: 1,
            },
            CalendarEvent {
                id: "soon".into(),
                title: "Soon".into(),
                date: "2026-07-24".into(),
                all_day: false,
                start_minutes: 600,
                end_minutes: 660,
                notes: String::new(),
                color: 2,
            },
            CalendarEvent {
                id: "far".into(),
                title: "Far".into(),
                date: "2026-08-01".into(),
                all_day: true,
                start_minutes: 0,
                end_minutes: 0,
                notes: String::new(),
                color: 3,
            },
        ];
        let rows = build_upcoming(&events, "2026-07-21", 7);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "today");
        assert_eq!(rows[1].id, "soon");
        assert!(rows[1].time_label.contains("10:00"));
    }

    #[test]
    fn decode_config_accepts_legacy_four_field_blob() {
        #[derive(serde::Serialize)]
        struct Legacy {
            events: Vec<CalendarEvent>,
            view_year: i32,
            view_month: u8,
            selected_date: String,
        }
        let legacy = Legacy {
            events: vec![CalendarEvent {
                id: "e1".into(),
                title: "Legacy".into(),
                date: "2026-07-21".into(),
                all_day: true,
                start_minutes: 0,
                end_minutes: 0,
                notes: String::new(),
                color: 2,
            }],
            view_year: 2026,
            view_month: 7,
            selected_date: "2026-07-21".into(),
        };
        let bytes = bincode::serde::encode_to_vec(&legacy, bincode::config::standard()).unwrap();
        let cfg = decode_config(&bytes);
        assert_eq!(cfg.events.len(), 1);
        assert_eq!(cfg.events[0].title, "Legacy");
        assert!(cfg.show_upcoming);
        assert_eq!(cfg.time_step_minutes, 15);
    }

    #[test]
    fn duplicate_editor_switches_to_create_flow() {
        let bus = Arc::new(orchid_core::EventBus::new(
            orchid_core::EventBusConfig::default(),
        ));
        let orchid_config = Arc::new(RwLock::new(orchid_storage::OrchidConfig::default()));
        let id = Uuid::new_v4();
        let mut cfg = CalendarConfig::default();
        cfg.selected_date = "2026-07-21".into();
        cfg.view_year = 2026;
        cfg.view_month = 7;
        let widget = CalendarWidget::new(id, cfg, bus, orchid_config);

        open_new_editor(id);
        set_editor_title(id, "Original".into());
        save_editor(id);
        let snap = widget.snapshot().expect("snapshot");
        let WidgetPayload::Calendar(p) = snap.payload else {
            panic!("expected calendar");
        };
        let eid = p.events[0].id.clone();
        open_edit_editor(id, &eid);
        duplicate_editor(id);
        let snap2 = widget.snapshot().expect("snapshot");
        let WidgetPayload::Calendar(p2) = snap2.payload else {
            panic!("expected calendar");
        };
        assert!(p2.editor_open);
        assert!(p2.editor_is_new);
        assert_eq!(p2.editor_title, "Original");
        // Original still the only persisted event until save.
        assert_eq!(p2.events.len(), 1);
        save_editor(id);
        let snap3 = widget.snapshot().expect("snapshot");
        let WidgetPayload::Calendar(p3) = snap3.payload else {
            panic!("expected calendar");
        };
        assert_eq!(p3.events.len(), 2);
    }

    #[test]
    fn color_filter_limits_agenda_and_toggles_off() {
        let bus = Arc::new(orchid_core::EventBus::new(
            orchid_core::EventBusConfig::default(),
        ));
        let orchid_config = Arc::new(RwLock::new(orchid_storage::OrchidConfig::default()));
        let id = Uuid::new_v4();
        let mut cfg = CalendarConfig::default();
        cfg.selected_date = "2026-07-21".into();
        cfg.view_year = 2026;
        cfg.view_month = 7;
        cfg.events = vec![
            CalendarEvent {
                id: "blue".into(),
                title: "Blue".into(),
                date: "2026-07-21".into(),
                all_day: true,
                start_minutes: 0,
                end_minutes: 0,
                notes: String::new(),
                color: 0,
            },
            CalendarEvent {
                id: "green".into(),
                title: "Green".into(),
                date: "2026-07-21".into(),
                all_day: true,
                start_minutes: 0,
                end_minutes: 0,
                notes: String::new(),
                color: 1,
            },
        ];
        let widget = CalendarWidget::new(id, cfg, bus, orchid_config);
        set_color_filter(id, 1);
        let snap = widget.snapshot().expect("snapshot");
        let WidgetPayload::Calendar(p) = snap.payload else {
            panic!("expected calendar");
        };
        assert_eq!(p.color_filter, 1);
        assert_eq!(p.events.len(), 1);
        assert_eq!(p.events[0].id, "green");
        set_color_filter(id, 1); // toggle off
        let snap2 = widget.snapshot().expect("snapshot");
        let WidgetPayload::Calendar(p2) = snap2.payload else {
            panic!("expected calendar");
        };
        assert_eq!(p2.color_filter, -1);
        assert_eq!(p2.events.len(), 2);
    }

    #[test]
    fn open_new_editor_uses_default_duration_when_timed() {
        let bus = Arc::new(orchid_core::EventBus::new(
            orchid_core::EventBusConfig::default(),
        ));
        let orchid_config = Arc::new(RwLock::new(orchid_storage::OrchidConfig::default()));
        let id = Uuid::new_v4();
        let mut cfg = CalendarConfig::default();
        cfg.selected_date = "2026-07-21".into();
        cfg.default_all_day = false;
        cfg.default_duration_minutes = 90;
        let widget = CalendarWidget::new(id, cfg, bus, orchid_config);
        open_new_editor(id);
        let snap = widget.snapshot().expect("snapshot");
        let WidgetPayload::Calendar(p) = snap.payload else {
            panic!("expected calendar");
        };
        assert!(!p.editor_all_day);
        assert_eq!(p.editor_start_hour, 9);
        assert_eq!(p.editor_start_min, 0);
        assert_eq!(p.editor_end_hour, 10);
        assert_eq!(p.editor_end_min, 30);
    }

    #[test]
    fn search_all_events_matches_title_and_notes() {
        let bus = Arc::new(orchid_core::EventBus::new(
            orchid_core::EventBusConfig::default(),
        ));
        let orchid_config = Arc::new(RwLock::new(orchid_storage::OrchidConfig::default()));
        let id = Uuid::new_v4();
        let mut cfg = CalendarConfig::default();
        cfg.selected_date = "2026-07-21".into();
        cfg.events.push(CalendarEvent {
            id: "e1".into(),
            title: "Dentist".into(),
            date: "2026-07-22".into(),
            all_day: false,
            start_minutes: 600,
            end_minutes: 660,
            notes: "bring insurance card".into(),
            color: 0,
        });
        let _widget = CalendarWidget::new(id, cfg, bus, orchid_config);
        let by_title = search_all_events("dent", 10);
        assert_eq!(by_title.len(), 1);
        assert_eq!(by_title[0].event_id, "e1");
        let by_notes = search_all_events("insurance", 10);
        assert_eq!(by_notes.len(), 1);
    }

    #[test]
    fn save_and_delete_event_via_live_api() {
        let bus = Arc::new(orchid_core::EventBus::new(
            orchid_core::EventBusConfig::default(),
        ));
        let orchid_config = Arc::new(RwLock::new(orchid_storage::OrchidConfig::default()));
        let id = Uuid::new_v4();
        let mut cfg = CalendarConfig::default();
        cfg.selected_date = "2026-07-21".into();
        cfg.view_year = 2026;
        cfg.view_month = 7;
        let widget = CalendarWidget::new(id, cfg, bus, orchid_config);

        open_new_editor(id);
        set_editor_title(id, "Standup".into());
        set_editor_all_day(id, false);
        set_editor_start(id, 9, 0);
        set_editor_end(id, 9, 30);
        save_editor(id);

        let snap = widget.snapshot().expect("snapshot");
        let WidgetPayload::Calendar(p) = snap.payload else {
            panic!("expected calendar payload");
        };
        assert_eq!(p.events.len(), 1);
        assert_eq!(p.events[0].title, "Standup");
        assert!(!p.events[0].all_day);
        assert!(p.events[0].time_label.contains("09:00"));

        let eid = p.events[0].id.clone();
        delete_event(id, &eid);
        let snap2 = widget.snapshot().expect("snapshot");
        let WidgetPayload::Calendar(p2) = snap2.payload else {
            panic!("expected calendar payload");
        };
        assert!(p2.events.is_empty());
    }
}
