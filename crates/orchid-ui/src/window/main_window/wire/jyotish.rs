//! Jyotish callbacks.

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
    pub(super) fn wire_jyotish(self: &Arc<Self>, t: &Weak<Self>) {
        self.window.on_jyotish_prev_day({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_prev_day(&id);
                }
            }
        });
        self.window.on_jyotish_next_day({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_next_day(&id);
                }
            }
        });
        self.window.on_jyotish_go_today({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_go_today(&id);
                }
            }
        });
        self.window.on_jyotish_select_tab({
            let t = t.clone();
            move |id, tab| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_select_tab(&id, tab);
                }
            }
        });
        self.window.on_jyotish_select_offset({
            let t = t.clone();
            move |id, offset| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_select_offset(&id, offset);
                }
            }
        });
        self.window.on_jyotish_month_nav({
            let t = t.clone();
            move |id, delta| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_month_nav(&id, delta);
                }
            }
        });
        self.window.on_jyotish_year_nav({
            let t = t.clone();
            move |id, delta| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_year_nav(&id, delta);
                }
            }
        });
        self.window.on_jyotish_open_month({
            let t = t.clone();
            move |id, offset| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_open_month(&id, offset);
                }
            }
        });
        self.window.on_jyotish_open_year({
            let t = t.clone();
            move |id, offset| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_open_year(&id, offset);
                }
            }
        });
        self.window.on_jyotish_select_life_year({
            let t = t.clone();
            move |id, year| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_select_life_year(&id, year);
                }
            }
        });
        self.window.on_jyotish_rectify_start({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_rectify_start(&id);
                }
            }
        });
        self.window.on_jyotish_rectify_set_window({
            let t = t.clone();
            move |id, approx_minute, half_window| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_rectify_set_window(&id, approx_minute, half_window);
                }
            }
        });
        self.window.on_jyotish_rectify_answer({
            let t = t.clone();
            move |id, question_idx, option_idx| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_rectify_answer(&id, question_idx, option_idx);
                }
            }
        });
        self.window.on_jyotish_rectify_add_event({
            let t = t.clone();
            move |id, kind_idx, year| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_rectify_add_event(&id, kind_idx, year);
                }
            }
        });
        self.window.on_jyotish_rectify_remove_event({
            let t = t.clone();
            move |id, idx| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_rectify_remove_event(&id, idx);
                }
            }
        });
        self.window.on_jyotish_rectify_next({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_rectify_next(&id);
                }
            }
        });
        self.window.on_jyotish_rectify_back({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_rectify_back(&id);
                }
            }
        });
        self.window.on_jyotish_rectify_refine({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_rectify_refine(&id);
                }
            }
        });
        self.window.on_jyotish_rectify_accept({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_rectify_accept(&id);
                }
            }
        });
        self.window.on_jyotish_rectify_cancel({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_rectify_cancel(&id);
                }
            }
        });
        self.window.on_jyotish_export_day({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_export_day(&id);
                }
            }
        });
        self.window.on_jyotish_export_week({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_export_week(&id);
                }
            }
        });
        self.window.on_jyotish_open_cities({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_open_cities(&id);
                }
            }
        });
        self.window.on_jyotish_close_cities({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_close_cities(&id);
                }
            }
        });
        self.window.on_jyotish_select_city({
            let t = t.clone();
            move |id, idx| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_select_city(&id, idx);
                }
            }
        });
        self.window.on_jyotish_remove_city({
            let t = t.clone();
            move |id, idx| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_remove_city(&id, idx);
                }
            }
        });
        self.window.on_jyotish_search_cities({
            let t = t.clone();
            move |id, q| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_search_cities(&id, &q);
                }
            }
        });
        self.window.on_jyotish_add_city({
            let t = t.clone();
            move |id, name, lat, lon| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_add_city(&id, &name, lat, lon);
                }
            }
        });

        self.window.on_jyotish_open_profiles({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_open_profiles(&id);
                }
            }
        });
        self.window.on_jyotish_close_profiles({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_close_profiles(&id);
                }
            }
        });
        self.window.on_jyotish_select_profile({
            let t = t.clone();
            move |id, idx| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_select_profile(&id, idx);
                }
            }
        });
        self.window.on_jyotish_remove_profile({
            let t = t.clone();
            move |id, idx| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_remove_profile(&id, idx);
                }
            }
        });
        self.window.on_jyotish_begin_add_profile({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_begin_add_profile(&id);
                }
            }
        });
        self.window.on_jyotish_begin_edit_profile({
            let t = t.clone();
            move |id, idx| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_begin_edit_profile(&id, idx);
                }
            }
        });
        self.window.on_jyotish_cancel_edit_profile({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_cancel_edit_profile(&id);
                }
            }
        });
        self.window.on_jyotish_set_profile_gender({
            let t = t.clone();
            move |id, g| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_set_profile_gender(&id, g);
                }
            }
        });
        self.window.on_jyotish_search_birth_places({
            let t = t.clone();
            move |id, q| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_search_birth_places(&id, &q);
                }
            }
        });
        self.window.on_jyotish_set_birth_place({
            let t = t.clone();
            move |id, name, lat, lon, tz| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_set_birth_place(&id, &name, lat, lon, &tz);
                }
            }
        });
        self.window.on_jyotish_nav_profile_cal({
            let t = t.clone();
            move |id, delta| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_nav_profile_cal(&id, delta);
                }
            }
        });
        self.window.on_jyotish_set_profile_cal_mode({
            let t = t.clone();
            move |id, mode| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_set_profile_cal_mode(&id, mode);
                }
            }
        });
        self.window.on_jyotish_set_profile_cal_day({
            let t = t.clone();
            move |id, day| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_set_profile_cal_day(&id, day);
                }
            }
        });
        self.window.on_jyotish_set_profile_time({
            let t = t.clone();
            move |id, hour, minute| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_set_profile_time(&id, hour, minute);
                }
            }
        });
        self.window.on_jyotish_nudge_profile_offset({
            let t = t.clone();
            move |id, minutes| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_nudge_profile_offset(&id, minutes);
                }
            }
        });
        self.window.on_jyotish_upsert_profile({
            let t = t.clone();
            move |id, index, name, gender, date, time, offset, place, lat, lon| {
                if let Some(c) = t.upgrade() {
                    c.on_jyotish_upsert_profile(
                        &id, index, &name, gender, &date, &time, offset, &place, lat, lon,
                    );
                }
            }
        });
    }
}
