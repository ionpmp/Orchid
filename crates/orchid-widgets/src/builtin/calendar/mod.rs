//! Local calendar / agenda — month grid, day list, CRUD events.

pub mod config;

use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use chrono::{Datelike, Duration, Local, NaiveDate, Weekday};
use dashmap::DashMap;
use parking_lot::RwLock;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::{CalendarDayCell, CalendarEventRow, CalendarPayload};
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};
use orchid_storage::{LifecycleState, WidgetSize};

pub use config::{format_date, format_minutes, parse_date, CalendarConfig, CalendarEvent};

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
}

struct CalendarHandle {
    instance_id: Uuid,
    config: Arc<RwLock<CalendarConfig>>,
    ui: Arc<RwLock<UiState>>,
    orchid_config: Arc<RwLock<orchid_storage::OrchidConfig>>,
    bus: Arc<orchid_core::EventBus>,
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
pub fn select_date(instance_id: Uuid, date_key: &str) {
    let Some(d) = parse_date(date_key) else {
        return;
    };
    let Some(h) = CALENDAR_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        cfg.selected_date = format_date(d);
        cfg.view_year = d.year();
        cfg.view_month = d.month() as u8;
        cfg.normalize();
    }
    h.publish();
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
    let date = h.config.read().selected_date.clone();
    {
        let mut ui = h.ui.write();
        ui.editor_open = true;
        ui.editing_id = None;
        ui.draft = Some(EventDraft::blank_on(&date));
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
        if let Some(id) = editing_id {
            if let Some(ev) = cfg.events.iter_mut().find(|e| e.id == id) {
                ev.title = draft.title;
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
                title: draft.title,
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
        let days = build_month_grid(
            cfg.view_year,
            cfg.view_month,
            &cfg.selected_date,
            &today,
            first_day,
            &cfg.events,
        );

        let mut day_events: Vec<CalendarEvent> = cfg
            .events
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

        CalendarPayload {
            year: cfg.view_year,
            month: i32::from(cfg.view_month),
            selected_date: cfg.selected_date,
            today_date: today,
            first_day_of_week: i32::from(first_day),
            days,
            events,
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
        }
    }
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
        Ok(())
    }

    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_close(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
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
        let mut cfg: CalendarConfig = state_codec::restore_state(bytes).unwrap_or_default();
        cfg.normalize();
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
            has_settings_panel: false,
        }
    }
}

/// Descriptor ready to register on a widget registry.
#[must_use]
pub fn descriptor() -> WidgetDescriptor {
    let factory: WidgetFactory = Arc::new(|ctx: WidgetContext, state_bytes| {
        let mut cfg = match state_bytes {
            Some(bytes) => state_codec::restore_state::<CalendarConfig>(bytes).unwrap_or_default(),
            None => CalendarConfig::default(),
        };
        cfg.normalize();
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
