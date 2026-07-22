//! Exact-ish panchanga transition times via forward scan + bisection.
//!
//! The day-score still uses local noon, but the UI can show when the current
//! tithi / nakshatra / yoga / karana ends.

use chrono::{DateTime, Duration as ChronoDuration, Utc};

use super::astro::{ayanamsa_deg, julian_day, moon_longitude, sun_longitude};
use super::config::AyanamsaSystem;

const NAK_SPAN: f64 = 360.0 / 27.0;

fn norm360(x: f64) -> f64 {
    x.rem_euclid(360.0)
}

/// Which panchanga limb to track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limb {
    /// Tithi index 1..=30.
    Tithi,
    /// Nakshatra index 0..=26.
    Nakshatra,
    /// Yoga index 0..=26.
    Yoga,
    /// Raw half-tithi number 0..=59 (karana slot), not the 11-name remap.
    KaranaSlot,
}

fn limb_index(at: DateTime<Utc>, system: AyanamsaSystem, limb: Limb) -> u8 {
    let jd = julian_day(at);
    let t = (jd - 2_451_545.0) / 36_525.0;
    let aya = ayanamsa_deg(jd, system);
    let sun_sid = norm360(sun_longitude(t) - aya);
    let moon_sid = norm360(moon_longitude(t) - aya);
    let elong = norm360(moon_sid - sun_sid);
    match limb {
        Limb::Tithi => ((elong / 12.0).floor() as u8) + 1,
        Limb::Nakshatra => (moon_sid / NAK_SPAN).floor() as u8 % 27,
        Limb::Yoga => (norm360(sun_sid + moon_sid) / NAK_SPAN).floor() as u8 % 27,
        Limb::KaranaSlot => (elong / 6.0).floor() as u8 % 60,
    }
}

/// Instant (UTC) when `limb` next changes after `from`, searching up to `max_hours`.
#[must_use]
pub fn next_transition(
    from: DateTime<Utc>,
    system: AyanamsaSystem,
    limb: Limb,
    max_hours: i64,
) -> Option<DateTime<Utc>> {
    let start = limb_index(from, system, limb);
    let step = ChronoDuration::minutes(20);
    let limit = from + ChronoDuration::hours(max_hours);
    let mut lo = from;
    let mut hi = from + step;
    let mut found = false;
    while hi <= limit {
        if limb_index(hi, system, limb) != start {
            found = true;
            break;
        }
        lo = hi;
        hi += step;
    }
    if !found {
        return None;
    }

    // Bisect to ~30s precision.
    for _ in 0..32 {
        let mid = lo + (hi - lo) / 2;
        if mid <= lo || mid >= hi {
            break;
        }
        if limb_index(mid, system, limb) == start {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(hi)
}

/// Convenience: next ends for the four main limbs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanchangaEnds {
    /// UTC end of the current tithi.
    pub tithi: Option<DateTime<Utc>>,
    /// UTC end of the current nakshatra.
    pub nakshatra: Option<DateTime<Utc>>,
    /// UTC end of the current yoga.
    pub yoga: Option<DateTime<Utc>>,
    /// UTC end of the current karana slot.
    pub karana: Option<DateTime<Utc>>,
}

/// Compute end times for all four limbs from `from`.
#[must_use]
pub fn panchanga_ends(from: DateTime<Utc>, system: AyanamsaSystem) -> PanchangaEnds {
    PanchangaEnds {
        tithi: next_transition(from, system, Limb::Tithi, 48),
        nakshatra: next_transition(from, system, Limb::Nakshatra, 48),
        yoga: next_transition(from, system, Limb::Yoga, 48),
        karana: next_transition(from, system, Limb::KaranaSlot, 24),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn next_tithi_changes_index() {
        let from = Utc.with_ymd_and_hms(2026, 7, 21, 6, 0, 0).unwrap();
        let end = next_transition(from, AyanamsaSystem::Lahiri, Limb::Tithi, 48)
            .expect("tithi should end within 48h");
        let before = limb_index(from, AyanamsaSystem::Lahiri, Limb::Tithi);
        let after = limb_index(end, AyanamsaSystem::Lahiri, Limb::Tithi);
        assert_ne!(before, after);
        let just_before = end - ChronoDuration::seconds(45);
        assert_eq!(
            limb_index(just_before, AyanamsaSystem::Lahiri, Limb::Tithi),
            before
        );
    }

    #[test]
    fn panchanga_ends_all_present() {
        let from = Utc.with_ymd_and_hms(2026, 3, 15, 12, 0, 0).unwrap();
        let ends = panchanga_ends(from, AyanamsaSystem::Lahiri);
        assert!(ends.tithi.is_some());
        assert!(ends.nakshatra.is_some());
        assert!(ends.yoga.is_some());
        assert!(ends.karana.is_some());
    }
}
