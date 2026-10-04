//! Optimize widget callbacks.

use std::sync::{Arc, Weak};

use super::MainWindowController;

impl MainWindowController {
    pub(super) fn wire_optimize(self: &Arc<Self>, t: &Weak<Self>) {
        self.window.on_optimize_tab_changed({
            let t = t.clone();
            move |id, tab| {
                if let Some(c) = t.upgrade() {
                    c.on_optimize_tab(&id, tab);
                }
            }
        });
        self.window.on_optimize_toggled({
            let t = t.clone();
            move |id, tweak| {
                if let Some(c) = t.upgrade() {
                    c.on_optimize_toggle(&id, &tweak);
                }
            }
        });
        self.window.on_optimize_choice({
            let t = t.clone();
            move |id, tweak, index| {
                if let Some(c) = t.upgrade() {
                    c.on_optimize_choice(&id, &tweak, index);
                }
            }
        });
        self.window.on_optimize_refresh({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_optimize_refresh(&id);
                }
            }
        });
        self.window.on_optimize_restart_explorer({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_optimize_restart(&id);
                }
            }
        });
    }
}
