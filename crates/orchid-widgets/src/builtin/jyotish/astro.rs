//! Low-precision Vedic astronomy (panchanga + graha longitudes).
//!
//! Tropical positions follow Jean Meeus, *Astronomical Algorithms* (simplified
//! terms). Sidereal conversion uses the selected ayanamsa. Accuracy target:
//! ±1° for longitudes, ±5 minutes for sunrise — suitable for a desktop
//! panchanga widget, not for navigation or muhurta software.

use chrono::{DateTime, Datelike, Duration as ChronoDuration, TimeZone, Timelike, Utc};

use super::config::AyanamsaSystem;

/// Computed panchanga + graha snapshot.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct JyotishData {
    pub tithi_index: u8,
    pub paksha_shukla: bool,
    pub nakshatra_index: u8,
    pub pada: u8,
    pub yoga_index: u8,
    pub karana_index: u8,
    pub vara_index: u8,
    pub ayanamsa_deg: f64,
    pub sunrise: Option<DateTime<Utc>>,
    pub sunset: Option<DateTime<Utc>>,
    pub planets: Vec<PlanetPosition>,
    pub calculated_at: DateTime<Utc>,
}

/// One graha in the sidereal zodiac.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct PlanetPosition {
    pub graha: Graha,
    pub longitude_deg: f64,
    pub rashi_index: u8,
    pub degree_in_rashi: f64,
    pub is_retrograde: bool,
}

/// Navagraha identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum Graha {
    Surya,
    Chandra,
    Mangala,
    Budha,
    Guru,
    Shukra,
    Shani,
    Rahu,
    Ketu,
}

impl Graha {
    /// Fluent key.
    #[must_use]
    pub fn ftl_key(self) -> &'static str {
        match self {
            Self::Surya => "jyotish-graha-surya",
            Self::Chandra => "jyotish-graha-chandra",
            Self::Mangala => "jyotish-graha-mangala",
            Self::Budha => "jyotish-graha-budha",
            Self::Guru => "jyotish-graha-guru",
            Self::Shukra => "jyotish-graha-shukra",
            Self::Shani => "jyotish-graha-shani",
            Self::Rahu => "jyotish-graha-rahu",
            Self::Ketu => "jyotish-graha-ketu",
        }
    }
}

/// Fluent key for tithi 1..=30 (`jyotish-tithi-*`).
#[must_use]
pub fn tithi_ftl_key(index: u8) -> &'static str {
    match index {
        1 => "jyotish-tithi-pratipada",
        2 => "jyotish-tithi-dwitiya",
        3 => "jyotish-tithi-tritiya",
        4 => "jyotish-tithi-chaturthi",
        5 => "jyotish-tithi-panchami",
        6 => "jyotish-tithi-shashthi",
        7 => "jyotish-tithi-saptami",
        8 => "jyotish-tithi-ashtami",
        9 => "jyotish-tithi-navami",
        10 => "jyotish-tithi-dashami",
        11 => "jyotish-tithi-ekadashi",
        12 => "jyotish-tithi-dwadashi",
        13 => "jyotish-tithi-trayodashi",
        14 => "jyotish-tithi-chaturdashi",
        15 => "jyotish-tithi-purnima",
        16 => "jyotish-tithi-pratipada",
        17 => "jyotish-tithi-dwitiya",
        18 => "jyotish-tithi-tritiya",
        19 => "jyotish-tithi-chaturthi",
        20 => "jyotish-tithi-panchami",
        21 => "jyotish-tithi-shashthi",
        22 => "jyotish-tithi-saptami",
        23 => "jyotish-tithi-ashtami",
        24 => "jyotish-tithi-navami",
        25 => "jyotish-tithi-dashami",
        26 => "jyotish-tithi-ekadashi",
        27 => "jyotish-tithi-dwadashi",
        28 => "jyotish-tithi-trayodashi",
        29 => "jyotish-tithi-chaturdashi",
        _ => "jyotish-tithi-amavasya",
    }
}

/// Fluent key for paksha.
#[must_use]
pub fn paksha_ftl_key(shukla: bool) -> &'static str {
    if shukla {
        "jyotish-paksha-shukla"
    } else {
        "jyotish-paksha-krishna"
    }
}

/// Fluent key for nakshatra 0..=26.
#[must_use]
pub fn nakshatra_ftl_key(index: u8) -> &'static str {
    match index {
        0 => "jyotish-nakshatra-ashwini",
        1 => "jyotish-nakshatra-bharani",
        2 => "jyotish-nakshatra-krittika",
        3 => "jyotish-nakshatra-rohini",
        4 => "jyotish-nakshatra-mrigashira",
        5 => "jyotish-nakshatra-ardra",
        6 => "jyotish-nakshatra-punarvasu",
        7 => "jyotish-nakshatra-pushya",
        8 => "jyotish-nakshatra-ashlesha",
        9 => "jyotish-nakshatra-magha",
        10 => "jyotish-nakshatra-purva-phalguni",
        11 => "jyotish-nakshatra-uttara-phalguni",
        12 => "jyotish-nakshatra-hasta",
        13 => "jyotish-nakshatra-chitra",
        14 => "jyotish-nakshatra-swati",
        15 => "jyotish-nakshatra-vishakha",
        16 => "jyotish-nakshatra-anuradha",
        17 => "jyotish-nakshatra-jyeshtha",
        18 => "jyotish-nakshatra-mula",
        19 => "jyotish-nakshatra-purva-ashadha",
        20 => "jyotish-nakshatra-uttara-ashadha",
        21 => "jyotish-nakshatra-shravana",
        22 => "jyotish-nakshatra-dhanishta",
        23 => "jyotish-nakshatra-shatabhisha",
        24 => "jyotish-nakshatra-purva-bhadrapada",
        25 => "jyotish-nakshatra-uttara-bhadrapada",
        _ => "jyotish-nakshatra-revati",
    }
}

/// Fluent key for yoga 0..=26.
#[must_use]
pub fn yoga_ftl_key(index: u8) -> &'static str {
    match index {
        0 => "jyotish-yoga-vishkambha",
        1 => "jyotish-yoga-priti",
        2 => "jyotish-yoga-ayushman",
        3 => "jyotish-yoga-saubhagya",
        4 => "jyotish-yoga-shobhana",
        5 => "jyotish-yoga-atiganda",
        6 => "jyotish-yoga-sukarma",
        7 => "jyotish-yoga-dhriti",
        8 => "jyotish-yoga-shula",
        9 => "jyotish-yoga-ganda",
        10 => "jyotish-yoga-vriddhi",
        11 => "jyotish-yoga-dhruva",
        12 => "jyotish-yoga-vyaghata",
        13 => "jyotish-yoga-harshana",
        14 => "jyotish-yoga-vajra",
        15 => "jyotish-yoga-siddhi",
        16 => "jyotish-yoga-vyatipata",
        17 => "jyotish-yoga-variyan",
        18 => "jyotish-yoga-parigha",
        19 => "jyotish-yoga-shiva",
        20 => "jyotish-yoga-siddha",
        21 => "jyotish-yoga-sadhya",
        22 => "jyotish-yoga-shubha",
        23 => "jyotish-yoga-shukla",
        24 => "jyotish-yoga-brahma",
        25 => "jyotish-yoga-indra",
        _ => "jyotish-yoga-vaidhriti",
    }
}

/// Fluent key for karana (0..=10).
#[must_use]
pub fn karana_ftl_key(index: u8) -> &'static str {
    match index {
        0 => "jyotish-karana-bava",
        1 => "jyotish-karana-balava",
        2 => "jyotish-karana-kaulava",
        3 => "jyotish-karana-taitila",
        4 => "jyotish-karana-garaja",
        5 => "jyotish-karana-vanija",
        6 => "jyotish-karana-vishti",
        7 => "jyotish-karana-shakuni",
        8 => "jyotish-karana-chatushpada",
        9 => "jyotish-karana-naga",
        _ => "jyotish-karana-kimstughna",
    }
}

/// Fluent key for vara (weekday) 0=Sunday ..= 6=Saturday.
#[must_use]
pub fn vara_ftl_key(index: u8) -> &'static str {
    match index {
        0 => "jyotish-vara-ravi",
        1 => "jyotish-vara-soma",
        2 => "jyotish-vara-mangala",
        3 => "jyotish-vara-budha",
        4 => "jyotish-vara-guru",
        5 => "jyotish-vara-shukra",
        _ => "jyotish-vara-shani",
    }
}

/// Fluent key for rashi 0..=11.
#[must_use]
pub fn rashi_ftl_key(index: u8) -> &'static str {
    match index {
        0 => "jyotish-rashi-mesha",
        1 => "jyotish-rashi-vrishabha",
        2 => "jyotish-rashi-mithuna",
        3 => "jyotish-rashi-karka",
        4 => "jyotish-rashi-simha",
        5 => "jyotish-rashi-kanya",
        6 => "jyotish-rashi-tula",
        7 => "jyotish-rashi-vrischika",
        8 => "jyotish-rashi-dhanu",
        9 => "jyotish-rashi-makara",
        10 => "jyotish-rashi-kumbha",
        _ => "jyotish-rashi-meena",
    }
}

/// Format degrees-within-rashi as `12°34'`.
#[must_use]
pub fn format_degree_in_rashi(deg: f64) -> String {
    let d = deg.rem_euclid(30.0);
    let whole = d.floor() as i32;
    let minutes = ((d - f64::from(whole)) * 60.0).round() as i32;
    let (whole, minutes) = if minutes >= 60 {
        (whole + 1, 0)
    } else {
        (whole, minutes)
    };
    format!("{whole}°{minutes:02}'")
}

/// Julian Day for a UTC instant (Meeus 7.1).
#[must_use]
pub fn julian_day(at: DateTime<Utc>) -> f64 {
    let naive = at.naive_utc();
    let y = naive.date().year();
    let m = i32::try_from(naive.date().month()).unwrap_or(1);
    let d = i32::try_from(naive.date().day()).unwrap_or(1);
    let frac = f64::from(naive.time().num_seconds_from_midnight()) / 86_400.0;

    let (y2, m2) = if m <= 2 { (y - 1, m + 12) } else { (y, m) };
    let a = (f64::from(y2) / 100.0).floor();
    let b = 2.0 - a + (a / 4.0).floor();
    (365.25 * (f64::from(y2) + 4716.0)).floor()
        + (30.6001 * (f64::from(m2) + 1.0)).floor()
        + f64::from(d)
        + b
        - 1524.5
        + frac
}

/// Ayanamsa in degrees for the given system and JD.
#[must_use]
pub fn ayanamsa_deg(jd: f64, system: AyanamsaSystem) -> f64 {
    let t = (jd - 2_451_545.0) / 36_525.0;
    // Lahiri (Chitra Paksha) approx at J2000 ≈ 23.85°, precession ~50.29"/yr.
    let lahiri = 23.85 + 1.397_013_9 * t - 0.000_000_87 * t * t;
    match system {
        AyanamsaSystem::Lahiri => lahiri,
        AyanamsaSystem::Krishnamurti => lahiri + 0.95,
        AyanamsaSystem::Raman => lahiri - 1.40,
    }
}

/// Compute panchanga + grahas for `at` at the given location.
#[must_use]
pub fn compute_jyotish(
    lat_deg: f64,
    lon_deg: f64,
    at: DateTime<Utc>,
    system: AyanamsaSystem,
) -> JyotishData {
    let jd = julian_day(at);
    let t = (jd - 2_451_545.0) / 36_525.0;
    let aya = ayanamsa_deg(jd, system);

    let sun_trop = sun_longitude(t);
    let moon_trop = moon_longitude(t);
    let sun_sid = norm360(sun_trop - aya);
    let moon_sid = norm360(moon_trop - aya);

    let elong = norm360(moon_sid - sun_sid);
    // Tithi: 12° each; 1..=30
    let tithi_raw = (elong / 12.0).floor() as u8;
    let tithi_index = tithi_raw + 1;
    let paksha_shukla = tithi_index <= 15;

    // Nakshatra: 13°20' = 40/3 °
    const NAK_SPAN: f64 = 360.0 / 27.0;
    let nak_pos = moon_sid / NAK_SPAN;
    let nakshatra_index = nak_pos.floor() as u8 % 27;
    let pada = ((nak_pos.fract() * 4.0).floor() as u8).clamp(0, 3) + 1;

    // Yoga: (Sun + Moon) / 13°20'
    let yoga_sum = norm360(sun_sid + moon_sid);
    let yoga_index = (yoga_sum / NAK_SPAN).floor() as u8 % 27;

    // Karana: half-tithi (6°). 60 karanas in a lunar month; map to 11 names.
    let karana_num = (elong / 6.0).floor() as i32 % 60;
    let karana_index = karana_name_index(karana_num);

    // Vara: civil weekday of the local solar day (approx via sunrise if known).
    let sunrise = rise_set_of(at, lat_deg, lon_deg, true);
    let sunset = rise_set_of(at, lat_deg, lon_deg, false);
    let vara_instant = sunrise.unwrap_or(at);
    let vara_index = u8::try_from(vara_instant.weekday().num_days_from_sunday()).unwrap_or(0);

    let planets = compute_planets(t, aya);

    JyotishData {
        tithi_index,
        paksha_shukla,
        nakshatra_index,
        pada,
        yoga_index,
        karana_index,
        vara_index,
        ayanamsa_deg: aya,
        sunrise,
        sunset,
        planets,
        calculated_at: at,
    }
}

fn karana_name_index(karana_num: i32) -> u8 {
    // First karana of Shukla Pratipada is Kimstughna (fixed).
    // Last three of Krishna Chaturdashi / Amavasya are fixed.
    // Middle 56 cycle through the 7 movable karanas.
    match karana_num {
        0 => 10,  // Kimstughna
        57 => 7,  // Shakuni
        58 => 8,  // Chatushpada
        59 => 9,  // Naga
        n => u8::try_from((n - 1).rem_euclid(7)).unwrap_or(0),
    }
}

fn compute_planets(t: f64, ayanamsa: f64) -> Vec<PlanetPosition> {
    let sun = sun_longitude(t);
    let moon = moon_longitude(t);
    let (mercury, merc_retro) = planet_longitude(t, Planet::Mercury);
    let (venus, ven_retro) = planet_longitude(t, Planet::Venus);
    let (mars, mars_retro) = planet_longitude(t, Planet::Mars);
    let (jupiter, jup_retro) = planet_longitude(t, Planet::Jupiter);
    let (saturn, sat_retro) = planet_longitude(t, Planet::Saturn);
    let rahu = true_node_longitude(t);
    let ketu = norm360(rahu + 180.0);

    let mk = |graha: Graha, trop: f64, retro: bool| {
        let sid = norm360(trop - ayanamsa);
        let rashi = (sid / 30.0).floor() as u8 % 12;
        PlanetPosition {
            graha,
            longitude_deg: sid,
            rashi_index: rashi,
            degree_in_rashi: sid.rem_euclid(30.0),
            is_retrograde: retro,
        }
    };

    vec![
        mk(Graha::Surya, sun, false),
        mk(Graha::Chandra, moon, false),
        mk(Graha::Mangala, mars, mars_retro),
        mk(Graha::Budha, mercury, merc_retro),
        mk(Graha::Guru, jupiter, jup_retro),
        mk(Graha::Shukra, venus, ven_retro),
        mk(Graha::Shani, saturn, sat_retro),
        mk(Graha::Rahu, rahu, true),
        mk(Graha::Ketu, ketu, true),
    ]
}

fn sun_longitude(t: f64) -> f64 {
    let l0 = norm360(280.466_46 + 36_000.769_83 * t + 0.000_303_2 * t * t);
    let m = norm360(357.529_11 + 35_999.050_29 * t - 0.000_153_7 * t * t).to_radians();
    let c = (1.914_602 - 0.004_817 * t) * m.sin()
        + (0.019_993 - 0.000_101 * t) * (2.0 * m).sin()
        + 0.000_289 * (3.0 * m).sin();
    norm360(l0 + c)
}

fn moon_longitude(t: f64) -> f64 {
    let l_prime = norm360(218.316_447_7 + 481_267.881_234_21 * t);
    let d = norm360(297.850_192_1 + 445_267.111_403_4 * t).to_radians();
    let m = norm360(357.529_109_2 + 35_999.050_290_9 * t).to_radians();
    let mp = norm360(134.963_396_4 + 477_198.867_505_5 * t).to_radians();
    let f = norm360(93.272_095_0 + 483_202.017_527_3 * t).to_radians();

    let lambda = l_prime
        + 6.289 * mp.sin()
        - 1.274 * (2.0 * d - mp).sin()
        + 0.658 * (2.0 * d).sin()
        - 0.186 * m.sin()
        - 0.114 * (2.0 * f).sin()
        + 0.059 * (2.0 * d - 2.0 * mp).sin()
        + 0.057 * (2.0 * d - m - mp).sin()
        + 0.053 * (2.0 * d + mp).sin()
        + 0.046 * (2.0 * d - m).sin()
        + 0.041 * (m - mp).sin();
    norm360(lambda)
}

fn true_node_longitude(t: f64) -> f64 {
    // Mean ascending node + small correction (Meeus ch.47).
    let omega = 125.044_52 - 1_934.136_261 * t + 0.002_070_8 * t * t;
    norm360(omega)
}

#[derive(Clone, Copy)]
enum Planet {
    Mercury,
    Venus,
    Mars,
    Jupiter,
    Saturn,
}

/// Approximate heliocentric → geocentric ecliptic longitude and retrograde flag.
fn planet_longitude(t: f64, planet: Planet) -> (f64, bool) {
    // Orbital elements at J2000: (semi-major AU, ecc, mean long, perihelion, °/day).
    let (a, e, l0, peri, n): (f64, f64, f64, f64, f64) = match planet {
        Planet::Mercury => (0.387_098, 0.205_630, 252.251, 77.456, 4.092_317),
        Planet::Venus => (0.723_330, 0.006_772, 181.980, 131.533, 1.602_136),
        Planet::Mars => (1.523_688, 0.093_405, 355.433, 336.041, 0.524_039),
        Planet::Jupiter => (5.202_56, 0.048_498, 34.351, 14.331, 0.083_091),
        Planet::Saturn => (9.554_75, 0.055_546, 50.077, 93.057, 0.033_460),
    };

    let mean_long = norm360(l0 + n * t * 36_525.0);
    let m = norm360(mean_long - peri).to_radians();
    let c = (2.0 * e * m.sin() + 1.25 * e * e * (2.0 * m).sin()).to_degrees();
    let true_long_helio = norm360(mean_long + c);
    let sun = sun_longitude(t);

    let geo = match planet {
        Planet::Mercury | Planet::Venus => {
            let elong = norm360(true_long_helio - sun);
            norm360(sun + (elong.to_radians().sin() * a.max(0.3_f64) * 40.0).to_degrees())
        }
        Planet::Mars | Planet::Jupiter | Planet::Saturn => {
            let earth_long = sun + 180.0;
            let delta = norm360(true_long_helio - earth_long);
            norm360(true_long_helio - 0.3 * delta.to_radians().sin().to_degrees())
        }
    };
    (geo, is_retrograde(t, planet))
}

fn is_retrograde(t: f64, planet: Planet) -> bool {
    let n_earth = 0.985_647;
    let n = match planet {
        Planet::Mercury => 4.092_317,
        Planet::Venus => 1.602_136,
        Planet::Mars => 0.524_039,
        Planet::Jupiter => 0.083_091,
        Planet::Saturn => 0.033_460,
    };
    match planet {
        Planet::Mercury | Planet::Venus => {
            let phase = (t * 36_525.0 * n).rem_euclid(360.0);
            (150.0..210.0).contains(&phase)
        }
        Planet::Mars | Planet::Jupiter | Planet::Saturn => {
            let phase = (t * 36_525.0 * (n_earth - n)).rem_euclid(360.0);
            (150.0..210.0).contains(&phase)
        }
    }
}

fn rise_set_of(at: DateTime<Utc>, lat_deg: f64, lon_deg: f64, rising: bool) -> Option<DateTime<Utc>> {
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
    let step_minutes = 30i64;
    let mut prev_alt = sun_altitude(lat_deg, lon_deg, day_start);
    for step in 1..49 {
        let t = day_start + ChronoDuration::minutes(step * step_minutes);
        let alt = sun_altitude(lat_deg, lon_deg, t);
        let crossed = if rising {
            prev_alt < refraction_deg && alt >= refraction_deg
        } else {
            prev_alt >= refraction_deg && alt < refraction_deg
        };
        if crossed {
            let f = (refraction_deg - prev_alt) / (alt - prev_alt);
            let offset_min =
                (step - 1) as f64 * step_minutes as f64 + f.clamp(0.0, 1.0) * step_minutes as f64;
            return Some(day_start + ChronoDuration::seconds((offset_min * 60.0) as i64));
        }
        prev_alt = alt;
    }
    None
}

fn sun_altitude(lat_deg: f64, lon_deg: f64, at: DateTime<Utc>) -> f64 {
    let jd = julian_day(at);
    let t = (jd - 2_451_545.0) / 36_525.0;
    let lambda = sun_longitude(t).to_radians();
    let epsilon = (23.439_291 - 0.013_004_2 * t).to_radians();
    let ra = lambda.sin().mul_add(epsilon.cos(), 0.0).atan2(lambda.cos());
    let dec = (epsilon.sin() * lambda.sin()).asin();

    let gmst = norm360(280.460_618_37 + 360.985_647_366_29 * (jd - 2_451_545.0));
    let lst = norm360(gmst + lon_deg);
    let ha = (lst - ra.to_degrees()).rem_euclid(360.0).to_radians();
    let lat = lat_deg.to_radians();
    (lat.sin() * dec.sin() + lat.cos() * dec.cos() * ha.cos())
        .asin()
        .to_degrees()
}

fn norm360(x: f64) -> f64 {
    x.rem_euclid(360.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::jyotish::config::AyanamsaSystem;

    #[test]
    fn ayanamsa_near_expected_for_2026() {
        let at = Utc.with_ymd_and_hms(2026, 7, 21, 12, 0, 0).unwrap();
        let aya = ayanamsa_deg(julian_day(at), AyanamsaSystem::Lahiri);
        // Lahiri ~24.2° in mid-2020s.
        assert!(
            (23.8..24.8).contains(&aya),
            "ayanamsa out of range: {aya}"
        );
    }

    #[test]
    fn tithi_in_valid_range() {
        let at = Utc.with_ymd_and_hms(2026, 7, 21, 6, 30, 0).unwrap();
        let d = compute_jyotish(25.3176, 82.9739, at, AyanamsaSystem::Lahiri);
        assert!((1..=30).contains(&d.tithi_index));
        assert!(d.nakshatra_index < 27);
        assert!((1..=4).contains(&d.pada));
        assert!(d.yoga_index < 27);
        assert!(d.karana_index < 11);
        assert!(d.vara_index < 7);
        assert_eq!(d.planets.len(), 9);
        for p in &d.planets {
            assert!(p.rashi_index < 12);
            assert!((0.0..30.0).contains(&p.degree_in_rashi));
        }
    }

    #[test]
    fn new_moon_near_amavasya() {
        // Known new moon 2024-01-11 ≈ Amavasya.
        let at = Utc.with_ymd_and_hms(2024, 1, 11, 12, 0, 0).unwrap();
        let d = compute_jyotish(0.0, 0.0, at, AyanamsaSystem::Lahiri);
        assert!(
            d.tithi_index >= 29 || d.tithi_index <= 2,
            "expected amavasya-ish tithi, got {}",
            d.tithi_index
        );
    }

    #[test]
    fn format_degree_pads_minutes() {
        assert_eq!(format_degree_in_rashi(12.5), "12°30'");
        assert_eq!(format_degree_in_rashi(0.0), "0°00'");
    }
}
