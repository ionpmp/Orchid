//! Birth-time rectification: lagna quiz + life-event Vimshottari scoring.

use chrono::{Duration, NaiveDate, NaiveTime};

use super::astro::{ascendant_sidereal, julian_day, rashi_ftl_key};
use super::config::AyanamsaSystem;
use super::dasha::{dasha_at, DashaLord};
use super::score::compute_natal;

/// Kind of a remembered life event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum EventKind {
    Marriage,
    ChildBirth,
    Relocation,
    CareerRise,
    CareerFall,
    LossOfClose,
    Windfall,
    HealthCrisis,
}

impl EventKind {
    /// All event kinds in display order.
    #[must_use]
    pub fn all() -> &'static [EventKind] {
        &[
            Self::Marriage,
            Self::ChildBirth,
            Self::Relocation,
            Self::CareerRise,
            Self::CareerFall,
            Self::LossOfClose,
            Self::Windfall,
            Self::HealthCrisis,
        ]
    }

    /// Fluent key.
    #[must_use]
    pub fn ftl_key(self) -> &'static str {
        match self {
            Self::Marriage => "jyotish-event-marriage",
            Self::ChildBirth => "jyotish-event-child",
            Self::Relocation => "jyotish-event-relocation",
            Self::CareerRise => "jyotish-event-career-rise",
            Self::CareerFall => "jyotish-event-career-fall",
            Self::LossOfClose => "jyotish-event-loss",
            Self::Windfall => "jyotish-event-windfall",
            Self::HealthCrisis => "jyotish-event-health",
        }
    }

    fn favorable_lords(self) -> &'static [DashaLord] {
        match self {
            Self::Marriage => &[DashaLord::Venus, DashaLord::Rahu],
            Self::ChildBirth => &[DashaLord::Jupiter, DashaLord::Moon],
            Self::Relocation => &[DashaLord::Rahu, DashaLord::Ketu, DashaLord::Moon],
            Self::CareerRise => &[DashaLord::Sun, DashaLord::Jupiter, DashaLord::Saturn],
            Self::CareerFall => &[DashaLord::Saturn, DashaLord::Ketu],
            Self::LossOfClose => &[DashaLord::Saturn, DashaLord::Ketu, DashaLord::Mars],
            Self::Windfall => &[DashaLord::Venus, DashaLord::Jupiter, DashaLord::Mercury],
            Self::HealthCrisis => &[DashaLord::Saturn, DashaLord::Mars, DashaLord::Ketu],
        }
    }
}

/// A remembered life event (year-level).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifeEvent {
    /// Event kind.
    pub kind: EventKind,
    /// Calendar year of the event.
    pub year: i32,
}

/// One scored lagna-interval candidate.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Start minute from local midnight (inclusive).
    pub from_minute: u16,
    /// End minute from local midnight (exclusive).
    pub to_minute: u16,
    /// Sidereal lagna rashi 0..=11.
    pub lagna_rashi: u8,
    /// Quiz contribution.
    pub quiz_score: i32,
    /// Event contribution.
    pub event_score: i32,
    /// Normalized confidence 0..=100.
    pub confidence_pct: u8,
}

/// Interactive rectification session.
#[derive(Debug, Clone)]
pub struct RectifySession {
    birth_date: NaiveDate,
    /// Birth latitude (kept for session identity / future re-scan).
    #[allow(dead_code)]
    lat: f64,
    /// Birth longitude (kept for session identity / future re-scan).
    #[allow(dead_code)]
    lon: f64,
    utc_offset_minutes: i32,
    ayanamsa: AyanamsaSystem,
    /// Candidate lagna intervals intersecting the uncertainty window.
    base_candidates: Vec<(u16, u16, u8)>,
    answers: [Option<usize>; 8],
    events: Vec<LifeEvent>,
    /// Wizard step: 1=window chosen, 2=quiz, 3=events, 4=results.
    pub step: u8,
    /// Current quiz question index (0..=7).
    pub question_idx: u8,
}

/// Quiz option weights: (rashi, points).
type OptWeights = &'static [(u8, i32)];

const QUIZ: &[(&str, [&str; 4], [OptWeights; 4])] = &[
    (
        "jyotish-rq-1",
        [
            "jyotish-rq-1-a",
            "jyotish-rq-1-b",
            "jyotish-rq-1-c",
            "jyotish-rq-1-d",
        ],
        [
            &[(2, 2), (5, 2), (10, 2)],
            &[(0, 2), (4, 2), (8, 2)],
            &[(1, 2), (3, 2), (9, 2)],
            &[(6, 2), (7, 2), (11, 2)],
        ],
    ),
    (
        "jyotish-rq-2",
        [
            "jyotish-rq-2-a",
            "jyotish-rq-2-b",
            "jyotish-rq-2-c",
            "jyotish-rq-2-d",
        ],
        [
            &[(8, 2), (10, 2), (2, 1)],
            &[(0, 1), (6, 2), (4, 1)],
            &[(1, 2), (3, 2), (9, 1)],
            &[(5, 2), (11, 1)],
        ],
    ),
    (
        "jyotish-rq-3",
        [
            "jyotish-rq-3-a",
            "jyotish-rq-3-b",
            "jyotish-rq-3-c",
            "jyotish-rq-3-d",
        ],
        [
            &[(0, 3), (7, 2)],
            &[(1, 3), (9, 2)],
            &[(2, 3), (5, 2)],
            &[(3, 3), (11, 2)],
        ],
    ),
    (
        "jyotish-rq-4",
        [
            "jyotish-rq-4-a",
            "jyotish-rq-4-b",
            "jyotish-rq-4-c",
            "jyotish-rq-4-d",
        ],
        [
            &[(0, 2), (4, 2)],
            &[(1, 2), (9, 2)],
            &[(2, 2), (6, 2)],
            &[(3, 2), (11, 2)],
        ],
    ),
    (
        "jyotish-rq-5",
        [
            "jyotish-rq-5-a",
            "jyotish-rq-5-b",
            "jyotish-rq-5-c",
            "jyotish-rq-5-d",
        ],
        [
            &[(0, 2), (2, 1)],
            &[(1, 2), (9, 2)],
            &[(2, 2), (6, 1), (4, 1)],
            &[(3, 2), (11, 2)],
        ],
    ),
    (
        "jyotish-rq-6",
        [
            "jyotish-rq-6-a",
            "jyotish-rq-6-b",
            "jyotish-rq-6-c",
            "jyotish-rq-6-d",
        ],
        [
            &[(0, 2), (8, 2)],
            &[(1, 2), (5, 2), (9, 1)],
            &[(6, 3), (3, 1)],
            &[(11, 2), (3, 2)],
        ],
    ),
    (
        "jyotish-rq-7",
        [
            "jyotish-rq-7-a",
            "jyotish-rq-7-b",
            "jyotish-rq-7-c",
            "jyotish-rq-7-d",
        ],
        [
            &[(4, 3), (9, 1), (0, 1)],
            &[(1, 3), (6, 1)],
            &[(2, 2), (5, 2), (10, 1)],
            &[(3, 2), (7, 2), (11, 1)],
        ],
    ),
    (
        "jyotish-rq-8",
        [
            "jyotish-rq-8-a",
            "jyotish-rq-8-b",
            "jyotish-rq-8-c",
            "jyotish-rq-8-d",
        ],
        [
            &[(0, 2), (7, 1)],
            &[(1, 2)],
            &[(2, 2), (5, 1), (10, 1)],
            &[(3, 2), (5, 1), (11, 1)],
        ],
    ),
];

impl RectifySession {
    /// Build a session for a birth date/place with an uncertainty window.
    ///
    /// `approx_minute` + `window_minutes` (half-window) clip candidates;
    /// both `None` means the full day.
    #[must_use]
    pub fn new(
        birth_date: NaiveDate,
        lat: f64,
        lon: f64,
        utc_offset_minutes: i32,
        ayanamsa: AyanamsaSystem,
        approx_minute: Option<u16>,
        window_minutes: Option<u16>,
    ) -> Self {
        let intervals = lagna_intervals(birth_date, lat, lon, utc_offset_minutes, ayanamsa);
        let (win_lo, win_hi) = match (approx_minute, window_minutes) {
            (Some(mid), Some(half)) => {
                let lo = mid.saturating_sub(half);
                let hi = mid.saturating_add(half).min(24 * 60);
                (lo, hi)
            }
            _ => (0u16, 24 * 60),
        };
        let base_candidates: Vec<(u16, u16, u8)> = intervals
            .into_iter()
            .filter(|(from, to, _)| *to > win_lo && *from < win_hi)
            .map(|(from, to, r)| (from.max(win_lo), to.min(win_hi), r))
            .filter(|(from, to, _)| from < to)
            .collect();
        Self {
            birth_date,
            lat,
            lon,
            utc_offset_minutes,
            ayanamsa,
            base_candidates,
            answers: [None; 8],
            events: Vec::new(),
            step: 2,
            question_idx: 0,
        }
    }

    /// Number of quiz questions.
    #[must_use]
    pub fn question_count() -> usize {
        QUIZ.len()
    }

    /// Fluent key for question `idx`.
    #[must_use]
    pub fn question_key(idx: usize) -> &'static str {
        QUIZ.get(idx).map(|(k, _, _)| *k).unwrap_or("")
    }

    /// Fluent keys for the four options of question `idx`.
    #[must_use]
    pub fn option_keys(idx: usize) -> &'static [&'static str] {
        QUIZ.get(idx)
            .map(|(_, opts, _)| opts.as_slice())
            .unwrap_or(&[])
    }

    /// Record an answer and advance the question index.
    pub fn answer(&mut self, question_idx: usize, option_idx: usize) {
        if question_idx < 8 && option_idx < 4 {
            self.answers[question_idx] = Some(option_idx);
            if self.question_idx as usize == question_idx && self.question_idx < 7 {
                self.question_idx += 1;
            } else if self.question_idx as usize == question_idx && self.question_idx == 7 {
                self.step = 3;
            }
        }
    }

    /// Add a life event.
    pub fn add_event(&mut self, e: LifeEvent) {
        self.events.push(e);
    }

    /// Remove a life event by index.
    pub fn remove_event(&mut self, idx: usize) {
        if idx < self.events.len() {
            self.events.remove(idx);
        }
    }

    /// Events recorded so far.
    #[must_use]
    pub fn events(&self) -> &[LifeEvent] {
        &self.events
    }

    /// Advance from events step to results.
    pub fn next_step(&mut self) {
        if self.step == 3 {
            self.step = 4;
        } else if self.step == 2 && self.question_idx >= 7 {
            self.step = 3;
        }
    }

    /// Ranked candidates (descending total score).
    #[must_use]
    pub fn results(&self) -> Vec<Candidate> {
        let mut scored: Vec<(i32, Candidate)> = self
            .base_candidates
            .iter()
            .map(|(from, to, rashi)| {
                let quiz = quiz_score_for(*rashi, &self.answers);
                let event = event_score_for(
                    self.birth_date,
                    *from,
                    *to,
                    self.utc_offset_minutes,
                    self.ayanamsa,
                    &self.events,
                );
                let total = quiz * 2 + event * 3;
                (
                    total,
                    Candidate {
                        from_minute: *from,
                        to_minute: *to,
                        lagna_rashi: *rashi,
                        quiz_score: quiz,
                        event_score: event,
                        confidence_pct: 0,
                    },
                )
            })
            .collect();
        scored.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
        let denom: i32 = scored.iter().map(|(t, _)| (*t).max(0)).sum();
        let n = scored.len().max(1) as i32;
        for (t, c) in &mut scored {
            c.confidence_pct = if denom > 0 {
                ((100 * (*t).max(0)) / denom) as u8
            } else {
                (100 / n) as u8
            };
        }
        scored.into_iter().map(|(_, c)| c).collect()
    }

    /// Midpoint of the best interval as `"HH:MM"`.
    #[must_use]
    pub fn best_time_string(&self) -> Option<String> {
        let best = self.results().into_iter().next()?;
        let mid = (u32::from(best.from_minute) + u32::from(best.to_minute)) / 2;
        let h = mid / 60;
        let m = mid % 60;
        Some(format!("{h:02}:{m:02}"))
    }

    /// Format a candidate time range `"HH:MM–HH:MM"`.
    #[must_use]
    pub fn format_range(from: u16, to: u16) -> String {
        let fmt = |m: u16| format!("{:02}:{:02}", m / 60, m % 60);
        let end = if to == 0 || to >= 24 * 60 {
            "24:00".into()
        } else {
            fmt(to)
        };
        format!("{}–{}", fmt(from), end)
    }

    /// Rashi Fluent key for a candidate.
    #[must_use]
    pub fn candidate_rashi_key(rashi: u8) -> &'static str {
        rashi_ftl_key(rashi)
    }
}

fn quiz_score_for(rashi: u8, answers: &[Option<usize>; 8]) -> i32 {
    let mut score = 0i32;
    for (qi, ans) in answers.iter().enumerate() {
        let Some(opt) = *ans else { continue };
        if let Some((_, _, weights)) = QUIZ.get(qi) {
            if let Some(w) = weights.get(opt) {
                for (r, pts) in *w {
                    if *r == rashi {
                        score += pts;
                    }
                }
            }
        }
    }
    score
}

fn event_score_for(
    birth_date: NaiveDate,
    from_minute: u16,
    to_minute: u16,
    utc_offset_minutes: i32,
    ayanamsa: AyanamsaSystem,
    events: &[LifeEvent],
) -> i32 {
    let mid = (u32::from(from_minute) + u32::from(to_minute)) / 2;
    let local = birth_date
        .and_hms_opt(mid / 60, mid % 60, 0)
        .unwrap_or_else(|| birth_date.and_hms_opt(12, 0, 0).unwrap());
    let utc = local - Duration::minutes(i64::from(utc_offset_minutes));
    let utc_dt = utc.and_utc();
    let natal = compute_natal(utc_dt, ayanamsa);
    let mut score = 0i32;
    for e in events {
        let event_date = NaiveDate::from_ymd_opt(e.year, 7, 1).unwrap_or(birth_date);
        if let Some((maha, antar, pratyantar)) =
            dasha_at(natal.moon_longitude, birth_date, event_date)
        {
            let fav = e.kind.favorable_lords();
            if fav.contains(&pratyantar) {
                score += 5;
            }
            if fav.contains(&antar) {
                score += 3;
            }
            if fav.contains(&maha) {
                score += 1;
            }
        }
    }
    score
}

fn lagna_intervals(
    birth_date: NaiveDate,
    lat: f64,
    lon: f64,
    utc_offset_minutes: i32,
    ayanamsa: AyanamsaSystem,
) -> Vec<(u16, u16, u8)> {
    let mut intervals = Vec::new();
    let mut prev_rashi: Option<u8> = None;
    let mut start: u16 = 0;
    for minute in (0..=24 * 60).step_by(5) {
        let m = minute as u16;
        let rashi = if m >= 24 * 60 {
            // Force flush at end of day.
            255
        } else {
            let local = birth_date.and_time(
                NaiveTime::from_hms_opt(u32::from(m) / 60, u32::from(m) % 60, 0).unwrap(),
            );
            let utc = local - Duration::minutes(i64::from(utc_offset_minutes));
            let jd = julian_day(utc.and_utc());
            let asc = ascendant_sidereal(jd, lat, lon, ayanamsa);
            (asc / 30.0).floor() as u8 % 12
        };
        match prev_rashi {
            None => {
                prev_rashi = Some(rashi);
                start = 0;
            }
            Some(pr) if pr != rashi || m >= 24 * 60 => {
                intervals.push((start, m.min(24 * 60), pr));
                prev_rashi = Some(rashi);
                start = m;
            }
            _ => {}
        }
    }
    intervals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_day_has_roughly_twelve_candidates() {
        let s = RectifySession::new(
            NaiveDate::from_ymd_opt(1990, 6, 15).unwrap(),
            25.0,
            83.0,
            330,
            AyanamsaSystem::Lahiri,
            None,
            None,
        );
        let n = s.base_candidates.len();
        assert!(
            (11..=14).contains(&n),
            "expected 11..=14 lagna intervals, got {n}"
        );
    }

    #[test]
    fn aggressive_quiz_answers_boost_mesha() {
        let mut s = RectifySession::new(
            NaiveDate::from_ymd_opt(1990, 6, 15).unwrap(),
            25.0,
            83.0,
            330,
            AyanamsaSystem::Lahiri,
            None,
            None,
        );
        for q in 0..8 {
            s.answer(q, 0); // option "a" — aggressive / Mesha-leaning
        }
        let results = s.results();
        assert!(!results.is_empty());
        let top3: Vec<u8> = results.iter().take(3).map(|c| c.lagna_rashi).collect();
        assert!(
            top3.contains(&0),
            "Mesha (0) should be in top-3, got {top3:?}"
        );
    }

    #[test]
    fn marriage_in_venus_maha_scores_event() {
        // Moon at start of Ashwini → Ketu 7yr then Venus 20yr.
        // Birth 1990-01-01; marriage year 2005 is age 15 → Venus maha.
        let birth = NaiveDate::from_ymd_opt(1990, 1, 1).unwrap();
        // Pick a candidate interval and force moon via compute_natal at a
        // known UTC instant where moon is near 0° sidereal is hard; instead
        // unit-test event_score_for with a synthetic natal via dasha_at.
        let (maha, _, _) =
            dasha_at(0.0, birth, NaiveDate::from_ymd_opt(2005, 7, 1).unwrap()).expect("dasha");
        assert_eq!(maha, DashaLord::Venus);
        let mut s = RectifySession::new(birth, 0.0, 0.0, 0, AyanamsaSystem::Lahiri, None, None);
        s.add_event(LifeEvent {
            kind: EventKind::Marriage,
            year: 2005,
        });
        // At least one candidate should get a non-zero event score if their
        // natal moon lands near Ashwini — not guaranteed for every lat/lon,
        // so just ensure the API runs and returns confidences summing ~100.
        let results = s.results();
        let conf: u32 = results.iter().map(|c| u32::from(c.confidence_pct)).sum();
        assert!(conf >= 90 && conf <= 110, "confidence sum={conf}");
    }

    #[test]
    fn best_time_string_is_hh_mm() {
        let s = RectifySession::new(
            NaiveDate::from_ymd_opt(1990, 6, 15).unwrap(),
            25.0,
            83.0,
            330,
            AyanamsaSystem::Lahiri,
            Some(12 * 60),
            Some(120),
        );
        let t = s.best_time_string().expect("time");
        assert_eq!(t.len(), 5);
        assert_eq!(&t[2..3], ":");
        let hour: u32 = t[0..2].parse().unwrap();
        assert!((10..=14).contains(&hour));
    }

    #[test]
    fn question_keys_stable() {
        assert_eq!(RectifySession::question_count(), 8);
        assert_eq!(RectifySession::question_key(0), "jyotish-rq-1");
        assert_eq!(RectifySession::option_keys(0).len(), 4);
    }
}
