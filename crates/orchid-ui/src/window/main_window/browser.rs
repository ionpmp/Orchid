//! Browser widget handlers for [`MainWindowController`].

use std::sync::Arc;

use slint::SharedString;
use uuid::Uuid;

use crate::window::spawn;

use super::MainWindowController;

impl MainWindowController {
    fn parse_browser_id(id: &SharedString) -> Option<Uuid> {
        Uuid::parse_str(id.as_str()).ok()
    }

    pub(super) fn on_browser_select_tab(self: &Arc<Self>, id: &SharedString, index: i32) {
        let Some(inst) = Self::parse_browser_id(id) else {
            return;
        };
        orchid_widgets::builtin::browser::select_tab(inst, index);
        self.refresh_browser(inst);
    }

    pub(super) fn on_browser_new_tab(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_browser_id(id) else {
            return;
        };
        orchid_widgets::builtin::browser::new_tab(inst);
        self.refresh_browser(inst);
    }

    pub(super) fn on_browser_close_tab(self: &Arc<Self>, id: &SharedString, index: i32) {
        let Some(inst) = Self::parse_browser_id(id) else {
            return;
        };
        let closed_surface = orchid_widgets::builtin::browser::current_config(inst)
            .and_then(|cfg| cfg.tabs.get(index as usize).map(|t| t.id.clone()));
        orchid_widgets::builtin::browser::close_tab(inst, index);
        if let Some(tab_id) = closed_surface {
            if let Ok(surface) = Uuid::parse_str(&tab_id) {
                self.html_webview.destroy_surface(inst, surface);
            }
        }
        self.refresh_browser(inst);
    }

    pub(super) fn on_browser_navigate(self: &Arc<Self>, id: &SharedString, url: &SharedString) {
        let Some(inst) = Self::parse_browser_id(id) else {
            return;
        };
        orchid_widgets::builtin::browser::navigate(inst, url.as_str());
        self.refresh_browser(inst);
    }

    pub(super) fn on_browser_command(self: &Arc<Self>, id: &SharedString, command: &SharedString) {
        let Some(inst) = Self::parse_browser_id(id) else {
            return;
        };
        let Some(tab_id) = orchid_widgets::builtin::browser::active_tab_id(inst) else {
            return;
        };
        let Ok(surface) = Uuid::parse_str(&tab_id) else {
            return;
        };
        self.html_webview
            .command_surface(inst, surface, command.as_str());
    }

    pub(super) fn on_browser_open_external(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_browser_id(id) else {
            return;
        };
        let Some(cfg) = orchid_widgets::builtin::browser::current_config(inst) else {
            return;
        };
        let url = cfg.active_tab().url.clone();
        if url.is_empty() || url == "about:blank" {
            return;
        }
        if let Err(e) = opener::open(&url) {
            tracing::warn!(?e, %url, "browser open external");
            let title = self.locale.tr("widget-browser-name");
            self.push_notification(&title, &e.to_string(), 3);
        }
    }

    fn refresh_browser(self: &Arc<Self>, inst_id: Uuid) {
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = wm.refresh_snapshot_cache(inst_id).await;
            if let Some(c) = t.upgrade() {
                c.schedule_instance_patch(inst_id);
            }
        });
    }
}
