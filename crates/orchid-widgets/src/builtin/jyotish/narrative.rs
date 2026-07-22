//! Plain-language narrative built from a [`DayScore`].
//!
//! Translates scoring factors into Fluent keys: a headline, ranked
//! influences, and actionable advice. Context (personal vs generic, vara,
//! paksha, day seed) diversifies copy and reduces strip-level repetition.

use super::score::{DayColor, DayScore, Factor, FactorContribution, Valence};

/// Extra inputs that shape wording without changing the numeric score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NarrativeContext {
    /// Birth-chart layers were applied.
    pub personal: bool,
    /// Sunday=0 … Saturday=6.
    pub vara_index: u8,
    /// `true` = Shukla paksha.
    pub paksha_shukla: bool,
    /// Stable per-day seed (e.g. ordinal) for rotating advice variants.
    pub day_seed: u32,
}

impl NarrativeContext {
    /// Build context from score + panchanga limbs.
    #[must_use]
    pub fn new(score: &DayScore, vara_index: u8, paksha_shukla: bool, day_seed: u32) -> Self {
        Self {
            personal: score.personal,
            vara_index,
            paksha_shukla,
            day_seed,
        }
    }
}

/// Rendered narrative for a [`DayScore`], as Fluent keys ready for the UI.
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

/// Map a factor to an existing Fluent influence key (aligned with en-US).
pub(crate) fn influence_key(factor: Factor) -> Option<&'static str> {
    match factor {
        Factor::Tara(0) => Some("jyotish-influence-tara-janma"),
        Factor::Tara(1) => Some("jyotish-influence-tara-sampat"),
        Factor::Tara(2) => Some("jyotish-influence-tara-vipat"),
        Factor::Tara(3) => Some("jyotish-influence-tara-kshema"),
        Factor::Tara(4) => Some("jyotish-influence-tara-pratyak"),
        Factor::Tara(5) => Some("jyotish-influence-tara-sadhana"),
        Factor::Tara(6) => Some("jyotish-influence-tara-naidhana"),
        Factor::Tara(7) => Some("jyotish-influence-tara-mitra"),
        Factor::Tara(8) => Some("jyotish-influence-tara-parama-mitra"),
        Factor::Tara(_) => None,

        Factor::Chandra(1 | 3 | 6 | 7 | 10 | 11) => Some("jyotish-influence-chandra-high"),
        Factor::Chandra(4 | 8 | 12) => Some("jyotish-influence-chandra-low"),
        Factor::Chandra(_) => Some("jyotish-influence-chandra-neutral"),

        Factor::TithiClass(3) => Some("jyotish-influence-tithi-rikta"),
        Factor::TithiClass(4) => Some("jyotish-influence-tithi-purna"),
        Factor::TithiClass(_) => Some("jyotish-influence-tithi-flow"),

        Factor::Amavasya => Some("jyotish-influence-amavasya"),
        Factor::Purnima => Some("jyotish-influence-purnima"),

        Factor::BadYoga(16) => Some("jyotish-influence-yoga-vyatipata"),
        Factor::BadYoga(26) => Some("jyotish-influence-yoga-vaidhriti"),
        Factor::BadYoga(_) => Some("jyotish-influence-yoga-tense"),

        Factor::VishtiKarana => Some("jyotish-influence-vishti"),
        Factor::FixedKarana(_) => Some("jyotish-influence-karana-fixed"),
    }
}

fn pick_variant(seed: u32, options: &[&'static str]) -> &'static str {
    if options.is_empty() {
        return "jyotish-advice-flow";
    }
    options[seed as usize % options.len()]
}

fn color_advice(color: DayColor, seed: u32, personal: bool) -> &'static str {
    if personal {
        match color {
            DayColor::Green => pick_variant(
                seed,
                &[
                    "jyotish-advice-act",
                    "jyotish-advice-personal-advance",
                    "jyotish-advice-flow",
                ],
            ),
            DayColor::Yellow => pick_variant(
                seed,
                &[
                    "jyotish-advice-routine",
                    "jyotish-advice-personal-caution",
                    "jyotish-advice-flow",
                ],
            ),
            DayColor::Red => pick_variant(
                seed,
                &[
                    "jyotish-advice-avoid-big",
                    "jyotish-advice-personal-rest",
                    "jyotish-advice-routine",
                ],
            ),
        }
    } else {
        match color {
            DayColor::Green => pick_variant(seed, &["jyotish-advice-act", "jyotish-advice-flow"]),
            DayColor::Yellow => {
                pick_variant(seed, &["jyotish-advice-routine", "jyotish-advice-flow"])
            }
            DayColor::Red => pick_variant(
                seed.wrapping_add(1),
                &["jyotish-advice-avoid-big", "jyotish-advice-routine"],
            ),
        }
    }
}

fn factor_advice(factor: Factor) -> Option<&'static str> {
    match factor {
        Factor::VishtiKarana => Some("jyotish-advice-avoid-big"),
        Factor::Amavasya => Some("jyotish-advice-routine"),
        Factor::Tara(2 | 4 | 6) => Some("jyotish-advice-avoid-big"),
        Factor::BadYoga(_) => Some("jyotish-advice-routine"),
        Factor::Chandra(4 | 8 | 12) => Some("jyotish-advice-routine"),
        Factor::Purnima => Some("jyotish-advice-act"),
        _ => None,
    }
}

fn advice_keys(score: &DayScore, ctx: &NarrativeContext) -> Vec<&'static str> {
    let mut advice = Vec::new();
    let primary = color_advice(score.color, ctx.day_seed, ctx.personal);
    advice.push(primary);

    // Mode banner advice (generic vs personal).
    let mode_key = if ctx.personal {
        "jyotish-advice-mode-personal"
    } else {
        "jyotish-advice-mode-panchanga"
    };
    if !advice.contains(&mode_key) {
        advice.push(mode_key);
    }

    // Rank factor-specific tips by strength; skip duplicates / primary.
    let mut ranked: Vec<&FactorContribution> = score.factors.iter().collect();
    ranked.sort_by_key(|c| std::cmp::Reverse(c.strength));
    for c in ranked {
        if let Some(key) = factor_advice(c.factor) {
            if !advice.contains(&key) {
                advice.push(key);
            }
        }
        if advice.len() >= 4 {
            break;
        }
    }
    advice
}

fn contextual_influences(ctx: &NarrativeContext) -> Vec<&'static str> {
    let mut keys = Vec::new();
    keys.push(if ctx.personal {
        "jyotish-influence-layer-personal"
    } else {
        "jyotish-influence-layer-panchanga"
    });
    keys.push(if ctx.paksha_shukla {
        "jyotish-influence-paksha-shukla"
    } else {
        "jyotish-influence-paksha-krishna"
    });
    // Light vara seasoning (rotate among a few archetypes).
    let vara_keys = [
        "jyotish-influence-vara-sun",
        "jyotish-influence-vara-moon",
        "jyotish-influence-vara-mars",
        "jyotish-influence-vara-mercury",
        "jyotish-influence-vara-jupiter",
        "jyotish-influence-vara-venus",
        "jyotish-influence-vara-saturn",
    ];
    keys.push(vara_keys[usize::from(ctx.vara_index.min(6))]);
    keys
}

/// Build the plain-language [`Narrative`] for a computed [`DayScore`].
#[must_use]
pub fn build_narrative(score: &DayScore, ctx: &NarrativeContext) -> Narrative {
    let mut ranked: Vec<&FactorContribution> = score.factors.iter().collect();
    // Prefer strength, then |delta|, so Vishti outranks a mild equal delta.
    ranked.sort_by(|a, b| {
        b.strength
            .cmp(&a.strength)
            .then_with(|| b.delta.unsigned_abs().cmp(&a.delta.unsigned_abs()))
    });

    let mut influence_keys: Vec<&'static str> = ranked
        .into_iter()
        .filter(|c| c.valence != Valence::Neutral || c.delta != 0)
        .filter_map(|c| influence_key(c.factor))
        .take(3)
        .collect();

    if influence_keys.is_empty() {
        influence_keys.push("jyotish-influence-calm");
    }

    // Append at most one contextual line (paksha or layer), rotated by seed
    // so the 7-day strip does not repeat the same closing line.
    let context_pool = contextual_influences(ctx);
    let extra = context_pool[ctx.day_seed as usize % context_pool.len()];
    if !influence_keys.contains(&extra) && influence_keys.len() < 4 {
        influence_keys.push(extra);
    }

    Narrative {
        headline_key: headline_key(score.color),
        influence_keys,
        advice_keys: advice_keys(score, ctx),
    }
}

/// Convenience: narrative with only score-derived context (no vara/paksha).
#[must_use]
pub fn build_narrative_simple(score: &DayScore) -> Narrative {
    let ctx = NarrativeContext {
        personal: score.personal,
        vara_index: 0,
        paksha_shukla: true,
        day_seed: u32::from(score.score),
    };
    build_narrative(score, &ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin::jyotish::config::AyanamsaSystem;
    use crate::builtin::jyotish::score::{compute_day_score, contribute};
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
            Some("jyotish-influence-chandra-low")
        );
        assert_eq!(
            influence_key(Factor::VishtiKarana),
            Some("jyotish-influence-vishti")
        );
    }

    #[test]
    fn narrative_influences_are_sorted_by_strength_and_capped() {
        let score = DayScore {
            score: 40,
            color: DayColor::Yellow,
            factors: vec![
                contribute(Factor::TithiClass(0), 2),
                contribute(Factor::Tara(2), -18),
                contribute(Factor::Chandra(8), -14),
                contribute(Factor::BadYoga(16), -14),
                contribute(Factor::VishtiKarana, -8),
                contribute(Factor::Amavasya, -6),
            ],
            personal: true,
        };
        let ctx = NarrativeContext::new(&score, 2, true, 10);
        let narrative = build_narrative(&score, &ctx);
        assert!(narrative.influence_keys.len() <= 4);
        assert_eq!(narrative.influence_keys[0], "jyotish-influence-tara-vipat");
    }

    #[test]
    fn advice_differs_personal_vs_generic() {
        let score = DayScore {
            score: 70,
            color: DayColor::Green,
            factors: vec![contribute(Factor::TithiClass(4), 6)],
            personal: false,
        };
        let mut generic = score.clone();
        generic.personal = false;
        let mut personal = score.clone();
        personal.personal = true;
        let g = build_narrative(
            &generic,
            &NarrativeContext {
                personal: false,
                vara_index: 0,
                paksha_shukla: true,
                day_seed: 3,
            },
        );
        let p = build_narrative(
            &personal,
            &NarrativeContext {
                personal: true,
                vara_index: 0,
                paksha_shukla: true,
                day_seed: 3,
            },
        );
        assert!(g.advice_keys.contains(&"jyotish-advice-mode-panchanga"));
        assert!(p.advice_keys.contains(&"jyotish-advice-mode-personal"));
    }

    #[test]
    fn advice_variants_rotate_with_seed() {
        let score = DayScore {
            score: 30,
            color: DayColor::Red,
            factors: vec![contribute(Factor::VishtiKarana, -8)],
            personal: false,
        };
        let a = build_narrative(
            &score,
            &NarrativeContext {
                personal: false,
                vara_index: 1,
                paksha_shukla: false,
                day_seed: 0,
            },
        );
        let b = build_narrative(
            &score,
            &NarrativeContext {
                personal: false,
                vara_index: 1,
                paksha_shukla: false,
                day_seed: 1,
            },
        );
        // Primary color advice should differ across seeds for red generic.
        assert_ne!(a.advice_keys[0], b.advice_keys[0]);
    }

    #[test]
    fn build_narrative_smoke_from_real_score() {
        let at = Utc.with_ymd_and_hms(2026, 7, 21, 12, 0, 0).unwrap();
        let score = compute_day_score(at, AyanamsaSystem::Lahiri, None);
        let narrative = build_narrative_simple(&score);
        assert!(!narrative.headline_key.is_empty());
        assert!(narrative.influence_keys.len() <= 4);
    }
}
