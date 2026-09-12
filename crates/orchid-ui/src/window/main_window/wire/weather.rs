//! RSS and weather callbacks.

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
    pub(super) fn wire_weather(self: &Arc<Self>, t: &Weak<Self>) {
        self.window.on_rss_item_clicked({
            let t = t.clone();
            move |link| {
                if let Some(c) = t.upgrade() {
                    c.on_rss_item_clicked(&link);
                }
            }
        });
        self.window.on_weather_open_cities({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_weather_open_cities(&id);
                }
            }
        });
        self.window.on_weather_close_cities({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_weather_close_cities(&id);
                }
            }
        });
        self.window.on_weather_select_city({
            let t = t.clone();
            move |id, idx| {
                if let Some(c) = t.upgrade() {
                    c.on_weather_select_city(&id, idx);
                }
            }
        });
        self.window.on_weather_remove_city({
            let t = t.clone();
            move |id, idx| {
                if let Some(c) = t.upgrade() {
                    c.on_weather_remove_city(&id, idx);
                }
            }
        });
        self.window.on_weather_search_cities({
            let t = t.clone();
            move |id, q| {
                if let Some(c) = t.upgrade() {
                    c.on_weather_search_cities(&id, &q);
                }
            }
        });
        self.window.on_weather_add_city({
            let t = t.clone();
            move |id, name, lat, lon, tz| {
                if let Some(c) = t.upgrade() {
                    c.on_weather_add_city(&id, &name, lat, lon, &tz);
                }
            }
        });
        self.window.on_weather_use_my_location({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_weather_use_my_location(&id);
                }
            }
        });
        self.window.on_weather_select_day({
            let t = t.clone();
            move |id, idx| {
                if let Some(c) = t.upgrade() {
                    c.on_weather_select_day(&id, idx);
                }
            }
        });
    }
}
