//! Protection widget callbacks.

use std::sync::{Arc, Weak};

use super::MainWindowController;

impl MainWindowController {
    pub(super) fn wire_protect(self: &Arc<Self>, t: &Weak<Self>) {
        self.window.on_protect_tab_changed({
            let t = t.clone();
            move |id, tab| {
                if let Some(c) = t.upgrade() {
                    c.on_protect_tab(&id, tab);
                }
            }
        });
        self.window.on_protect_toggled({
            let t = t.clone();
            move |id, row| {
                if let Some(c) = t.upgrade() {
                    c.on_protect_toggle(&id, &row);
                }
            }
        });
        self.window.on_protect_scan({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_protect_scan(&id);
                }
            }
        });
        self.window.on_protect_clean({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_protect_clean(&id);
                }
            }
        });
        self.window.on_protect_select_drive({
            let t = t.clone();
            move |id, index| {
                if let Some(c) = t.upgrade() {
                    c.on_protect_select_drive(&id, index);
                }
            }
        });
        self.window.on_protect_set_passes({
            let t = t.clone();
            move |id, passes| {
                if let Some(c) = t.upgrade() {
                    c.on_protect_set_passes(&id, passes);
                }
            }
        });
        self.window.on_protect_wipe({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_protect_wipe(&id);
                }
            }
        });
        self.window.on_protect_cancel({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_protect_cancel(&id);
                }
            }
        });
        self.window.on_protect_refresh({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_protect_refresh(&id);
                }
            }
        });
        self.window.on_protect_add({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_protect_add(&id);
                }
            }
        });
    }
}
