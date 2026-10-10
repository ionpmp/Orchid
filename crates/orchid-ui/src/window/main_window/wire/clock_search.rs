//! Clock, recent files, and search.

use std::sync::{Arc, Weak};


use crate::window::main_window::MainWindowController;

impl MainWindowController {
    pub(super) fn wire_clock_search(self: &Arc<Self>, t: &Weak<Self>) {
        self.window.on_clock_open_cities({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_clock_open_cities(&id);
                }
            }
        });
        self.window.on_clock_close_cities({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_clock_close_cities(&id);
                }
            }
        });
        self.window.on_clock_remove_city({
            let t = t.clone();
            move |id, idx| {
                if let Some(c) = t.upgrade() {
                    c.on_clock_remove_city(&id, idx);
                }
            }
        });
        self.window.on_clock_move_city({
            let t = t.clone();
            move |id, idx, delta| {
                if let Some(c) = t.upgrade() {
                    c.on_clock_move_city(&id, idx, delta);
                }
            }
        });
        self.window.on_clock_search_cities({
            let t = t.clone();
            move |id, q| {
                if let Some(c) = t.upgrade() {
                    c.on_clock_search_cities(&id, &q);
                }
            }
        });
        self.window.on_clock_add_city({
            let t = t.clone();
            move |id, name, tz| {
                if let Some(c) = t.upgrade() {
                    c.on_clock_add_city(&id, &name, &tz);
                }
            }
        });
        self.window.on_recent_files_item_clicked({
            let t = t.clone();
            move |path| {
                if let Some(c) = t.upgrade() {
                    c.on_recent_files_item_clicked(&path);
                }
            }
        });
        self.window.on_search_query_changed({
            let t = t.clone();
            move |inst, q| {
                if let Some(c) = t.upgrade() {
                    c.on_search_query_changed(&inst, &q);
                }
            }
        });
        self.window.on_search_candidate_activated({
            let t = t.clone();
            move |inst, id| {
                if let Some(c) = t.upgrade() {
                    c.on_search_candidate_activated(&inst, &id);
                }
            }
        });
        self.window.on_search_selection_changed({
            let t = t.clone();
            move |inst, idx| {
                if let Some(c) = t.upgrade() {
                    c.on_search_selection_changed(&inst, idx);
                }
            }
        });
    }
}
