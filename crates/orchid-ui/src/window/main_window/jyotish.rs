//! Jyotish widget handlers for [`MainWindowController`].

use std::sync::Arc;

use slint::SharedString;
use tracing::warn;
use uuid::Uuid;

use orchid_i18n::LocaleManager;
use orchid_widgets::{JyotishPayload, WidgetPayload};

use crate::window::spawn;

use super::MainWindowController;

/// Per-instance notification edge-tracking for the Jyotish widget.
///
/// `seen` guards the very first observation of an instance so restoring a
/// workspace (or opening a new Jyotish widget) never fires a notification
/// purely from seeding the cached state.
#[derive(Debug, Clone, Copy)]
pub(super) struct JyotishNotifyState {
    seen: bool,
    in_rahukalam: bool,
    day_score_color: u8,
}

impl MainWindowController {
    /// Walk live Jyotish snapshots and push notifications on Rahu Kalam
    /// rising edges / today's day-color changes, per-instance toggles
    /// permitting. Called once per workspace rebuild.
    pub(super) fn sync_jyotish_notifications(self: &Arc<Self>) {
        let mut pending: Vec<(String, String, i32)> = Vec::new();
        {
            let mut state_map = self.jyotish_notify_state.lock();
            let jyotish_instances: Vec<Uuid> = self
                .widget_manager
                .list_instances()
                .into_iter()
                .filter(|inst| inst.type_id == "jyotish")
                .map(|inst| inst.id)
                .collect();
            state_map.retain(|id, _| jyotish_instances.contains(id));

            for inst_id in jyotish_instances {
                let Some(snap) = self.widget_manager.snapshot_cache().get(inst_id) else {
                    continue;
                };
                let WidgetPayload::Jyotish(payload) = &snap.payload else {
                    continue;
                };
                if payload.is_loading {
                    continue;
                }
                let (notify_day_color, notify_rahukalam) =
                    orchid_widgets::builtin::jyotish::current_config(inst_id)
                        .map(|cfg| (cfg.notify_day_color, cfg.notify_rahukalam))
                        .unwrap_or((true, true));

                let entry = state_map
                    .entry(inst_id)
                    .or_insert_with(|| JyotishNotifyState {
                        seen: false,
                        in_rahukalam: payload.in_rahukalam,
                        day_score_color: payload.day_score_color,
                    });

                if !entry.seen {
                    entry.seen = true;
                    entry.in_rahukalam = payload.in_rahukalam;
                    entry.day_score_color = payload.day_score_color;
                    continue;
                }

                if notify_rahukalam
                    && payload.is_today
                    && payload.in_rahukalam
                    && !entry.in_rahukalam
                {
                    pending.push((
                        self.locale.tr("widget-jyotish-name"),
                        self.locale.tr("jyotish-notify-rahukalam"),
                        2,
                    ));
                }
                entry.in_rahukalam = payload.in_rahukalam;

                if notify_day_color
                    && payload.is_today
                    && payload.day_score_color != entry.day_score_color
                {
                    let (body_key, severity) = match payload.day_score_color {
                        0 => ("jyotish-notify-day-green", 0),
                        1 => ("jyotish-notify-day-yellow", 1),
                        _ => ("jyotish-notify-day-red", 2),
                    };
                    pending.push((
                        self.locale.tr("widget-jyotish-name"),
                        self.locale.tr(body_key),
                        severity,
                    ));
                }
                entry.day_score_color = payload.day_score_color;
            }
        }
        for (title, body, severity) in pending {
            self.push_notification(&title, &body, severity);
        }
    }
}

impl MainWindowController {
    fn parse_jyotish_id(id: &SharedString) -> Option<Uuid> {
        Uuid::parse_str(id.as_str()).ok()
    }

    pub(super) fn on_jyotish_prev_day(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::shift_day(inst, -1);
    }

    pub(super) fn on_jyotish_next_day(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::shift_day(inst, 1);
    }

    pub(super) fn on_jyotish_go_today(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::go_today(inst);
    }

    pub(super) fn on_jyotish_select_tab(self: &Arc<Self>, id: &SharedString, tab: i32) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::select_tab(inst, tab.clamp(0, 3) as u8);
    }

    pub(super) fn on_jyotish_select_offset(self: &Arc<Self>, id: &SharedString, offset: i32) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::set_day_offset(inst, offset);
    }

    pub(super) fn on_jyotish_month_nav(self: &Arc<Self>, id: &SharedString, delta: i32) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::month_nav(inst, delta);
    }

    pub(super) fn on_jyotish_year_nav(self: &Arc<Self>, id: &SharedString, delta: i32) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::year_nav(inst, delta);
    }

    pub(super) fn on_jyotish_open_month(self: &Arc<Self>, id: &SharedString, offset: i32) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::set_month_offset(inst, offset);
    }

    pub(super) fn on_jyotish_open_year(self: &Arc<Self>, id: &SharedString, offset: i32) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::set_year_offset(inst, offset);
    }

    pub(super) fn on_jyotish_select_life_year(self: &Arc<Self>, id: &SharedString, year: i32) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::select_life_year(inst, year);
    }

    pub(super) fn on_jyotish_rectify_start(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::rectify_start(inst);
    }

    pub(super) fn on_jyotish_rectify_set_window(
        self: &Arc<Self>,
        id: &SharedString,
        approx_minute: i32,
        half_window: i32,
    ) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::rectify_set_window(inst, approx_minute, half_window);
    }

    pub(super) fn on_jyotish_rectify_answer(
        self: &Arc<Self>,
        id: &SharedString,
        question_idx: i32,
        option_idx: i32,
    ) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        if question_idx < 0 || option_idx < 0 {
            return;
        }
        orchid_widgets::builtin::jyotish::rectify_answer(
            inst,
            question_idx as usize,
            option_idx as usize,
        );
    }

    pub(super) fn on_jyotish_rectify_add_event(
        self: &Arc<Self>,
        id: &SharedString,
        kind_idx: i32,
        year: i32,
    ) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        if kind_idx < 0 {
            return;
        }
        orchid_widgets::builtin::jyotish::rectify_add_event(inst, kind_idx as usize, year);
    }

    pub(super) fn on_jyotish_rectify_remove_event(self: &Arc<Self>, id: &SharedString, idx: i32) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        if idx < 0 {
            return;
        }
        orchid_widgets::builtin::jyotish::rectify_remove_event(inst, idx as usize);
    }

    pub(super) fn on_jyotish_rectify_next(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::rectify_next_step(inst);
    }

    pub(super) fn on_jyotish_rectify_back(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::rectify_back(inst);
    }

    pub(super) fn on_jyotish_rectify_refine(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::rectify_refine(inst);
    }

    pub(super) fn on_jyotish_rectify_accept(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::rectify_accept(inst);
    }

    pub(super) fn on_jyotish_rectify_cancel(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::rectify_cancel(inst);
    }

    pub(super) fn on_jyotish_export_day(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        self.export_jyotish_to_clipboard(inst, false);
    }

    pub(super) fn on_jyotish_export_week(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        self.export_jyotish_to_clipboard(inst, true);
    }

    pub(super) fn on_jyotish_open_cities(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::set_picker_open(inst, true);
        self.refresh_jyotish(inst);
    }

    pub(super) fn on_jyotish_close_cities(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::set_picker_open(inst, false);
        self.refresh_jyotish(inst);
    }

    pub(super) fn on_jyotish_select_city(self: &Arc<Self>, id: &SharedString, index: i32) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        if index < 0 {
            return;
        }
        orchid_widgets::builtin::jyotish::select_city(inst, index as usize);
        self.persist_and_refresh_jyotish(inst);
    }

    pub(super) fn on_jyotish_remove_city(self: &Arc<Self>, id: &SharedString, index: i32) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        if index < 0 {
            return;
        }
        orchid_widgets::builtin::jyotish::remove_city(inst, index as usize);
        self.persist_and_refresh_jyotish(inst);
    }

    pub(super) fn on_jyotish_search_cities(
        self: &Arc<Self>,
        id: &SharedString,
        query: &SharedString,
    ) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::search_cities(inst, query.to_string());
        self.refresh_jyotish(inst);
    }

    pub(super) fn on_jyotish_add_city(
        self: &Arc<Self>,
        id: &SharedString,
        name: &SharedString,
        lat: f32,
        lon: f32,
    ) {
        let Some(inst) = Self::parse_jyotish_id(id) else {
            return;
        };
        orchid_widgets::builtin::jyotish::add_city(
            inst,
            name.to_string(),
            f64::from(lat),
            f64::from(lon),
        );
        self.persist_and_refresh_jyotish(inst);
    }

    fn refresh_jyotish(self: &Arc<Self>, inst_id: Uuid) {
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = wm.refresh_snapshot_cache(inst_id).await;
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }

    fn persist_and_refresh_jyotish(self: &Arc<Self>, inst_id: Uuid) {
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = wm.save_widget_state(inst_id).await {
                warn!(%inst_id, error = %e, "jyotish: persist config failed");
            }
            let _ = wm.refresh_snapshot_cache(inst_id).await;
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }

    fn export_jyotish_to_clipboard(self: &Arc<Self>, inst: Uuid, week: bool) {
        let Some(snap) = self.widget_manager.snapshot_cache().get(inst) else {
            return;
        };
        let WidgetPayload::Jyotish(payload) = &snap.payload else {
            return;
        };
        if payload.is_loading {
            return;
        }
        let text = if week {
            build_jyotish_week_export(payload, &self.locale)
        } else {
            build_jyotish_day_export(payload, &self.locale)
        };
        match crate::widgets::terminal::ArboardClipboard::new() {
            Ok(cb) => {
                if let Err(e) = cb.copy(&text) {
                    warn!(?e, "copy jyotish export");
                } else {
                    self.push_notification(
                        &self.locale.tr("widget-jyotish-name"),
                        &self.locale.tr("jyotish-exported"),
                        0,
                    );
                }
            }
            Err(e) => warn!(?e, "open clipboard for jyotish export"),
        }
    }
}

fn jyotish_color_name(locale: &LocaleManager, color: u8) -> String {
    let key = match color {
        0 => "jyotish-legend-green",
        1 => "jyotish-legend-yellow",
        _ => "jyotish-legend-red",
    };
    locale.tr(key)
}

fn build_jyotish_day_export(payload: &JyotishPayload, locale: &LocaleManager) -> String {
    let mut lines = vec![format!(
        "{} — {}",
        locale.tr("widget-jyotish-name"),
        payload.date_text
    )];
    if !payload.location_name.is_empty() {
        lines.push(payload.location_name.clone());
    }
    lines.push(format!(
        "{}: {} ({})",
        locale.tr("jyotish-score-label"),
        jyotish_color_name(locale, payload.score_color),
        payload.score_value
    ));
    if !payload.headline_key.is_empty() {
        lines.push(locale.tr(payload.headline_key));
    }
    lines.push(format!(
        "{}: {}",
        locale.tr("jyotish-label-tithi"),
        locale.tr(payload.tithi_key)
    ));
    lines.push(format!(
        "{}: {}",
        locale.tr("jyotish-label-nakshatra"),
        locale.tr(payload.nakshatra_key)
    ));
    lines.push(format!(
        "{}: {}",
        locale.tr("jyotish-label-yoga"),
        locale.tr(payload.yoga_key)
    ));
    lines.push(format!(
        "{}: {}",
        locale.tr("jyotish-label-karana"),
        locale.tr(payload.karana_key)
    ));
    lines.push(format!(
        "{}: {}",
        locale.tr("jyotish-label-vara"),
        locale.tr(payload.vara_key)
    ));
    if let Some(t) = &payload.rahukalam_text {
        lines.push(format!("{}: {t}", locale.tr("jyotish-label-rahukalam")));
    }
    if let Some(t) = &payload.yamagandam_text {
        lines.push(format!("{}: {t}", locale.tr("jyotish-label-yamagandam")));
    }
    if let Some(t) = &payload.gulika_text {
        lines.push(format!("{}: {t}", locale.tr("jyotish-label-gulika")));
    }
    for k in &payload.advice_keys {
        lines.push(format!("- {}", locale.tr(k)));
    }
    lines.join("\n")
}

fn build_jyotish_week_export(payload: &JyotishPayload, locale: &LocaleManager) -> String {
    let mut lines = vec![locale.tr("widget-jyotish-name")];
    if !payload.location_name.is_empty() {
        lines.push(payload.location_name.clone());
    }
    for chip in &payload.week_strip {
        lines.push(format!(
            "{} {} — {}",
            locale.tr(chip.weekday_key),
            chip.day_num,
            jyotish_color_name(locale, chip.color)
        ));
    }
    lines.join("\n")
}
