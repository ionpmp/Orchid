//! Clock-widget handlers for [`MainWindowController`].

use std::sync::Arc;

use slint::SharedString;
use tracing::warn;
use uuid::Uuid;

use crate::window::spawn;

use super::MainWindowController;

impl MainWindowController {
    pub(super) fn on_clock_open_cities(self: &Arc<Self>, id: &SharedString) {
        let Some(inst_id) = parse_uuid(id) else {
            return;
        };
        orchid_widgets::builtin::clock::set_picker_open(inst_id, true);
        self.refresh_clock(inst_id);
    }

    pub(super) fn on_clock_close_cities(self: &Arc<Self>, id: &SharedString) {
        let Some(inst_id) = parse_uuid(id) else {
            return;
        };
        orchid_widgets::builtin::clock::set_picker_open(inst_id, false);
        self.refresh_clock(inst_id);
    }

    pub(super) fn on_clock_remove_city(self: &Arc<Self>, id: &SharedString, index: i32) {
        let Some(inst_id) = parse_uuid(id) else {
            return;
        };
        if index < 0 {
            return;
        }
        orchid_widgets::builtin::clock::remove_city(inst_id, index as usize);
        self.persist_and_refresh_clock(inst_id);
    }

    pub(super) fn on_clock_move_city(
        self: &Arc<Self>,
        id: &SharedString,
        index: i32,
        delta: i32,
    ) {
        let Some(inst_id) = parse_uuid(id) else {
            return;
        };
        if index < 0 || delta == 0 {
            return;
        }
        orchid_widgets::builtin::clock::move_city(inst_id, index as usize, delta);
        self.persist_and_refresh_clock(inst_id);
    }

    /// Drain transient clock UI notices (e.g. geocoding failures) into toasts.
    pub(super) fn drain_clock_notice(self: &Arc<Self>, inst_id: Uuid) {
        let Some(key) = orchid_widgets::builtin::clock::take_notice(inst_id) else {
            return;
        };
        let title = self.locale.tr("widget-clock-name");
        let body = self.locale.tr(key);
        self.push_notification(&title, &body, 3);
    }

    pub(super) fn on_clock_search_cities(
        self: &Arc<Self>,
        id: &SharedString,
        query: &SharedString,
    ) {
        let Some(inst_id) = parse_uuid(id) else {
            return;
        };
        orchid_widgets::builtin::clock::search_cities(inst_id, query.to_string());
        self.refresh_clock(inst_id);
    }

    pub(super) fn on_clock_add_city(
        self: &Arc<Self>,
        id: &SharedString,
        name: &SharedString,
        timezone: &SharedString,
    ) {
        let Some(inst_id) = parse_uuid(id) else {
            return;
        };
        orchid_widgets::builtin::clock::add_city(
            inst_id,
            name.to_string(),
            timezone.to_string(),
        );
        self.persist_and_refresh_clock(inst_id);
    }

    fn refresh_clock(self: &Arc<Self>, inst_id: Uuid) {
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = wm.refresh_snapshot_cache(inst_id).await;
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }

    fn persist_and_refresh_clock(self: &Arc<Self>, inst_id: Uuid) {
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = wm.save_widget_state(inst_id).await {
                warn!(%inst_id, error = %e, "clock: persist config failed");
            }
            let _ = wm.refresh_snapshot_cache(inst_id).await;
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }
}

fn parse_uuid(id: &SharedString) -> Option<Uuid> {
    Uuid::parse_str(id.as_str()).ok()
}
