//! Daylight inauspicious windows: Rahu Kalam, Yamagandam, Gulika.
//!
//! Classical rule: divide the interval sunrise→sunset into eight equal parts
//! and pick the weekday-specific segment for each window.

use chrono::{DateTime, Duration as ChronoDuration, Utc};

/// One named inauspicious window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MuhurtaWindow {
    /// Inclusive start (UTC).
    pub start: DateTime<Utc>,
    /// Exclusive-ish end (UTC); UI formats as a closed range.
    pub end: DateTime<Utc>,
}

/// Daylight muhurta windows for a civil day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayMuhurtas {
    /// Rahu Kalam.
    pub rahukalam: MuhurtaWindow,
    /// Yamagandam (Yama Ghantaka).
    pub yamagandam: MuhurtaWindow,
    /// Gulika Kalam.
    pub gulika: MuhurtaWindow,
}

/// 1-based eighth (1..=8) of the daylight span for each weekday.
/// Index: Sunday=0 … Saturday=6 (matches `Weekday::num_days_from_sunday`).
const RAHU_EIGHTH: [u8; 7] = [8, 2, 7, 5, 6, 4, 3];
const YAMA_EIGHTH: [u8; 7] = [5, 4, 3, 2, 1, 7, 6];
const GULIKA_EIGHTH: [u8; 7] = [7, 6, 5, 4, 3, 2, 1];

fn eighth_window(
    sunrise: DateTime<Utc>,
    sunset: DateTime<Utc>,
    eighth: u8,
) -> Option<MuhurtaWindow> {
    if sunset <= sunrise || !(1..=8).contains(&eighth) {
        return None;
    }
    let span = sunset - sunrise;
    let part = span / 8;
    if part <= ChronoDuration::zero() {
        return None;
    }
    let start = sunrise + part * i32::from(eighth - 1);
    let end = sunrise + part * i32::from(eighth);
    Some(MuhurtaWindow { start, end })
}

/// Compute Rahu / Yama / Gulika for the local day defined by `sunrise`/`sunset`.
///
/// `vara_index` is Sunday=0 … Saturday=6.
#[must_use]
pub fn day_muhurtas(
    sunrise: DateTime<Utc>,
    sunset: DateTime<Utc>,
    vara_index: u8,
) -> Option<DayMuhurtas> {
    let idx = usize::from(vara_index.min(6));
    Some(DayMuhurtas {
        rahukalam: eighth_window(sunrise, sunset, RAHU_EIGHTH[idx])?,
        yamagandam: eighth_window(sunrise, sunset, YAMA_EIGHTH[idx])?,
        gulika: eighth_window(sunrise, sunset, GULIKA_EIGHTH[idx])?,
    })
}

/// Whether `at` falls inside `window` (`start <= at < end`).
#[must_use]
pub fn in_window(at: DateTime<Utc>, window: MuhurtaWindow) -> bool {
    at >= window.start && at < window.end
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, TimeZone};

    #[test]
    fn saturday_rahu_is_third_eighth() {
        // Synthetic 8h day: each eighth = 1h. Saturday (6) Rahu = 3rd → 09:00–10:00.
        let rise = Utc.with_ymd_and_hms(2026, 7, 18, 6, 0, 0).unwrap();
        let set = Utc.with_ymd_and_hms(2026, 7, 18, 14, 0, 0).unwrap();
        assert_eq!(rise.weekday().num_days_from_sunday(), 6);
        let m = day_muhurtas(rise, set, 6).expect("windows");
        assert_eq!(
            m.rahukalam.start,
            Utc.with_ymd_and_hms(2026, 7, 18, 8, 0, 0).unwrap()
        );
        assert_eq!(
            m.rahukalam.end,
            Utc.with_ymd_and_hms(2026, 7, 18, 9, 0, 0).unwrap()
        );
    }

    #[test]
    fn sunday_gulika_is_seventh_eighth() {
        let rise = Utc.with_ymd_and_hms(2026, 7, 19, 6, 0, 0).unwrap();
        let set = Utc.with_ymd_and_hms(2026, 7, 19, 14, 0, 0).unwrap();
        assert_eq!(rise.weekday().num_days_from_sunday(), 0);
        let m = day_muhurtas(rise, set, 0).expect("windows");
        assert_eq!(
            m.gulika.start,
            Utc.with_ymd_and_hms(2026, 7, 19, 12, 0, 0).unwrap()
        );
        assert_eq!(
            m.gulika.end,
            Utc.with_ymd_and_hms(2026, 7, 19, 13, 0, 0).unwrap()
        );
    }

    #[test]
    fn in_window_half_open() {
        let w = MuhurtaWindow {
            start: Utc.with_ymd_and_hms(2026, 1, 1, 10, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2026, 1, 1, 11, 0, 0).unwrap(),
        };
        assert!(in_window(w.start, w));
        assert!(!in_window(w.end, w));
    }
}
