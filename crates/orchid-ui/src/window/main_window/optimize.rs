//! Optimize widget handlers.

use std::sync::Arc;

use slint::SharedString;
use uuid::Uuid;

use super::MainWindowController;

impl MainWindowController {
    fn parse_optimize_id(id: &SharedString) -> Option<Uuid> {
        Uuid::parse_str(id.as_str()).ok()
    }

    pub(super) fn on_optimize_tab(self: &Arc<Self>, id: &SharedString, tab: i32) {
        let Some(inst) = Self::parse_optimize_id(id) else {
            return;
        };
        orchid_widgets::builtin::optimize::set_tab(inst, tab);
    }

    pub(super) fn on_optimize_toggle(self: &Arc<Self>, id: &SharedString, tweak: &SharedString) {
        let Some(inst) = Self::parse_optimize_id(id) else {
            return;
        };
        orchid_widgets::builtin::optimize::toggle(inst, tweak.as_str());
    }

    pub(super) fn on_optimize_choice(
        self: &Arc<Self>,
        id: &SharedString,
        tweak: &SharedString,
        index: i32,
    ) {
        let Some(inst) = Self::parse_optimize_id(id) else {
            return;
        };
        orchid_widgets::builtin::optimize::set_choice(inst, tweak.as_str(), index);
    }

    pub(super) fn on_optimize_refresh(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_optimize_id(id) else {
            return;
        };
        orchid_widgets::builtin::optimize::refresh(inst);
    }

    pub(super) fn on_optimize_search(self: &Arc<Self>, id: &SharedString, query: &SharedString) {
        let Some(inst) = Self::parse_optimize_id(id) else {
            return;
        };
        orchid_widgets::builtin::optimize::set_query(inst, query.as_str());
    }

    pub(super) fn on_optimize_preset(self: &Arc<Self>, id: &SharedString, preset: &SharedString) {
        let Some(inst) = Self::parse_optimize_id(id) else {
            return;
        };
        orchid_widgets::builtin::optimize::apply_preset(inst, preset.as_str());
    }

    pub(super) fn on_optimize_restart(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_optimize_id(id) else {
            return;
        };
        orchid_widgets::builtin::optimize::restart_explorer(inst);
    }
}
