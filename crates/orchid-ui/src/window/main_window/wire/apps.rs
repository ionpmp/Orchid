//! Media, calculator, notes, browser, calendar.

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
    pub(super) fn wire_apps(self: &Arc<Self>, t: &Weak<Self>) {
        self.window.on_media_play_pause({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_media_play_pause();
                }
            }
        });
        self.window.on_media_next({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_media_command("next");
                }
            }
        });
        self.window.on_media_previous({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_media_command("previous");
                }
            }
        });
        self.window.on_audio_player_command({
            let t = t.clone();
            move |id, cmd| {
                if let Some(c) = t.upgrade() {
                    c.on_audio_player_command(&id, &cmd);
                }
            }
        });
        self.window.on_video_player_command({
            let t = t.clone();
            move |id, cmd| {
                if let Some(c) = t.upgrade() {
                    c.on_video_player_command(&id, &cmd);
                }
            }
        });

        self.window.on_calculator_button({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_calculator_button(&id);
                }
            }
        });
        self.window.on_calculator_key({
            let t = t.clone();
            move |text, ctrl, shift| {
                if let Some(c) = t.upgrade() {
                    c.on_calculator_key(&text, ctrl, shift);
                }
            }
        });
        self.window.on_calculator_history({
            let t = t.clone();
            move |index| {
                if let Some(c) = t.upgrade() {
                    c.on_calculator_history(index);
                }
            }
        });

        self.window.on_notes_body_changed({
            let t = t.clone();
            move |id, text| {
                if let Some(c) = t.upgrade() {
                    c.on_notes_body_changed(&id, &text);
                }
            }
        });
        self.window.on_notes_title_changed({
            let t = t.clone();
            move |id, text| {
                if let Some(c) = t.upgrade() {
                    c.on_notes_title_changed(&id, &text);
                }
            }
        });
        self.window.on_notes_select_tab({
            let t = t.clone();
            move |id, index| {
                if let Some(c) = t.upgrade() {
                    c.on_notes_select_tab(&id, index);
                }
            }
        });
        self.window.on_notes_new_tab({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_notes_new_tab(&id);
                }
            }
        });
        self.window.on_notes_close_tab({
            let t = t.clone();
            move |id, index| {
                if let Some(c) = t.upgrade() {
                    c.on_notes_close_tab(&id, index);
                }
            }
        });
        self.window.on_notes_toggle_wrap({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_notes_toggle_wrap(&id);
                }
            }
        });
        self.window.on_notes_toggle_mono({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_notes_toggle_mono(&id);
                }
            }
        });
        self.window.on_notes_zoom({
            let t = t.clone();
            move |id, delta| {
                if let Some(c) = t.upgrade() {
                    c.on_notes_zoom(&id, delta);
                }
            }
        });
        self.window.on_notes_clear({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_notes_clear(&id);
                }
            }
        });
        self.window.on_notes_find({
            let t = t.clone();
            move |id, query, forward| {
                if let Some(c) = t.upgrade() {
                    c.on_notes_find(&id, &query, forward);
                }
            }
        });
        self.window.on_browser_select_tab({
            let t = t.clone();
            move |id, index| {
                if let Some(c) = t.upgrade() {
                    c.on_browser_select_tab(&id, index);
                }
            }
        });
        self.window.on_browser_new_tab({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_browser_new_tab(&id);
                }
            }
        });
        self.window.on_browser_close_tab({
            let t = t.clone();
            move |id, index| {
                if let Some(c) = t.upgrade() {
                    c.on_browser_close_tab(&id, index);
                }
            }
        });
        self.window.on_browser_navigate({
            let t = t.clone();
            move |id, url| {
                if let Some(c) = t.upgrade() {
                    c.on_browser_navigate(&id, &url);
                }
            }
        });
        self.window.on_browser_command({
            let t = t.clone();
            move |id, cmd| {
                if let Some(c) = t.upgrade() {
                    c.on_browser_command(&id, &cmd);
                }
            }
        });
        self.window.on_browser_open_external({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_browser_open_external(&id);
                }
            }
        });
        self.window.on_browser_find({
            let t = t.clone();
            move |id, query, forward| {
                if let Some(c) = t.upgrade() {
                    c.on_browser_find(&id, &query, forward);
                }
            }
        });
        self.window.on_browser_toggle_bookmark({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_browser_toggle_bookmark(&id);
                }
            }
        });
        self.window.on_browser_open_bookmark({
            let t = t.clone();
            move |id, index| {
                if let Some(c) = t.upgrade() {
                    c.on_browser_open_bookmark(&id, index);
                }
            }
        });
        self.window.on_browser_remove_bookmark({
            let t = t.clone();
            move |id, index| {
                if let Some(c) = t.upgrade() {
                    c.on_browser_remove_bookmark(&id, index);
                }
            }
        });
        self.window.on_browser_embed_bounds({
            let t = t.clone();
            move |id, x, y, w, h, vis| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        c.on_browser_embed_bounds(inst, x, y, w, h, vis);
                    }
                }
            }
        });

        self.window.on_calendar_select_date({
            let t = t.clone();
            move |id, date| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_select_date(&id, &date);
                }
            }
        });
        self.window.on_calendar_activate_day({
            let t = t.clone();
            move |id, date| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_activate_day(&id, &date);
                }
            }
        });
        self.window.on_calendar_shift_month({
            let t = t.clone();
            move |id, delta| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_shift_month(&id, delta);
                }
            }
        });
        self.window.on_calendar_goto_today({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_goto_today(&id);
                }
            }
        });
        self.window.on_calendar_open_new({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_open_new(&id);
                }
            }
        });
        self.window.on_calendar_open_edit({
            let t = t.clone();
            move |id, event_id| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_open_edit(&id, &event_id);
                }
            }
        });
        self.window.on_calendar_close_editor({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_close_editor(&id);
                }
            }
        });
        self.window.on_calendar_save_editor({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_save_editor(&id);
                }
            }
        });
        self.window.on_calendar_duplicate_editor({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_duplicate_editor(&id);
                }
            }
        });
        self.window.on_calendar_set_color_filter({
            let t = t.clone();
            move |id, color| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_set_color_filter(&id, color);
                }
            }
        });
        self.window.on_calendar_request_delete({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_request_delete(&id);
                }
            }
        });
        self.window.on_calendar_confirm_delete({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_confirm_delete(&id);
                }
            }
        });
        self.window.on_calendar_cancel_delete({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_cancel_delete(&id);
                }
            }
        });
        self.window.on_calendar_editor_title({
            let t = t.clone();
            move |id, text| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_editor_title(&id, &text);
                }
            }
        });
        self.window.on_calendar_editor_date({
            let t = t.clone();
            move |id, date| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_editor_date(&id, &date);
                }
            }
        });
        self.window.on_calendar_shift_editor_date({
            let t = t.clone();
            move |id, delta| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_shift_editor_date(&id, delta);
                }
            }
        });
        self.window.on_calendar_editor_all_day({
            let t = t.clone();
            move |id, all_day| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_editor_all_day(&id, all_day);
                }
            }
        });
        self.window.on_calendar_nudge_editor_start({
            let t = t.clone();
            move |id, delta| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_nudge_editor_start(&id, delta);
                }
            }
        });
        self.window.on_calendar_nudge_editor_end({
            let t = t.clone();
            move |id, delta| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_nudge_editor_end(&id, delta);
                }
            }
        });
        self.window.on_calendar_editor_notes({
            let t = t.clone();
            move |id, text| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_editor_notes(&id, &text);
                }
            }
        });
        self.window.on_calendar_editor_color({
            let t = t.clone();
            move |id, color| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_editor_color(&id, color);
                }
            }
        });
    }
}
