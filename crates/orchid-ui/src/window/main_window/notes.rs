//! Notes / scratchpad widget handlers for [`MainWindowController`].

use std::sync::Arc;

use slint::SharedString;
use uuid::Uuid;

use crate::window::spawn;

use super::MainWindowController;

impl MainWindowController {
    fn parse_notes_id(id: &SharedString) -> Option<Uuid> {
        Uuid::parse_str(id.as_str()).ok()
    }

    pub(super) fn on_notes_body_changed(self: &Arc<Self>, id: &SharedString, text: &SharedString) {
        let Some(inst) = Self::parse_notes_id(id) else {
            return;
        };
        orchid_widgets::builtin::notes::set_body(inst, text.to_string());
        // Snapshot patch arrives via WidgetSnapshotUpdated; no full rebuild.
    }

    pub(super) fn on_notes_title_changed(self: &Arc<Self>, id: &SharedString, text: &SharedString) {
        let Some(inst) = Self::parse_notes_id(id) else {
            return;
        };
        orchid_widgets::builtin::notes::set_title(inst, text.to_string());
    }

    pub(super) fn on_notes_select_tab(self: &Arc<Self>, id: &SharedString, index: i32) {
        let Some(inst) = Self::parse_notes_id(id) else {
            return;
        };
        orchid_widgets::builtin::notes::select_tab(inst, index);
        self.refresh_notes(inst);
    }

    pub(super) fn on_notes_new_tab(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_notes_id(id) else {
            return;
        };
        orchid_widgets::builtin::notes::new_tab(inst);
        self.refresh_notes(inst);
    }

    pub(super) fn on_notes_close_tab(self: &Arc<Self>, id: &SharedString, index: i32) {
        let Some(inst) = Self::parse_notes_id(id) else {
            return;
        };
        orchid_widgets::builtin::notes::close_tab(inst, index);
        self.refresh_notes(inst);
    }

    pub(super) fn on_notes_toggle_wrap(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_notes_id(id) else {
            return;
        };
        orchid_widgets::builtin::notes::toggle_wrap(inst);
        self.refresh_notes(inst);
    }

    pub(super) fn on_notes_toggle_mono(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_notes_id(id) else {
            return;
        };
        orchid_widgets::builtin::notes::toggle_mono(inst);
        self.refresh_notes(inst);
    }

    pub(super) fn on_notes_zoom(self: &Arc<Self>, id: &SharedString, delta: i32) {
        let Some(inst) = Self::parse_notes_id(id) else {
            return;
        };
        orchid_widgets::builtin::notes::zoom(inst, delta);
        self.refresh_notes(inst);
    }

    pub(super) fn on_notes_clear(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_notes_id(id) else {
            return;
        };
        orchid_widgets::builtin::notes::clear_active(inst);
        self.refresh_notes(inst);
    }

    pub(super) fn on_notes_find(
        self: &Arc<Self>,
        id: &SharedString,
        query: &SharedString,
        forward: bool,
    ) {
        let Some(inst) = Self::parse_notes_id(id) else {
            return;
        };
        orchid_widgets::builtin::notes::find(inst, query.as_str(), forward);
        self.refresh_notes(inst);
    }

    fn refresh_notes(self: &Arc<Self>, inst_id: Uuid) {
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
