//! Persistent config for the local calendar / agenda widget.

#![allow(missing_docs)]

use bincode::{Decode, Encode};
use chrono::{Datelike, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One calendar event.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    /// Local calendar date as `YYYY-MM-DD`.
    pub date: String,
    pub all_day: bool,
    /// Minutes from midnight (0..=1439). Ignored when `all_day`.
    pub start_minutes: u16,
    /// Minutes from midnight (0..=1439). Ignored when `all_day`.
    pub end_minutes: u16,
    pub notes: String,
    /// Accent color index (0..=5).
    pub color: u8,
}

impl CalendarEvent {
    #[must_use]
    pub fn blank_on(date: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title: String::new(),
            date: date.to_string(),
            all_day: true,
            start_minutes: 9 * 60,
            end_minutes: 10 * 60,
            notes: String::new(),
            color: 0,
        }
    }
}

/// Persisted calendar widget state.
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct CalendarConfig {
    pub events: Vec<CalendarEvent>,
    pub view_year: i32,
    /// 1..=12
    pub view_month: u8,
    /// Selected day as `YYYY-MM-DD`.
    pub selected_date: String,
}

impl Default for CalendarConfig {
    fn default() -> Self {
        let today = Local::now().date_naive();
        Self {
            events: Vec::new(),
            view_year: today.year(),
            view_month: today.month() as u8,
            selected_date: format_date(today),
        }
    }
}

impl CalendarConfig {
    /// Clamp fields and repair invalid dates.
    pub fn normalize(&mut self) {
        if !(1..=12).contains(&self.view_month) {
            self.view_month = 1;
        }
        if self.view_year < 1 {
            self.view_year = 1;
        }
        if parse_date(&self.selected_date).is_none() {
            if let Some(d) = NaiveDate::from_ymd_opt(self.view_year, u32::from(self.view_month), 1)
            {
                self.selected_date = format_date(d);
            } else {
                let today = Local::now().date_naive();
                self.view_year = today.year();
                self.view_month = today.month() as u8;
                self.selected_date = format_date(today);
            }
        }
        for ev in &mut self.events {
            ev.color = ev.color.min(5);
            ev.start_minutes = ev.start_minutes.min(23 * 60 + 59);
            ev.end_minutes = ev.end_minutes.min(23 * 60 + 59);
            if ev.end_minutes < ev.start_minutes {
                ev.end_minutes = ev.start_minutes;
            }
            if parse_date(&ev.date).is_none() {
                ev.date = self.selected_date.clone();
            }
        }
    }
}

/// Format a naive date as `YYYY-MM-DD`.
#[must_use]
pub fn format_date(d: NaiveDate) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day())
}

/// Parse `YYYY-MM-DD` into a naive date.
#[must_use]
pub fn parse_date(s: &str) -> Option<NaiveDate> {
    let parts: Vec<_> = s.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let y: i32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let d: u32 = parts[2].parse().ok()?;
    NaiveDate::from_ymd_opt(y, m, d)
}

/// Format minutes-from-midnight as `HH:MM`.
#[must_use]
pub fn format_minutes(mins: u16) -> String {
    let h = mins / 60;
    let m = mins % 60;
    format!("{h:02}:{m:02}")
}
