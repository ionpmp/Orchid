//! Protection widget handlers.

use std::sync::Arc;

use slint::SharedString;
use uuid::Uuid;

use super::MainWindowController;

impl MainWindowController {
    fn parse_protect_id(id: &SharedString) -> Option<Uuid> {
        Uuid::parse_str(id.as_str()).ok()
    }

    pub(super) fn on_protect_tab(self: &Arc<Self>, id: &SharedString, tab: i32) {
        let Some(inst) = Self::parse_protect_id(id) else {
            return;
        };
        orchid_widgets::builtin::protect::set_tab(inst, tab);
    }

    pub(super) fn on_protect_toggle(self: &Arc<Self>, id: &SharedString, row: &SharedString) {
        let Some(inst) = Self::parse_protect_id(id) else {
            return;
        };
        orchid_widgets::builtin::protect::toggle_row(inst, row.as_str());
    }

    pub(super) fn on_protect_scan(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_protect_id(id) else {
            return;
        };
        orchid_widgets::builtin::protect::scan(inst);
    }

    pub(super) fn on_protect_clean(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_protect_id(id) else {
            return;
        };
        orchid_widgets::builtin::protect::clean(inst);
    }

    pub(super) fn on_protect_select_drive(self: &Arc<Self>, id: &SharedString, index: i32) {
        let Some(inst) = Self::parse_protect_id(id) else {
            return;
        };
        orchid_widgets::builtin::protect::select_drive(inst, index);
    }

    pub(super) fn on_protect_set_passes(self: &Arc<Self>, id: &SharedString, passes: i32) {
        let Some(inst) = Self::parse_protect_id(id) else {
            return;
        };
        orchid_widgets::builtin::protect::set_passes(inst, passes);
    }

    pub(super) fn on_protect_wipe(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_protect_id(id) else {
            return;
        };
        orchid_widgets::builtin::protect::start_wipe(inst);
    }

    pub(super) fn on_protect_cancel(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_protect_id(id) else {
            return;
        };
        orchid_widgets::builtin::protect::cancel_wipe(inst);
    }

    pub(super) fn on_protect_refresh(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_protect_id(id) else {
            return;
        };
        orchid_widgets::builtin::protect::refresh_network(inst);
    }
}
