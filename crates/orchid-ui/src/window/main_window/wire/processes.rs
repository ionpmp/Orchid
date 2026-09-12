//! Processes widget callbacks.

#![allow(unused_imports)]

use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};
use std::time::Instant;

use slint::ComponentHandle;
use tracing::warn;
use uuid::Uuid;

use crate::error::Result;
use crate::slint_generated::{NotificationGlobal, ShortcutBindings};
use crate::window::errors::{ui_localized_error, viewer_localized_error};
use crate::window::main_window::{MainWindowController, PasswordCopyKind};
use crate::window::spawn;

impl MainWindowController {
    pub(super) fn wire_processes(self: &Arc<Self>, t: &Weak<Self>) {
        self.window.on_processes_tab_changed({
            let t = t.clone();
            move |id, tab| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_tab_changed(&id, tab);
                }
            }
        });
        self.window.on_processes_search_changed({
            let t = t.clone();
            move |id, q| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_search_changed(&id, &q);
                }
            }
        });
        self.window.on_processes_sort_column_clicked({
            let t = t.clone();
            move |id, col| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_sort_column_clicked(&id, col);
                }
            }
        });
        self.window.on_processes_process_clicked({
            let t = t.clone();
            move |id, pid| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_process_clicked(&id, pid);
                }
            }
        });
        self.window.on_processes_process_context({
            let t = t.clone();
            move |id, pid, x, y| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_process_context(&id, pid, x, y);
                }
            }
        });
        self.window.on_processes_end_task({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_end_task(&id);
                }
            }
        });
        self.window.on_processes_end_tree({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_end_tree(&id);
                }
            }
        });
        self.window.on_processes_open_location({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_open_location(&id);
                }
            }
        });
        self.window.on_processes_copy_pid({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_copy_pid(&id);
                }
            }
        });
        self.window.on_processes_copy_path({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_copy_path(&id);
                }
            }
        });
        self.window.on_processes_service_clicked({
            let t = t.clone();
            move |id, name| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_service_clicked(&id, &name);
                }
            }
        });
        self.window.on_processes_service_start({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_service_start(&id);
                }
            }
        });
        self.window.on_processes_service_stop({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_service_stop(&id);
                }
            }
        });
        self.window.on_processes_service_restart({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_service_restart(&id);
                }
            }
        });
        self.window.on_processes_startup_clicked({
            let t = t.clone();
            move |id, entry| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_startup_clicked(&id, &entry);
                }
            }
        });
        self.window.on_processes_startup_toggle({
            let t = t.clone();
            move |id, entry, enabled| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_startup_toggle(&id, &entry, enabled);
                }
            }
        });
        self.window.on_processes_startup_open_location({
            let t = t.clone();
            move |id, entry| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_startup_open_location(&id, &entry);
                }
            }
        });
        self.window.on_processes_user_clicked({
            let t = t.clone();
            move |id, sid| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_user_clicked(&id, sid);
                }
            }
        });
        self.window.on_processes_user_disconnect({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_user_disconnect(&id);
                }
            }
        });
        self.window.on_processes_user_sign_out({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_user_sign_out(&id);
                }
            }
        });
        self.window.on_processes_confirm_yes({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_confirm_yes(&id);
                }
            }
        });
        self.window.on_processes_confirm_no({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_processes_confirm_no(&id);
                }
            }
        });
    }
}
