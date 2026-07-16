//! Weather-widget handlers for [`MainWindowController`].

use std::sync::Arc;

use slint::SharedString;
use tracing::warn;
use uuid::Uuid;

use crate::window::spawn;

use super::MainWindowController;

impl MainWindowController {
    pub(super) fn on_weather_open_cities(self: &Arc<Self>, id: &SharedString) {
        let Some(inst_id) = parse_uuid(id) else {
            return;
        };
        orchid_widgets::builtin::weather::set_picker_open(inst_id, true);
        self.refresh_weather(inst_id);
    }

    pub(super) fn on_weather_close_cities(self: &Arc<Self>, id: &SharedString) {
        let Some(inst_id) = parse_uuid(id) else {
            return;
        };
        orchid_widgets::builtin::weather::set_picker_open(inst_id, false);
        self.refresh_weather(inst_id);
    }

    pub(super) fn on_weather_select_city(self: &Arc<Self>, id: &SharedString, index: i32) {
        let Some(inst_id) = parse_uuid(id) else {
            return;
        };
        if index < 0 {
            return;
        }
        orchid_widgets::builtin::weather::select_city(inst_id, index as usize);
        self.persist_and_refresh_weather(inst_id);
    }

    pub(super) fn on_weather_remove_city(self: &Arc<Self>, id: &SharedString, index: i32) {
        let Some(inst_id) = parse_uuid(id) else {
            return;
        };
        if index < 0 {
            return;
        }
        orchid_widgets::builtin::weather::remove_city(inst_id, index as usize);
        self.persist_and_refresh_weather(inst_id);
    }

    pub(super) fn on_weather_search_cities(self: &Arc<Self>, id: &SharedString, query: &SharedString) {
        let Some(inst_id) = parse_uuid(id) else {
            return;
        };
        orchid_widgets::builtin::weather::search_cities(inst_id, query.to_string());
        self.refresh_weather(inst_id);
    }

    pub(super) fn on_weather_add_city(
        self: &Arc<Self>,
        id: &SharedString,
        name: &SharedString,
        lat: f32,
        lon: f32,
        timezone: &SharedString,
    ) {
        let Some(inst_id) = parse_uuid(id) else {
            return;
        };
        orchid_widgets::builtin::weather::add_city(
            inst_id,
            name.to_string(),
            f64::from(lat),
            f64::from(lon),
            timezone.to_string(),
        );
        self.persist_and_refresh_weather(inst_id);
    }

    pub(super) fn on_weather_select_day(self: &Arc<Self>, id: &SharedString, index: i32) {
        let Some(inst_id) = parse_uuid(id) else {
            return;
        };
        if index < 0 {
            return;
        }
        orchid_widgets::builtin::weather::select_day(inst_id, index as usize);
        self.refresh_weather(inst_id);
    }

    fn refresh_weather(self: &Arc<Self>, inst_id: Uuid) {
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = wm.refresh_snapshot_cache(inst_id).await;
            if let Some(c) = t.upgrade() {
                c.schedule_rebuild();
            }
        });
    }

    fn persist_and_refresh_weather(self: &Arc<Self>, inst_id: Uuid) {
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = wm.save_widget_state(inst_id).await {
                warn!(%inst_id, error = %e, "weather: persist config failed");
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
