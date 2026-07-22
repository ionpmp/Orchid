//! Day-quality (auspiciousness) scoring built on the panchanga.
//!
//! This is a heuristic "traffic light" score, not a substitute for a full
//! muhurta consultation: it combines classical panchanga rules (tithi
//! quality, yoga, karana) with, when natal data is available, taras
//! (nakshatra counting) and chandrashtama-style moon-house checks.

use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, TimeZone, Utc};

use super::astro::{ayanamsa_deg, julian_day, karana_name_index, moon_longitude, sun_longitude};
use super::config::AyanamsaSystem;

const NAK_SPAN: f64 = 360.0 / 27.0;

fn norm360(x: f64) -> f64 {
    x.rem_euclid(360.0)
}

/// Traffic-light classification of a [`DayScore`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayColor {
    /// Favorable (score >= 65).
    Green,
    /// Mixed (40..=64).
    Yellow,
    /// Unfavorable (score < 40).
    Red,
}

/// A single scoring input and its signed contribution to the total.
///
/// The payload carries enough detail (nakshatra tara index, chandra house,
/// tithi class, yoga/karana index) for [`super::narrative`] to explain the
/// score without recomputing the panchanga.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Factor {
    /// Tara (nakshatra-counting) category, 0..=8, from the janma nakshatra.
    Tara(u8),
    /// 1-based house of the day's moon from the natal moon, 1..=12.
    Chandra(u8),
    /// Tithi class 0..=4 (Nanda, Bhadra, Jaya, Rikta, Purna).
    TithiClass(u8),
    /// Tithi is Amavasya (new moon, index 30).
    Amavasya,
    /// Tithi is Purnima (full moon, index 15).
    Purnima,
    /// Inauspicious yoga index (5, 8, 9, 12, 14, 16, 18 or 26).
    BadYoga(u8),
    /// Karana is Vishti (Bhadra).
    VishtiKarana,
    /// Fixed karana (Shakuni, Chatushpada or Naga), index 7..=9.
    FixedKarana(u8),
}

/// A computed day score with its contributing factors.
#[derive(Debug, Clone, PartialEq)]
pub struct DayScore {
    /// Clamped 0..=100 score.
    pub score: u8,
    /// Traffic-light classification of `score`.
    pub color: DayColor,
    /// Ordered (insertion order) list of factors and their signed weights.
    pub factors: Vec<(Factor, i8)>,
}

/// Natal (birth-chart) data required for tara/chandra scoring.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NatalInfo {
    /// Birth (janma) nakshatra index, 0..=26.
    pub janma_nakshatra: u8,
    /// Birth moon rashi index, 0..=11.
    pub moon_rashi: u8,
    /// Sidereal moon longitude at birth, in degrees.
    pub moon_longitude: f64,
    /// Birth year (UTC), used for age-dependent narrative in future work.
    pub birth_year: i32,
}

/// Tara (nakshatra-counting) index of `nakshatra_index` from `janma_nakshatra`.
pub(crate) fn tara_index(nakshatra_index: u8, janma_nakshatra: u8) -> u8 {
    let n = u32::from(nakshatra_index);
    let j = u32::from(janma_nakshatra);
    (((n + 27) - j) % 27 % 9) as u8
}

/// Signed weight of a tara category (0..=8).
pub(crate) fn tara_weight(tara: u8) -> i8 {
    match tara {
        1 | 3 | 5 | 7 | 8 => 12,
        0 => -4,
        2 | 4 | 6 => -18,
        _ => 0,
    }
}

/// 1-based house of `day_moon_rashi` counted from `natal_moon_rashi`.
pub(crate) fn chandra_house(day_moon_rashi: u8, natal_moon_rashi: u8) -> u8 {
    let d = u32::from(day_moon_rashi);
    let n = u32::from(natal_moon_rashi);
    ((((d + 12) - n) % 12) + 1) as u8
}

/// Signed weight of a chandra house (1..=12); neutral houses score 0.
pub(crate) fn chandra_weight(house: u8) -> i8 {
    match house {
        1 | 3 | 6 | 7 | 10 | 11 => 10,
        4 | 8 | 12 => -14,
        _ => 0,
    }
}

/// Tithi class 0..=4 for a 1-based tithi index (1..=30).
pub(crate) fn tithi_class(tithi_index: u8) -> u8 {
    let t = ((tithi_index.saturating_sub(1)) % 15) + 1;
    match t {
        1 | 6 | 11 => 0,
        2 | 7 | 12 => 1,
        3 | 8 | 13 => 2,
        4 | 9 | 14 => 3,
        _ => 4,
    }
}

/// Signed weight of a tithi class (0..=4).
pub(crate) fn tithi_class_weight(class: u8) -> i8 {
    match class {
        3 => -8,
        4 => 6,
        _ => 2,
    }
}

/// Signed weight for an inauspicious yoga index, if any.
pub(crate) fn yoga_weight(yoga_index: u8) -> Option<i8> {
    match yoga_index {
        16 | 26 => Some(-14),
        5 | 8 | 9 | 12 | 14 | 18 => Some(-10),
        _ => None,
    }
}

/// Classify a clamped 0..=100 score into a [`DayColor`].
pub(crate) fn classify_color(score: u8) -> DayColor {
    if score >= 65 {
        DayColor::Green
    } else if score >= 40 {
        DayColor::Yellow
    } else {
        DayColor::Red
    }
}

/// Sum `base` with every factor weight and clamp to 0..=100.
pub(crate) fn total(base: i32, factors: &[(Factor, i8)]) -> u8 {
    let sum: i32 = factors.iter().map(|(_, w)| i32::from(*w)).sum();
    (base + sum).clamp(0, 100) as u8
}

/// Compute the day-quality score for `at`.
///
/// When `natal` is `None` only panchanga-only factors (tithi, yoga, karana)
/// apply and the base score is higher (62 vs. 55), since tara/chandra checks
/// are unavailable without a birth chart.
#[must_use]
pub fn compute_day_score(
    at: DateTime<Utc>,
    ayanamsa: AyanamsaSystem,
    natal: Option<&NatalInfo>,
) -> DayScore {
    let jd = julian_day(at);
    let t = (jd - 2_451_545.0) / 36_525.0;
    let aya = ayanamsa_deg(jd, ayanamsa);

    let sun_sid = norm360(sun_longitude(t) - aya);
    let moon_sid = norm360(moon_longitude(t) - aya);

    let nak_index = (moon_sid / NAK_SPAN).floor() as u8 % 27;
    let day_moon_rashi = (moon_sid / 30.0).floor() as u8 % 12;

    let elong = norm360(moon_sid - sun_sid);
    let tithi_index = (elong / 12.0).floor() as u8 + 1;
    let yoga_index = (norm360(sun_sid + moon_sid) / NAK_SPAN).floor() as u8 % 27;
    let karana_num = (elong / 6.0).floor() as i32 % 60;
    let karana_index = karana_name_index(karana_num);

    let base: i32 = if natal.is_some() { 55 } else { 62 };
    let mut factors: Vec<(Factor, i8)> = Vec::new();

    if let Some(natal) = natal {
        let tara = tara_index(nak_index, natal.janma_nakshatra);
        factors.push((Factor::Tara(tara), tara_weight(tara)));

        let house = chandra_house(day_moon_rashi, natal.moon_rashi);
        let weight = chandra_weight(house);
        if weight != 0 {
            factors.push((Factor::Chandra(house), weight));
        }
    }

    let class = tithi_class(tithi_index);
    factors.push((Factor::TithiClass(class), tithi_class_weight(class)));

    if tithi_index == 30 {
        factors.push((Factor::Amavasya, -6));
    } else if tithi_index == 15 {
        factors.push((Factor::Purnima, 3));
    }

    if let Some(weight) = yoga_weight(yoga_index) {
        factors.push((Factor::BadYoga(yoga_index), weight));
    }

    if karana_index == 6 {
        factors.push((Factor::VishtiKarana, -8));
    } else if matches!(karana_index, 7..=9) {
        factors.push((Factor::FixedKarana(karana_index), -4));
    }

    let score = total(base, &factors);
    let color = classify_color(score);
    DayScore {
        score,
        color,
        factors,
    }
}

/// Derive [`NatalInfo`] from a birth instant (UTC).
#[must_use]
pub fn compute_natal(birth_utc: DateTime<Utc>, ayanamsa: AyanamsaSystem) -> NatalInfo {
    let jd = julian_day(birth_utc);
    let t = (jd - 2_451_545.0) / 36_525.0;
    let aya = ayanamsa_deg(jd, ayanamsa);
    let moon_sid = norm360(moon_longitude(t) - aya);

    NatalInfo {
        janma_nakshatra: (moon_sid / NAK_SPAN).floor() as u8 % 27,
        moon_rashi: (moon_sid / 30.0).floor() as u8 % 12,
        moon_longitude: moon_sid,
        birth_year: birth_utc.year(),
    }
}

/// UTC instant for local solar noon on `date` at `longitude` (degrees east).
///
/// Approximates the local-to-UTC offset as `longitude / 15°` hours (i.e.
/// ignores the equation of time), which is adequate for panchanga-scale
/// scoring.
#[must_use]
pub fn local_noon_utc(date: NaiveDate, longitude: f64) -> DateTime<Utc> {
    let offset_seconds = (longitude / 15.0 * 3600.0).round() as i64;
    let noon = NaiveTime::from_hms_opt(12, 0, 0).unwrap_or_default();
    let naive = date.and_time(noon) - chrono::Duration::seconds(offset_seconds);
    Utc.from_utc_datetime(&naive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    #[test]
    fn tara_vipat_and_kin_are_strongly_negative() {
        // Vipat (2), Pratyak (4), Naidhana (6) are the classically
        // inauspicious taras.
        for bad in [2, 4, 6] {
            assert_eq!(tara_weight(bad), -18);
        }
        // nakshatra 5 counted from janma 3: (5+27-3)%27%9 = 2 (Vipat).
        assert_eq!(tara_index(5, 3), 2);
    }

    #[test]
    fn tara_good_categories_are_positive() {
        for good in [1, 3, 5, 7, 8] {
            assert_eq!(tara_weight(good), 12);
        }
        assert_eq!(tara_weight(0), -4);
    }

    #[test]
    fn chandra_house_eight_is_penalized() {
        // day rashi 7 from natal rashi 0 -> house 8 (chandrashtama-like).
        let house = chandra_house(7, 0);
        assert_eq!(house, 8);
        assert_eq!(chandra_weight(house), -14);
    }

    #[test]
    fn chandra_kendra_trikona_houses_are_favorable() {
        for house in [1u8, 3, 6, 7, 10, 11] {
            assert_eq!(chandra_weight(house), 10, "house {house}");
        }
    }

    #[test]
    fn score_thresholds_map_to_expected_colors() {
        assert_eq!(classify_color(65), DayColor::Green);
        assert_eq!(classify_color(100), DayColor::Green);
        assert_eq!(classify_color(64), DayColor::Yellow);
        assert_eq!(classify_color(40), DayColor::Yellow);
        assert_eq!(classify_color(39), DayColor::Red);
        assert_eq!(classify_color(0), DayColor::Red);
    }

    #[test]
    fn total_clamps_to_valid_range() {
        assert_eq!(total(62, &[(Factor::VishtiKarana, -8)]), 54);
        assert_eq!(
            total(10, &[(Factor::Tara(2), -18), (Factor::Amavasya, -6)]),
            0
        );
        assert_eq!(total(95, &[(Factor::Purnima, 3)]), 98);
        assert_eq!(total(99, &[(Factor::TithiClass(4), 6)]), 100);
    }

    #[test]
    fn compute_natal_is_within_valid_ranges() {
        let birth = Utc.with_ymd_and_hms(1990, 6, 15, 4, 30, 0).unwrap();
        let natal = compute_natal(birth, AyanamsaSystem::Lahiri);
        assert!(natal.janma_nakshatra < 27);
        assert!(natal.moon_rashi < 12);
        assert!((0.0..360.0).contains(&natal.moon_longitude));
        assert_eq!(natal.birth_year, 1990);
    }

    #[test]
    fn local_noon_utc_shifts_by_longitude() {
        let date = NaiveDate::from_ymd_opt(2026, 7, 21).unwrap();
        let at_greenwich = local_noon_utc(date, 0.0);
        assert_eq!(at_greenwich.hour(), 12);

        // 82.9739°E (Varanasi) ~ +5h32m -> noon local is ~06:28 UTC.
        let at_varanasi = local_noon_utc(date, 82.9739);
        assert!(at_varanasi < at_greenwich);
    }

    #[test]
    fn day_score_smoke_over_400_days() {
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        let natal = NatalInfo {
            janma_nakshatra: 4,
            moon_rashi: 2,
            moon_longitude: 63.0,
            birth_year: 1990,
        };
        for day in 0..400 {
            let at = start + chrono::Duration::days(day);

            let without_natal = compute_day_score(at, AyanamsaSystem::Lahiri, None);
            assert_eq!(without_natal.color, classify_color(without_natal.score));

            let with_natal = compute_day_score(at, AyanamsaSystem::Lahiri, Some(&natal));
            assert_eq!(with_natal.color, classify_color(with_natal.score));
            assert!(with_natal
                .factors
                .iter()
                .any(|(f, _)| matches!(f, Factor::Tara(_))));
        }
    }
}
