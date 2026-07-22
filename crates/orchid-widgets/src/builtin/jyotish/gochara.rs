//! Soft gochara (transit) modifier for month / year aggregates.
//!
//! Uses Guru, Shani, and Rahu positions relative to the natal Moon rashi.
//! This is intentionally a light heuristic — not a full gochara engine.

use chrono::{DateTime, Utc};

use super::astro::{ayanamsa_deg, julian_day, planet_longitude, true_node_longitude, Planet};
use super::config::AyanamsaSystem;

/// Soft year/month tint from −2 (challenging) to +2 (supportive).
#[must_use]
pub fn gochara_modifier(
    natal_moon_rashi: u8,
    at: DateTime<Utc>,
    ayanamsa: AyanamsaSystem,
) -> i8 {
    let jd = julian_day(at);
    let t = (jd - 2_451_545.0) / 36_525.0;
    let aya = ayanamsa_deg(jd, ayanamsa);

    let guru = house_from(
        natal_moon_rashi,
        sidereal_rashi(planet_longitude(t, Planet::Jupiter).0, aya),
    );
    let shani = house_from(
        natal_moon_rashi,
        sidereal_rashi(planet_longitude(t, Planet::Saturn).0, aya),
    );
    let rahu = house_from(
        natal_moon_rashi,
        sidereal_rashi(true_node_longitude(t), aya),
    );

    let mut score = 0i8;
    // Guru in kendra / trikona from Moon is supportive.
    if matches!(guru, 1 | 5 | 7 | 9 | 10) {
        score += 1;
    }
    // Shani in dusthana / lagna stress is challenging.
    if matches!(shani, 1 | 4 | 8 | 12) {
        score -= 1;
    }
    // Rahu in the 8th is a classic soft warning.
    if rahu == 8 {
        score -= 1;
    }
    // Extra Guru boost in 9th / 5th.
    if matches!(guru, 5 | 9) {
        score += 1;
    }
    score.clamp(-2, 2)
}

/// Fluent key for a gochara note, or empty when neutral / unused.
#[must_use]
pub fn gochara_note_key(modifier: i8) -> &'static str {
    match modifier {
        2 | 1 => "jyotish-gochara-favorable",
        -1 | -2 => "jyotish-gochara-challenging",
        _ => "",
    }
}

/// Nudge green / yellow / red day counts by a soft gochara tint.
#[must_use]
pub fn tint_counts(green: u16, yellow: u16, red: u16, modifier: i8) -> (u16, u16, u16) {
    let mut g = i32::from(green);
    let mut y = i32::from(yellow);
    let mut r = i32::from(red);
    match modifier {
        2 => {
            let n = ((y / 6).max(1)).min(y);
            y -= n;
            g += n;
        }
        1 => {
            let n = (y / 10).min(y);
            y -= n;
            g += n;
        }
        -1 => {
            let n = (g / 10).min(g);
            g -= n;
            y += n;
        }
        -2 => {
            let n = ((g / 6).max(1)).min(g);
            g -= n;
            y += n;
            let n2 = (y / 8).min(y);
            y -= n2;
            r += n2;
        }
        _ => {}
    }
    (
        u16::try_from(g.max(0)).unwrap_or(0),
        u16::try_from(y.max(0)).unwrap_or(0),
        u16::try_from(r.max(0)).unwrap_or(0),
    )
}

fn sidereal_rashi(trop_lon: f64, ayanamsa: f64) -> u8 {
    let sid = (trop_lon - ayanamsa).rem_euclid(360.0);
    (sid / 30.0).floor() as u8 % 12
}

/// Whole-sign house of `transit_rashi` counted from `natal_moon_rashi` (1..=12).
fn house_from(natal_moon_rashi: u8, transit_rashi: u8) -> u8 {
    (transit_rashi + 12 - (natal_moon_rashi % 12)) % 12 + 1
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn house_from_is_one_indexed() {
        assert_eq!(house_from(0, 0), 1);
        assert_eq!(house_from(0, 7), 8);
        assert_eq!(house_from(3, 3), 1);
        assert_eq!(house_from(3, 5), 3);
    }

    #[test]
    fn tint_counts_moves_mass_with_modifier() {
        let (g, y, r) = tint_counts(100, 50, 20, 2);
        assert!(g > 100);
        assert!(y < 50);
        assert_eq!(g + y + r, 170);

        let (g2, y2, r2) = tint_counts(100, 50, 20, -2);
        assert!(g2 < 100);
        assert!(r2 >= 20);
        assert_eq!(g2 + y2 + r2, 170);
    }

    #[test]
    fn gochara_modifier_is_clamped() {
        let at = Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap();
        let m = gochara_modifier(0, at, AyanamsaSystem::Lahiri);
        assert!((-2..=2).contains(&m));
    }
}
