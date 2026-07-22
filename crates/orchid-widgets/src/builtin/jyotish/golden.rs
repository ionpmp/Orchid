//! Golden / regression fixtures for ayanamsa and panchanga limbs.
//!
//! Anchors mix hard astronomical facts (new/full moon neighbourhood) with
//! locked outputs of *this* Meeus-based engine so refactors cannot drift
//! silently. Absolute Swiss-Ephemeris parity is not claimed.

use chrono::{TimeZone, Utc};

use super::astro::{ayanamsa_deg, compute_jyotish, julian_day};
use super::config::AyanamsaSystem;

/// One panchanga fixture: UTC noon sample at (lat, lon).
struct Fixture {
    y: i32,
    m: u32,
    d: u32,
    lat: f64,
    lon: f64,
    /// Expected tithi 1..=30 (±1 allowed, with 30↔1 wrap).
    tithi: u8,
    /// Expected nakshatra index 0..=26 (±1 allowed, with wrap).
    nakshatra: u8,
}

fn fixtures() -> Vec<Fixture> {
    // Locked against `print_golden_actuals` (Lahiri, UTC 12:00).
    let v_lat = 25.3176;
    let v_lon = 82.9739;
    vec![
        Fixture {
            y: 2024,
            m: 1,
            d: 11,
            lat: v_lat,
            lon: v_lon,
            tithi: 30,
            nakshatra: 19,
        },
        Fixture {
            y: 2024,
            m: 3,
            d: 25,
            lat: v_lat,
            lon: v_lon,
            tithi: 16,
            nakshatra: 12,
        },
        Fixture {
            y: 2024,
            m: 6,
            d: 6,
            lat: v_lat,
            lon: v_lon,
            tithi: 1,
            nakshatra: 4,
        },
        Fixture {
            y: 2024,
            m: 8,
            d: 19,
            lat: v_lat,
            lon: v_lon,
            tithi: 15,
            nakshatra: 22,
        },
        Fixture {
            y: 2024,
            m: 10,
            d: 2,
            lat: v_lat,
            lon: v_lon,
            tithi: 30,
            nakshatra: 12,
        },
        Fixture {
            y: 2024,
            m: 12,
            d: 15,
            lat: v_lat,
            lon: v_lon,
            tithi: 16,
            nakshatra: 4,
        },
        Fixture {
            y: 2025,
            m: 1,
            d: 29,
            lat: v_lat,
            lon: v_lon,
            tithi: 30,
            nakshatra: 21,
        },
        Fixture {
            y: 2025,
            m: 3,
            d: 14,
            lat: v_lat,
            lon: v_lon,
            tithi: 16,
            nakshatra: 11,
        },
        Fixture {
            y: 2025,
            m: 5,
            d: 12,
            lat: v_lat,
            lon: v_lon,
            tithi: 15,
            nakshatra: 15,
        },
        Fixture {
            y: 2025,
            m: 7,
            d: 21,
            lat: v_lat,
            lon: v_lon,
            tithi: 27,
            nakshatra: 3,
        },
        Fixture {
            y: 2025,
            m: 9,
            d: 7,
            lat: v_lat,
            lon: v_lon,
            tithi: 15,
            nakshatra: 23,
        },
        Fixture {
            y: 2025,
            m: 11,
            d: 20,
            lat: v_lat,
            lon: v_lon,
            tithi: 1,
            nakshatra: 16,
        },
        Fixture {
            y: 2026,
            m: 1,
            d: 3,
            lat: v_lat,
            lon: v_lon,
            tithi: 16,
            nakshatra: 6,
        },
        Fixture {
            y: 2026,
            m: 2,
            d: 17,
            lat: v_lat,
            lon: v_lon,
            tithi: 30,
            nakshatra: 22,
        },
        Fixture {
            y: 2026,
            m: 4,
            d: 2,
            lat: v_lat,
            lon: v_lon,
            tithi: 16,
            nakshatra: 13,
        },
        Fixture {
            y: 2026,
            m: 5,
            d: 16,
            lat: v_lat,
            lon: v_lon,
            tithi: 30,
            nakshatra: 1,
        },
        Fixture {
            y: 2026,
            m: 7,
            d: 21,
            lat: v_lat,
            lon: v_lon,
            tithi: 8,
            nakshatra: 13,
        },
        Fixture {
            y: 2026,
            m: 3,
            d: 20,
            lat: 51.4769,
            lon: 0.0,
            tithi: 2,
            nakshatra: 26,
        },
        Fixture {
            y: 2026,
            m: 6,
            d: 21,
            lat: 51.4769,
            lon: 0.0,
            tithi: 7,
            nakshatra: 11,
        },
        Fixture {
            y: 2026,
            m: 9,
            d: 22,
            lat: 40.7128,
            lon: -74.006,
            tithi: 11,
            nakshatra: 21,
        },
        Fixture {
            y: 2026,
            m: 12,
            d: 21,
            lat: -33.8688,
            lon: 151.2093,
            tithi: 13,
            nakshatra: 2,
        },
    ]
}

fn nak_close(got: u8, expected: u8) -> bool {
    let d = (i16::from(got) - i16::from(expected)).abs();
    d <= 1 || d >= 26
}

fn tithi_close(got: u8, expected: u8) -> bool {
    let d = (i16::from(got) - i16::from(expected)).abs();
    d <= 1 || (got == 1 && expected == 30) || (got == 30 && expected == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ayanamsa_lahiri_regression_anchors() {
        let samples = [
            (2000, 1, 1, 23.85_f64),
            (2010, 1, 1, 24.04),
            (2020, 1, 1, 24.22),
            (2026, 7, 1, 24.35),
        ];
        for (y, m, d, expected) in samples {
            let at = Utc.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap();
            let aya = ayanamsa_deg(julian_day(at), AyanamsaSystem::Lahiri);
            assert!(
                (aya - expected).abs() < 0.15,
                "{y}-{m}-{d}: ayanamsa {aya} vs {expected}"
            );
        }
    }

    #[test]
    fn ayanamsa_system_offsets_stable() {
        let at = Utc.with_ymd_and_hms(2026, 7, 21, 12, 0, 0).unwrap();
        let jd = julian_day(at);
        let lahiri = ayanamsa_deg(jd, AyanamsaSystem::Lahiri);
        let kp = ayanamsa_deg(jd, AyanamsaSystem::Krishnamurti);
        let raman = ayanamsa_deg(jd, AyanamsaSystem::Raman);
        assert!((kp - lahiri - 0.95).abs() < 1e-9);
        assert!((raman - lahiri + 1.40).abs() < 1e-9);
    }

    #[test]
    fn golden_panchanga_fixtures_within_tolerance() {
        let all = fixtures();
        assert!(all.len() >= 20, "need ≥20 fixtures, got {}", all.len());
        let mut ok = 0usize;
        let mut failures = Vec::new();
        for f in &all {
            let at = Utc.with_ymd_and_hms(f.y, f.m, f.d, 12, 0, 0).unwrap();
            let d = compute_jyotish(f.lat, f.lon, at, AyanamsaSystem::Lahiri);
            let tithi_pass = tithi_close(d.tithi_index, f.tithi);
            let nak_pass = nak_close(d.nakshatra_index, f.nakshatra);
            if tithi_pass && nak_pass {
                ok += 1;
            } else {
                failures.push(format!(
                    "{:04}-{:02}-{:02}: tithi={} expect≈{} nak={} expect≈{}",
                    f.y, f.m, f.d, d.tithi_index, f.tithi, d.nakshatra_index, f.nakshatra
                ));
            }
        }
        let rate = ok as f64 / all.len() as f64;
        assert!(
            rate >= 0.95,
            "panchanga fixture pass rate {rate:.2} < 0.95; failures:\n{}",
            failures.join("\n")
        );
    }

    /// Run with `--ignored --nocapture` to refresh locked Exact values.
    #[test]
    #[ignore]
    fn print_golden_actuals() {
        for f in fixtures() {
            let at = Utc.with_ymd_and_hms(f.y, f.m, f.d, 12, 0, 0).unwrap();
            let d = compute_jyotish(f.lat, f.lon, at, AyanamsaSystem::Lahiri);
            println!(
                "{:04}-{:02}-{:02} tithi={} nak={} yoga={} karana={}",
                f.y, f.m, f.d, d.tithi_index, d.nakshatra_index, d.yoga_index, d.karana_index
            );
        }
    }
}
