//! Vimshottari daśā (planetary period) calculation.
//!
//! The Vimshottari system divides a 120-year cycle among the nine grahas in
//! a fixed order, each ruling the nakshatra it is assigned to (three times
//! around the 27 nakshatras). A native's first (balance) mahā-daśā is a
//! fraction of its full length, proportional to how far the moon had
//! already travelled through the janma nakshatra at birth.

use chrono::{Duration as ChronoDuration, NaiveDate};

const NAK_SPAN: f64 = 360.0 / 27.0;
const DAYS_PER_YEAR: f64 = 365.25;

/// One of the nine Vimshottari daśā lords.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum DashaLord {
    Ketu,
    Venus,
    Sun,
    Moon,
    Mars,
    Rahu,
    Jupiter,
    Saturn,
    Mercury,
}

/// Vimshottari order, starting from Ketu (Ashwini's lord).
const ORDER: [DashaLord; 9] = [
    DashaLord::Ketu,
    DashaLord::Venus,
    DashaLord::Sun,
    DashaLord::Moon,
    DashaLord::Mars,
    DashaLord::Rahu,
    DashaLord::Jupiter,
    DashaLord::Saturn,
    DashaLord::Mercury,
];

impl DashaLord {
    /// Fluent key, reusing the existing `jyotish-graha-*` graha names.
    #[must_use]
    pub fn ftl_key(self) -> &'static str {
        match self {
            Self::Ketu => "jyotish-graha-ketu",
            Self::Venus => "jyotish-graha-shukra",
            Self::Sun => "jyotish-graha-surya",
            Self::Moon => "jyotish-graha-chandra",
            Self::Mars => "jyotish-graha-mangala",
            Self::Rahu => "jyotish-graha-rahu",
            Self::Jupiter => "jyotish-graha-guru",
            Self::Saturn => "jyotish-graha-shani",
            Self::Mercury => "jyotish-graha-budha",
        }
    }

    /// Full Vimshottari period length, in years.
    #[must_use]
    pub fn years(self) -> f64 {
        match self {
            Self::Ketu => 7.0,
            Self::Venus => 20.0,
            Self::Sun => 6.0,
            Self::Moon => 10.0,
            Self::Mars => 7.0,
            Self::Rahu => 18.0,
            Self::Jupiter => 16.0,
            Self::Saturn => 19.0,
            Self::Mercury => 17.0,
        }
    }

    fn order_index(self) -> usize {
        ORDER.iter().position(|l| *l == self).unwrap_or(0)
    }
}

fn lord_of_nakshatra(nakshatra_index: u8) -> DashaLord {
    ORDER[usize::from(nakshatra_index % 9)]
}

fn add_years(date: NaiveDate, years: f64) -> NaiveDate {
    date + ChronoDuration::days((years * DAYS_PER_YEAR).round() as i64)
}

/// A contiguous span ruled by a single daśā lord.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DashaPeriod {
    /// Ruling lord for this span.
    pub lord: DashaLord,
    /// Inclusive start date.
    pub from: NaiveDate,
    /// Exclusive end date.
    pub to: NaiveDate,
}

/// Mahā-daśā (main period) timeline from `birth` until at least `until`.
///
/// `natal_moon_lon` is the sidereal moon longitude (degrees) at birth. The
/// first entry's length is the balance of the janma nakshatra's daśā
/// remaining at birth; subsequent entries run the full Vimshottari order.
#[must_use]
pub fn maha_dashas(natal_moon_lon: f64, birth: NaiveDate, until: NaiveDate) -> Vec<DashaPeriod> {
    let lon = natal_moon_lon.rem_euclid(360.0);
    let nak_pos = lon / NAK_SPAN;
    let nak_index = nak_pos.floor() as u8 % 27;
    let fraction_elapsed = nak_pos.fract();

    let start_lord = lord_of_nakshatra(nak_index);
    let mut periods = Vec::new();

    let first_len = start_lord.years() * (1.0 - fraction_elapsed);
    let first_to = add_years(birth, first_len);
    periods.push(DashaPeriod {
        lord: start_lord,
        from: birth,
        to: first_to,
    });

    let mut cursor = first_to;
    let mut idx = start_lord.order_index();
    while cursor < until {
        idx = (idx + 1) % 9;
        let lord = ORDER[idx];
        let to = add_years(cursor, lord.years());
        periods.push(DashaPeriod {
            lord,
            from: cursor,
            to,
        });
        cursor = to;
    }

    periods
}

/// Antar-daśā (sub-period) breakdown of a single mahā-daśā, starting with
/// the mahā lord's own antar-daśā and proceeding through the Vimshottari
/// order.
#[must_use]
pub fn antar_dashas(maha: &DashaPeriod) -> Vec<DashaPeriod> {
    subdivide(maha, maha.lord.order_index())
}

/// Pratyantar-daśā (sub-sub-period) breakdown of a single antar-daśā,
/// starting with the antar lord's own pratyantar and cycling the
/// Vimshottari order.
#[must_use]
pub fn pratyantar_dashas(antar: &DashaPeriod) -> Vec<DashaPeriod> {
    subdivide(antar, antar.lord.order_index())
}

fn subdivide(parent: &DashaPeriod, start_idx: usize) -> Vec<DashaPeriod> {
    let total_days = ((parent.to - parent.from).num_days() as f64).max(0.0);

    let mut periods = Vec::with_capacity(9);
    let mut cursor = parent.from;
    for i in 0..9 {
        let lord = ORDER[(start_idx + i) % 9];
        let days = (total_days * (lord.years() / 120.0)).round() as i64;
        let to = cursor + ChronoDuration::days(days);
        periods.push(DashaPeriod {
            lord,
            from: cursor,
            to,
        });
        cursor = to;
    }

    if let Some(last) = periods.last_mut() {
        last.to = parent.to;
    }
    periods
}

/// Mahā / antar / pratyantar periods active on `date`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DashaStack {
    /// Outer (mahā) period.
    pub maha: DashaPeriod,
    /// Middle (antar) period.
    pub antar: DashaPeriod,
    /// Inner (pratyantar) period.
    pub pratyantar: DashaPeriod,
}

/// Full Vimshottari stack active on `date`.
#[must_use]
pub fn dasha_stack_at(
    natal_moon_lon: f64,
    birth: NaiveDate,
    date: NaiveDate,
) -> Option<DashaStack> {
    let mahas = maha_dashas(natal_moon_lon, birth, date + ChronoDuration::days(1));
    let maha = *mahas
        .iter()
        .find(|p| p.from <= date && date < p.to)
        .or_else(|| mahas.last())?;
    let antars = antar_dashas(&maha);
    let antar = *antars
        .iter()
        .find(|p| p.from <= date && date < p.to)
        .or_else(|| antars.last())?;
    let pratyantars = pratyantar_dashas(&antar);
    let pratyantar = *pratyantars
        .iter()
        .find(|p| p.from <= date && date < p.to)
        .or_else(|| pratyantars.last())?;
    Some(DashaStack {
        maha,
        antar,
        pratyantar,
    })
}

/// The (mahā, antar, pratyantar) lord triple active on `date`.
#[must_use]
pub fn dasha_at(
    natal_moon_lon: f64,
    birth: NaiveDate,
    date: NaiveDate,
) -> Option<(DashaLord, DashaLord, DashaLord)> {
    dasha_stack_at(natal_moon_lon, birth, date).map(|s| {
        (
            s.maha.lord,
            s.antar.lord,
            s.pratyantar.lord,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn years_between(from: NaiveDate, to: NaiveDate) -> f64 {
        ((to - from).num_days() as f64) / DAYS_PER_YEAR
    }

    #[test]
    fn full_cycle_sums_to_120_years() {
        let total: f64 = ORDER.iter().map(|l| l.years()).sum();
        assert!((total - 120.0).abs() < 1e-9, "total was {total}");
    }

    #[test]
    fn ashwini_full_maha_chain_spans_about_120_years() {
        let birth = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        // Start of Ashwini → full Ketu balance; request a span covering one cycle.
        let until = birth + ChronoDuration::days((120.0 * DAYS_PER_YEAR) as i64 + 2);
        let mahas = maha_dashas(0.0, birth, until);
        assert!(!mahas.is_empty());
        assert_eq!(mahas[0].lord, DashaLord::Ketu);
        let spanned = years_between(mahas[0].from, mahas.last().unwrap().to);
        assert!(
            (spanned - 120.0).abs() < 0.05,
            "expected ~120y maha chain, got {spanned}"
        );
        let lord_years: f64 = mahas.iter().map(|p| years_between(p.from, p.to)).sum();
        assert!(
            (lord_years - 120.0).abs() < 0.05,
            "sum of maha lengths was {lord_years}"
        );
    }

    #[test]
    fn ashwini_start_begins_full_ketu_dasha() {
        let birth = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        // Longitude 0.0 is the very start of Ashwini (fraction elapsed = 0).
        let mahas = maha_dashas(0.0, birth, birth + ChronoDuration::days(365 * 8));
        assert_eq!(mahas[0].lord, DashaLord::Ketu);
        assert!((years_between(mahas[0].from, mahas[0].to) - 7.0).abs() < 0.05);
    }

    #[test]
    fn mid_ashwini_leaves_half_the_ketu_dasha() {
        let birth = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        // Midpoint of Ashwini (13°20' nakshatra / 2).
        let lon = NAK_SPAN / 2.0;
        let mahas = maha_dashas(lon, birth, birth + ChronoDuration::days(365 * 8));
        assert_eq!(mahas[0].lord, DashaLord::Ketu);
        assert!(
            (years_between(mahas[0].from, mahas[0].to) - 3.5).abs() < 0.05,
            "expected ~3.5y remaining, got {}",
            years_between(mahas[0].from, mahas[0].to)
        );
    }

    #[test]
    fn maha_dashas_covers_the_requested_span() {
        let birth = NaiveDate::from_ymd_opt(1990, 3, 10).unwrap();
        let until = birth + ChronoDuration::days(365 * 50);
        let mahas = maha_dashas(200.0, birth, until);
        assert!(mahas.last().unwrap().to >= until);
        for pair in mahas.windows(2) {
            assert_eq!(pair[0].to, pair[1].from);
        }
    }

    #[test]
    fn antar_dasha_starts_with_the_maha_lord() {
        let birth = NaiveDate::from_ymd_opt(1990, 3, 10).unwrap();
        let mahas = maha_dashas(200.0, birth, birth + ChronoDuration::days(365 * 25));
        let maha = &mahas[1];
        let antars = antar_dashas(maha);
        assert_eq!(antars.len(), 9);
        assert_eq!(antars[0].lord, maha.lord);
        assert_eq!(antars[0].from, maha.from);
        assert_eq!(antars.last().unwrap().to, maha.to);
    }

    #[test]
    fn dasha_at_returns_consistent_lords() {
        let birth = NaiveDate::from_ymd_opt(1990, 3, 10).unwrap();
        let date = birth + ChronoDuration::days(3000);
        let (maha, antar, pratyantar) =
            dasha_at(200.0, birth, date).expect("dasha lookup should succeed");
        let stack = dasha_stack_at(200.0, birth, date).expect("stack");
        assert_eq!(maha, stack.maha.lord);
        assert_eq!(antar, stack.antar.lord);
        assert_eq!(pratyantar, stack.pratyantar.lord);
        assert!(stack.maha.from <= date && date < stack.maha.to);
        assert!(stack.antar.from <= date && date < stack.antar.to);
        assert!(stack.pratyantar.from <= date && date < stack.pratyantar.to);
    }

    #[test]
    fn pratyantar_starts_with_antar_lord() {
        let birth = NaiveDate::from_ymd_opt(1990, 3, 10).unwrap();
        let mahas = maha_dashas(200.0, birth, birth + ChronoDuration::days(365 * 25));
        let antar = &antar_dashas(&mahas[1])[0];
        let pratyantars = pratyantar_dashas(antar);
        assert_eq!(pratyantars.len(), 9);
        assert_eq!(pratyantars[0].lord, antar.lord);
        assert_eq!(pratyantars[0].from, antar.from);
        assert_eq!(pratyantars.last().unwrap().to, antar.to);
    }
}
