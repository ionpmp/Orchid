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
    /// New events default to all-day when true.
    #[serde(default = "default_true")]
    pub default_all_day: bool,
    /// Show notes preview under agenda rows.
    #[serde(default = "default_true")]
    pub show_notes_preview: bool,
    /// Show the next-7-days upcoming strip.
    #[serde(default = "default_true")]
    pub show_upcoming: bool,
    /// Minute step for time nudge buttons (15 or 30).
    #[serde(default = "default_time_step")]
    pub time_step_minutes: u8,
    /// Default timed-event length in minutes (30 / 60 / 90).
    #[serde(default = "default_duration")]
    pub default_duration_minutes: u16,
}

fn default_true() -> bool {
    true
}

fn default_time_step() -> u8 {
    15
}

fn default_duration() -> u16 {
    60
}

impl Default for CalendarConfig {
    fn default() -> Self {
        let today = Local::now().date_naive();
        Self {
            events: Vec::new(),
            view_year: today.year(),
            view_month: today.month() as u8,
            selected_date: format_date(today),
            default_all_day: true,
            show_notes_preview: true,
            show_upcoming: true,
            time_step_minutes: 15,
            default_duration_minutes: 60,
        }
    }
}

impl CalendarConfig {
    /// Clamp time-step to supported values.
    #[must_use]
    pub fn clamp_time_step(step: u8) -> u8 {
        if step >= 30 {
            30
        } else {
            15
        }
    }

    /// Clamp default duration to supported values.
    #[must_use]
    pub fn clamp_duration(mins: u16) -> u16 {
        match mins {
            0..=44 => 30,
            45..=74 => 60,
            _ => 90,
        }
    }

    /// Clamp fields and repair invalid dates.
    pub fn normalize(&mut self) {
        if !(1..=12).contains(&self.view_month) {
            self.view_month = 1;
        }
        if self.view_year < 1 {
            self.view_year = 1;
        }
        self.time_step_minutes = Self::clamp_time_step(self.time_step_minutes);
        self.default_duration_minutes = Self::clamp_duration(self.default_duration_minutes);
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

/// Decode persisted bytes, accepting the pre-settings layout.
#[must_use]
pub fn decode_config(bytes: &[u8]) -> CalendarConfig {
    if let Ok((mut cfg, _)) =
        bincode::serde::decode_from_slice::<CalendarConfig, _>(bytes, bincode::config::standard())
    {
        cfg.normalize();
        return cfg;
    }

    #[derive(Deserialize)]
    struct Legacy {
        events: Vec<CalendarEvent>,
        view_year: i32,
        view_month: u8,
        selected_date: String,
    }

    if let Ok((legacy, _)) =
        bincode::serde::decode_from_slice::<Legacy, _>(bytes, bincode::config::standard())
    {
        let mut cfg = CalendarConfig {
            events: legacy.events,
            view_year: legacy.view_year,
            view_month: legacy.view_month,
            selected_date: legacy.selected_date,
            ..CalendarConfig::default()
        };
        cfg.normalize();
        return cfg;
    }

    CalendarConfig::default()
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

/// Parse a user-entered date (`YYYY-MM-DD` or `YYYYMMDD`).
#[must_use]
pub fn parse_date_input(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    if let Some(d) = parse_date(s) {
        return Some(d);
    }
    if s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit()) {
        let y: i32 = s[0..4].parse().ok()?;
        let m: u32 = s[4..6].parse().ok()?;
        let d: u32 = s[6..8].parse().ok()?;
        return NaiveDate::from_ymd_opt(y, m, d);
    }
    None
}

/// Format minutes-from-midnight as `HH:MM`.
#[must_use]
pub fn format_minutes(mins: u16) -> String {
    let h = mins / 60;
    let m = mins % 60;
    format!("{h:02}:{m:02}")
}
