//! Jyotish widget — Vedic panchanga from local astronomical calculations.

pub mod astro;
pub mod config;
pub mod dasha;
pub mod gochara;
#[cfg(test)]
pub mod golden;
pub mod muhurta;
pub mod narrative;
pub mod rectify;
pub mod score;
pub mod transitions;

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration as ChronoDuration, NaiveDate, NaiveTime, Utc, Weekday};
use dashmap::DashMap;
use parking_lot::RwLock;
use tracing::warn;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::{
    JyotishAntarRow, JyotishCityEntry, JyotishDashaNow, JyotishDayChip, JyotishFactorRow,
    JyotishMonthCell, JyotishMonthSummary, JyotishPayload, JyotishPlanetRow,
    JyotishRectifyCandidate, JyotishRectifyView, JyotishSearchHit, JyotishYearSummary,
};
use crate::widget::refresh::PeriodicRefresh;
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};
use orchid_storage::{LifecycleState, LocaleConfig, WidgetSize};

use super::weather::provider::{GeocodingHit, OpenMeteoProvider, WeatherProvider};

pub use astro::{compute_jyotish, JyotishData};
pub use config::{decode_config, AyanamsaSystem, JyotishConfig, JyotishLocation};
pub use dasha::{
    antar_dashas, dasha_at, dasha_stack_at, maha_dashas, pratyantar_dashas, DashaLord, DashaPeriod,
    DashaStack,
};
pub use gochara::{gochara_modifier, gochara_note_key, tint_counts};
pub use muhurta::{day_muhurtas, in_window, DayMuhurtas, MuhurtaWindow};
pub use narrative::{build_narrative, build_narrative_simple, Narrative, NarrativeContext};
pub use rectify::{Candidate, EventKind, LifeEvent, RectifySession};
pub use score::{
    compute_day_score, compute_natal, contribute, local_noon_utc, DayColor, DayScore, Factor,
    FactorContribution, NatalInfo, Valence, BASE_GENERIC, BASE_NATAL, THRESHOLD_GREEN,
    THRESHOLD_YELLOW,
};
pub use transitions::{next_transition, panchanga_ends, Limb, PanchangaEnds};

/// Stable type id.
pub const TYPE_ID: &str = "jyotish";

static JYOTISH_LIVE: LazyLock<DashMap<Uuid, Arc<JyotishHandle>>> = LazyLock::new(DashMap::new);

/// Config-derived fingerprint used to decide when the day-color and year
/// caches must be invalidated (birth data, ayanamsa, personal-layer toggle,
/// or the active location — sunrise/muhurta/panchanga are computed locally).
type NatalFingerprint = (
    Option<String>,
    Option<String>,
    i32,
    AyanamsaSystem,
    bool,
    (i32, i32),
);

fn fingerprint_of(cfg: &JyotishConfig) -> NatalFingerprint {
    (
        cfg.birth_date.clone(),
        cfg.birth_time.clone(),
        cfg.birth_utc_offset_minutes,
        cfg.ayanamsa,
        cfg.enable_personal,
        location_key(cfg.active_location()),
    )
}

/// Coords cache key (centi-degrees) so float noise does not duplicate entries.
fn location_key(loc: &JyotishLocation) -> (i32, i32) {
    (
        (loc.latitude * 100.0).round() as i32,
        (loc.longitude * 100.0).round() as i32,
    )
}

/// In-widget location-picker UI state (not persisted).
#[derive(Clone, Default)]
struct JyotishUiState {
    picker_open: bool,
    search_query: String,
    search_results: Vec<GeocodingHit>,
    search_busy: bool,
    search_generation: u64,
}

/// Build the location chip/picker list and geocoding-result rows shared by
/// [`JyotishHandle::render_payload`] and [`loading_payload`].
fn cities_and_search(
    cfg: &JyotishConfig,
    ui: &JyotishUiState,
) -> (Vec<JyotishCityEntry>, Vec<JyotishSearchHit>) {
    let cities = cfg
        .locations
        .iter()
        .enumerate()
        .map(|(i, loc)| JyotishCityEntry {
            name: loc.name.clone(),
            active: i == cfg.active_index,
        })
        .collect();
    let search_results = ui
        .search_results
        .iter()
        .map(|h| JyotishSearchHit {
            name: h.name.clone(),
            detail: h.detail.clone(),
            latitude: h.latitude,
            longitude: h.longitude,
        })
        .collect();
    (cities, search_results)
}

struct JyotishHandle {
    instance_id: Uuid,
    config: Arc<RwLock<JyotishConfig>>,
    provider: Arc<dyn WeatherProvider>,
    ui: Arc<RwLock<JyotishUiState>>,
    data: Arc<RwLock<Option<JyotishData>>>,
    natal: Arc<RwLock<Option<NatalInfo>>>,
    natal_fingerprint: RwLock<Option<NatalFingerprint>>,
    color_cache: DashMap<NaiveDate, u8>,
    /// Cached year-tab aggregates: (natal fingerprint, year, months, gochara key).
    year_cache: RwLock<Option<YearCacheEntry>>,
    rectify: RwLock<Option<RectifySession>>,
    rectify_wizard_step: RwLock<u8>,
    /// Absolute civil year expanded on the Life tab (`None` = collapsed).
    life_detail_year: RwLock<Option<i32>>,
    bus: Arc<orchid_core::EventBus>,
}

type YearCacheEntry = (
    NatalFingerprint,
    i32,
    Vec<JyotishMonthSummary>,
    &'static str,
);

impl JyotishHandle {
    fn publish(&self) {
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    fn recalculate(&self) {
        let cfg = {
            let mut guard = self.config.write();
            guard.normalize();
            guard.clone()
        };

        let at = Utc::now() + ChronoDuration::days(i64::from(cfg.day_offset));
        let data = compute_jyotish(cfg.latitude(), cfg.longitude(), at, cfg.ayanamsa);
        *self.data.write() = Some(data);

        let fingerprint = fingerprint_of(&cfg);
        let fingerprint_changed = {
            let mut last = self.natal_fingerprint.write();
            if *last == Some(fingerprint.clone()) {
                false
            } else {
                *last = Some(fingerprint);
                true
            }
        };
        if fingerprint_changed {
            self.color_cache.clear();
            *self.year_cache.write() = None;
        }

        let natal = birth_datetime_utc(&cfg).map(|dt| compute_natal(dt, cfg.ayanamsa));
        *self.natal.write() = natal;
    }

    fn score_natal<'a>(cfg: &JyotishConfig, natal: Option<&'a NatalInfo>) -> Option<&'a NatalInfo> {
        if cfg.enable_personal {
            natal
        } else {
            None
        }
    }

    /// Auspiciousness color (0=green, 1=yellow, 2=red) for `date`, memoized.
    fn day_color(&self, date: NaiveDate, cfg: &JyotishConfig, natal: Option<&NatalInfo>) -> u8 {
        if let Some(c) = self.color_cache.get(&date) {
            return *c;
        }
        let at = local_noon_utc(date, cfg.longitude());
        let score = compute_day_score(at, cfg.ayanamsa, natal);
        let color = color_of(score.color);
        self.color_cache.insert(date, color);
        color
    }

    fn build_week_strip(&self, cfg: &JyotishConfig, today: NaiveDate) -> Vec<JyotishDayChip> {
        let natal = *self.natal.read();
        let natal_score = Self::score_natal(cfg, natal.as_ref());
        (cfg.day_offset - 3..=cfg.day_offset + 3)
            .map(|offset| {
                let date = today + ChronoDuration::days(i64::from(offset));
                JyotishDayChip {
                    weekday_key: weekday_key(date.weekday()),
                    day_num: date.day() as u8,
                    color: self.day_color(date, cfg, natal_score),
                    offset,
                    is_selected: offset == cfg.day_offset,
                }
            })
            .collect()
    }

    #[allow(clippy::type_complexity)]
    fn build_month(
        &self,
        cfg: &JyotishConfig,
        today: NaiveDate,
    ) -> (&'static str, i32, Vec<JyotishMonthCell>, u8, u16, u16, u16) {
        let natal = *self.natal.read();
        let natal_score = Self::score_natal(cfg, natal.as_ref());
        let (year, month) = month_year_from_offset(today, cfg.month_offset);
        let first_weekday = NaiveDate::from_ymd_opt(year, month, 1)
            .map(|d| d.weekday().num_days_from_monday() as u8)
            .unwrap_or(0);
        let days_in = days_in_month(year, month);

        let mut green = 0u16;
        let mut yellow = 0u16;
        let mut red = 0u16;
        let cells = (1..=days_in)
            .filter_map(|day| NaiveDate::from_ymd_opt(year, month, day).map(|date| (day, date)))
            .map(|(day, date)| {
                let color = self.day_color(date, cfg, natal_score);
                match color {
                    0 => green += 1,
                    1 => yellow += 1,
                    _ => red += 1,
                }
                let offset = (date - today).num_days() as i32;
                JyotishMonthCell {
                    day: day as u8,
                    color,
                    is_today: date == today,
                    is_selected: offset == cfg.day_offset,
                    offset,
                }
            })
            .collect();

        let (green, yellow, red) = if let Some(n) = natal_score {
            let mid = NaiveDate::from_ymd_opt(year, month, 15)
                .unwrap_or_else(|| NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(today));
            let at = local_noon_utc(mid, cfg.longitude());
            let modif = gochara_modifier(n.moon_rashi, at, cfg.ayanamsa);
            tint_counts(green, yellow, red, modif)
        } else {
            (green, yellow, red)
        };

        (
            month_key(month),
            year,
            cells,
            first_weekday,
            green,
            yellow,
            red,
        )
    }

    fn build_year(
        &self,
        cfg: &JyotishConfig,
        today: NaiveDate,
    ) -> (i32, Vec<JyotishMonthSummary>, &'static str) {
        let natal = *self.natal.read();
        let natal_score = Self::score_natal(cfg, natal.as_ref());
        let year = today.year() + cfg.year_offset;
        let fingerprint: NatalFingerprint = fingerprint_of(cfg);
        if let Some((fp, cached_year, months, note)) = self.year_cache.read().as_ref() {
            if *fp == fingerprint && *cached_year == year {
                return (year, months.clone(), note);
            }
        }
        let year_gochara = natal_score.map(|n| {
            let mid = NaiveDate::from_ymd_opt(year, 6, 15).unwrap_or(today);
            let at = local_noon_utc(mid, cfg.longitude());
            gochara_modifier(n.moon_rashi, at, cfg.ayanamsa)
        });
        let months: Vec<JyotishMonthSummary> = (1..=12u32)
            .map(|month| {
                let days_in = days_in_month(year, month);
                let mut green = 0u16;
                let mut yellow = 0u16;
                let mut red = 0u16;
                for day in 1..=days_in {
                    if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
                        match self.day_color(date, cfg, natal_score) {
                            0 => green += 1,
                            1 => yellow += 1,
                            _ => red += 1,
                        }
                    }
                }
                let (green, yellow, red) = if let Some(n) = natal_score {
                    let mid = NaiveDate::from_ymd_opt(year, month, 15).unwrap_or_else(|| {
                        NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(today)
                    });
                    let at = local_noon_utc(mid, cfg.longitude());
                    let modif = gochara_modifier(n.moon_rashi, at, cfg.ayanamsa);
                    tint_counts(green, yellow, red, modif)
                } else {
                    (green, yellow, red)
                };
                let month_offset =
                    (year - today.year()) * 12 + (month as i32 - today.month() as i32);
                JyotishMonthSummary {
                    month_key: month_key(month),
                    green,
                    yellow,
                    red,
                    month_offset,
                }
            })
            .collect();
        let note = year_gochara.map(gochara_note_key).unwrap_or("");
        *self.year_cache.write() = Some((fingerprint, year, months.clone(), note));
        (year, months, note)
    }

    fn build_life(
        &self,
        cfg: &JyotishConfig,
        natal: &NatalInfo,
        today: NaiveDate,
    ) -> Vec<JyotishYearSummary> {
        let birth_date = cfg
            .birth_date
            .as_deref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .or_else(|| NaiveDate::from_ymd_opt(natal.birth_year, 1, 1))
            .unwrap_or(today);
        let current_year = today.year();
        let selected = *self.life_detail_year.read();

        (natal.birth_year..=current_year)
            .map(|year| {
                let mut green = 0u16;
                let mut yellow = 0u16;
                let mut red = 0u16;
                for month in 1..=12u32 {
                    if let Some(date) = NaiveDate::from_ymd_opt(year, month, 15) {
                        match self.day_color(date, cfg, Some(natal)) {
                            0 => green += 1,
                            1 => yellow += 1,
                            _ => red += 1,
                        }
                    }
                }
                let mid = NaiveDate::from_ymd_opt(year, 6, 15).unwrap_or(today);
                let modif = gochara_modifier(
                    natal.moon_rashi,
                    local_noon_utc(mid, cfg.longitude()),
                    cfg.ayanamsa,
                );
                let (green, yellow, red) = tint_counts(green * 30, yellow * 30, red * 30, modif);
                let dasha_key = dasha_at(natal.moon_longitude, birth_date, mid)
                    .map(|(maha, _, _)| maha.ftl_key())
                    .unwrap_or("");
                JyotishYearSummary {
                    year,
                    green,
                    yellow,
                    red,
                    dasha_key,
                    year_offset: year - current_year,
                    is_selected: selected == Some(year),
                    is_current: year == current_year,
                }
            })
            .collect()
    }

    fn build_life_antars(
        &self,
        cfg: &JyotishConfig,
        natal: &NatalInfo,
        today: NaiveDate,
        locale: &LocaleConfig,
    ) -> Vec<JyotishAntarRow> {
        let Some(year) = *self.life_detail_year.read() else {
            return Vec::new();
        };
        let birth_date = cfg
            .birth_date
            .as_deref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .or_else(|| NaiveDate::from_ymd_opt(natal.birth_year, 1, 1))
            .unwrap_or(today);
        let mid = NaiveDate::from_ymd_opt(year, 6, 15).unwrap_or(today);
        let Some(stack) = dasha_stack_at(natal.moon_longitude, birth_date, mid) else {
            return Vec::new();
        };
        antar_dashas(&stack.maha)
            .into_iter()
            .map(|period| JyotishAntarRow {
                lord_key: period.lord.ftl_key(),
                from_text: format_naive_date(locale, period.from),
                to_text: format_naive_date(locale, period_end_display(period.to)),
                is_current: period.from <= today && today < period.to,
            })
            .collect()
    }

    fn build_dasha_now(
        &self,
        cfg: &JyotishConfig,
        natal: &NatalInfo,
        selected_date: NaiveDate,
        locale: &LocaleConfig,
    ) -> Option<JyotishDashaNow> {
        let birth_date = cfg
            .birth_date
            .as_deref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())?;
        let stack = dasha_stack_at(natal.moon_longitude, birth_date, selected_date)?;
        Some(JyotishDashaNow {
            maha_key: stack.maha.lord.ftl_key(),
            antar_key: stack.antar.lord.ftl_key(),
            pratyantar_key: stack.pratyantar.lord.ftl_key(),
            maha_range: period_range_text(locale, stack.maha),
            antar_range: period_range_text(locale, stack.antar),
            pratyantar_range: period_range_text(locale, stack.pratyantar),
        })
    }

    fn render_payload(&self, locale: &LocaleConfig) -> JyotishPayload {
        let cfg = self.config.read().clone();
        let Some(data) = self.data.read().clone() else {
            let ui = self.ui.read().clone();
            return loading_payload(&cfg, &ui);
        };
        let natal = *self.natal.read();

        let at = data.calculated_at;
        let date_text = locale.format_date(at);
        let fmt_time = |t: DateTime<Utc>| locale.format_time(t);

        let planets = if cfg.show_planets {
            data.planets
                .iter()
                .map(|p| JyotishPlanetRow {
                    graha_key: p.graha.ftl_key(),
                    rashi_key: astro::rashi_ftl_key(p.rashi_index),
                    degree_text: astro::format_degree_in_rashi(p.degree_in_rashi),
                    is_retrograde: p.is_retrograde,
                })
                .collect()
        } else {
            Vec::new()
        };

        let today = Utc::now().date_naive();
        let selected_date = today + ChronoDuration::days(i64::from(cfg.day_offset));
        let noon_at = local_noon_utc(selected_date, cfg.longitude());
        let natal_for_score = if cfg.enable_personal {
            natal.as_ref()
        } else {
            None
        };
        let day_score = compute_day_score(noon_at, cfg.ayanamsa, natal_for_score);
        // Instantaneous score: live clock on "today", otherwise noon of the
        // selected civil day (same sample as the day score).
        let now_at = if cfg.day_offset == 0 { at } else { noon_at };
        let now_score = compute_day_score(now_at, cfg.ayanamsa, natal_for_score);
        let primary_score = if cfg.day_offset == 0 {
            &now_score
        } else {
            &day_score
        };
        let day_seed = u32::try_from(selected_date.num_days_from_ce()).unwrap_or(0);
        let narrative_ctx =
            NarrativeContext::new(primary_score, data.vara_index, data.paksha_shukla, day_seed);
        let narrative = build_narrative(primary_score, &narrative_ctx);
        let factor_rows = factor_rows_from_score(primary_score);

        let ends = transitions::panchanga_ends(at, cfg.ayanamsa);
        let fmt_end = |t: Option<DateTime<Utc>>| t.map(fmt_time);

        let muhurtas = match (data.sunrise, data.sunset) {
            (Some(rise), Some(set)) => day_muhurtas(rise, set, data.vara_index),
            _ => None,
        };
        let fmt_range =
            |w: muhurta::MuhurtaWindow| format!("{}–{}", fmt_time(w.start), fmt_time(w.end));
        // Always track `in_rahukalam` for notifications; gate display strings only.
        let in_rahukalam = muhurtas
            .as_ref()
            .is_some_and(|m| in_window(at, m.rahukalam));
        let (rahukalam_text, yamagandam_text, gulika_text) = if cfg.show_rahukalam {
            if let Some(m) = muhurtas {
                (
                    Some(fmt_range(m.rahukalam)),
                    Some(fmt_range(m.yamagandam)),
                    Some(fmt_range(m.gulika)),
                )
            } else {
                (None, None, None)
            }
        } else {
            (None, None, None)
        };

        let week_strip = self.build_week_strip(&cfg, today);
        let (
            month_key_val,
            month_year,
            month_cells,
            month_first_weekday,
            month_green,
            month_yellow,
            month_red,
        ) = self.build_month(&cfg, today);
        let (year_value, year_months, year_gochara_key) = self.build_year(&cfg, today);
        let personal_active = cfg.enable_personal && natal.is_some();
        let life_years = if personal_active {
            natal
                .as_ref()
                .map(|n| self.build_life(&cfg, n, today))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let life_antars = if personal_active {
            natal
                .as_ref()
                .map(|n| self.build_life_antars(&cfg, n, today, locale))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let dasha_now = if personal_active {
            natal
                .as_ref()
                .and_then(|n| self.build_dasha_now(&cfg, n, selected_date, locale))
        } else {
            None
        };
        let detail_year = *self.life_detail_year.read();
        let gochara_note_key = if personal_active && cfg.active_tab == 3 {
            detail_year
                .and_then(|year| {
                    natal.as_ref().map(|n| {
                        let mid = NaiveDate::from_ymd_opt(year, 6, 15).unwrap_or(today);
                        gochara_note_key(gochara_modifier(
                            n.moon_rashi,
                            local_noon_utc(mid, cfg.longitude()),
                            cfg.ayanamsa,
                        ))
                    })
                })
                .unwrap_or("")
        } else if personal_active {
            year_gochara_key
        } else {
            ""
        };

        let wizard_step = *self.rectify_wizard_step.read();
        let rectify_view = {
            let guard = self.rectify.read();
            build_rectify_view(wizard_step, guard.as_ref())
        };

        let ui = self.ui.read().clone();
        let (cities, search_results) = cities_and_search(&cfg, &ui);

        JyotishPayload {
            date_text,
            location_name: cfg.location_name().to_string(),
            cities,
            active_city_index: cfg.active_index,
            picker_open: ui.picker_open,
            search_query: ui.search_query.clone(),
            search_results,
            search_busy: ui.search_busy,
            ayanamsa_key: cfg.ayanamsa.ftl_key(),
            ayanamsa_deg_text: format!("{:.2}°", data.ayanamsa_deg),
            day_offset: cfg.day_offset,
            is_today: cfg.day_offset == 0,
            tithi_key: astro::tithi_ftl_key(data.tithi_index),
            paksha_key: astro::paksha_ftl_key(data.paksha_shukla),
            tithi_end_text: fmt_end(ends.tithi),
            nakshatra_key: astro::nakshatra_ftl_key(data.nakshatra_index),
            pada: data.pada,
            nakshatra_end_text: fmt_end(ends.nakshatra),
            yoga_key: astro::yoga_ftl_key(data.yoga_index),
            yoga_end_text: fmt_end(ends.yoga),
            karana_key: astro::karana_ftl_key(data.karana_index),
            karana_end_text: fmt_end(ends.karana),
            vara_key: astro::vara_ftl_key(data.vara_index),
            sunrise_time: if cfg.show_sunrise_sunset {
                data.sunrise.map(fmt_time)
            } else {
                None
            },
            sunset_time: if cfg.show_sunrise_sunset {
                data.sunset.map(fmt_time)
            } else {
                None
            },
            rahukalam_text,
            yamagandam_text,
            gulika_text,
            in_rahukalam,
            planets,
            show_planets: cfg.show_planets,
            is_loading: false,
            active_tab: cfg.active_tab,
            score_color: color_of(primary_score.color),
            now_score_color: color_of(now_score.color),
            day_score_color: color_of(day_score.color),
            score_value: primary_score.score,
            factors: factor_rows,
            personal_mode: primary_score.personal,
            headline_key: narrative.headline_key,
            influence_keys: narrative.influence_keys,
            advice_keys: narrative.advice_keys,
            week_strip,
            month_key: month_key_val,
            month_year,
            month_cells,
            month_first_weekday,
            month_green,
            month_yellow,
            month_red,
            year_value,
            year_months,
            life_years,
            life_detail_year: detail_year.unwrap_or(0),
            life_antars,
            has_dasha_now: dasha_now.is_some(),
            dasha_now: dasha_now.unwrap_or_default(),
            gochara_note_key,
            has_birth_data: cfg.has_birth_data(),
            rectify: rectify_view,
        }
    }

    fn select_life_year(&self, year: i32) {
        let mut guard = self.life_detail_year.write();
        *guard = match *guard {
            Some(y) if y == year => None,
            _ => Some(year.clamp(1800, 2200)),
        };
        drop(guard);
        {
            let mut cfg = self.config.write();
            cfg.active_tab = 3;
        }
        self.publish();
    }

    fn rectify_start(&self) {
        // Resume a draft session when present; otherwise open the window picker.
        if let Some(session) = self.rectify.read().as_ref() {
            *self.rectify_wizard_step.write() = session.step.max(1);
        } else {
            *self.rectify_wizard_step.write() = 1;
        }
        self.publish();
    }

    fn rectify_set_window(&self, approx_minute: i32, half_window: i32) {
        let cfg = self.config.read().clone();
        let Some(birth_date) = cfg
            .birth_date
            .as_deref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        else {
            return;
        };
        let (approx, half) = if half_window < 0 {
            (None, None)
        } else {
            (
                Some(approx_minute.clamp(0, 24 * 60) as u16),
                Some(half_window.clamp(0, 24 * 60) as u16),
            )
        };
        let session = RectifySession::new(
            birth_date,
            cfg.latitude(),
            cfg.longitude(),
            cfg.birth_utc_offset_minutes,
            cfg.ayanamsa,
            approx,
            half,
        );
        *self.rectify.write() = Some(session);
        *self.rectify_wizard_step.write() = 2;
        self.publish();
    }

    fn rectify_answer(&self, question_idx: usize, option_idx: usize) {
        if let Some(session) = self.rectify.write().as_mut() {
            session.answer(question_idx, option_idx);
            *self.rectify_wizard_step.write() = session.step;
        }
        self.publish();
    }

    fn rectify_add_event(&self, kind_idx: usize, year: i32) {
        if let Some(kind) = EventKind::all().get(kind_idx).copied() {
            if let Some(session) = self.rectify.write().as_mut() {
                let _ = session.add_event(LifeEvent { kind, year });
            }
        }
        self.publish();
    }

    fn rectify_remove_event(&self, idx: usize) {
        if let Some(session) = self.rectify.write().as_mut() {
            session.remove_event(idx);
        }
        self.publish();
    }

    fn rectify_next_step(&self) {
        if *self.rectify_wizard_step.read() == 1 {
            return;
        }
        if let Some(session) = self.rectify.write().as_mut() {
            session.next_step();
        }
        if let Some(session) = self.rectify.read().as_ref() {
            *self.rectify_wizard_step.write() = session.step;
        }
        self.publish();
    }

    fn rectify_back(&self) {
        let wizard = *self.rectify_wizard_step.read();
        if wizard <= 1 {
            // Close UI but keep draft (does not touch birth fields).
            *self.rectify_wizard_step.write() = 0;
            self.publish();
            return;
        }
        let leave_to_window = self
            .rectify
            .write()
            .as_mut()
            .map(RectifySession::back)
            .unwrap_or(false);
        if leave_to_window {
            *self.rectify_wizard_step.write() = 1;
        } else if let Some(session) = self.rectify.read().as_ref() {
            *self.rectify_wizard_step.write() = session.step;
        }
        self.publish();
    }

    fn rectify_refine(&self) {
        if let Some(session) = self.rectify.write().as_mut() {
            if session.refine_around_best(30) {
                *self.rectify_wizard_step.write() = 4;
            }
        }
        self.publish();
    }

    fn rectify_accept(&self) {
        let best = self
            .rectify
            .read()
            .as_ref()
            .and_then(RectifySession::best_time_string);
        if let Some(best) = best {
            let mut cfg = self.config.write();
            cfg.birth_time = Some(best);
            cfg.birth_time_rectified = true;
        }
        *self.rectify.write() = None;
        *self.rectify_wizard_step.write() = 0;
        self.recalculate();
        self.publish();
    }

    fn rectify_cancel(&self) {
        // Close the overlay; keep the session as a draft. Birth fields untouched.
        *self.rectify_wizard_step.write() = 0;
        self.publish();
    }

    fn rectify_discard_draft(&self) {
        *self.rectify.write() = None;
        *self.rectify_wizard_step.write() = 0;
        self.publish();
    }

    fn set_picker_open(&self, open: bool) {
        let mut ui = self.ui.write();
        ui.picker_open = open;
        if !open {
            ui.search_query.clear();
            ui.search_results.clear();
            ui.search_busy = false;
        }
        drop(ui);
        self.publish();
    }

    fn select_city(&self, index: usize) {
        {
            let mut cfg = self.config.write();
            if index < cfg.locations.len() {
                cfg.active_index = index;
            }
        }
        self.recalculate();
        self.publish();
    }

    fn remove_city(&self, index: usize) {
        {
            let mut cfg = self.config.write();
            if cfg.locations.len() <= 1 || index >= cfg.locations.len() {
                return;
            }
            cfg.locations.remove(index);
            if cfg.active_index > index {
                cfg.active_index -= 1;
            } else if cfg.active_index >= cfg.locations.len() {
                cfg.active_index = cfg.locations.len().saturating_sub(1);
            }
            cfg.normalize();
        }
        self.recalculate();
        self.publish();
    }

    fn add_city(&self, location: JyotishLocation) {
        {
            let mut cfg = self.config.write();
            if let Some(existing) = cfg
                .locations
                .iter()
                .position(|l| location_key(l) == location_key(&location))
            {
                cfg.active_index = existing;
            } else {
                cfg.locations.push(location);
                cfg.active_index = cfg.locations.len() - 1;
            }
            cfg.normalize();
        }
        {
            let mut ui = self.ui.write();
            ui.picker_open = false;
            ui.search_query.clear();
            ui.search_results.clear();
            ui.search_busy = false;
        }
        // Keep an open rectify draft aligned with the new place.
        {
            let cfg = self.config.read().clone();
            if let Some(session) = self.rectify.write().as_mut() {
                session.resync_place(
                    cfg.latitude(),
                    cfg.longitude(),
                    cfg.birth_utc_offset_minutes,
                    cfg.ayanamsa,
                );
            }
        }
        self.recalculate();
        self.publish();
    }

    fn search_cities(&self, query: String) {
        let generation = {
            let mut ui = self.ui.write();
            ui.search_query = query.clone();
            ui.search_generation = ui.search_generation.wrapping_add(1);
            ui.search_busy = !query.trim().is_empty();
            if query.trim().is_empty() {
                ui.search_results.clear();
                ui.search_busy = false;
            }
            ui.search_generation
        };
        self.publish();
        if query.trim().is_empty() {
            return;
        }

        let provider = self.provider.clone();
        let ui = self.ui.clone();
        let bus = self.bus.clone();
        let instance_id = self.instance_id;
        tokio::spawn(async move {
            // Debounce keystrokes so we do not hammer the geocoding API.
            tokio::time::sleep(std::time::Duration::from_millis(280)).await;
            if ui.read().search_generation != generation {
                return;
            }
            let result = provider.search_cities(&query).await;
            let mut slot = ui.write();
            if slot.search_generation != generation {
                return;
            }
            slot.search_busy = false;
            match result {
                Ok(hits) => slot.search_results = hits,
                Err(e) => {
                    warn!(%instance_id, error = %e, "jyotish geocoding failed");
                    slot.search_results.clear();
                }
            }
            drop(slot);
            bus.publish(
                orchid_core::EventSource::Widget(instance_id),
                WidgetSnapshotUpdated { instance_id },
            );
        });
    }
}

fn birth_datetime_utc(cfg: &JyotishConfig) -> Option<DateTime<Utc>> {
    let date = NaiveDate::parse_from_str(cfg.birth_date.as_deref()?, "%Y-%m-%d").ok()?;
    let time = cfg
        .birth_time
        .as_deref()
        .and_then(|t| NaiveTime::parse_from_str(t, "%H:%M").ok())
        .unwrap_or_else(|| NaiveTime::from_hms_opt(12, 0, 0).unwrap());
    let local = date.and_time(time);
    let utc_naive = local - ChronoDuration::minutes(i64::from(cfg.birth_utc_offset_minutes));
    Some(utc_naive.and_utc())
}

fn color_of(color: DayColor) -> u8 {
    match color {
        DayColor::Green => 0,
        DayColor::Yellow => 1,
        DayColor::Red => 2,
    }
}

fn format_naive_date(locale: &LocaleConfig, date: NaiveDate) -> String {
    let noon = date
        .and_hms_opt(12, 0, 0)
        .unwrap_or_else(|| date.and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap_or_default()));
    locale.format_date(noon.and_utc())
}

/// Exclusive period end → last inclusive civil day for display.
fn period_end_display(exclusive_to: NaiveDate) -> NaiveDate {
    exclusive_to.pred_opt().unwrap_or(exclusive_to)
}

fn period_range_text(locale: &LocaleConfig, period: DashaPeriod) -> String {
    format!(
        "{} – {}",
        format_naive_date(locale, period.from),
        format_naive_date(locale, period_end_display(period.to))
    )
}

fn factor_rows_from_score(score: &DayScore) -> Vec<JyotishFactorRow> {
    let mut rows: Vec<JyotishFactorRow> = score
        .factors
        .iter()
        .filter_map(|c| {
            let label_key = narrative::influence_key(c.factor)?;
            let valence = match c.valence {
                Valence::Favorable => 0,
                Valence::Unfavorable => 1,
                Valence::Neutral => 2,
            };
            Some(JyotishFactorRow {
                label_key,
                delta: c.delta,
                strength: c.strength,
                valence,
            })
        })
        .collect();
    rows.sort_by(|a, b| {
        b.strength
            .cmp(&a.strength)
            .then_with(|| b.delta.unsigned_abs().cmp(&a.delta.unsigned_abs()))
    });
    rows.truncate(6);
    rows
}

/// Fluent key for a weekday (`jyotish-wd-mon` … `jyotish-wd-sun`).
fn weekday_key(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "jyotish-wd-mon",
        Weekday::Tue => "jyotish-wd-tue",
        Weekday::Wed => "jyotish-wd-wed",
        Weekday::Thu => "jyotish-wd-thu",
        Weekday::Fri => "jyotish-wd-fri",
        Weekday::Sat => "jyotish-wd-sat",
        Weekday::Sun => "jyotish-wd-sun",
    }
}

/// Fluent key for a 1-based month (`jyotish-month-1` … `jyotish-month-12`).
fn month_key(month: u32) -> &'static str {
    match month {
        1 => "jyotish-month-1",
        2 => "jyotish-month-2",
        3 => "jyotish-month-3",
        4 => "jyotish-month-4",
        5 => "jyotish-month-5",
        6 => "jyotish-month-6",
        7 => "jyotish-month-7",
        8 => "jyotish-month-8",
        9 => "jyotish-month-9",
        10 => "jyotish-month-10",
        11 => "jyotish-month-11",
        _ => "jyotish-month-12",
    }
}

/// Resolve the (year, 1-based month) reached by shifting `month_offset`
/// months from `today`'s month.
fn month_year_from_offset(today: NaiveDate, month_offset: i32) -> (i32, u32) {
    let m0 = today.month() as i32 - 1 + month_offset;
    let year = today.year() + m0.div_euclid(12);
    let month = (m0.rem_euclid(12) + 1) as u32;
    (year, month)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let first = NaiveDate::from_ymd_opt(year, month, 1);
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    match (first, next) {
        (Some(f), Some(n)) => (n - f).num_days() as u32,
        _ => 30,
    }
}

fn build_rectify_view(wizard_step: u8, session: Option<&RectifySession>) -> JyotishRectifyView {
    let mut view = JyotishRectifyView {
        has_draft: session.is_some() && wizard_step == 0,
        ..Default::default()
    };
    if wizard_step == 0 {
        return view;
    }
    view.event_kind_keys = EventKind::all().iter().map(|k| k.ftl_key()).collect();
    let max_year = Utc::now().date_naive().year() + 1;
    match session {
        Some(session) => {
            // Window picker can be revisited while a session draft exists.
            view.step = if wizard_step == 1 { 1 } else { session.step };
            view.question_idx = session.question_idx;
            view.question_total = RectifySession::question_count() as u8;
            let qi = usize::from(session.question_idx);
            if qi < RectifySession::question_count() {
                view.question_key = RectifySession::question_key(qi);
                view.option_keys = RectifySession::option_keys(qi).to_vec();
            }
            view.events = session
                .events()
                .iter()
                .map(|e| (e.kind.ftl_key(), e.year))
                .collect();
            view.error_key = session.last_error_key.unwrap_or("");
            view.event_year_min = session.birth_year();
            view.event_year_max = max_year;
            view.can_go_back = wizard_step > 1
                || session.step > 2
                || (session.step == 2 && session.question_idx > 0);
            if view.step >= 4 {
                let ranked = session.top_results();
                view.can_refine = ranked.len() > 1
                    || ranked
                        .first()
                        .is_some_and(|c| c.to_minute.saturating_sub(c.from_minute) > 20);
                view.candidates = ranked
                    .into_iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let total = c.quiz_score * 2 + c.event_score * 3;
                        JyotishRectifyCandidate {
                            range: RectifySession::format_range(c.from_minute, c.to_minute),
                            rashi_key: RectifySession::candidate_rashi_key(c.lagna_rashi),
                            confidence_pct: c.confidence_pct,
                            quiz_score: c.quiz_score,
                            event_score: c.event_score,
                            total_score: total,
                            is_top: i == 0,
                        }
                    })
                    .collect();
            }
        }
        None => {
            view.step = 1;
            view.can_go_back = false;
            view.event_year_max = max_year;
        }
    }
    view
}

/// Hit produced by [`search_catalog`].
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct JyotishCatalogHit {
    pub instance_id: Uuid,
    pub title: String,
    pub subtitle: String,
    pub score: i32,
    pub day_offset: i32,
}

/// Static keyword catalog matched (case-insensitively) against the query.
///
/// `(keyword, title, subtitle, force_today)` — `force_today` pins the action
/// to `day_offset = 0` regardless of the instance's currently viewed day
/// (used for "today" / Rahu Kalam hits); other keywords keep the instance's
/// current offset so a generic "jyotish" search returns to wherever the
/// widget was left.
const KEYWORDS: &[(&str, &str, &str, bool)] = &[
    ("today", "Jyotish · Today", "Panchanga for today", true),
    ("jyotish", "Jyotish", "Vedic panchanga", false),
    (
        "panchanga",
        "Jyotish · Panchanga",
        "Tithi, nakshatra, yoga, karana",
        false,
    ),
    ("rahu", "Rahu Kalam", "Inauspicious window", true),
    ("rahukalam", "Rahu Kalam", "Inauspicious window", true),
    ("tithi", "Jyotish · Tithi", "Lunar day", false),
    ("nakshatra", "Jyotish · Nakshatra", "Lunar mansion", false),
    ("yoga", "Jyotish · Yoga", "Panchanga yoga", false),
    ("karana", "Jyotish · Karana", "Half-tithi", false),
    ("muhurta", "Jyotish · Muhurta", "Auspicious windows", false),
    ("dasha", "Jyotish · Dasha", "Vimshottari periods", false),
];

/// Score a query against the static keyword catalog.
///
/// Exposed separately from [`search_catalog`] so keyword matching can be
/// unit-tested without a live [`JyotishHandle`].
#[must_use]
pub fn score_keyword_match(query: &str) -> Option<(i32, &'static str, &'static str, bool)> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return None;
    }
    let mut best: Option<(i32, &'static str, &'static str, bool)> = None;
    for (kw, title, subtitle, force_today) in KEYWORDS.iter().copied() {
        let score = if kw == q {
            100
        } else if kw.starts_with(q.as_str()) {
            85
        } else if q.starts_with(kw) || kw.contains(q.as_str()) {
            70
        } else {
            continue;
        };
        if best.is_none_or(|(best_score, ..)| score > best_score) {
            best = Some((score, title, subtitle, force_today));
        }
    }
    best
}

/// Search across live Jyotish instances for keyword or location hits.
#[must_use]
pub fn search_catalog(query: &str, limit: usize) -> Vec<JyotishCatalogHit> {
    let q = query.trim().to_lowercase();
    if q.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for entry in JYOTISH_LIVE.iter() {
        let instance_id = *entry.key();
        let cfg = entry.value().config.read();
        let hit = score_keyword_match(&q).or_else(|| {
            let loc_l = cfg.location_name().to_lowercase();
            (!loc_l.is_empty() && loc_l.contains(&q)).then_some((60, "Jyotish", "Panchanga", false))
        });
        if let Some((score, title, subtitle, force_today)) = hit {
            hits.push(JyotishCatalogHit {
                instance_id,
                title: title.to_string(),
                subtitle: subtitle.to_string(),
                score,
                day_offset: if force_today { 0 } else { cfg.day_offset },
            });
        }
    }
    hits.sort_by_key(|h| std::cmp::Reverse(h.score));
    hits.truncate(limit);
    hits
}

/// Snapshot the live config for the settings dialog.
#[must_use]
pub fn current_config(instance_id: Uuid) -> Option<JyotishConfig> {
    JYOTISH_LIVE
        .get(&instance_id)
        .map(|h| h.config.read().clone())
}

/// Apply a settings-dialog mutation to the live config.
pub fn update_config(instance_id: Uuid, mutate: impl FnOnce(&mut JyotishConfig)) {
    let Some(h) = JYOTISH_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        mutate(&mut cfg);
        cfg.normalize();
    }
    *h.year_cache.write() = None;
    // Keep an open rectify draft aligned with the current place / ayanamsa.
    {
        let cfg = h.config.read().clone();
        if let Some(session) = h.rectify.write().as_mut() {
            session.resync_place(
                cfg.latitude(),
                cfg.longitude(),
                cfg.birth_utc_offset_minutes,
                cfg.ayanamsa,
            );
        }
    }
    h.recalculate();
    h.publish();
}

/// Shift the viewed day by `delta` (−1 / +1) and refresh.
pub fn shift_day(instance_id: Uuid, delta: i32) {
    update_config(instance_id, |cfg| {
        cfg.day_offset = cfg.day_offset.saturating_add(delta);
    });
}

/// Jump back to today.
pub fn go_today(instance_id: Uuid) {
    update_config(instance_id, |cfg| {
        cfg.day_offset = 0;
    });
}

/// Select the active tab (0=day, 1=month, 2=year, 3=life retrospective).
pub fn select_tab(instance_id: Uuid, tab: u8) {
    update_config(instance_id, |cfg| {
        cfg.active_tab = tab;
    });
}

/// Jump directly to an absolute day offset (e.g. a week-strip or month-grid
/// tap) and switch to the day tab.
pub fn set_day_offset(instance_id: Uuid, offset: i32) {
    update_config(instance_id, |cfg| {
        cfg.day_offset = offset;
        cfg.active_tab = 0;
    });
}

/// Step the month grid by `delta` months.
pub fn month_nav(instance_id: Uuid, delta: i32) {
    update_config(instance_id, |cfg| {
        cfg.month_offset = cfg.month_offset.saturating_add(delta);
    });
}

/// Jump directly to an absolute month offset (e.g. from the year view) and
/// switch to the month tab.
pub fn set_month_offset(instance_id: Uuid, offset: i32) {
    update_config(instance_id, |cfg| {
        cfg.month_offset = offset;
        cfg.active_tab = 1;
    });
}

/// Step the year view by `delta` years.
pub fn year_nav(instance_id: Uuid, delta: i32) {
    update_config(instance_id, |cfg| {
        cfg.year_offset = cfg.year_offset.saturating_add(delta);
    });
}

/// Jump directly to an absolute year offset and switch to the year tab.
pub fn set_year_offset(instance_id: Uuid, offset: i32) {
    update_config(instance_id, |cfg| {
        cfg.year_offset = offset;
        cfg.active_tab = 2;
    });
}

/// Toggle Life-tab expansion for an absolute civil year (antar list).
pub fn select_life_year(instance_id: Uuid, year: i32) {
    if let Some(h) = JYOTISH_LIVE.get(&instance_id) {
        h.select_life_year(year);
    }
}

/// Open / close the location picker overlay.
pub fn set_picker_open(instance_id: Uuid, open: bool) {
    if let Some(h) = JYOTISH_LIVE.get(&instance_id) {
        h.set_picker_open(open);
    }
}

/// Select which configured location is shown.
pub fn select_city(instance_id: Uuid, index: usize) {
    if let Some(h) = JYOTISH_LIVE.get(&instance_id) {
        h.select_city(index);
    }
}

/// Remove a configured location (keeps at least one).
pub fn remove_city(instance_id: Uuid, index: usize) {
    if let Some(h) = JYOTISH_LIVE.get(&instance_id) {
        h.remove_city(index);
    }
}

/// Update the location-search query and kick off geocoding.
pub fn search_cities(instance_id: Uuid, query: String) {
    if let Some(h) = JYOTISH_LIVE.get(&instance_id) {
        h.search_cities(query);
    }
}

/// Add a location from a geocoding hit and make it active.
pub fn add_city(instance_id: Uuid, name: String, latitude: f64, longitude: f64) {
    if let Some(h) = JYOTISH_LIVE.get(&instance_id) {
        h.add_city(JyotishLocation {
            name,
            latitude,
            longitude,
        });
    }
}

/// Open the rectification wizard at the window-picker step.
pub fn rectify_start(instance_id: Uuid) {
    if let Some(h) = JYOTISH_LIVE.get(&instance_id) {
        h.rectify_start();
    }
}

/// Choose the uncertainty window and start the quiz session.
///
/// `half_window < 0` means "unknown time of day" (full day of candidates).
pub fn rectify_set_window(instance_id: Uuid, approx_minute: i32, half_window: i32) {
    if let Some(h) = JYOTISH_LIVE.get(&instance_id) {
        h.rectify_set_window(approx_minute, half_window);
    }
}

/// Answer quiz question `question_idx` with option `option_idx`.
pub fn rectify_answer(instance_id: Uuid, question_idx: usize, option_idx: usize) {
    if let Some(h) = JYOTISH_LIVE.get(&instance_id) {
        h.rectify_answer(question_idx, option_idx);
    }
}

/// Record a life event by [`EventKind`] index (see [`EventKind::all`]) and
/// calendar year.
pub fn rectify_add_event(instance_id: Uuid, kind_idx: usize, year: i32) {
    if let Some(h) = JYOTISH_LIVE.get(&instance_id) {
        h.rectify_add_event(kind_idx, year);
    }
}

/// Remove a recorded life event by index.
pub fn rectify_remove_event(instance_id: Uuid, idx: usize) {
    if let Some(h) = JYOTISH_LIVE.get(&instance_id) {
        h.rectify_remove_event(idx);
    }
}

/// Advance the rectification wizard to its next step.
pub fn rectify_next_step(instance_id: Uuid) {
    if let Some(h) = JYOTISH_LIVE.get(&instance_id) {
        h.rectify_next_step();
    }
}

/// Step backward in the rectification wizard.
pub fn rectify_back(instance_id: Uuid) {
    if let Some(h) = JYOTISH_LIVE.get(&instance_id) {
        h.rectify_back();
    }
}

/// Narrow the candidate window around the current top result.
pub fn rectify_refine(instance_id: Uuid) {
    if let Some(h) = JYOTISH_LIVE.get(&instance_id) {
        h.rectify_refine();
    }
}

/// Accept the top-ranked candidate as the rectified birth time.
pub fn rectify_accept(instance_id: Uuid) {
    if let Some(h) = JYOTISH_LIVE.get(&instance_id) {
        h.rectify_accept();
    }
}

/// Close the rectification overlay, keeping a draft session.
pub fn rectify_cancel(instance_id: Uuid) {
    if let Some(h) = JYOTISH_LIVE.get(&instance_id) {
        h.rectify_cancel();
    }
}

/// Discard a draft rectification session entirely.
pub fn rectify_discard_draft(instance_id: Uuid) {
    if let Some(h) = JYOTISH_LIVE.get(&instance_id) {
        h.rectify_discard_draft();
    }
}

/// Jyotish widget implementation.
pub struct JyotishWidget {
    instance_id: Uuid,
    handle: Arc<JyotishHandle>,
    orchid_config: Arc<RwLock<orchid_storage::OrchidConfig>>,
    refresh: PeriodicRefresh,
}

impl std::fmt::Debug for JyotishWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JyotishWidget")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl JyotishWidget {
    /// Construct with config.
    pub fn new(
        instance_id: Uuid,
        mut config: JyotishConfig,
        provider: Arc<dyn WeatherProvider>,
        bus: Arc<orchid_core::EventBus>,
        orchid_config: Arc<RwLock<orchid_storage::OrchidConfig>>,
    ) -> Self {
        config.normalize();
        let handle = Arc::new(JyotishHandle {
            instance_id,
            config: Arc::new(RwLock::new(config)),
            provider,
            ui: Arc::new(RwLock::new(JyotishUiState::default())),
            data: Arc::new(RwLock::new(None)),
            natal: Arc::new(RwLock::new(None)),
            natal_fingerprint: RwLock::new(None),
            color_cache: DashMap::new(),
            year_cache: RwLock::new(None),
            rectify: RwLock::new(None),
            rectify_wizard_step: RwLock::new(0),
            life_detail_year: RwLock::new(None),
            bus,
        });
        JYOTISH_LIVE.insert(instance_id, Arc::clone(&handle));
        Self {
            instance_id,
            handle,
            orchid_config,
            refresh: PeriodicRefresh::new(Duration::from_secs(10 * 60)),
        }
    }

    fn recalculate(&self) {
        self.handle.recalculate();
    }
}

#[async_trait]
impl Widget for JyotishWidget {
    fn type_id(&self) -> &'static str {
        TYPE_ID
    }

    fn instance_id(&self) -> Uuid {
        self.instance_id
    }

    async fn on_create(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.recalculate();
        Ok(())
    }

    async fn on_activate(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        let handle = Arc::clone(&self.handle);
        self.refresh.start(move || {
            let handle = Arc::clone(&handle);
            async move {
                handle.recalculate();
                handle.publish();
            }
        });
        Ok(())
    }

    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.refresh.stop();
        Ok(())
    }

    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.refresh.stop();
        Ok(())
    }

    async fn on_close(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.refresh.stop();
        JYOTISH_LIVE.remove(&self.instance_id);
        Ok(())
    }

    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }

    fn snapshot(&self) -> Option<WidgetSnapshot> {
        let cfg = self.handle.config.read().clone();
        let locale = self.orchid_config.read().locale.clone();
        let has_data = self.handle.data.read().is_some();
        let payload = if has_data {
            self.handle.render_payload(&locale)
        } else {
            let ui = self.handle.ui.read().clone();
            loading_payload(&cfg, &ui)
        };
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title: cfg.location_name().to_string(),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::Jyotish(Box::new(payload)),
        })
    }

    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        let cfg = self.handle.config.read().clone();
        state_codec::save_state(&cfg)
    }

    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let cfg = decode_config(bytes)?;
        *self.handle.config.write() = cfg;
        Ok(())
    }

    fn capabilities(&self) -> WidgetCapabilities {
        WidgetCapabilities {
            supports_resize: true,
            min_size: Some(WidgetSize::Small),
            max_size: None,
            preferred_size: Some(WidgetSize::Medium),
            allows_grouping: true,
            keeps_state_when_unloaded: true,
            has_settings_panel: true,
        }
    }
}

fn loading_payload(cfg: &JyotishConfig, ui: &JyotishUiState) -> JyotishPayload {
    let (cities, search_results) = cities_and_search(cfg, ui);
    JyotishPayload {
        date_text: String::new(),
        location_name: cfg.location_name().to_string(),
        cities,
        active_city_index: cfg.active_index,
        picker_open: ui.picker_open,
        search_query: ui.search_query.clone(),
        search_results,
        search_busy: ui.search_busy,
        ayanamsa_key: cfg.ayanamsa.ftl_key(),
        ayanamsa_deg_text: String::new(),
        day_offset: cfg.day_offset,
        is_today: cfg.day_offset == 0,
        tithi_key: "jyotish-tithi-pratipada",
        paksha_key: "jyotish-paksha-shukla",
        tithi_end_text: None,
        nakshatra_key: "jyotish-nakshatra-ashwini",
        pada: 1,
        nakshatra_end_text: None,
        yoga_key: "jyotish-yoga-vishkambha",
        yoga_end_text: None,
        karana_key: "jyotish-karana-bava",
        karana_end_text: None,
        vara_key: "jyotish-vara-ravi",
        sunrise_time: None,
        sunset_time: None,
        rahukalam_text: None,
        yamagandam_text: None,
        gulika_text: None,
        in_rahukalam: false,
        planets: Vec::new(),
        show_planets: cfg.show_planets,
        is_loading: true,
        active_tab: cfg.active_tab,
        score_color: 0,
        now_score_color: 0,
        day_score_color: 0,
        score_value: 0,
        factors: Vec::new(),
        personal_mode: false,
        headline_key: "",
        influence_keys: Vec::new(),
        advice_keys: Vec::new(),
        week_strip: Vec::new(),
        month_key: "jyotish-month-1",
        month_year: 0,
        month_cells: Vec::new(),
        month_first_weekday: 0,
        month_green: 0,
        month_yellow: 0,
        month_red: 0,
        year_value: 0,
        year_months: Vec::new(),
        life_years: Vec::new(),
        life_detail_year: 0,
        life_antars: Vec::new(),
        has_dasha_now: false,
        dasha_now: JyotishDashaNow::default(),
        gochara_note_key: "",
        has_birth_data: cfg.has_birth_data(),
        rectify: JyotishRectifyView::default(),
    }
}

/// Descriptor ready to register on a widget registry.
#[must_use]
pub fn descriptor(http_client: reqwest::Client) -> WidgetDescriptor {
    let provider: Arc<dyn WeatherProvider> = Arc::new(OpenMeteoProvider::new(http_client));
    let factory: WidgetFactory = Arc::new(move |ctx: WidgetContext, state_bytes| {
        let cfg = match state_bytes {
            Some(bytes) => decode_config(bytes).unwrap_or_default(),
            None => JyotishConfig::default(),
        };
        Ok(Box::new(JyotishWidget::new(
            ctx.instance_id,
            cfg,
            provider.clone(),
            ctx.bus.clone(),
            ctx.config.clone(),
        )) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-jyotish-name",
        description_key: "widget-jyotish-desc",
        icon_name: "jyotish",
        category: WidgetCategory::Astronomy,
        default_size: WidgetSize::Medium,
        min_size: Some(WidgetSize::Small),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: true,
        factory,
    }
}

#[cfg(test)]
mod search_tests {
    use super::*;

    #[test]
    fn score_keyword_match_exact_beats_prefix() {
        let (score, title, _subtitle, force_today) =
            score_keyword_match("rahukalam").expect("keyword hit");
        assert_eq!(title, "Rahu Kalam");
        assert!(force_today);
        assert_eq!(score, 100);
    }

    #[test]
    fn score_keyword_match_prefix_hit() {
        let (_score, title, _subtitle, force_today) =
            score_keyword_match("tith").expect("prefix hit");
        assert_eq!(title, "Jyotish · Tithi");
        assert!(!force_today);
    }

    #[test]
    fn score_keyword_match_no_hit_for_unrelated_query() {
        assert!(score_keyword_match("xyzzy").is_none());
        assert!(score_keyword_match("").is_none());
    }

    #[test]
    fn score_keyword_match_today_forces_day_zero() {
        let (_score, _title, _subtitle, force_today) =
            score_keyword_match("today").expect("today hit");
        assert!(force_today);
    }

    fn test_bus() -> Arc<orchid_core::EventBus> {
        Arc::new(orchid_core::EventBus::new(
            orchid_core::EventBusConfig::default(),
        ))
    }

    fn test_provider() -> Arc<dyn WeatherProvider> {
        Arc::new(OpenMeteoProvider::new(reqwest::Client::new()))
    }

    // `JYOTISH_LIVE` is a crate-wide static, so other tests' instances may
    // still be registered when these run in parallel — assert on the
    // instance under test rather than the total hit count.
    #[test]
    fn search_catalog_matches_keyword_and_respects_day_offset() {
        let bus = test_bus();
        let orchid_config = Arc::new(RwLock::new(orchid_storage::OrchidConfig::default()));
        let id = Uuid::new_v4();
        let cfg = JyotishConfig {
            day_offset: 5,
            ..JyotishConfig::default()
        };
        let _widget = JyotishWidget::new(id, cfg, test_provider(), bus, orchid_config);

        let hits = search_catalog("nakshatra", 50);
        let hit = hits
            .iter()
            .find(|h| h.instance_id == id)
            .expect("hit for this instance");
        // Generic keyword hit keeps the instance's current offset.
        assert_eq!(hit.day_offset, 5);

        let today_hits = search_catalog("rahu", 50);
        let today_hit = today_hits
            .iter()
            .find(|h| h.instance_id == id)
            .expect("rahu hit for this instance");
        assert_eq!(today_hit.day_offset, 0);
    }

    #[test]
    fn search_catalog_matches_location_name() {
        let bus = test_bus();
        let orchid_config = Arc::new(RwLock::new(orchid_storage::OrchidConfig::default()));
        let id = Uuid::new_v4();
        let cfg = JyotishConfig {
            locations: vec![JyotishLocation {
                name: "Zzyzxville".into(),
                ..JyotishLocation::default()
            }],
            ..JyotishConfig::default()
        };
        let _widget = JyotishWidget::new(id, cfg, test_provider(), bus, orchid_config);

        let hits = search_catalog("zzyzxville", 50);
        assert!(hits.iter().any(|h| h.instance_id == id));
    }

    #[test]
    fn search_catalog_empty_query_returns_nothing() {
        assert!(search_catalog("", 10).is_empty());
        assert!(search_catalog("jyotish", 0).is_empty());
    }

    #[test]
    fn month_color_cache_reuses_entries_on_second_build() {
        let bus = test_bus();
        let orchid_config = Arc::new(RwLock::new(orchid_storage::OrchidConfig::default()));
        let id = Uuid::new_v4();
        let _widget = JyotishWidget::new(
            id,
            JyotishConfig::default(),
            test_provider(),
            bus,
            orchid_config,
        );
        let h = JYOTISH_LIVE.get(&id).expect("live handle");
        let cfg = h.config.read().clone();
        let today = Utc::now().date_naive();
        h.color_cache.clear();
        assert_eq!(h.color_cache.len(), 0);
        let _ = h.build_month(&cfg, today);
        let filled = h.color_cache.len();
        assert!(
            filled >= 28,
            "expected a full month of colors, got {filled}"
        );
        let _ = h.build_month(&cfg, today);
        assert_eq!(
            h.color_cache.len(),
            filled,
            "second month build must reuse the color cache"
        );
    }

    #[test]
    fn jyotish_payload_struct_stays_bounded() {
        // Heap data (Vec/String) is separate; keep the stack-shaped payload
        // from growing unchecked so snapshots stay cheap to move when Boxed.
        assert!(
            std::mem::size_of::<JyotishPayload>() < 2048,
            "JyotishPayload is {} bytes",
            std::mem::size_of::<JyotishPayload>()
        );
    }

    #[test]
    fn add_city_dedups_by_rounded_location_and_switches_active() {
        let bus = test_bus();
        let orchid_config = Arc::new(RwLock::new(orchid_storage::OrchidConfig::default()));
        let id = Uuid::new_v4();
        let _widget = JyotishWidget::new(
            id,
            JyotishConfig::default(),
            test_provider(),
            bus,
            orchid_config,
        );
        let h = JYOTISH_LIVE.get(&id).expect("live handle");

        h.add_city(JyotishLocation {
            name: "Ujjain".into(),
            latitude: 23.1765,
            longitude: 75.7885,
        });
        assert_eq!(h.config.read().locations.len(), 2);
        assert_eq!(h.config.read().active_index, 1);

        // Re-adding the same (rounded) coordinates should not duplicate —
        // just re-select the existing entry.
        h.add_city(JyotishLocation {
            name: "Varanasi".into(),
            latitude: 25.3176,
            longitude: 82.9739,
        });
        assert_eq!(h.config.read().locations.len(), 2);
        assert_eq!(h.config.read().active_index, 0);
    }

    #[test]
    fn select_city_switches_active_location() {
        let bus = test_bus();
        let orchid_config = Arc::new(RwLock::new(orchid_storage::OrchidConfig::default()));
        let id = Uuid::new_v4();
        let _widget = JyotishWidget::new(
            id,
            JyotishConfig::default(),
            test_provider(),
            bus,
            orchid_config,
        );
        let h = JYOTISH_LIVE.get(&id).expect("live handle");
        h.add_city(JyotishLocation {
            name: "Ujjain".into(),
            latitude: 23.1765,
            longitude: 75.7885,
        });

        h.select_city(0);
        assert_eq!(h.config.read().active_index, 0);
        assert_eq!(h.config.read().location_name(), "Varanasi");
    }

    #[test]
    fn remove_city_keeps_at_least_one_location() {
        let bus = test_bus();
        let orchid_config = Arc::new(RwLock::new(orchid_storage::OrchidConfig::default()));
        let id = Uuid::new_v4();
        let _widget = JyotishWidget::new(
            id,
            JyotishConfig::default(),
            test_provider(),
            bus,
            orchid_config,
        );
        let h = JYOTISH_LIVE.get(&id).expect("live handle");

        // Only one location — removal is a no-op.
        h.remove_city(0);
        assert_eq!(h.config.read().locations.len(), 1);

        h.add_city(JyotishLocation {
            name: "Ujjain".into(),
            latitude: 23.1765,
            longitude: 75.7885,
        });
        assert_eq!(h.config.read().locations.len(), 2);
        h.remove_city(0);
        assert_eq!(h.config.read().locations.len(), 1);
        assert_eq!(h.config.read().location_name(), "Ujjain");
        // Cannot go below one location.
        h.remove_city(0);
        assert_eq!(h.config.read().locations.len(), 1);
    }

    #[test]
    fn location_change_invalidates_day_color_cache() {
        let bus = test_bus();
        let orchid_config = Arc::new(RwLock::new(orchid_storage::OrchidConfig::default()));
        let id = Uuid::new_v4();
        let _widget = JyotishWidget::new(
            id,
            JyotishConfig::default(),
            test_provider(),
            bus,
            orchid_config,
        );
        let h = JYOTISH_LIVE.get(&id).expect("live handle");
        let cfg = h.config.read().clone();
        let today = Utc::now().date_naive();
        let _ = h.build_month(&cfg, today);
        assert!(!h.color_cache.is_empty());

        h.add_city(JyotishLocation {
            name: "Ujjain".into(),
            latitude: 23.1765,
            longitude: 75.7885,
        });
        assert!(
            h.color_cache.is_empty(),
            "changing the active location must invalidate cached day colors"
        );
    }
}
