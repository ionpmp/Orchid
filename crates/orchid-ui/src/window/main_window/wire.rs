//! Slint callback wiring for [`MainWindowController`].

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use slint::ComponentHandle;

use tracing::warn;
use uuid::Uuid;


use crate::window::errors::{
    ui_localized_error, viewer_localized_error,
};
use crate::window::spawn;
use crate::error::Result;
use crate::slint_generated::NotificationGlobal;


use super::{MainWindowController, PasswordCopyKind};

impl MainWindowController {
        pub(super) fn wire_callbacks(self: &Arc<Self>) -> Result<()> {
        let t = Arc::downgrade(self);
        self.window.on_ui_tick({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.drain_fm_ingest_failure_notification();
                    if c.config_reload_pending.swap(false, Ordering::AcqRel) {
                        if let Err(e) = c.apply_hot_config() {
                            warn!(?e, "config hot-reload");
                            let reason = ui_localized_error(&c.locale, &e);
                            let body = c.locale.tr_args(
                                "settings-config-reload-failed",
                                &orchid_i18n::FluentArgs::new().with("reason", reason),
                            );
                            c.push_notification(
                                &c.locale.tr("settings-panel-title"),
                                &body,
                                2,
                            );
                        }
                    }
                    let canvas_size_mismatch = c.sync_canvas_size_from_winit();
                    if canvas_size_mismatch {
                        c.update_gesture_bounds();
                        if c.config.read().appearance.density == orchid_storage::Density::Hybrid {
                            let _ = c.apply_theme();
                        }
                    }
                    let gestures = {
                        let mut rec = c.gesture_recognizer.lock();
                        rec.tick(Instant::now())
                    };
                    c.handle_recognized_gestures(gestures);
                    c.check_vault_auto_lock();
                    let scale = c.window.window().scale_factor();
                    let scale_changed = {
                        let mut last = c.last_window_scale.lock();
                        if (scale - *last).abs() > 0.001 {
                            *last = scale;
                            true
                        } else {
                            false
                        }
                    };
                    let rebuild_flag = c.rebuild_pending.swap(false, Ordering::AcqRel);
                    let from_layout = rebuild_flag || canvas_size_mismatch;
                    let need_full = from_layout || scale_changed;
                    // While the user drags or resizes, full rebuild + terminal patch are far too
                    // heavy to run on every ~60Hz tick; defer until the gesture ends.
                    // Do not require `!canvas_size_mismatch`: winit can report sub-pixel / jittery
                    // size every frame; that would set `from_layout` and force a full rebuild
                    // during drag, undoing the preview path. `sync_canvas_size_from_winit` still
                    // runs so `canvas_size` stays current; a pending rebuild flushes when the
                    // gesture ends. We only bypass defer for scale (DPR) changes, which are rare
                    // mid-gesture but need a full pass immediately.
                    let live_gesture = {
                        let d = c.drag_offset.lock();
                        let r = c.resize_override.lock();
                        !d.is_empty() || !r.is_empty()
                    };
                    let defer_heavy = live_gesture && !scale_changed;
                    if need_full {
                        if defer_heavy {
                            c.rebuild_pending.store(true, Ordering::Release);
                        } else {
                            c.widget_manager.drain_frame_dirty_ids();
                            let _ = c.rebuild_workspace_model();
                        }
                    } else if !defer_heavy {
                        let dirty = c.widget_manager.drain_frame_dirty_ids();
                        if !dirty.is_empty() {
                            let _ = c.patch_workspace_frames(&dirty);
                        }
                    }
                }
            }
        });
        self.window.on_get_started_clicked({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_get_started();
                }
            }
        });
        self.window.on_workspace_clicked({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_workspace_clicked(&id);
                }
            }
        });
        self.window.on_workspace_create_clicked({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_workspace_create();
                }
            }
        });
        self.window.on_workspace_orb_dismiss({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_workspace_orb_dismiss();
                }
            }
        });
        self.window.on_canvas_long_pressed({
            let t = t.clone();
            move |cx, cy, vx, vy| {
                if let Some(c) = t.upgrade() {
                    c.on_canvas_long_pressed(cx, cy, vx, vy);
                }
            }
        });
        self.window.on_canvas_scrolled({
            let t = t.clone();
            move |vx, vy| {
                if let Some(c) = t.upgrade() {
                    c.on_canvas_scrolled(vx, vy);
                }
            }
        });
        self.window.on_catalog_pick({
            let t = t.clone();
            move |type_id| {
                if let Some(c) = t.upgrade() {
                    c.on_catalog_pick(&type_id);
                }
            }
        });
        self.window.on_catalog_dismiss({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_catalog_dismiss();
                }
            }
        });
        self.window.on_catalog_search_changed({
            let t = t.clone();
            move |q| {
                if let Some(c) = t.upgrade() {
                    c.on_catalog_search_changed(&q);
                }
            }
        });
        self.window.on_command_palette_query_changed({
            let t = t.clone();
            move |q| {
                if let Some(c) = t.upgrade() {
                    c.on_command_palette_query_changed(&q);
                }
            }
        });
        self.window.on_command_palette_candidate_activated({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_command_palette_candidate_activated(&id);
                }
            }
        });
        self.window.on_command_palette_selection_changed({
            let t = t.clone();
            move |idx| {
                if let Some(c) = t.upgrade() {
                    c.on_command_palette_selection_changed(idx);
                }
            }
        });
        self.window.on_command_palette_dismiss({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_command_palette_dismiss();
                }
            }
        });
        self.window.on_settings_dismiss({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_settings_dismiss();
                }
            }
        });
        self.window.on_settings_section_selected({
            let t = t.clone();
            move |idx| {
                if let Some(c) = t.upgrade() {
                    c.on_settings_section_selected(idx);
                }
            }
        });
        self.window.on_settings_field_changed({
            let t = t.clone();
            move |section, key, value| {
                if let Some(c) = t.upgrade() {
                    c.on_settings_field_changed(&section, &key, &value);
                }
            }
        });
        self.window.on_settings_open_config({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.open_config_file();
                }
            }
        });
        self.window.on_notification_center_dismiss({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_notification_center_dismiss();
                }
            }
        });
        self.window.global::<NotificationGlobal>().on_clear_all({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.clear_notifications();
                }
            }
        });
        self.window.global::<NotificationGlobal>().on_dismiss_item({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.dismiss_notification(id.as_str());
                }
            }
        });
        self.window.on_onboarding_next_clicked({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_onboarding_next();
                }
            }
        });
        self.window.on_onboarding_back_clicked({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_onboarding_back();
                }
            }
        });
        self.window.on_onboarding_skip_clicked({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.on_onboarding_skip();
                }
            }
        });
        self.window.on_widget_close_clicked({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_close(&id);
                }
            }
        });
        self.window.on_widget_close_confirm_save({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_close_confirm_save(&id);
                }
            }
        });
        self.window.on_widget_close_confirm_discard({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_close_confirm_discard(&id);
                }
            }
        });
        self.window.on_widget_close_confirm_cancel({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_close_confirm_cancel(&id);
                }
            }
        });
        self.window.on_widget_drag_started({
            let t = t.clone();
            move |id, lx, ly| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_drag_started(&id, lx, ly);
                }
            }
        });
        self.window.on_widget_drag_moved({
            let t = t.clone();
            move |id, canvas_x, canvas_y| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_drag_moved(&id, canvas_x, canvas_y);
                }
            }
        });
        self.window.on_widget_drag_ended({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_drag_ended(&id);
                }
            }
        });
        self.window.on_widget_resize_started({
            let t = t.clone();
            move |id, corner, press_x, press_y| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_resize_started(&id, &corner, press_x, press_y);
                }
            }
        });
        self.window.on_widget_resize_moved({
            let t = t.clone();
            move |id, canvas_x, canvas_y| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_resize_moved(&id, canvas_x, canvas_y);
                }
            }
        });
        self.window.on_widget_resize_ended({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    c.on_widget_resize_ended(&id);
                }
            }
        });
        self.window.on_group_tab_clicked({
            let t = t.clone();
            move |group_id, member_id| {
                if let Some(c) = t.upgrade() {
                    c.on_group_tab_clicked(&group_id, &member_id);
                }
            }
        });
        self.window.on_group_tab_closed({
            let t = t.clone();
            move |group_id, member_id| {
                if let Some(c) = t.upgrade() {
                    c.on_group_tab_closed(&group_id, &member_id);
                }
            }
        });
        self.window.on_group_tab_move_left({
            let t = t.clone();
            move |group_id, member_id| {
                if let Some(c) = t.upgrade() {
                    c.on_group_tab_move(&group_id, &member_id, -1);
                }
            }
        });
        self.window.on_group_tab_move_right({
            let t = t.clone();
            move |group_id, member_id| {
                if let Some(c) = t.upgrade() {
                    c.on_group_tab_move(&group_id, &member_id, 1);
                }
            }
        });
        self.window.on_group_dissolve_clicked({
            let t = t.clone();
            move |group_id| {
                if let Some(c) = t.upgrade() {
                    c.on_group_dissolve_clicked(&group_id);
                }
            }
        });
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
        self.window.on_rss_item_clicked({
            let t = t.clone();
            move |link| {
                if let Some(c) = t.upgrade() {
                    c.on_rss_item_clicked(&link);
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
            move |title, username, password, url| {
                if let Some(c) = t.upgrade() {
                    c.on_password_add_entry_commit(&title, &username, &password, &url);
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

        macro_rules! viewer_spawn {
            ($weak:expr, $fut:expr) => {{
                let tw = $weak.clone();
                spawn::spawn_local_compat(async move {
                    if let Err(e) = $fut.await {
                        warn!(?e, "viewer action");
                        if let Some(c) = tw.upgrade() {
                            let title = c.locale.tr("widget-viewer-name");
                            let reason = viewer_localized_error(&c.locale, &e.to_string());
                            let body = c.locale.tr_args(
                                "viewer-action-failed",
                                &orchid_i18n::FluentArgs::new().with("reason", reason),
                            );
                            c.push_notification(&title, &body, 3);
                        }
                    }
                    // Snapshot updates publish WidgetSnapshotUpdated → frame-dirty →
                    // patch_workspace_frames. A full workspace rebuild here made pan /
                    // scroll / zoom hitch on every interaction.
                });
            }};
        }

        self.window.on_viewer_image_zoom_in({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::image_zoom_in(inst));
                    }
                }
            }
        });
        self.window.on_viewer_image_zoom_out({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::image_zoom_out(inst));
                    }
                }
            }
        });
        self.window.on_viewer_image_fit({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::image_fit(inst));
                    }
                }
            }
        });
        self.window.on_viewer_image_actual_size({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::image_actual_size(inst));
                    }
                }
            }
        });
        self.window.on_viewer_image_rotate_cw({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::image_rotate_cw(inst));
                    }
                }
            }
        });
        self.window.on_viewer_image_rotate_ccw({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::image_rotate_ccw(inst));
                    }
                }
            }
        });
        self.window.on_viewer_image_flip_h({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::image_flip_h(inst));
                    }
                }
            }
        });
        self.window.on_viewer_image_flip_v({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::image_flip_v(inst));
                    }
                }
            }
        });
        self.window.on_viewer_image_pan({
            let t = t.clone();
            move |id, dx, dy| {
                if t.upgrade().is_some() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        spawn::spawn_local_compat(async move {
                            if let Err(e) =
                                orchid_widgets::builtin::viewer::image_pan(inst, dx, dy).await
                            {
                                warn!(?e, "viewer pan");
                            }
                            // Frame patch via WidgetSnapshotUpdated (no full rebuild).
                        });
                    }
                }
            }
        });
        self.window.on_viewer_viewport_changed({
            let t = t.clone();
            move |id, w, h| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(
                            tw,
                            orchid_widgets::builtin::viewer::set_viewport(inst, w, h)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_pdf_prev_page({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::pdf_prev_page(inst));
                    }
                }
            }
        });
        self.window.on_viewer_pdf_next_page({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::pdf_next_page(inst));
                    }
                }
            }
        });
        self.window.on_viewer_pdf_fit_width({
            let t = t.clone();
            move |id, vw| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(
                            tw,
                            orchid_widgets::builtin::viewer::pdf_fit_width(inst, vw)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_pdf_fit_page({
            let t = t.clone();
            move |id, vw, vh| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(
                            tw,
                            orchid_widgets::builtin::viewer::pdf_fit_page(inst, vw, vh)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_pdf_zoom_in({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::pdf_zoom_in(inst));
                    }
                }
            }
        });
        self.window.on_viewer_pdf_zoom_out({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::pdf_zoom_out(inst));
                    }
                }
            }
        });
        self.window.on_viewer_pdf_go_to_page({
            let t = t.clone();
            move |id, page| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::pdf_go_to_page(inst, page));
                    }
                }
            }
        });
        self.window.on_viewer_pdf_copy_text({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        spawn::spawn_local_compat(async move {
                            let text = match orchid_widgets::builtin::viewer::pdf_current_page_text(
                                inst,
                            )
                            .await
                            {
                                Ok(t) => t,
                                Err(e) => {
                                    warn!(?e, "viewer pdf copy text");
                                    if let Some(c) = tw.upgrade() {
                                        let title = c.locale.tr("widget-viewer-name");
                                        let reason =
                                            viewer_localized_error(&c.locale, &e.to_string());
                                        let body = c.locale.tr_args(
                                            "viewer-action-failed",
                                            &orchid_i18n::FluentArgs::new().with("reason", reason),
                                        );
                                        c.push_notification(&title, &body, 3);
                                    }
                                    return;
                                }
                            };
                            let Some(c) = tw.upgrade() else {
                                return;
                            };
                            match crate::widgets::terminal::ArboardClipboard::new() {
                                Ok(cb) => {
                                    if let Err(e) = cb.copy(&text) {
                                        warn!(?e, "viewer pdf clipboard copy");
                                        let title = c.locale.tr("widget-viewer-name");
                                        let body = c.locale.tr("viewer-pdf-copy-failed");
                                        c.push_notification(&title, &body, 3);
                                    } else {
                                        let title = c.locale.tr("widget-viewer-name");
                                        let body = if text.trim().is_empty() {
                                            c.locale.tr("viewer-pdf-copy-empty")
                                        } else {
                                            c.locale.tr("viewer-pdf-copied")
                                        };
                                        c.push_notification(&title, &body, 1);
                                    }
                                }
                                Err(e) => {
                                    warn!(?e, "viewer pdf clipboard open");
                                    let title = c.locale.tr("widget-viewer-name");
                                    let body = c.locale.tr("viewer-pdf-copy-failed");
                                    c.push_notification(&title, &body, 3);
                                }
                            }
                        });
                    }
                }
            }
        });
        self.window.on_viewer_archive_navigate_into({
            let t = t.clone();
            move |id, path| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let p = path.to_string();
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(
                            tw,
                            orchid_widgets::builtin::viewer::archive_navigate_into(inst, p)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_archive_navigate_up({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::archive_navigate_up(inst));
                    }
                }
            }
        });
        self.window.on_viewer_archive_select({
            let t = t.clone();
            move |id, path| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let p = path.to_string();
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(
                            tw,
                            orchid_widgets::builtin::viewer::archive_select(inst, p)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_archive_extract_selected({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::archive_extract_selected(inst));
                    }
                }
            }
        });
        self.window.on_viewer_archive_extract_all({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::archive_extract_all(inst));
                    }
                }
            }
        });
        self.window.on_viewer_text_toggle_edit({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::text_toggle_edit(inst));
                    }
                }
            }
        });
        self.window.on_viewer_text_save({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(tw, orchid_widgets::builtin::viewer::text_save(inst));
                    }
                }
            }
        });
        self.window.on_viewer_text_edited({
            let t = t.clone();
            move |id, text| {
                if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                    if let Some(c) = t.upgrade() {
                        *c.last_text_edit_instance.lock() = Some(inst);
                    }
                    let body = text.to_string();
                    // Push edits without schedule_rebuild so the multiline
                    // TextInput keeps caret position; dirty ● uses local state.
                    spawn::spawn_local_compat(async move {
                        if let Err(e) =
                            orchid_widgets::builtin::viewer::text_push_edit(inst, body).await
                        {
                            warn!(?e, "viewer text edit");
                        }
                    });
                }
            }
        });
        self.window.on_viewer_text_scroll({
            let t = t.clone();
            move |id, delta| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(
                            tw,
                            orchid_widgets::builtin::viewer::text_scroll(inst, delta)
                        );
                    }
                }
            }
        });

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
        self.window.on_fm_breadcrumb_clicked({
            let t = t.clone();
            move |fm_id, pane, path| {
                if let Some(c) = t.upgrade() {
                    c.on_fm_breadcrumb_clicked(&fm_id, pane, &path);
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
        Ok(())
    }
}
