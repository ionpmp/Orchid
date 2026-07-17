//! Processes widget handlers for [`MainWindowController`].

use std::sync::Arc;

use slint::SharedString;
use uuid::Uuid;

use crate::slint_generated::ProcessesConfirmDialog;
use crate::window::spawn;

use super::MainWindowController;

impl MainWindowController {
    fn parse_processes_id(id: &SharedString) -> Option<Uuid> {
        Uuid::parse_str(id.as_str()).ok()
    }

    pub(super) fn on_processes_tab_changed(self: &Arc<Self>, id: &SharedString, tab: i32) {
        let Some(inst_id) = Self::parse_processes_id(id) else {
            return;
        };
        orchid_widgets::builtin::processes::set_tab(inst_id, tab);
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_search_changed(self: &Arc<Self>, id: &SharedString, q: &SharedString) {
        let Some(inst_id) = Self::parse_processes_id(id) else {
            return;
        };
        orchid_widgets::builtin::processes::update_search(inst_id, q.to_string());
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_sort_column_clicked(self: &Arc<Self>, id: &SharedString, col: i32) {
        let Some(inst_id) = Self::parse_processes_id(id) else {
            return;
        };
        orchid_widgets::builtin::processes::set_sort(inst_id, col);
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_process_clicked(self: &Arc<Self>, id: &SharedString, pid: i32) {
        let Some(inst_id) = Self::parse_processes_id(id) else {
            return;
        };
        orchid_widgets::builtin::processes::select_process(inst_id, pid as u32);
        self.hide_processes_context(inst_id);
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_process_context(
        self: &Arc<Self>,
        id: &SharedString,
        pid: i32,
        x: f32,
        y: f32,
    ) {
        let Some(inst_id) = Self::parse_processes_id(id) else {
            return;
        };
        orchid_widgets::builtin::processes::select_process(inst_id, pid as u32);
        self.processes_context
            .write()
            .insert(inst_id, (true, x, y));
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_end_task(self: &Arc<Self>, id: &SharedString) {
        let Some(inst_id) = Self::parse_processes_id(id) else {
            return;
        };
        self.show_processes_confirm(inst_id, "end-task");
    }

    pub(super) fn on_processes_end_tree(self: &Arc<Self>, id: &SharedString) {
        let Some(inst_id) = Self::parse_processes_id(id) else {
            return;
        };
        self.show_processes_confirm(inst_id, "end-tree");
    }

    pub(super) fn on_processes_open_location(self: &Arc<Self>, id: &SharedString) {
        let Some(inst_id) = Self::parse_processes_id(id) else {
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

    pub(super) fn on_processes_copy_pid(self: &Arc<Self>, id: &SharedString) {
        let Some(inst_id) = Self::parse_processes_id(id) else {
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

    pub(super) fn on_processes_copy_path(self: &Arc<Self>, id: &SharedString) {
        let Some(inst_id) = Self::parse_processes_id(id) else {
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

    pub(super) fn on_processes_service_clicked(self: &Arc<Self>, id: &SharedString, name: &SharedString) {
        let Some(inst_id) = Self::parse_processes_id(id) else {
            return;
        };
        orchid_widgets::builtin::processes::select_service(inst_id, name.to_string());
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_service_start(self: &Arc<Self>, id: &SharedString) {
        self.service_action(id, |iid, name| {
            orchid_widgets::builtin::processes::service_start(iid, name)
        });
    }

    pub(super) fn on_processes_service_stop(self: &Arc<Self>, id: &SharedString) {
        self.service_action(id, |iid, name| {
            orchid_widgets::builtin::processes::service_stop(iid, name)
        });
    }

    pub(super) fn on_processes_service_restart(self: &Arc<Self>, id: &SharedString) {
        self.service_action(id, |iid, name| {
            orchid_widgets::builtin::processes::service_restart(iid, name)
        });
    }

    pub(super) fn on_processes_startup_clicked(self: &Arc<Self>, id: &SharedString, entry: &SharedString) {
        let Some(inst_id) = Self::parse_processes_id(id) else {
            return;
        };
        orchid_widgets::builtin::processes::select_startup(inst_id, entry.to_string());
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_startup_toggle(
        self: &Arc<Self>,
        id: &SharedString,
        entry: &SharedString,
        enabled: bool,
    ) {
        let Some(inst_id) = Self::parse_processes_id(id) else {
            return;
        };
        if let Err(e) = orchid_widgets::builtin::processes::startup_set_enabled(
            inst_id,
            entry.as_str(),
            enabled,
        ) {
            self.push_notification(
                &self.locale.tr("widget-processes-name"),
                &e,
                1,
            );
        }
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_startup_open_location(
        self: &Arc<Self>,
        id: &SharedString,
        entry: &SharedString,
    ) {
        let Some(inst_id) = Self::parse_processes_id(id) else {
            return;
        };
        if let Err(e) =
            orchid_widgets::builtin::processes::startup_open_location(inst_id, entry.as_str())
        {
            self.push_notification(
                &self.locale.tr("widget-processes-name"),
                &e,
                1,
            );
        }
    }

    pub(super) fn on_processes_user_clicked(self: &Arc<Self>, id: &SharedString, session_id: i32) {
        let Some(inst_id) = Self::parse_processes_id(id) else {
            return;
        };
        orchid_widgets::builtin::processes::select_session(inst_id, session_id as u32);
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_user_disconnect(self: &Arc<Self>, id: &SharedString) {
        let Some(inst_id) = Self::parse_processes_id(id) else {
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

    pub(super) fn on_processes_user_sign_out(self: &Arc<Self>, id: &SharedString) {
        let Some(inst_id) = Self::parse_processes_id(id) else {
            return;
        };
        self.show_processes_confirm(inst_id, "sign-out");
    }

    pub(super) fn on_processes_confirm_yes(self: &Arc<Self>, id: &SharedString) {
        let Some(inst_id) = Self::parse_processes_id(id) else {
            return;
        };
        let action = self
            .processes_confirm
            .write()
            .remove(&inst_id)
            .map(|d| d.pending_action.to_string())
            .unwrap_or_default();
        match action.as_str() {
            "end-task" => {
                let pid = self.selected_process_pid(inst_id);
                if pid != 0 {
                    if let Err(e) = orchid_widgets::builtin::processes::kill_process(inst_id, pid) {
                        self.push_notification(
                            &self.locale.tr("widget-processes-name"),
                            &e,
                            1,
                        );
                    }
                }
            }
            "end-tree" => {
                let pid = self.selected_process_pid(inst_id);
                if pid != 0 {
                    if let Err(e) =
                        orchid_widgets::builtin::processes::kill_process_tree(inst_id, pid)
                    {
                        self.push_notification(
                            &self.locale.tr("widget-processes-name"),
                            &e,
                            1,
                        );
                    }
                }
            }
            "sign-out" => {
                let session = self.selected_session_id(inst_id);
                if session != u32::MAX {
                    if let Err(e) =
                        orchid_widgets::builtin::processes::user_sign_out(inst_id, session)
                    {
                        self.push_notification(
                            &self.locale.tr("widget-processes-name"),
                            &e,
                            1,
                        );
                    }
                }
            }
            _ => {}
        }
        self.hide_processes_context(inst_id);
        self.refresh_processes(inst_id);
    }

    pub(super) fn on_processes_confirm_no(self: &Arc<Self>, id: &SharedString) {
        let Some(inst_id) = Self::parse_processes_id(id) else {
            return;
        };
        self.processes_confirm.write().remove(&inst_id);
        self.hide_processes_context(inst_id);
        self.refresh_processes(inst_id);
    }

    fn show_processes_confirm(self: &Arc<Self>, inst_id: Uuid, action: &str) {
        let (title_key, message) = match action {
            "end-task" => {
                let pid = self.selected_process_pid(inst_id);
                if pid == 0 {
                    return;
                }
                let name = self.selected_process_name(inst_id);
                (
                    "processes-confirm-title",
                    self.locale.tr_args(
                        "processes-confirm-end-task",
                        &orchid_i18n::FluentArgs::new()
                            .with("name", name)
                            .with("pid", pid.to_string()),
                    ),
                )
            }
            "end-tree" => {
                let pid = self.selected_process_pid(inst_id);
                if pid == 0 {
                    return;
                }
                let name = self.selected_process_name(inst_id);
                (
                    "processes-confirm-title",
                    self.locale.tr_args(
                        "processes-confirm-end-tree",
                        &orchid_i18n::FluentArgs::new()
                            .with("name", name)
                            .with("pid", pid.to_string()),
                    ),
                )
            }
            "sign-out" => {
                let session = self.selected_session_id(inst_id);
                if session == u32::MAX {
                    return;
                }
                let user = self.selected_session_user(inst_id);
                (
                    "processes-confirm-title",
                    self.locale.tr_args(
                        "processes-confirm-sign-out",
                        &orchid_i18n::FluentArgs::new().with("user", user),
                    ),
                )
            }
            _ => return,
        };

        let dlg = ProcessesConfirmDialog {
            visible: true,
            title: self.locale.tr(title_key).into(),
            message: message.into(),
            confirm_label: self.locale.tr("action-confirm-yes").into(),
            cancel_label: self.locale.tr("action-confirm-no").into(),
            pending_action: action.into(),
        };
        self.processes_confirm.write().insert(inst_id, dlg);
        self.hide_processes_context(inst_id);
        self.refresh_processes(inst_id);
    }

    fn service_action(
        self: &Arc<Self>,
        id: &SharedString,
        f: impl FnOnce(Uuid, &str) -> Result<(), String>,
    ) {
        let Some(inst_id) = Self::parse_processes_id(id) else {
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

    fn selected_process_name(&self, inst_id: Uuid) -> String {
        let pid = self.selected_process_pid(inst_id);
        self.widget_manager
            .snapshot_cache()
            .get(inst_id)
            .and_then(|ws| match &ws.payload {
                orchid_widgets::WidgetPayload::Processes(p) => p
                    .processes
                    .iter()
                    .find(|r| !r.is_group_header && r.pid == pid)
                    .map(|r| r.name.clone()),
                _ => None,
            })
            .unwrap_or_else(|| format!("PID {pid}"))
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

    fn selected_session_user(&self, inst_id: Uuid) -> String {
        let sid = self.selected_session_id(inst_id);
        self.widget_manager
            .snapshot_cache()
            .get(inst_id)
            .and_then(|ws| match &ws.payload {
                orchid_widgets::WidgetPayload::Processes(p) => p
                    .users
                    .iter()
                    .find(|u| u.session_id == sid)
                    .map(|u| u.user_name.clone()),
                _ => None,
            })
            .unwrap_or_else(|| format!("session {sid}"))
    }

    fn hide_processes_context(&self, inst_id: Uuid) {
        self.processes_context
            .write()
            .insert(inst_id, (false, 0.0, 0.0));
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
