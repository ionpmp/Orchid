//! Terminal pane callbacks.

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
    pub(super) fn wire_terminal(self: &Arc<Self>, t: &Weak<Self>) {
        self.window.on_terminal_key_pressed({
            let t = t.clone();
            move |id, text, ctrl, shift, alt| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_key(&id, &text, ctrl, shift, alt);
                }
            }
        });
        self.window.on_terminal_viewport_changed({
            let t = t.clone();
            move |id, w, h| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_viewport(&id, w, h);
                }
            }
        });
        self.window.on_terminal_tab_clicked({
            let t = t.clone();
            move |id, idx| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_tab_clicked(&id, idx);
                }
            }
        });
        self.window.on_terminal_tab_closed({
            let t = t.clone();
            move |id, idx| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_tab_closed(&id, idx);
                }
            }
        });
        self.window.on_terminal_tab_new({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_tab_new(&id);
                }
            }
        });
        self.window.on_terminal_split_horizontal({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_split_horizontal(&id);
                }
            }
        });
        self.window.on_terminal_split_vertical({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_split_vertical(&id);
                }
            }
        });
        self.window.on_terminal_pane_clicked({
            let t = t.clone();
            move |id, sid| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_pane_clicked(&id, &sid);
                }
            }
        });
        self.window.on_terminal_pane_closed({
            let t = t.clone();
            move |id, sid| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_pane_closed(&id, &sid);
                }
            }
        });
        self.window.on_terminal_split_drag_moved({
            let t = t.clone();
            move |id, first, second, fx, fy| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_split_drag_moved(&id, &first, &second, fx, fy);
                }
            }
        });
        self.window.on_terminal_shortcut({
            let t = t.clone();
            move |id, action| {
                if let Some(c) = t.upgrade() {
                    c.on_terminal_shortcut(&id, &action);
                }
            }
        });
    }
}
