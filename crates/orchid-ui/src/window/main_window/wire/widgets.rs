//! Widget chrome, docking, groups.

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
    pub(super) fn wire_widgets(self: &Arc<Self>, t: &Weak<Self>) {
        self.window.on_widget_close_clicked({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_close(&id);
                }
            }
        });
        self.window.on_widget_settings_clicked({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_settings_clicked(&id);
                }
            }
        });
        self.window.on_widget_settings_field_changed({
            let t = t.clone();
            move |id, key, value| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_settings_field_changed(&id, &key, &value);
                }
            }
        });
        self.window.on_widget_settings_dismiss({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_settings_dismiss(&id);
                }
            }
        });
        self.window.on_widget_close_confirm_save({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_close_confirm_save(&id);
                }
            }
        });
        self.window.on_widget_close_confirm_discard({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_close_confirm_discard(&id);
                }
            }
        });
        self.window.on_widget_close_confirm_cancel({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_close_confirm_cancel(&id);
                }
            }
        });
        self.window.on_widget_drag_started({
            let t = t.clone();
            move |id, lx, ly| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_drag_started(&id, lx, ly);
                }
            }
        });
        self.window.on_widget_drag_moved({
            let t = t.clone();
            move |id, canvas_x, canvas_y| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_drag_moved(&id, canvas_x, canvas_y);
                }
            }
        });
        self.window.on_widget_drag_ended({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_drag_ended(&id);
                }
            }
        });
        self.window.on_widget_activate({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_activate(&id);
                }
            }
        });
        self.window.on_widget_undock_clicked({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_undock(&id);
                }
            }
        });
        self.window.on_widget_dock_clicked({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_dock(&id);
                }
            }
        });
        self.window.on_widget_minimize_clicked({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_minimize(&id);
                }
            }
        });
        self.window.on_widget_maximize_clicked({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_maximize_toggle(&id);
                }
            }
        });
        self.window.on_window_taskbar_activate({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_window_taskbar_activate(&id);
                }
            }
        });
        self.window.on_widget_resize_started({
            let t = t.clone();
            move |id, corner, press_x, press_y| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_resize_started(&id, &corner, press_x, press_y);
                }
            }
        });
        self.window.on_widget_resize_moved({
            let t = t.clone();
            move |id, canvas_x, canvas_y| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_resize_moved(&id, canvas_x, canvas_y);
                }
            }
        });
        self.window.on_widget_resize_ended({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_resize_ended(&id);
                }
            }
        });
        self.window.on_group_tab_clicked({
            let t = t.clone();
            move |group_id, member_id| {
                if let Some(c) = t.upgrade() {
                    c.on_group_tab_clicked(&group_id, &member_id);
                }
            }
        });
        self.window.on_group_tab_closed({
            let t = t.clone();
            move |group_id, member_id| {
                if let Some(c) = t.upgrade() {
                    c.on_group_tab_closed(&group_id, &member_id);
                }
            }
        });
        self.window.on_group_tab_move_left({
            let t = t.clone();
            move |group_id, member_id| {
                if let Some(c) = t.upgrade() {
                    c.on_group_tab_move(&group_id, &member_id, -1);
                }
            }
        });
        self.window.on_group_tab_move_right({
            let t = t.clone();
            move |group_id, member_id| {
                if let Some(c) = t.upgrade() {
                    c.on_group_tab_move(&group_id, &member_id, 1);
                }
            }
        });
        self.window.on_group_dissolve_clicked({
            let t = t.clone();
            move |group_id| {
                if let Some(c) = t.upgrade() {
                    c.on_group_dissolve_clicked(&group_id);
                }
            }
        });
    }
}
