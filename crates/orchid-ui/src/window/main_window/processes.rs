//! Processes widget handlers for [`MainWindowController`].

use std::sync::Arc;

use slint::SharedString;
use uuid::Uuid;

use crate::window::spawn;

use super::MainWindowController;

impl MainWindowController {
    pub(super) fn find_active_processes_widget(&self) -> Option<Uuid> {
        let w = self.workspace_manager.active().ok()?;
        for inst in self.widget_manager.instances_for_workspace(w.id) {
            if inst.type_id == "processes" {
                return Some(inst.id);
            }
        }
        None
    }

    pub(super) fn on_processes_tab_changed(self: &Arc<Self>, tab: i32) {
        let Some(inst_id) = self.find_active_processes_widget() else {
            return;
        };
        orchid_widgets::builtin::processes::set_tab(inst_id, tab);
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_search_changed(self: &Arc<Self>, q: &SharedString) {
        let Some(inst_id) = self.find_active_processes_widget() else {
            return;
        };
        orchid_widgets::builtin::processes::update_search(inst_id, q.to_string());
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_sort_column_clicked(self: &Arc<Self>, col: i32) {
        let Some(inst_id) = self.find_active_processes_widget() else {
            return;
        };
        orchid_widgets::builtin::processes::set_sort(inst_id, col);
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_process_clicked(self: &Arc<Self>, pid: i32) {
        let Some(inst_id) = self.find_active_processes_widget() else {
            return;
        };
        orchid_widgets::builtin::processes::select_process(inst_id, pid as u32);
        self.hide_processes_context(inst_id);
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_process_context(self: &Arc<Self>, pid: i32, x: f32, y: f32) {
        let Some(inst_id) = self.find_active_processes_widget() else {
            return;
        };
        orchid_widgets::builtin::processes::select_process(inst_id, pid as u32);
        self.processes_context
            .write()
            .insert(inst_id, (true, x, y));
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_end_task(self: &Arc<Self>) {
        let Some(inst_id) = self.find_active_processes_widget() else {
            return;
        };
        let pid = self.selected_process_pid(inst_id);
        if pid == 0 {
            return;
        }
        if let Err(e) = orchid_widgets::builtin::processes::kill_process(inst_id, pid) {
            self.push_notification(
                &self.locale.tr("widget-processes-name"),
                &e,
                1,
            );
        }
        self.hide_processes_context(inst_id);
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_end_tree(self: &Arc<Self>) {
        let Some(inst_id) = self.find_active_processes_widget() else {
            return;
        };
        let pid = self.selected_process_pid(inst_id);
        if pid == 0 {
            return;
        }
        if let Err(e) = orchid_widgets::builtin::processes::kill_process_tree(inst_id, pid) {
            self.push_notification(
                &self.locale.tr("widget-processes-name"),
                &e,
                1,
            );
        }
        self.hide_processes_context(inst_id);
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_open_location(self: &Arc<Self>) {
        let Some(inst_id) = self.find_active_processes_widget() else {
            return;
        };
        let pid = self.selected_process_pid(inst_id);
        if pid == 0 {
            return;
        }
        if let Err(e) = orchid_widgets::builtin::processes::open_file_location(inst_id, pid) {
            self.push_notification(
                &self.locale.tr("widget-processes-name"),
                &e,
                1,
            );
        }
        self.hide_processes_context(inst_id);
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_copy_pid(self: &Arc<Self>) {
        let Some(inst_id) = self.find_active_processes_widget() else {
            return;
        };
        let pid = self.selected_process_pid(inst_id);
        if pid == 0 {
            return;
        }
        self.copy_plain_text(&pid.to_string());
        self.hide_processes_context(inst_id);
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_copy_path(self: &Arc<Self>) {
        let Some(inst_id) = self.find_active_processes_widget() else {
            return;
        };
        let pid = self.selected_process_pid(inst_id);
        let Some(path) = orchid_widgets::builtin::processes::process_path(inst_id, pid) else {
            return;
        };
        if path.is_empty() {
            return;
        }
        self.copy_plain_text(&path);
        self.hide_processes_context(inst_id);
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_service_clicked(self: &Arc<Self>, name: &SharedString) {
        let Some(inst_id) = self.find_active_processes_widget() else {
            return;
        };
        orchid_widgets::builtin::processes::select_service(inst_id, name.to_string());
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_service_start(self: &Arc<Self>) {
        self.service_action(|id, name| {
            orchid_widgets::builtin::processes::service_start(id, name)
        });
    }

    pub(super) fn on_processes_service_stop(self: &Arc<Self>) {
        self.service_action(|id, name| orchid_widgets::builtin::processes::service_stop(id, name));
    }

    pub(super) fn on_processes_service_restart(self: &Arc<Self>) {
        self.service_action(|id, name| {
            orchid_widgets::builtin::processes::service_restart(id, name)
        });
    }

    pub(super) fn on_processes_startup_clicked(self: &Arc<Self>, id: &SharedString) {
        let Some(inst_id) = self.find_active_processes_widget() else {
            return;
        };
        orchid_widgets::builtin::processes::select_startup(inst_id, id.to_string());
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_startup_toggle(self: &Arc<Self>, id: &SharedString, enabled: bool) {
        let Some(inst_id) = self.find_active_processes_widget() else {
            return;
        };
        if let Err(e) =
            orchid_widgets::builtin::processes::startup_set_enabled(inst_id, id.as_str(), enabled)
        {
            self.push_notification(
                &self.locale.tr("widget-processes-name"),
                &e,
                1,
            );
        }
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_startup_open_location(self: &Arc<Self>, id: &SharedString) {
        let Some(inst_id) = self.find_active_processes_widget() else {
            return;
        };
        if let Err(e) =
            orchid_widgets::builtin::processes::startup_open_location(inst_id, id.as_str())
        {
            self.push_notification(
                &self.locale.tr("widget-processes-name"),
                &e,
                1,
            );
        }
    }

    pub(super) fn on_processes_user_clicked(self: &Arc<Self>, session_id: i32) {
        let Some(inst_id) = self.find_active_processes_widget() else {
            return;
        };
        orchid_widgets::builtin::processes::select_session(inst_id, session_id as u32);
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_user_disconnect(self: &Arc<Self>) {
        let Some(inst_id) = self.find_active_processes_widget() else {
            return;
        };
        let session = self.selected_session_id(inst_id);
        if session == u32::MAX {
            return;
        }
        if let Err(e) = orchid_widgets::builtin::processes::user_disconnect(inst_id, session) {
            self.push_notification(
                &self.locale.tr("widget-processes-name"),
                &e,
                1,
            );
        }
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_user_sign_out(self: &Arc<Self>) {
        let Some(inst_id) = self.find_active_processes_widget() else {
            return;
        };
        let session = self.selected_session_id(inst_id);
        if session == u32::MAX {
            return;
        }
        if let Err(e) = orchid_widgets::builtin::processes::user_sign_out(inst_id, session) {
            self.push_notification(
                &self.locale.tr("widget-processes-name"),
                &e,
                1,
            );
        }
        self.refresh_processes(inst_id);
    }

    fn service_action(self: &Arc<Self>, f: impl FnOnce(Uuid, &str) -> Result<(), String>) {
        let Some(inst_id) = self.find_active_processes_widget() else {
            return;
        };
        let name = self.selected_service_name(inst_id);
        if name.is_empty() {
            return;
        }
        if let Err(e) = f(inst_id, &name) {
            self.push_notification(
                &self.locale.tr("widget-processes-name"),
                &e,
                1,
            );
        }
        self.refresh_processes(inst_id);
    }

    fn selected_process_pid(&self, inst_id: Uuid) -> u32 {
        self.widget_manager
            .snapshot_cache()
            .get(inst_id)
            .and_then(|ws| match &ws.payload {
                orchid_widgets::WidgetPayload::Processes(p) => Some(p.selected_pid),
                _ => None,
            })
            .unwrap_or(0)
    }

    fn selected_service_name(&self, inst_id: Uuid) -> String {
        self.widget_manager
            .snapshot_cache()
            .get(inst_id)
            .and_then(|ws| match &ws.payload {
                orchid_widgets::WidgetPayload::Processes(p) => Some(p.selected_service.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn selected_session_id(&self, inst_id: Uuid) -> u32 {
        self.widget_manager
            .snapshot_cache()
            .get(inst_id)
            .and_then(|ws| match &ws.payload {
                orchid_widgets::WidgetPayload::Processes(p) => Some(p.selected_session),
                _ => None,
            })
            .unwrap_or(u32::MAX)
    }

    fn hide_processes_context(&self, inst_id: Uuid) {
        self.processes_context.write().insert(inst_id, (false, 0.0, 0.0));
    }

    fn copy_plain_text(self: &Arc<Self>, text: &str) {
        match crate::widgets::terminal::ArboardClipboard::new() {
            Ok(cb) => {
                let _ = cb.copy(text);
                self.push_notification(
                    &self.locale.tr("widget-processes-name"),
                    &self.locale.tr("processes-copied"),
                    0,
                );
            }
            Err(e) => {
                self.push_notification(
                    &self.locale.tr("widget-processes-name"),
                    &e.to_string(),
                    1,
                );
            }
        }
    }

    fn refresh_processes(self: &Arc<Self>, inst_id: Uuid) {
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
