//! Date/time formatting helpers for [`super::LocaleConfig`].

use std::fmt::Write;

use chrono::{DateTime, Local, TimeZone, Utc};

use super::LocaleConfig;

impl LocaleConfig {
    const DEFAULT_DATE: &'static str = "%b %d";
    const DEFAULT_TIME: &'static str = "%H:%M";
    const DEFAULT_DATETIME: &'static str = "%Y-%m-%d %H:%M";
    const DEFAULT_DATETIME_DATE: &'static str = "%Y-%m-%d";

    /// Format `dt` in local time using `date_format` and `time_format`, or
    /// [`Self::DEFAULT_DATETIME`] when unset or invalid.
    #[must_use]
    pub fn format_datetime(&self, dt: DateTime<Utc>) -> String {
        let local = dt.with_timezone(&Local);
        let date_fmt = self
            .date_format
            .as_deref()
            .unwrap_or(Self::DEFAULT_DATETIME_DATE);
        let time_fmt = self
            .time_format
            .as_deref()
            .unwrap_or(Self::DEFAULT_TIME);

        if self.date_format.is_some() || self.time_format.is_some() {
            if let Some(s) = try_format_pair(&local, date_fmt, time_fmt) {
                return s;
            }
        }
        format_or_default(&local, Self::DEFAULT_DATETIME)
    }

    /// Format the time portion of `dt` in local time.
    #[must_use]
    pub fn format_time(&self, dt: DateTime<Utc>) -> String {
        let local = dt.with_timezone(&Local);
        if let Some(fmt) = self.time_format.as_deref() {
            if let Some(s) = try_format(&local, fmt) {
                return s;
            }
        }
        format_or_default(&local, Self::DEFAULT_TIME)
    }

    /// Format the date portion of `dt` in local time.
    #[must_use]
    pub fn format_date(&self, dt: DateTime<Utc>) -> String {
        let local = dt.with_timezone(&Local);
        if let Some(fmt) = self.date_format.as_deref() {
            if let Some(s) = try_format(&local, fmt) {
                return s;
            }
        }
        format_or_default(&local, Self::DEFAULT_DATE)
    }
}

fn try_format<Tz: TimeZone>(dt: &DateTime<Tz>, fmt: &str) -> Option<String>
where
    Tz::Offset: std::fmt::Display,
{
    if fmt.is_empty() {
        return None;
    }
    let mut buf = String::new();
    write!(buf, "{}", dt.format(fmt)).ok().map(|()| buf)
}

fn try_format_pair<Tz: TimeZone>(dt: &DateTime<Tz>, date_fmt: &str, time_fmt: &str) -> Option<String>
where
    Tz::Offset: std::fmt::Display,
{
    let date = try_format(dt, date_fmt)?;
    let time = try_format(dt, time_fmt)?;
    Some(format!("{date} {time}"))
}

fn format_or_default<Tz: TimeZone>(dt: &DateTime<Tz>, default_fmt: &str) -> String
where
    Tz::Offset: std::fmt::Display,
{
    try_format(dt, default_fmt).unwrap_or_else(|| {
        let mut buf = String::new();
        let _ = write!(buf, "{}", dt.to_rfc3339());
        buf
    })
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn utc(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, min, 0).unwrap()
    }

    #[test]
    fn defaults_when_formats_unset() {
        let locale = LocaleConfig::default();
        let dt = utc(2026, 7, 8, 14, 30);
        let local = dt.with_timezone(&Local);
        assert_eq!(locale.format_time(dt), local.format("%H:%M").to_string());
        assert_eq!(locale.format_date(dt), local.format("%b %d").to_string());
        assert_eq!(
            locale.format_datetime(dt),
            local.format("%Y-%m-%d %H:%M").to_string()
        );
    }

    #[test]
    fn custom_formats_are_used() {
        let locale = LocaleConfig {
            date_format: Some("%d.%m.%Y".into()),
            time_format: Some("%I:%M %p".into()),
            ..LocaleConfig::default()
        };
        let dt = utc(2026, 7, 8, 14, 30);
        let local = dt.with_timezone(&Local);
        assert_eq!(locale.format_date(dt), local.format("%d.%m.%Y").to_string());
        assert_eq!(locale.format_time(dt), local.format("%I:%M %p").to_string());
        assert_eq!(
            locale.format_datetime(dt),
            format!(
                "{} {}",
                local.format("%d.%m.%Y"),
                local.format("%I:%M %p")
            )
        );
    }

    #[test]
    fn invalid_format_falls_back_to_defaults() {
        let locale = LocaleConfig {
            date_format: Some("%".into()),
            time_format: Some("%".into()),
            ..LocaleConfig::default()
        };
        let dt = utc(2026, 7, 8, 14, 30);
        let local = dt.with_timezone(&Local);
        assert_eq!(locale.format_date(dt), local.format("%b %d").to_string());
        assert_eq!(locale.format_time(dt), local.format("%H:%M").to_string());
        assert_eq!(
            locale.format_datetime(dt),
            local.format("%Y-%m-%d %H:%M").to_string()
        );
    }

    #[test]
    fn partial_custom_datetime_uses_default_for_missing_part() {
        let locale = LocaleConfig {
            date_format: Some("%Y/%m/%d".into()),
            time_format: None,
            ..LocaleConfig::default()
        };
        let dt = utc(2026, 7, 8, 14, 30);
        let local = dt.with_timezone(&Local);
        assert_eq!(
            locale.format_datetime(dt),
            format!("{} {}", local.format("%Y/%m/%d"), local.format("%H:%M"))
        );
    }
}
