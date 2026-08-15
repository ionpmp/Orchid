//! Timeline / map / calendar browse helpers for the folder playlist.

use chrono::{Datelike, Local, NaiveDate, TimeZone};

use orchid_viewers::{CalDayItem, ImageThumbItem, MapPinItem};

/// Single-image view.
pub const BROWSE_PHOTO: u8 = 0;
/// Chronological list.
pub const BROWSE_TIMELINE: u8 = 1;
/// GPS scatter plot.
pub const BROWSE_MAP: u8 = 2;
/// Month grid.
pub const BROWSE_CALENDAR: u8 = 3;

/// Sort playlist thumbs newest-first by shoot date, then name.
#[must_use]
pub fn timeline_items(items: &[ImageThumbItem]) -> Vec<ImageThumbItem> {
    let mut out = items.to_vec();
    out.sort_by(|a, b| {
        b.taken_ms
            .cmp(&a.taken_ms)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

/// Normalize GPS thumbs onto a 0…1 canvas (north up).
#[must_use]
pub fn map_pins(items: &[ImageThumbItem]) -> Vec<MapPinItem> {
    let gps: Vec<&ImageThumbItem> = items.iter().filter(|t| t.has_gps).collect();
    if gps.is_empty() {
        return Vec::new();
    }
    let mut min_lat = f32::MAX;
    let mut max_lat = f32::MIN;
    let mut min_lon = f32::MAX;
    let mut max_lon = f32::MIN;
    for t in &gps {
        min_lat = min_lat.min(t.gps_lat);
        max_lat = max_lat.max(t.gps_lat);
        min_lon = min_lon.min(t.gps_lon);
        max_lon = max_lon.max(t.gps_lon);
    }
    let span_lat = (max_lat - min_lat).max(0.002);
    let span_lon = (max_lon - min_lon).max(0.002);
    gps.into_iter()
        .map(|t| MapPinItem {
            path: t.path.clone(),
            name: t.name.clone(),
            x: ((t.gps_lon - min_lon) / span_lon).clamp(0.04, 0.96),
            y: (1.0 - (t.gps_lat - min_lat) / span_lat).clamp(0.04, 0.96),
            selected: t.selected,
            rgba: t.rgba.clone(),
            width: t.width,
            height: t.height,
        })
        .collect()
}

/// Monday-first month cells for `year`/`month` (1–12).
#[must_use]
pub fn calendar_days(items: &[ImageThumbItem], year: i32, month: u32) -> (String, Vec<CalDayItem>) {
    let month = month.clamp(1, 12);
    let title = NaiveDate::from_ymd_opt(year, month, 1)
        .map(|d| d.format("%B %Y").to_string())
        .unwrap_or_else(|| format!("{year}-{month:02}"));
    let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else {
        return (title, Vec::new());
    };
    let start_pad = first.weekday().num_days_from_monday() as usize;
    let days_in = days_in_month(year, month);
    let mut cells = vec![CalDayItem::default(); start_pad];
    for day in 1..=days_in {
        let key = format!("{year:04}-{month:02}-{day:02}");
        let hits: Vec<&ImageThumbItem> = items.iter().filter(|t| t.date_text == key).collect();
        let first_hit = hits.first().copied();
        cells.push(CalDayItem {
            day: day as u8,
            count: hits.len() as u32,
            selected: hits.iter().any(|t| t.selected),
            path: first_hit.map(|t| t.path.clone()).unwrap_or_default(),
            rgba: first_hit.and_then(|t| t.rgba.clone()),
            width: first_hit.map(|t| t.width).unwrap_or(0),
            height: first_hit.map(|t| t.height).unwrap_or(0),
        });
    }
    while cells.len() % 7 != 0 {
        cells.push(CalDayItem::default());
    }
    (title, cells)
}

/// Year/month from a `YYYY-MM-DD` label, or today.
#[must_use]
pub fn month_from_date(date_text: &str) -> (i32, u32) {
    if let Some((y, m, _)) = parse_ymd(date_text) {
        return (y, m);
    }
    let now = Local::now();
    (now.year(), now.month())
}

/// Step `year`/`month` by `delta` months.
#[must_use]
pub fn shift_month(year: i32, month: u32, delta: i32) -> (i32, u32) {
    let mut n = i32::from(month as i16) + delta;
    let mut y = year;
    while n < 1 {
        n += 12;
        y -= 1;
    }
    while n > 12 {
        n -= 12;
        y += 1;
    }
    (y, n as u32)
}

/// Parse `YYYY-MM-DD` or EXIF `YYYY:MM:DD`.
#[must_use]
pub fn parse_ymd(raw: &str) -> Option<(i32, u32, u32)> {
    let t = raw.trim();
    if t.len() < 10 {
        return None;
    }
    let y: i32 = t.get(0..4)?.parse().ok()?;
    let sep = t.as_bytes().get(4).copied()?;
    if sep != b'-' && sep != b':' {
        return None;
    }
    let m: u32 = t.get(5..7)?.parse().ok()?;
    let d: u32 = t.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some((y, m, d))
}

/// Milliseconds since epoch for a `YYYY-MM-DD` (local midnight).
#[must_use]
pub fn taken_ms_from_date(raw: &str) -> i64 {
    let Some((y, m, d)) = parse_ymd(raw) else {
        return 0;
    };
    NaiveDate::from_ymd_opt(y, m, d)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .and_then(|ndt| Local.from_local_datetime(&ndt).single())
        .map(|t| t.timestamp_millis())
        .unwrap_or(0)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    let first = NaiveDate::from_ymd_opt(year, month, 1);
    match (first, next) {
        (Some(a), Some(b)) => (b - a).num_days() as u32,
        _ => 30,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thumb(name: &str, date: &str, gps: Option<(f32, f32)>) -> ImageThumbItem {
        ImageThumbItem {
            path: format!("local:/{name}"),
            name: name.into(),
            size_bytes: 1,
            date_text: date.into(),
            rating: 0,
            rgba: None,
            width: 0,
            height: 0,
            selected: name == "b.jpg",
            index: 1,
            taken_ms: taken_ms_from_date(date),
            has_gps: gps.is_some(),
            gps_lat: gps.map(|g| g.0).unwrap_or(0.0),
            gps_lon: gps.map(|g| g.1).unwrap_or(0.0),
        }
    }

    #[test]
    fn timeline_newest_first() {
        let items = [
            thumb("a.jpg", "2024-01-01", None),
            thumb("b.jpg", "2025-06-15", None),
            thumb("c.jpg", "2024-01-01", None),
        ];
        let sorted = timeline_items(&items);
        assert_eq!(sorted[0].name, "b.jpg");
        assert_eq!(sorted[1].name, "a.jpg");
    }

    #[test]
    fn map_pins_normalize() {
        let items = [
            thumb("a.jpg", "2024-01-01", Some((55.0, 37.0))),
            thumb("b.jpg", "2024-01-02", Some((56.0, 38.0))),
        ];
        let pins = map_pins(&items);
        assert_eq!(pins.len(), 2);
        assert!(pins[0].x < pins[1].x);
        assert!(pins[0].y > pins[1].y);
        assert!(pins.iter().any(|p| p.selected));
    }

    #[test]
    fn calendar_counts_day() {
        let items = [
            thumb("a.jpg", "2024-03-15", None),
            thumb("b.jpg", "2024-03-15", None),
            thumb("c.jpg", "2024-03-16", None),
        ];
        let (title, days) = calendar_days(&items, 2024, 3);
        assert!(title.contains("2024"));
        let fifteenth = days.iter().find(|d| d.day == 15).unwrap();
        assert_eq!(fifteenth.count, 2);
        assert!(fifteenth.selected);
        assert_eq!(days.len() % 7, 0);
    }

    #[test]
    fn parse_and_shift_month() {
        assert_eq!(parse_ymd("2024:03:15 12:00:00"), Some((2024, 3, 15)));
        assert_eq!(shift_month(2024, 1, -1), (2023, 12));
        assert_eq!(shift_month(2024, 12, 1), (2025, 1));
    }
}
