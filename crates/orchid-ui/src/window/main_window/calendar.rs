//! Calendar / agenda widget handlers for [`MainWindowController`].

use std::sync::Arc;

use slint::SharedString;
use uuid::Uuid;

use crate::window::spawn;

use super::MainWindowController;

impl MainWindowController {
    fn parse_calendar_id(id: &SharedString) -> Option<Uuid> {
        Uuid::parse_str(id.as_str()).ok()
    }

    pub(super) fn on_calendar_select_date(
        self: &Arc<Self>,
        id: &SharedString,
        date: &SharedString,
    ) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::select_date(inst, date.as_str());
        self.refresh_calendar(inst);
    }

    pub(super) fn on_calendar_activate_day(
        self: &Arc<Self>,
        id: &SharedString,
        date: &SharedString,
    ) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::activate_day(inst, date.as_str());
        self.refresh_calendar(inst);
    }

    pub(super) fn on_calendar_shift_month(self: &Arc<Self>, id: &SharedString, delta: i32) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::shift_month(inst, delta);
        self.refresh_calendar(inst);
    }

    pub(super) fn on_calendar_goto_today(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::goto_today(inst);
        self.refresh_calendar(inst);
    }

    pub(super) fn on_calendar_open_new(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::open_new_editor(inst);
        self.refresh_calendar(inst);
    }

    pub(super) fn on_calendar_open_edit(
        self: &Arc<Self>,
        id: &SharedString,
        event_id: &SharedString,
    ) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::open_edit_editor(inst, event_id.as_str());
        self.refresh_calendar(inst);
    }

    pub(super) fn on_calendar_close_editor(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::close_editor(inst);
        self.refresh_calendar(inst);
    }

    pub(super) fn on_calendar_save_editor(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::save_editor(inst);
        self.refresh_calendar(inst);
    }

    pub(super) fn on_calendar_duplicate_editor(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::duplicate_editor(inst);
        self.refresh_calendar(inst);
    }

    pub(super) fn on_calendar_set_color_filter(self: &Arc<Self>, id: &SharedString, color: i32) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::set_color_filter(inst, color);
        self.refresh_calendar(inst);
    }

    pub(super) fn on_calendar_request_delete(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::request_delete(inst);
        self.refresh_calendar(inst);
    }

    pub(super) fn on_calendar_confirm_delete(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::confirm_delete(inst);
        self.refresh_calendar(inst);
    }

    pub(super) fn on_calendar_cancel_delete(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::cancel_delete(inst);
        self.refresh_calendar(inst);
    }

    pub(super) fn on_calendar_editor_title(
        self: &Arc<Self>,
        id: &SharedString,
        text: &SharedString,
    ) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::set_editor_title(inst, text.to_string());
    }

    pub(super) fn on_calendar_editor_date(
        self: &Arc<Self>,
        id: &SharedString,
        date: &SharedString,
    ) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::set_editor_date(inst, date.to_string());
        self.refresh_calendar(inst);
    }

    pub(super) fn on_calendar_shift_editor_date(self: &Arc<Self>, id: &SharedString, delta: i32) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::shift_editor_date(inst, delta);
        self.refresh_calendar(inst);
    }

    pub(super) fn on_calendar_editor_all_day(self: &Arc<Self>, id: &SharedString, all_day: bool) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::set_editor_all_day(inst, all_day);
        self.refresh_calendar(inst);
    }

    pub(super) fn on_calendar_nudge_editor_start(
        self: &Arc<Self>,
        id: &SharedString,
        delta_minutes: i32,
    ) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::nudge_editor_start(inst, delta_minutes);
        self.refresh_calendar(inst);
    }

    pub(super) fn on_calendar_nudge_editor_end(
        self: &Arc<Self>,
        id: &SharedString,
        delta_minutes: i32,
    ) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::nudge_editor_end(inst, delta_minutes);
        self.refresh_calendar(inst);
    }

    pub(super) fn on_calendar_editor_notes(
        self: &Arc<Self>,
        id: &SharedString,
        text: &SharedString,
    ) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::set_editor_notes(inst, text.to_string());
    }

    pub(super) fn on_calendar_editor_color(self: &Arc<Self>, id: &SharedString, color: i32) {
        let Some(inst) = Self::parse_calendar_id(id) else {
            return;
        };
        orchid_widgets::builtin::calendar::set_editor_color(inst, color);
        self.refresh_calendar(inst);
    }

    fn refresh_calendar(self: &Arc<Self>, inst_id: Uuid) {
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = wm.refresh_snapshot_cache(inst_id).await;
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }
}
