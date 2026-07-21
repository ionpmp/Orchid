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
}
