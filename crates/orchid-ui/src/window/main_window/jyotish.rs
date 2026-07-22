//! Jyotish widget handlers for [`MainWindowController`].

use std::sync::Arc;

use slint::SharedString;
use uuid::Uuid;

use super::MainWindowController;

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
}
