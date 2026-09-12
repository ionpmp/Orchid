//! File-manager callbacks.

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
    pub(super) fn wire_fm(self: &Arc<Self>, t: &Weak<Self>) {
        self.window.on_fm_sidebar_clicked({
            let t = t.clone();
            move |fm_id, id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_sidebar_clicked(&fm_id, &id);
                }
            }
        });
        self.window.on_fm_toggle_dual_pane({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_toggle_dual_pane(&fm_id);
                }
            }
        });
        self.window.on_fm_toggle_show_hidden({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_toggle_show_hidden(&fm_id);
                }
            }
        });
        self.window.on_fm_toggle_click_behavior({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_toggle_click_behavior(&fm_id);
                }
            }
        });
        self.window.on_fm_pane_clicked({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_pane_clicked(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_tab_clicked({
            let t = t.clone();
            move |fm_id, pane, tab_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_tab_clicked(&fm_id, pane, &tab_id);
                }
            }
        });
        self.window.on_fm_tab_closed({
            let t = t.clone();
            move |fm_id, pane, tab_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_tab_closed(&fm_id, pane, &tab_id);
                }
            }
        });
        self.window.on_fm_tab_new({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_tab_new(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_new_folder({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_new_folder(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_nav_back({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_nav_back(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_nav_forward({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_nav_forward(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_nav_up({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_nav_up(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_nav_home({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_nav_home(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_history_pick({
            let t = t.clone();
            move |fm_id, pane, path| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_history_pick(&fm_id, pane, &path);
                }
            }
        });
        self.window.on_fm_breadcrumb_clicked({
            let t = t.clone();
            move |fm_id, pane, path| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_breadcrumb_clicked(&fm_id, pane, &path);
                }
            }
        });
        self.window.on_fm_path_edit_changed({
            let t = t.clone();
            move |fm_id, pane, typed| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_path_edit_changed(&fm_id, pane, &typed);
                }
            }
        });
        self.window.on_fm_path_edit_commit({
            let t = t.clone();
            move |fm_id, pane, path| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_path_edit_commit(&fm_id, pane, &path);
                }
            }
        });
        self.window.on_fm_view_mode_cycle({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_view_mode_cycle(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_sort_cycle({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_sort_cycle(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_sort_column_clicked({
            let t = t.clone();
            move |fm_id, pane, col| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_sort_column_clicked(&fm_id, pane, col);
                }
            }
        });
        self.window.on_fm_quick_filter_changed({
            let t = t.clone();
            move |fm_id, pane, q| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_quick_filter_changed(&fm_id, pane, &q);
                }
            }
        });
        self.window.on_fm_viewport_changed({
            let t = t.clone();
            move |fm_id, pane, y, h, w| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_viewport_changed(&fm_id, pane, y, h, w);
                }
            }
        });
        self.window.on_fm_entry_clicked({
            let t = t.clone();
            move |fm_id, pane, path, ctrl| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_clicked(&fm_id, pane, &path, ctrl);
                }
            }
        });
        self.window.on_fm_entry_shift_clicked({
            let t = t.clone();
            move |fm_id, pane, path| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_shift_clicked(&fm_id, pane, &path);
                }
            }
        });
        self.window.on_fm_entry_double_clicked({
            let t = t.clone();
            move |fm_id, pane, path, is_dir| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_double_clicked(&fm_id, pane, &path, is_dir);
                }
            }
        });
        self.window.on_fm_entry_context({
            let t = t.clone();
            move |fm_id, pane, path, x, y| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_context(&fm_id, pane, &path, x, y);
                }
            }
        });
        self.window.on_fm_context_action({
            let t = t.clone();
            move |fm_id, action_id, paths| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_context_action(&fm_id, &action_id, &paths);
                }
            }
        });
        self.window.on_fm_context_dismiss({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_context_dismiss(&fm_id);
                }
            }
        });
        self.window.on_fm_confirm_yes({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_confirm_yes(&fm_id);
                }
            }
        });
        self.window.on_fm_confirm_no({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_confirm_no(&fm_id);
                }
            }
        });
        self.window.on_fm_conflict_choice({
            let t = t.clone();
            move |fm_id, choice, apply_all| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_conflict_choice(&fm_id, &choice, apply_all);
                }
            }
        });
        self.window.on_fm_rename_commit({
            let t = t.clone();
            move |fm_id, old_path, new_name| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_rename_commit(&fm_id, &old_path, &new_name);
                }
            }
        });
        self.window.on_fm_rename_cancel({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_rename_cancel(&fm_id);
                }
            }
        });
        self.window.on_fm_tag_commit({
            let t = t.clone();
            move |fm_id, tag| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_tag_commit(&fm_id, &tag);
                }
            }
        });
        self.window.on_fm_tag_cancel({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_tag_cancel(&fm_id);
                }
            }
        });
        self.window.on_fm_passphrase_commit({
            let t = t.clone();
            move |fm_id, pw| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_passphrase_commit(&fm_id, &pw);
                }
            }
        });
        self.window.on_fm_passphrase_cancel({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_passphrase_cancel(&fm_id);
                }
            }
        });
        self.window.on_fm_passphrase_biometric({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_passphrase_biometric(&fm_id);
                }
            }
        });
        self.window.on_fm_managed_policy_close({
            let t = t.clone();
            move |fm_id| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_managed_policy_close(&fm_id);
                }
            }
        });
        self.window.on_fm_select_all({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_select_all(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_selection_command({
            let t = t.clone();
            move |fm_id, pane, cmd| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_selection_command(&fm_id, pane, &cmd);
                }
            }
        });
        self.window.on_fm_nav_command({
            let t = t.clone();
            move |fm_id, pane, cmd| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_nav_command(&fm_id, pane, &cmd);
                }
            }
        });
        self.window.on_fm_marquee_select({
            let t = t.clone();
            move |fm_id, pane, from, to, additive, columns| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_marquee_select(&fm_id, pane, from, to, additive, columns);
                }
            }
        });
        self.window.on_fm_delete_selected({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_delete_selected(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_copy_selected({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_copy_selected(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_paste_clipboard({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_paste_clipboard(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_rename_selected({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_rename_selected(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_deselect_all({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_deselect_all(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_open_selected({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_open_selected(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_move_selection({
            let t = t.clone();
            move |fm_id, pane, delta, extend| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_move_selection(&fm_id, pane, delta, extend);
                }
            }
        });
        self.window.on_fm_entry_drag_start({
            let t = t.clone();
            move |fm_id, pane, path| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_drag_start(&fm_id, pane, &path);
                }
            }
        });
        self.window.on_fm_entry_drag_hover({
            let t = t.clone();
            move |fm_id, pane, path| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_drag_hover(&fm_id, pane, &path);
                }
            }
        });
        self.window.on_fm_entry_drag_drop({
            let t = t.clone();
            move |fm_id, pane, path| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_drag_drop(&fm_id, pane, &path);
                }
            }
        });
        self.window.on_fm_entry_drag_cancel({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_drag_cancel(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_pane_drag_hover({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_pane_drag_hover(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_drop_on_current_dir({
            let t = t.clone();
            move |fm_id, pane| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_drop_on_current_dir(&fm_id, pane);
                }
            }
        });
        self.window.on_fm_entry_drag_scroll({
            let t = t.clone();
            move |fm_id, pane, mouse_x, mouse_y, viewport_y, width| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_entry_drag_scroll(&fm_id, pane, mouse_x, mouse_y, viewport_y, width);
                }
            }
        });
        self.window.on_fm_error_action({
            let t = t.clone();
            move |_fm_id, _pane| {
                if let Some(c) = t.upgrade() {
                    c.open_config_file();
                }
            }
        });
    }
}
