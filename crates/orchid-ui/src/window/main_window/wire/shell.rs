//! Workspace chrome, catalog, settings, onboarding.

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
    pub(super) fn wire_shell(self: &Arc<Self>, t: &Weak<Self>) {
        self.window.on_ui_tick({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.drain_fm_ingest_failure_notification();
                    c.drain_ipc_open_paths();
                    if c.config_reload_pending.swap(false, Ordering::AcqRel) {
                        if let Err(e) = c.apply_hot_config() {
                            warn!(?e, "config hot-reload");
                            let reason = ui_localized_error(&c.locale, &e);
                            let body = c.locale.tr_args(
                                "settings-config-reload-failed",
                                &orchid_i18n::FluentArgs::new().with("reason", reason),
                            );
                            c.push_notification(&c.locale.tr("settings-panel-title"), &body, 2);
                        }
                    }
                    let canvas_size_mismatch = c.sync_canvas_size_from_winit();
                    if canvas_size_mismatch {
                        c.update_gesture_bounds();
                        // Widgets that were sleeping because they sat outside the old
                        // (smaller) viewport can become visible again after a resize/
                        // maximize; re-check now instead of leaving them frozen until
                        // some unrelated widget-driven rebuild happens to run.
                        c.schedule_visibility_sync();
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
                    c.flush_notifications(false);
                    c.flush_html_webview_nav();
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
        self.window.on_workspace_orb_toggle({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.toggle_workspace_orb();
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
        self.window.on_catalog_show({
            let t = t.clone();
            move || {
                if let Some(c) = t.upgrade() {
                    c.show_widget_catalog_center();
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
        self.window
            .global::<ShortcutBindings>()
            .on_resolve_fm_action({
                let t = t.clone();
                move |ctrl, alt, shift, meta, key| {
                    t.upgrade()
                        .map(|c| {
                            let shortcuts = c.config.read().shortcuts.clone();
                            crate::window::main_window::input::resolve_fm_action_from_slint(
                                &shortcuts,
                                ctrl,
                                alt,
                                shift,
                                meta,
                                key.as_str(),
                            )
                            .into()
                        })
                        .unwrap_or_default()
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
    }
}
