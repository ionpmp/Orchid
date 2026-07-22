//! Jyotish widget — Vedic panchanga from local astronomical calculations.

pub mod astro;
pub mod config;
pub mod dasha;
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
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::{
    JyotishDayChip, JyotishMonthCell, JyotishMonthSummary, JyotishPayload, JyotishPlanetRow,
    JyotishRectifyView, JyotishYearSummary,
};
use crate::widget::refresh::PeriodicRefresh;
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};
use orchid_storage::{LifecycleState, LocaleConfig, WidgetSize};

pub use astro::{compute_jyotish, JyotishData};
pub use config::{AyanamsaSystem, JyotishConfig};
pub use dasha::{antar_dashas, dasha_at, maha_dashas, DashaLord, DashaPeriod};
pub use muhurta::{day_muhurtas, in_window, DayMuhurtas, MuhurtaWindow};
pub use narrative::{build_narrative, Narrative};
pub use rectify::{Candidate, EventKind, LifeEvent, RectifySession};
pub use score::{
    compute_day_score, compute_natal, local_noon_utc, DayColor, DayScore, Factor, NatalInfo,
};
pub use transitions::{next_transition, panchanga_ends, Limb, PanchangaEnds};

/// Stable type id.
pub const TYPE_ID: &str = "jyotish";

static JYOTISH_LIVE: LazyLock<DashMap<Uuid, Arc<JyotishHandle>>> = LazyLock::new(DashMap::new);

/// Config-derived fingerprint used to decide when the day-color cache must
/// be invalidated (birth date/time/offset or ayanamsa changed).
type NatalFingerprint = (Option<String>, Option<String>, i32, AyanamsaSystem);

struct JyotishHandle {
    instance_id: Uuid,
    config: Arc<RwLock<JyotishConfig>>,
    data: Arc<RwLock<Option<JyotishData>>>,
    natal: Arc<RwLock<Option<NatalInfo>>>,
    natal_fingerprint: RwLock<Option<NatalFingerprint>>,
    color_cache: DashMap<NaiveDate, u8>,
    rectify: RwLock<Option<RectifySession>>,
    rectify_wizard_step: RwLock<u8>,
    bus: Arc<orchid_core::EventBus>,
}

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
        let data = compute_jyotish(cfg.latitude, cfg.longitude, at, cfg.ayanamsa);
        *self.data.write() = Some(data);

        let fingerprint: NatalFingerprint = (
            cfg.birth_date.clone(),
            cfg.birth_time.clone(),
            cfg.birth_utc_offset_minutes,
            cfg.ayanamsa,
        );
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
        }

        let natal = birth_datetime_utc(&cfg).map(|dt| compute_natal(dt, cfg.ayanamsa));
        *self.natal.write() = natal;
    }

    /// Auspiciousness color (0=green, 1=yellow, 2=red) for `date`, memoized.
    fn day_color(&self, date: NaiveDate, cfg: &JyotishConfig, natal: Option<&NatalInfo>) -> u8 {
        if let Some(c) = self.color_cache.get(&date) {
            return *c;
        }
        let at = local_noon_utc(date, cfg.longitude);
        let score = compute_day_score(at, cfg.ayanamsa, natal);
        let color = color_of(score.color);
        self.color_cache.insert(date, color);
        color
    }

    fn build_week_strip(&self, cfg: &JyotishConfig, today: NaiveDate) -> Vec<JyotishDayChip> {
        let natal = *self.natal.read();
        (cfg.day_offset - 3..=cfg.day_offset + 3)
            .map(|offset| {
                let date = today + ChronoDuration::days(i64::from(offset));
                JyotishDayChip {
                    weekday_key: weekday_key(date.weekday()),
                    day_num: date.day() as u8,
                    color: self.day_color(date, cfg, natal.as_ref()),
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
                let color = self.day_color(date, cfg, natal.as_ref());
                match color {
                    0 => green += 1,
                    1 => yellow += 1,
                    _ => red += 1,
                }
                JyotishMonthCell {
                    day: day as u8,
                    color,
                    is_today: date == today,
                    offset: (date - today).num_days() as i32,
                }
            })
            .collect();

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

    fn build_year(&self, cfg: &JyotishConfig, today: NaiveDate) -> (i32, Vec<JyotishMonthSummary>) {
        let natal = *self.natal.read();
        let year = today.year() + cfg.year_offset;
        let months = (1..=12u32)
            .map(|month| {
                let days_in = days_in_month(year, month);
                let mut green = 0u16;
                let mut yellow = 0u16;
                let mut red = 0u16;
                for day in 1..=days_in {
                    if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
                        match self.day_color(date, cfg, natal.as_ref()) {
                            0 => green += 1,
                            1 => yellow += 1,
                            _ => red += 1,
                        }
                    }
                }
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
        (year, months)
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
                let dasha_key = NaiveDate::from_ymd_opt(year, 6, 15)
                    .and_then(|d| dasha_at(natal.moon_longitude, birth_date, d))
                    .map(|(maha, _)| maha.ftl_key())
                    .unwrap_or("");
                JyotishYearSummary {
                    year,
                    green: green * 30,
                    yellow: yellow * 30,
                    red: red * 30,
                    dasha_key,
                    year_offset: year - current_year,
                }
            })
            .collect()
    }

    fn render_payload(&self, locale: &LocaleConfig) -> JyotishPayload {
        let cfg = self.config.read().clone();
        let Some(data) = self.data.read().clone() else {
            return loading_payload(&cfg);
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
        let noon_at = local_noon_utc(selected_date, cfg.longitude);
        let day_score = compute_day_score(noon_at, cfg.ayanamsa, natal.as_ref());
        // Instantaneous score: live clock on "today", otherwise noon of the
        // selected civil day (same sample as the day score).
        let now_at = if cfg.day_offset == 0 { at } else { noon_at };
        let now_score = compute_day_score(now_at, cfg.ayanamsa, natal.as_ref());
        let primary_score = if cfg.day_offset == 0 {
            &now_score
        } else {
            &day_score
        };
        let narrative = build_narrative(primary_score);

        let ends = transitions::panchanga_ends(at, cfg.ayanamsa);
        let fmt_end = |t: Option<DateTime<Utc>>| t.map(fmt_time);

        let muhurtas = match (data.sunrise, data.sunset) {
            (Some(rise), Some(set)) => day_muhurtas(rise, set, data.vara_index),
            _ => None,
        };
        let fmt_range =
            |w: muhurta::MuhurtaWindow| format!("{}–{}", fmt_time(w.start), fmt_time(w.end));
        let (rahukalam_text, yamagandam_text, gulika_text, in_rahukalam) = if let Some(m) = muhurtas
        {
            (
                Some(fmt_range(m.rahukalam)),
                Some(fmt_range(m.yamagandam)),
                Some(fmt_range(m.gulika)),
                in_window(at, m.rahukalam),
            )
        } else {
            (None, None, None, false)
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
        let (year_value, year_months) = self.build_year(&cfg, today);
        let life_years = natal
            .as_ref()
            .map(|n| self.build_life(&cfg, n, today))
            .unwrap_or_default();

        let wizard_step = *self.rectify_wizard_step.read();
        let rectify_view = {
            let guard = self.rectify.read();
            build_rectify_view(wizard_step, guard.as_ref())
        };

        JyotishPayload {
            date_text,
            location_name: cfg.location_name.clone(),
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
            has_birth_data: cfg.has_birth_data(),
            rectify: rectify_view,
        }
    }

    fn rectify_start(&self) {
        *self.rectify.write() = None;
        *self.rectify_wizard_step.write() = 1;
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
            cfg.latitude,
            cfg.longitude,
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
        }
        self.publish();
    }

    fn rectify_add_event(&self, kind_idx: usize, year: i32) {
        if let Some(kind) = EventKind::all().get(kind_idx).copied() {
            if let Some(session) = self.rectify.write().as_mut() {
                session.add_event(LifeEvent { kind, year });
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
        *self.rectify.write() = None;
        *self.rectify_wizard_step.write() = 0;
        self.publish();
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
    let mut view = JyotishRectifyView::default();
    if wizard_step == 0 {
        return view;
    }
    view.event_kind_keys = EventKind::all().iter().map(|k| k.ftl_key()).collect();
    match session {
        Some(session) => {
            view.step = session.step;
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
            if session.step >= 4 {
                view.candidates = session
                    .results()
                    .into_iter()
                    .map(|c| {
                        (
                            RectifySession::format_range(c.from_minute, c.to_minute),
                            RectifySession::candidate_rashi_key(c.lagna_rashi),
                            c.confidence_pct,
                        )
                    })
                    .collect();
            }
        }
        None => {
            view.step = 1;
        }
    }
    view
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

/// Jump directly to an absolute year offset (e.g. from the life
/// retrospective) and switch to the year tab.
pub fn set_year_offset(instance_id: Uuid, offset: i32) {
    update_config(instance_id, |cfg| {
        cfg.year_offset = offset;
        cfg.active_tab = 2;
    });
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

/// Accept the top-ranked candidate as the rectified birth time.
pub fn rectify_accept(instance_id: Uuid) {
    if let Some(h) = JYOTISH_LIVE.get(&instance_id) {
        h.rectify_accept();
    }
}

/// Cancel the rectification wizard without changing the birth time.
pub fn rectify_cancel(instance_id: Uuid) {
    if let Some(h) = JYOTISH_LIVE.get(&instance_id) {
        h.rectify_cancel();
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
        bus: Arc<orchid_core::EventBus>,
        orchid_config: Arc<RwLock<orchid_storage::OrchidConfig>>,
    ) -> Self {
        config.normalize();
        let handle = Arc::new(JyotishHandle {
            instance_id,
            config: Arc::new(RwLock::new(config)),
            data: Arc::new(RwLock::new(None)),
            natal: Arc::new(RwLock::new(None)),
            natal_fingerprint: RwLock::new(None),
            color_cache: DashMap::new(),
            rectify: RwLock::new(None),
            rectify_wizard_step: RwLock::new(0),
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
            loading_payload(&cfg)
        };
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title: cfg.location_name.clone(),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::Jyotish(Box::new(payload)),
        })
    }

    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        let cfg = self.handle.config.read().clone();
        state_codec::save_state(&cfg)
    }

    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let mut cfg: JyotishConfig = state_codec::restore_state(bytes)?;
        cfg.normalize();
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

fn loading_payload(cfg: &JyotishConfig) -> JyotishPayload {
    JyotishPayload {
        date_text: String::new(),
        location_name: cfg.location_name.clone(),
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
        has_birth_data: cfg.has_birth_data(),
        rectify: JyotishRectifyView::default(),
    }
}

/// Descriptor ready to register on a widget registry.
#[must_use]
pub fn descriptor() -> WidgetDescriptor {
    let factory: WidgetFactory = Arc::new(|ctx: WidgetContext, state_bytes| {
        let cfg = match state_bytes {
            Some(bytes) => state_codec::restore_state::<JyotishConfig>(bytes).unwrap_or_default(),
            None => JyotishConfig::default(),
        };
        Ok(Box::new(JyotishWidget::new(
            ctx.instance_id,
            cfg,
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
