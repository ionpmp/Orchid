//! Password vault callbacks.

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
    pub(super) fn wire_password(self: &Arc<Self>, t: &Weak<Self>) {
        self.window.on_password_search_changed({
            let t = t.clone();
            move |q| {
                if let Some(c) = t.upgrade() {
                    c.on_password_search_changed(&q);
                }
            }
        });
        self.window.on_password_entry_clicked({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_password_entry_clicked(&id);
                }
            }
        });
        self.window.on_password_copy_password({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_password_copy(&id, PasswordCopyKind::Password);
                }
            }
        });
        self.window.on_password_copy_username({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_password_copy(&id, PasswordCopyKind::Username);
                }
            }
        });
        self.window.on_password_copy_totp({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_password_copy(&id, PasswordCopyKind::Totp);
                }
            }
        });
        self.window.on_password_open_url({
            let t = t.clone();
            move |url| {
                if let Some(c) = t.upgrade() {
                    c.on_password_open_url(&url);
                }
            }
        });
        self.window.on_password_unlock_submit({
            let t = t.clone();
            move |passphrase| {
                if let Some(c) = t.upgrade() {
                    c.on_password_unlock_submit(&passphrase);
                }
            }
        });
        self.window.on_password_unlock_biometric({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_password_unlock_biometric();
                }
            }
        });
        self.window.on_password_lock_vault({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_password_lock_vault();
                }
            }
        });
        self.window.on_password_add_entry_request({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_password_add_entry_request();
                }
            }
        });
        self.window.on_password_add_entry_commit({
            let t = t.clone();
            move |title, username, password, url, notes, group| {
                if let Some(c) = t.upgrade() {
                    c.on_password_add_entry_commit(
                        &title, &username, &password, &url, &notes, &group,
                    );
                }
            }
        });
        self.window.on_password_add_entry_cancel({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_password_add_entry_cancel();
                }
            }
        });
        self.window.on_password_add_entry_generate_password({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_password_add_entry_generate_password();
                }
            }
        });
        self.window.on_password_edit_entry_request({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_password_edit_entry_request();
                }
            }
        });
        self.window.on_password_generate_request({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_password_generate_request();
                }
            }
        });
        self.window.on_password_group_clicked({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_password_group_clicked(&id);
                }
            }
        });
    }
}
