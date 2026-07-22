//! Plain-language narrative built from a [`DayScore`].
//!
//! Translates the raw scoring [`Factor`]s into an ordered set of Fluent
//! keys: a headline for the overall color, the top contributing influences
//! (by absolute weight), and a short list of actionable advice keys.

use super::score::{DayColor, DayScore, Factor};

/// Rendered narrative for a [`DayScore`], as Fluent keys ready for the UI
/// layer to resolve via `LocaleManager`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Narrative {
    /// Overall headline key, keyed off [`DayColor`].
    pub headline_key: &'static str,
    /// Top contributing factors, most impactful first (max 4).
    pub influence_keys: Vec<&'static str>,
    /// Suggested actions/cautions for the day.
    pub advice_keys: Vec<&'static str>,
}

fn headline_key(color: DayColor) -> &'static str {
    match color {
        DayColor::Green => "jyotish-headline-green",
        DayColor::Yellow => "jyotish-headline-yellow",
        DayColor::Red => "jyotish-headline-red",
    }
}

fn influence_key(factor: Factor) -> Option<&'static str> {
    match factor {
        Factor::Tara(0) => Some("jyotish-influence-tara-janma"),
        Factor::Tara(1) => Some("jyotish-influence-tara-sampat"),
        Factor::Tara(2) => Some("jyotish-influence-tara-vipat"),
        Factor::Tara(3) => Some("jyotish-influence-tara-kshema"),
        Factor::Tara(4) => Some("jyotish-influence-tara-pratyak"),
        Factor::Tara(5) => Some("jyotish-influence-tara-sadhaka"),
        Factor::Tara(6) => Some("jyotish-influence-tara-naidhana"),
        Factor::Tara(7) => Some("jyotish-influence-tara-mitra"),
        Factor::Tara(8) => Some("jyotish-influence-tara-parama-mitra"),
        Factor::Tara(_) => None,

        Factor::Chandra(4) => Some("jyotish-influence-chandra-house-4"),
        Factor::Chandra(8) => Some("jyotish-influence-chandra-house-8"),
        Factor::Chandra(12) => Some("jyotish-influence-chandra-house-12"),
        Factor::Chandra(1 | 3 | 6 | 7 | 10 | 11) => Some("jyotish-influence-chandra-favorable"),
        Factor::Chandra(_) => None,

        Factor::TithiClass(0) => Some("jyotish-influence-tithi-nanda"),
        Factor::TithiClass(1) => Some("jyotish-influence-tithi-bhadra"),
        Factor::TithiClass(2) => Some("jyotish-influence-tithi-jaya"),
        Factor::TithiClass(3) => Some("jyotish-influence-tithi-rikta"),
        Factor::TithiClass(4) => Some("jyotish-influence-tithi-purna"),
        Factor::TithiClass(_) => None,

        Factor::Amavasya => Some("jyotish-influence-amavasya"),
        Factor::Purnima => Some("jyotish-influence-purnima"),

        Factor::BadYoga(16) => Some("jyotish-influence-yoga-vyatipata"),
        Factor::BadYoga(26) => Some("jyotish-influence-yoga-vaidhriti"),
        Factor::BadYoga(5) => Some("jyotish-influence-yoga-atiganda"),
        Factor::BadYoga(8) => Some("jyotish-influence-yoga-shula"),
        Factor::BadYoga(9) => Some("jyotish-influence-yoga-ganda"),
        Factor::BadYoga(12) => Some("jyotish-influence-yoga-vyaghata"),
        Factor::BadYoga(14) => Some("jyotish-influence-yoga-vajra"),
        Factor::BadYoga(18) => Some("jyotish-influence-yoga-parigha"),
        Factor::BadYoga(_) => None,

        Factor::VishtiKarana => Some("jyotish-influence-vishti"),

        Factor::FixedKarana(7) => Some("jyotish-influence-karana-shakuni"),
        Factor::FixedKarana(8) => Some("jyotish-influence-karana-chatushpada"),
        Factor::FixedKarana(9) => Some("jyotish-influence-karana-naga"),
        Factor::FixedKarana(_) => None,
    }
}

fn advice_keys(score: &DayScore) -> Vec<&'static str> {
    let mut advice = Vec::new();

    match score.color {
        DayColor::Red => advice.push("jyotish-advice-avoid-major-decisions"),
        DayColor::Yellow => advice.push("jyotish-advice-proceed-with-caution"),
        DayColor::Green => advice.push("jyotish-advice-favorable-for-new-beginnings"),
    }

    for (factor, _) in &score.factors {
        let key = match factor {
            Factor::VishtiKarana => Some("jyotish-advice-avoid-starting-tasks"),
            Factor::Amavasya => Some("jyotish-advice-favor-spiritual-practice"),
            Factor::Tara(2 | 4 | 6) => Some("jyotish-advice-postpone-travel"),
            Factor::BadYoga(_) => Some("jyotish-advice-double-check-plans"),
            Factor::Chandra(4 | 8 | 12) => Some("jyotish-advice-avoid-risky-ventures"),
            _ => None,
        };
        if let Some(key) = key {
            if !advice.contains(&key) {
                advice.push(key);
            }
        }
    }

    advice
}

/// Build the plain-language [`Narrative`] for a computed [`DayScore`].
#[must_use]
pub fn build_narrative(score: &DayScore) -> Narrative {
    let mut ranked: Vec<&(Factor, i8)> = score.factors.iter().collect();
    ranked.sort_by_key(|(_, weight)| std::cmp::Reverse(weight.unsigned_abs()));

    let influence_keys = ranked
        .into_iter()
        .filter_map(|(factor, _)| influence_key(*factor))
        .take(4)
        .collect();

    Narrative {
        headline_key: headline_key(score.color),
        influence_keys,
        advice_keys: advice_keys(score),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::jyotish::config::AyanamsaSystem;
    use crate::builtin::jyotish::score::compute_day_score;
    use chrono::{TimeZone, Utc};

    #[test]
    fn headline_matches_color() {
        assert_eq!(headline_key(DayColor::Green), "jyotish-headline-green");
        assert_eq!(headline_key(DayColor::Yellow), "jyotish-headline-yellow");
        assert_eq!(headline_key(DayColor::Red), "jyotish-headline-red");
    }

    #[test]
    fn influence_maps_bad_tara() {
        assert_eq!(
            influence_key(Factor::Tara(2)),
            Some("jyotish-influence-tara-vipat")
        );
        assert_eq!(
            influence_key(Factor::Chandra(8)),
            Some("jyotish-influence-chandra-house-8")
        );
        assert_eq!(
            influence_key(Factor::VishtiKarana),
            Some("jyotish-influence-vishti")
        );
    }

    #[test]
    fn narrative_influences_are_sorted_by_absolute_weight_and_capped_at_four() {
        let score = DayScore {
            score: 40,
            color: DayColor::Yellow,
            factors: vec![
                (Factor::TithiClass(0), 2),
                (Factor::Tara(2), -18),
                (Factor::Chandra(8), -14),
                (Factor::BadYoga(16), -14),
                (Factor::VishtiKarana, -8),
                (Factor::Amavasya, -6),
            ],
        };
        let narrative = build_narrative(&score);
        assert_eq!(narrative.influence_keys.len(), 4);
        assert_eq!(narrative.influence_keys[0], "jyotish-influence-tara-vipat");
    }

    #[test]
    fn advice_flags_vishti_and_bad_tara() {
        let score = DayScore {
            score: 30,
            color: DayColor::Red,
            factors: vec![(Factor::VishtiKarana, -8), (Factor::Tara(6), -18)],
        };
        let advice = advice_keys(&score);
        assert!(advice.contains(&"jyotish-advice-avoid-major-decisions"));
        assert!(advice.contains(&"jyotish-advice-avoid-starting-tasks"));
        assert!(advice.contains(&"jyotish-advice-postpone-travel"));
    }

    #[test]
    fn build_narrative_smoke_from_real_score() {
        let at = Utc.with_ymd_and_hms(2026, 7, 21, 12, 0, 0).unwrap();
        let score = compute_day_score(at, AyanamsaSystem::Lahiri, None);
        let narrative = build_narrative(&score);
        assert!(!narrative.headline_key.is_empty());
        assert!(narrative.influence_keys.len() <= 4);
    }
}
