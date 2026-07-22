//! Payload for the local calendar / agenda widget.

#![allow(missing_docs)]

/// One cell in the 6×7 month grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarDayCell {
    pub date_key: String,
    pub day: i32,
    pub in_month: bool,
    pub is_today: bool,
    pub is_selected: bool,
    /// Up to three accent color indices for event dots.
    pub dot_colors: Vec<i32>,
    pub event_count: i32,
}

/// One agenda row for the selected day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarEventRow {
    pub id: String,
    pub title: String,
    /// Preformatted time label (`09:00–10:30`) or empty when all-day
    /// (UI fills "All day" via i18n).
    pub time_label: String,
    pub notes_preview: String,
    pub all_day: bool,
    pub color: i32,
}

/// One upcoming event in the next-7-days strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarUpcomingRow {
    pub id: String,
    pub title: String,
    pub date_key: String,
    pub time_label: String,
    pub all_day: bool,
    pub color: i32,
}

/// Render payload for the calendar widget.
#[derive(Debug, Clone)]
pub struct CalendarPayload {
    pub year: i32,
    /// 1..=12
    pub month: i32,
    pub selected_date: String,
    pub today_date: String,
    /// 0 = Sunday, 1 = Monday (from locale settings).
    pub first_day_of_week: i32,
    pub days: Vec<CalendarDayCell>,
    pub events: Vec<CalendarEventRow>,
    pub upcoming: Vec<CalendarUpcomingRow>,
    pub show_upcoming: bool,
    pub show_notes_preview: bool,
    pub time_step_minutes: i32,
    /// Active color filter, or `-1` when showing all colors.
    pub color_filter: i32,
    pub editor_open: bool,
    /// Empty when creating a new event.
    pub editor_event_id: String,
    pub editor_is_new: bool,
    pub editor_title: String,
    pub editor_date: String,
    pub editor_all_day: bool,
    pub editor_start_hour: i32,
    pub editor_start_min: i32,
    pub editor_end_hour: i32,
    pub editor_end_min: i32,
    pub editor_notes: String,
    pub editor_color: i32,
    /// Confirmation sheet visible above the editor.
    pub delete_confirm_open: bool,
}
