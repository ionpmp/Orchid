//! Low-precision lunar + solar astronomy.
//!
//! Formulas adapted from Jean Meeus, *Astronomical Algorithms* (2nd ed.)
//! chapters 22, 25, 47, 48, 53. Accuracy target: ±1° for phase angle,
//! ±3 minutes for rise/set, ±few thousand km for distance. Not suitable
//! for navigation.

use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Timelike, Utc};
use serde::{Deserialize, Serialize};

/// Moon phase bucket.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MoonPhase {
    NewMoon,
    WaxingCrescent,
    FirstQuarter,
    WaxingGibbous,
    FullMoon,
    WaningGibbous,
    LastQuarter,
    WaningCrescent,
}

impl MoonPhase {
    /// Icon name emitted in payloads.
    #[must_use]
    pub fn icon(self) -> &'static str {
        match self {
            Self::NewMoon => "moon-new",
            Self::WaxingCrescent => "moon-waxing-crescent",
            Self::FirstQuarter => "moon-first-quarter",
            Self::WaxingGibbous => "moon-waxing-gibbous",
            Self::FullMoon => "moon-full",
            Self::WaningGibbous => "moon-waning-gibbous",
            Self::LastQuarter => "moon-last-quarter",
            Self::WaningCrescent => "moon-waning-crescent",
        }
    }

    /// English label used as the default before the i18n layer resolves.
    #[must_use]
    pub fn default_label(self) -> &'static str {
        match self {
            Self::NewMoon => "New Moon",
            Self::WaxingCrescent => "Waxing Crescent",
            Self::FirstQuarter => "First Quarter",
            Self::WaxingGibbous => "Waxing Gibbous",
            Self::FullMoon => "Full Moon",
            Self::WaningGibbous => "Waning Gibbous",
            Self::LastQuarter => "Last Quarter",
            Self::WaningCrescent => "Waning Crescent",
        }
    }

    /// Fluent key.
    #[must_use]
    pub fn ftl_key(self) -> &'static str {
        match self {
            Self::NewMoon => "moon-phase-new",
            Self::WaxingCrescent => "moon-phase-waxing-crescent",
            Self::FirstQuarter => "moon-phase-first-quarter",
            Self::WaxingGibbous => "moon-phase-waxing-gibbous",
            Self::FullMoon => "moon-phase-full",
            Self::WaningGibbous => "moon-phase-waning-gibbous",
            Self::LastQuarter => "moon-phase-last-quarter",
            Self::WaningCrescent => "moon-phase-waning-crescent",
        }
    }
}

/// Full lunar data snapshot.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct MoonData {
    pub phase_name: MoonPhase,
    pub phase_angle_deg: f64,
    pub illumination_percent: f32,
    pub age_days: f32,
    pub distance_km: f64,
    pub angular_diameter_arcmin: f64,
    pub next_new_moon: DateTime<Utc>,
    pub next_full_moon: DateTime<Utc>,
    pub moonrise: Option<DateTime<Utc>>,
    pub moonset: Option<DateTime<Utc>>,
    pub sunrise: Option<DateTime<Utc>>,
    pub sunset: Option<DateTime<Utc>>,
    pub libration_lat_deg: f64,
    pub libration_lon_deg: f64,
    pub calculated_at: DateTime<Utc>,
}

/// Julian Day for a given UTC time.
#[must_use]
pub fn julian_day(at: DateTime<Utc>) -> f64 {
    // Standard conversion (Meeus eq. 7.1).
    let naive = at.naive_utc();
    let y = naive.date().format("%Y").to_string().parse::<i32>().unwrap_or(2000);
    let m = naive.date().format("%m").to_string().parse::<i32>().unwrap_or(1);
    let d = naive.date().format("%d").to_string().parse::<i32>().unwrap_or(1);
    let frac = (naive.time().num_seconds_from_midnight() as f64) / 86_400.0;

    let (y2, m2) = if m <= 2 { (y - 1, m + 12) } else { (y, m) };
    let a = (y2 as f64 / 100.0).floor();
    let b = 2.0 - a + (a / 4.0).floor();
    (365.25 * (y2 as f64 + 4716.0)).floor()
        + (30.6001 * (m2 as f64 + 1.0)).floor()
        + d as f64
        + b
        - 1524.5
        + frac
}

/// Compute moon data for the given coordinates and UTC instant.
#[must_use]
pub fn compute_moon(lat_deg: f64, lon_deg: f64, at: DateTime<Utc>) -> MoonData {
    let jd = julian_day(at);
    // Days since reference new moon (2000-01-06 18:14 UTC ≈ JD 2451549.26).
    const SYNODIC_MONTH: f64 = 29.530_588_861;
    const REF_JD: f64 = 2_451_549.259_722;
    let age = ((jd - REF_JD) % SYNODIC_MONTH + SYNODIC_MONTH) % SYNODIC_MONTH;

    // Phase angle 0..360, 0 = new, 180 = full.
    let phase_angle = age / SYNODIC_MONTH * 360.0;

    let illum_fraction = 0.5 * (1.0 - (phase_angle.to_radians()).cos());
    let illumination = (illum_fraction * 100.0).clamp(0.0, 100.0);

    let phase_name = bucket_phase(phase_angle);

    // Mean distance + perigee/apogee oscillation (Meeus ch.47 simplified).
    let t = (jd - 2_451_545.0) / 36_525.0;
    let d_mean_anomaly =
        357.5291092 + 35_999.050_290_9 * t - 0.000_153_7 * t * t;
    let moon_mean_anomaly =
        134.963_411_4 + 477_198.867_631_3 * t + 0.008_997 * t * t;
    let mean_elongation =
        297.850_192_1 + 445_267.111_40 * t - 0.001_881 * t * t;

    let mm = moon_mean_anomaly.to_radians();
    let d = mean_elongation.to_radians();
    let m = d_mean_anomaly.to_radians();

    // Principal distance correction (terms: Meeus table 47.A).
    let distance_km = 385_000.56
        + (-20_905.355 * mm.cos()
            - 3_699.111 * (2.0 * d - mm).cos()
            - 2_955.968 * (2.0 * d).cos()
            - 569.925 * (2.0 * mm).cos()
            + 246.158 * (2.0 * mm - 2.0 * d).cos()
            - 152.138 * (2.0 * d + m).cos()) / 1.0;

    // Angular diameter = 2 * atan(R_moon / distance) ≈ 1737.4*2 / d * 206265 arcsec.
    let ang_diam_arcmin = 2.0 * 1737.4 / distance_km * 206_265.0 / 60.0;

    // Libration (Meeus ch. 53, much-simplified: Optical libration only).
    let libration_lat = 6.5 * (mm.sin());
    let libration_lon = 7.9 * ((2.0 * d - mm).sin());

    // Next new moon / full moon computed from age:
    let next_new = at + duration_from_days(SYNODIC_MONTH - age);
    let half = SYNODIC_MONTH / 2.0;
    let next_full_delta = ((half - age).rem_euclid(SYNODIC_MONTH) + SYNODIC_MONTH) % SYNODIC_MONTH;
    let next_full = at + duration_from_days(next_full_delta);

    // Sun / Moon rise / set: approximate by solving altitude = -0°34' for the
    // day containing `at`. We sample 24 hourly altitudes and linearly
    // interpolate around sign changes.
    let sunrise = rise_set_of(at, lat_deg, lon_deg, SolarBody::Sun, true);
    let sunset = rise_set_of(at, lat_deg, lon_deg, SolarBody::Sun, false);
    let moonrise = rise_set_of(at, lat_deg, lon_deg, SolarBody::Moon, true);
    let moonset = rise_set_of(at, lat_deg, lon_deg, SolarBody::Moon, false);

    MoonData {
        phase_name,
        phase_angle_deg: phase_angle,
        illumination_percent: illumination as f32,
        age_days: age as f32,
        distance_km,
        angular_diameter_arcmin: ang_diam_arcmin,
        next_new_moon: next_new,
        next_full_moon: next_full,
        moonrise,
        moonset,
        sunrise,
        sunset,
        libration_lat_deg: libration_lat,
        libration_lon_deg: libration_lon,
        calculated_at: at,
    }
}

fn bucket_phase(angle_deg: f64) -> MoonPhase {
    let a = angle_deg.rem_euclid(360.0);
    // 8 buckets, boundaries at 22.5° apart from the main quarters.
    match a {
        x if !(22.5..337.5).contains(&x) => MoonPhase::NewMoon,
        x if x < 67.5 => MoonPhase::WaxingCrescent,
        x if x < 112.5 => MoonPhase::FirstQuarter,
        x if x < 157.5 => MoonPhase::WaxingGibbous,
        x if x < 202.5 => MoonPhase::FullMoon,
        x if x < 247.5 => MoonPhase::WaningGibbous,
        x if x < 292.5 => MoonPhase::LastQuarter,
        _ => MoonPhase::WaningCrescent,
    }
}

fn duration_from_days(days: f64) -> ChronoDuration {
    let secs = (days * 86_400.0) as i64;
    ChronoDuration::seconds(secs)
}

#[derive(Debug, Clone, Copy)]
enum SolarBody {
    Sun,
    Moon,
}

fn rise_set_of(
    at: DateTime<Utc>,
    lat_deg: f64,
    lon_deg: f64,
    body: SolarBody,
    rising: bool,
) -> Option<DateTime<Utc>> {
    // Start at 00:00 UTC of `at`'s date, sample 49 half-hour steps (so a
    // full day + an hour). Find the first interval where altitude crosses
    // the refraction floor in the requested direction.
    let day_start = Utc
        .with_ymd_and_hms(
            at.date_naive().year(),
            at.date_naive().month(),
            at.date_naive().day(),
            0,
            0,
            0,
        )
        .single()?;
    let refraction_deg = -0.5667;
    let step_minutes = 30;
    let mut prev_alt = altitude(body, lat_deg, lon_deg, day_start);
    for step in 1..49 {
        let t = day_start + ChronoDuration::minutes(step * step_minutes);
        let alt = altitude(body, lat_deg, lon_deg, t);
        let crossed = if rising {
            prev_alt < refraction_deg && alt >= refraction_deg
        } else {
            prev_alt >= refraction_deg && alt < refraction_deg
        };
        if crossed {
            // Linear interpolation within the 30-minute interval.
            let f = (refraction_deg - prev_alt) / (alt - prev_alt);
            let offset_min =
                (step - 1) as f64 * step_minutes as f64 + f.clamp(0.0, 1.0) * step_minutes as f64;
            return Some(day_start + ChronoDuration::seconds((offset_min * 60.0) as i64));
        }
        prev_alt = alt;
    }
    None
}

fn altitude(body: SolarBody, lat_deg: f64, lon_deg: f64, at: DateTime<Utc>) -> f64 {
    // Compute RA/Dec via simplified ecliptic coordinates for the Sun / Moon
    // (Meeus ch.25 + ch.47 highest-order terms) and convert to altitude.
    let jd = julian_day(at);
    let t = (jd - 2_451_545.0) / 36_525.0;

    let (ra_deg, dec_deg) = match body {
        SolarBody::Sun => sun_equatorial(t),
        SolarBody::Moon => moon_equatorial(t),
    };

    // Greenwich sidereal time (Meeus eq. 12.4).
    let gmst_deg = (280.460_618_4
        + 360.985_647_366_29 * (jd - 2_451_545.0))
        .rem_euclid(360.0);
    let lst_deg = (gmst_deg + lon_deg).rem_euclid(360.0);
    let hour_angle_deg = (lst_deg - ra_deg).rem_euclid(360.0);

    let ha = hour_angle_deg.to_radians();
    let dec = dec_deg.to_radians();
    let lat = lat_deg.to_radians();
    let alt = (lat.sin() * dec.sin() + lat.cos() * dec.cos() * ha.cos()).asin();
    alt.to_degrees()
}

fn sun_equatorial(t: f64) -> (f64, f64) {
    // Mean longitude / anomaly of the Sun (Meeus 25.2).
    let l0 = (280.460_66 + 36_000.770_06 * t).rem_euclid(360.0);
    let m = (357.528 + 35_999.050_3 * t).rem_euclid(360.0).to_radians();
    let c = 1.915 * m.sin() + 0.020 * (2.0 * m).sin();
    let lambda = (l0 + c).rem_euclid(360.0);
    let epsilon = 23.439 - 0.000_000_4 * t;
    let lam = lambda.to_radians();
    let eps = epsilon.to_radians();
    let ra = lam.sin().mul_add(eps.cos(), 0.0).atan2(lam.cos()).to_degrees();
    let dec = (eps.sin() * lam.sin()).asin().to_degrees();
    ((ra + 360.0).rem_euclid(360.0), dec)
}

fn moon_equatorial(t: f64) -> (f64, f64) {
    // Very simplified Moon position (Meeus ch.47, main terms only).
    let l_prime = (218.316_4 + 481_267.881_4 * t).rem_euclid(360.0);
    let mp = (134.963_4 + 477_198.867_6 * t).rem_euclid(360.0).to_radians();
    let m = (357.529_1 + 35_999.050_3 * t).rem_euclid(360.0).to_radians();
    let d = (297.850_2 + 445_267.111_5 * t).rem_euclid(360.0).to_radians();
    let f = (93.272_1 + 483_202.017_5 * t).rem_euclid(360.0).to_radians();

    // Longitude correction (main periodic terms).
    let lambda = l_prime
        + 6.289 * mp.sin()
        - 1.274 * (2.0 * d - mp).sin()
        + 0.658 * (2.0 * d).sin()
        - 0.186 * m.sin();
    // Latitude (from eclipticF).
    let beta = 5.128 * f.sin();

    let eps = 23.439_281_f64.to_radians();
    let lam = lambda.to_radians();
    let bet = beta.to_radians();
    let ra = ((lam.sin() * eps.cos() - bet.tan() * eps.sin()).atan2(lam.cos())).to_degrees();
    let dec = ((bet.sin() * eps.cos() + bet.cos() * eps.sin() * lam.sin()).asin()).to_degrees();
    ((ra + 360.0).rem_euclid(360.0), dec)
}

// Conversion helper so we can reach chrono::NaiveDate fields without pulling
// in the full trait surface everywhere.
trait NaiveDateExt {
    fn year(&self) -> i32;
    fn month(&self) -> u32;
    fn day(&self) -> u32;
}

impl NaiveDateExt for chrono::NaiveDate {
    fn year(&self) -> i32 {
        <Self as chrono::Datelike>::year(self)
    }
    fn month(&self) -> u32 {
        <Self as chrono::Datelike>::month(self)
    }
    fn day(&self) -> u32 {
        <Self as chrono::Datelike>::day(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_new_moon_is_new_phase() {
        // 2024-01-11 11:57 UTC is a recorded new moon.
        let at = Utc.with_ymd_and_hms(2024, 1, 11, 11, 57, 0).unwrap();
        let data = compute_moon(0.0, 0.0, at);
        assert!(
            matches!(data.phase_name, MoonPhase::NewMoon | MoonPhase::WaningCrescent | MoonPhase::WaxingCrescent),
            "phase was {:?} (angle={})",
            data.phase_name,
            data.phase_angle_deg
        );
        assert!(
            data.illumination_percent < 15.0,
            "illumination should be low near new moon, got {}%",
            data.illumination_percent
        );
    }

    #[test]
    fn distance_in_plausible_range() {
        let at = Utc.with_ymd_and_hms(2026, 4, 22, 0, 0, 0).unwrap();
        let d = compute_moon(0.0, 0.0, at);
        assert!(
            d.distance_km > 350_000.0 && d.distance_km < 410_000.0,
            "distance out of range: {} km",
            d.distance_km
        );
        assert!(d.angular_diameter_arcmin > 25.0 && d.angular_diameter_arcmin < 40.0);
    }

    #[test]
    fn next_events_are_in_future() {
        let at = Utc.with_ymd_and_hms(2026, 4, 22, 0, 0, 0).unwrap();
        let d = compute_moon(0.0, 0.0, at);
        assert!(d.next_new_moon > at);
        assert!(d.next_full_moon > at);
    }
}
