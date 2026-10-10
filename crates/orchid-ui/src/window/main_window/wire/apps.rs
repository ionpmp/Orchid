//! Media, calculator, notes, browser, calendar.

use std::sync::{Arc, Weak};

use uuid::Uuid;

use crate::window::main_window::MainWindowController;

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
        self.window.on_agent_send({
            let t = t.clone();
            move |id, text| {
                if let Some(c) = t.upgrade() {
                    c.on_agent_send(&id, &text);
                }
            }
        });
        self.window.on_agent_confirm({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_agent_confirm(&id);
                }
            }
        });
        self.window.on_agent_dismiss({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_agent_dismiss(&id);
                }
            }
        });
        self.window.on_agent_clear({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_agent_clear(&id);
                }
            }
        });
        self.window.on_mail_select_account({
            let t = t.clone();
            move |id, account| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_select_account(&id, &account);
                }
            }
        });
        self.window.on_mail_select_folder({
            let t = t.clone();
            move |id, folder| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_select_folder(&id, &folder);
                }
            }
        });
        self.window.on_mail_select_message({
            let t = t.clone();
            move |id, uid| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_select_message(&id, uid);
                }
            }
        });
        self.window.on_mail_search({
            let t = t.clone();
            move |id, query| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_search(&id, &query);
                }
            }
        });
        self.window.on_mail_save_attachment({
            let t = t.clone();
            move |id, part| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_save_attachment(&id, &part);
                }
            }
        });
        self.window.on_mail_refresh({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_refresh(&id);
                }
            }
        });
        self.window.on_mail_open_wizard({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_open_wizard(&id);
                }
            }
        });
        self.window.on_mail_show_mailbox({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_show_mailbox(&id);
                }
            }
        });
        self.window.on_mail_wizard_set({
            let t = t.clone();
            move |id, field, value| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_wizard_set(&id, &field, &value);
                }
            }
        });
        self.window.on_mail_wizard_discover({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_wizard_discover(&id);
                }
            }
        });
        self.window.on_mail_wizard_save({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_wizard_save(&id);
                }
            }
        });
        self.window.on_mail_wizard_oauth({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_wizard_oauth(&id);
                }
            }
        });
        self.window.on_mail_open_compose({
            let t = t.clone();
            move |id, kind| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_open_compose(&id, &kind);
                }
            }
        });
        self.window.on_mail_compose_set({
            let t = t.clone();
            move |id, field, value| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_compose_set(&id, &field, &value);
                }
            }
        });
        self.window.on_mail_compose_send({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_compose_send(&id);
                }
            }
        });
        self.window.on_mail_compose_draft({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_compose_draft(&id);
                }
            }
        });
        self.window.on_mail_compose_attach({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_compose_attach(&id);
                }
            }
        });
        self.window.on_mail_compose_remove({
            let t = t.clone();
            move |id, path| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_compose_remove(&id, &path);
                }
            }
        });
        self.window.on_mail_toggle_seen({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_toggle_seen(&id);
                }
            }
        });
        self.window.on_mail_toggle_flagged({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_toggle_flagged(&id);
                }
            }
        });
        self.window.on_mail_mark_read({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_mark_read(&id);
                }
            }
        });
        self.window.on_mail_mark_unread({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_mark_unread(&id);
                }
            }
        });
        self.window.on_mail_delete({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_delete(&id);
                }
            }
        });
        self.window.on_mail_move({
            let t = t.clone();
            move |id, path| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_move(&id, &path);
                }
            }
        });
        self.window.on_mail_set_remote_images({
            let t = t.clone();
            move |id, allow| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_set_remote_images(&id, allow);
                }
            }
        });
        self.window.on_mail_embed_bounds({
            let t = t.clone();
            move |id, x, y, w, h, vis| {
                if let Some(c) = t.upgrade() {
                    c.on_mail_embed_bounds(&id, x, y, w, h, vis);
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
        self.window.on_calendar_caldav_save({
            let t = t.clone();
            move |id, url, user, password| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_caldav_save(&id, &url, &user, &password);
                }
            }
        });
        self.window.on_calendar_caldav_sync({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_calendar_caldav_sync(&id);
                }
            }
        });
        self.window.on_contacts_select({
            let t = t.clone();
            move |id, card| {
                if let Some(c) = t.upgrade() {
                    c.on_contacts_select(&id, &card);
                }
            }
        });
        self.window.on_contacts_new({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_contacts_new(&id);
                }
            }
        });
        self.window.on_contacts_set_field({
            let t = t.clone();
            move |id, key, value| {
                if let Some(c) = t.upgrade() {
                    c.on_contacts_set_field(&id, &key, &value);
                }
            }
        });
        self.window.on_contacts_save({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_contacts_save(&id);
                }
            }
        });
        self.window.on_contacts_delete({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_contacts_delete(&id);
                }
            }
        });
        self.window.on_contacts_save_account({
            let t = t.clone();
            move |id, url, user, password| {
                if let Some(c) = t.upgrade() {
                    c.on_contacts_save_account(&id, &url, &user, &password);
                }
            }
        });
        self.window.on_contacts_sync({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_contacts_sync(&id);
                }
            }
        });
        self.window.on_contacts_select({
            let t = t.clone();
            move |id, card| {
                if let Some(c) = t.upgrade() {
                    c.on_contacts_select(&id, &card);
                }
            }
        });
        self.window.on_contacts_new({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_contacts_new(&id);
                }
            }
        });
        self.window.on_contacts_set_field({
            let t = t.clone();
            move |id, key, value| {
                if let Some(c) = t.upgrade() {
                    c.on_contacts_set_field(&id, &key, &value);
                }
            }
        });
        self.window.on_contacts_save({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_contacts_save(&id);
                }
            }
        });
        self.window.on_contacts_delete({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_contacts_delete(&id);
                }
            }
        });
        self.window.on_contacts_save_account({
            let t = t.clone();
            move |id, url, user, password| {
                if let Some(c) = t.upgrade() {
                    c.on_contacts_save_account(&id, &url, &user, &password);
                }
            }
        });
        self.window.on_contacts_sync({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_contacts_sync(&id);
                }
            }
        });
    }
}
