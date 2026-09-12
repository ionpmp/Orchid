//! Viewer callbacks.

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
    pub(super) fn wire_viewer(self: &Arc<Self>, t: &Weak<Self>) {
        macro_rules! viewer_spawn {
            ($weak:expr, $inst:expr, $fut:expr) => {{
                if let Some(c) = $weak.upgrade() {
                    c.bring_floating_to_front($inst);
                }
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
                        crate::window::main_window::image_touch::remember_viewer(inst);
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::image_zoom_in(inst)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_image_zoom_out({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        crate::window::main_window::image_touch::remember_viewer(inst);
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::image_zoom_out(inst)
                        );
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
                        viewer_spawn!(tw, inst, orchid_widgets::builtin::viewer::image_fit(inst));
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
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::image_actual_size(inst)
                        );
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
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::image_rotate_cw(inst)
                        );
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
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::image_rotate_ccw(inst)
                        );
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
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::image_flip_h(inst)
                        );
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
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::image_flip_v(inst)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_image_pan({
            let t = t.clone();
            move |id, dx, dy| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        crate::window::main_window::image_touch::remember_viewer(inst);
                        c.bring_floating_to_front(inst);
                        spawn::spawn_local_compat(async move {
                            if let Err(e) =
                                orchid_widgets::builtin::viewer::image_pan(inst, dx, dy).await
                            {
                                warn!(?e, "viewer pan");
                            }
                        });
                    }
                }
            }
        });
        self.window.on_viewer_image_command({
            let t = t.clone();
            move |id, cmd| {
                if let Some(c) = t.upgrade() {
                    c.apply_viewer_window_command(cmd.as_str());
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        crate::window::main_window::image_touch::remember_viewer(inst);
                        if cmd.as_str() == "reveal-folder" {
                            if let Some(folder) =
                                orchid_widgets::builtin::viewer::current_image_folder(inst)
                            {
                                c.reveal_folder_in_fm(folder);
                            }
                            return;
                        }
                        c.bring_floating_to_front(inst);
                        let cmd = cmd.to_string();
                        spawn::spawn_local_compat(async move {
                            if let Err(e) =
                                orchid_widgets::builtin::viewer::image_command(inst, &cmd).await
                            {
                                warn!(?e, "viewer image command");
                            }
                        });
                    }
                }
            }
        });
        self.window.on_viewer_viewport_changed({
            let t = t.clone();
            move |id, w, h| {
                if let Some(_c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        // Viewport size updates must not steal z-order.
                        spawn::spawn_local_compat(async move {
                            if let Err(e) =
                                orchid_widgets::builtin::viewer::set_viewport(inst, w, h).await
                            {
                                warn!(?e, "viewer viewport");
                            }
                        });
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
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::pdf_prev_page(inst)
                        );
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
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::pdf_next_page(inst)
                        );
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
                            inst,
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
                            inst,
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
                        viewer_spawn!(tw, inst, orchid_widgets::builtin::viewer::pdf_zoom_in(inst));
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
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::pdf_zoom_out(inst)
                        );
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
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::pdf_go_to_page(inst, page)
                        );
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
                            let text =
                                match orchid_widgets::builtin::viewer::pdf_current_page_text(inst)
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
                                                &orchid_i18n::FluentArgs::new()
                                                    .with("reason", reason),
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
        self.window.on_viewer_pdf_extract_page({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        spawn::spawn_local_compat(async move {
                            match orchid_widgets::builtin::viewer::pdf_extract_page(inst).await {
                                Ok(path) => {
                                    if let Some(c) = tw.upgrade() {
                                        let title = c.locale.tr("widget-viewer-name");
                                        let body = c.locale.tr_args(
                                            "viewer-pdf-extracted",
                                            &orchid_i18n::FluentArgs::new().with("path", path),
                                        );
                                        c.push_notification(&title, &body, 2);
                                    }
                                }
                                Err(e) => {
                                    warn!(?e, "viewer pdf extract page");
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
                                }
                            }
                        });
                    }
                }
            }
        });
        self.window.on_viewer_pdf_find({
            let t = t.clone();
            move |id, query, match_case, dir| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let q = query.to_string();
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::pdf_find(inst, q, match_case, dir)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_pdf_pointer({
            let t = t.clone();
            move |id, phase, x, y| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::pdf_pointer(inst, phase, x, y)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_pdf_outline_goto({
            let t = t.clone();
            move |id, page| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::pdf_outline_goto(inst, page)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_pdf_print({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        spawn::spawn_local_compat(async move {
                            match orchid_widgets::builtin::viewer::pdf_print(inst).await {
                                Ok(()) => {
                                    if let Some(c) = tw.upgrade() {
                                        let title = c.locale.tr("widget-viewer-name");
                                        let body = c.locale.tr("viewer-pdf-printed");
                                        c.push_notification(&title, &body, 1);
                                    }
                                }
                                Err(e) => {
                                    warn!(?e, "viewer pdf print");
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
                                }
                            }
                        });
                    }
                }
            }
        });
        self.window.on_viewer_pdf_highlight({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        spawn::spawn_local_compat(async move {
                            match orchid_widgets::builtin::viewer::pdf_highlight(inst).await {
                                Ok(path) => {
                                    if let Some(c) = tw.upgrade() {
                                        let title = c.locale.tr("widget-viewer-name");
                                        let body = c.locale.tr_args(
                                            "viewer-pdf-highlighted",
                                            &orchid_i18n::FluentArgs::new().with("path", path),
                                        );
                                        c.push_notification(&title, &body, 2);
                                    }
                                }
                                Err(e) => {
                                    warn!(?e, "viewer pdf highlight");
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
                                }
                            }
                        });
                    }
                }
            }
        });
        self.window.on_viewer_pdf_comment({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        spawn::spawn_local_compat(async move {
                            match orchid_widgets::builtin::viewer::pdf_comment(inst).await {
                                Ok(path) => {
                                    if let Some(c) = tw.upgrade() {
                                        let title = c.locale.tr("widget-viewer-name");
                                        let body = c.locale.tr_args(
                                            "viewer-pdf-commented",
                                            &orchid_i18n::FluentArgs::new().with("path", path),
                                        );
                                        c.push_notification(&title, &body, 2);
                                    }
                                }
                                Err(e) => {
                                    warn!(?e, "viewer pdf comment");
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
                            inst,
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
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::archive_navigate_up(inst)
                        );
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
                            inst,
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
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::archive_extract_selected(inst)
                        );
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
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::archive_extract_all(inst)
                        );
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
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::text_toggle_edit(inst)
                        );
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
                        viewer_spawn!(tw, inst, orchid_widgets::builtin::viewer::text_save(inst));
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
                            inst,
                            orchid_widgets::builtin::viewer::text_scroll(inst, delta)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_text_action({
            let t = t.clone();
            move |id, action| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let action = action.to_string();
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::text_action(inst, action)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_text_set_encoding({
            let t = t.clone();
            move |id, label| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let label = label.to_string();
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::text_set_encoding(inst, label)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_text_find_request({
            let t = t.clone();
            move |id, query, forward, regex, multiline| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let query = query.to_string();
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::text_find(
                                inst, query, forward, regex, multiline
                            )
                        );
                    }
                }
            }
        });
        self.window.on_viewer_text_replace_request({
            let t = t.clone();
            move |id, query, replacement, all, regex, multiline| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let query = query.to_string();
                        let replacement = replacement.to_string();
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::text_replace(
                                inst,
                                query,
                                replacement,
                                all,
                                regex,
                                multiline
                            )
                        );
                    }
                }
            }
        });
        self.window.on_viewer_open_external({
            let t = t.clone();
            move |id| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        if let Err(e) =
                            orchid_widgets::builtin::viewer::open_current_externally(inst)
                        {
                            warn!(?e, "viewer open external");
                            let title = c.locale.tr("widget-viewer-name");
                            let body = e.to_string();
                            c.push_notification(&title, &body, 3);
                        }
                    }
                }
            }
        });
        self.window.on_viewer_html_command({
            let t = t.clone();
            move |id, cmd| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        c.on_html_command(inst, cmd.as_str());
                    }
                }
            }
        });
        self.window.on_viewer_html_embed_bounds({
            let t = t.clone();
            move |id, x, y, w, h, vis| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        c.on_html_embed_bounds(inst, x, y, w, h, vis);
                    }
                }
            }
        });

        self.window.on_viewer_media_command({
            let t = t.clone();
            move |id, cmd| {
                if let Some(c) = t.upgrade() {
                    c.apply_viewer_window_command(cmd.as_str());
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        c.bring_floating_to_front(inst);
                        let cmd = cmd.to_string();
                        spawn::spawn_local_compat(async move {
                            if let Err(e) =
                                orchid_widgets::builtin::viewer::media_command(inst, &cmd).await
                            {
                                warn!(?e, "viewer media command");
                            }
                        });
                    }
                }
            }
        });

        self.window.on_viewer_media_seek_frac({
            let t = t.clone();
            move |id, frac| {
                if let Some(_c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        spawn::spawn_local_compat(async move {
                            if let Err(e) =
                                orchid_widgets::builtin::viewer::media_seek_frac(inst, frac).await
                            {
                                warn!(?e, "viewer media seek");
                            }
                        });
                    }
                }
            }
        });

        self.window.on_viewer_document_action({
            let t = t.clone();
            move |id, action| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let action = action.to_string();
                        if action == "image-insert" {
                            spawn::spawn_local_compat(async move {
                                let pasted = crate::widgets::terminal::ArboardClipboard::new()
                                    .ok()
                                    .and_then(|cb| cb.paste_image_png().ok())
                                    .flatten();
                                let Some((bytes, w, h)) = pasted else {
                                    warn!("viewer document image-insert: no clipboard image");
                                    return;
                                };
                                if let Err(e) =
                                    orchid_widgets::builtin::viewer::document_preview_insert_image(
                                        inst, bytes, w, h,
                                    )
                                    .await
                                {
                                    warn!(?e, "viewer document image-insert");
                                }
                            });
                            return;
                        }
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::document_action(inst, action)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_document_text_edited({
            let t = t.clone();
            move |id, text| {
                if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                    if let Some(c) = t.upgrade() {
                        *c.last_text_edit_instance.lock() = Some(inst);
                    }
                    let body = text.to_string();
                    spawn::spawn_local_compat(async move {
                        if let Err(e) =
                            orchid_widgets::builtin::viewer::document_push_edit(inst, body).await
                        {
                            warn!(?e, "viewer document edit");
                        }
                    });
                }
            }
        });
        self.window.on_viewer_document_selection_changed({
            move |id, anchor, head| {
                if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                    spawn::spawn_local_compat(async move {
                        if let Err(e) = orchid_widgets::builtin::viewer::document_set_selection(
                            inst, anchor, head,
                        )
                        .await
                        {
                            warn!(?e, "viewer document selection");
                        }
                    });
                }
            }
        });
        self.window.on_viewer_document_viewport_changed({
            let t = t.clone();
            move |id, width| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::document_set_viewport_width(
                                inst, width
                            )
                        );
                    }
                }
            }
        });
        self.window.on_viewer_document_preview_pointer({
            let t = t.clone();
            move |id, phase, x, y, ctrl| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::document_preview_pointer(
                                inst, phase, x, y, ctrl
                            )
                        );
                    }
                }
            }
        });
        self.window.on_viewer_document_find_request({
            let t = t.clone();
            move |id, query, forward, match_case| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let query = query.to_string();
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::document_find(
                                inst, query, forward, match_case
                            )
                        );
                    }
                }
            }
        });
        self.window.on_viewer_document_replace_request({
            let t = t.clone();
            move |id, query, replacement, all, match_case| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let query = query.to_string();
                        let replacement = replacement.to_string();
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::document_replace(
                                inst,
                                query,
                                replacement,
                                all,
                                match_case
                            )
                        );
                    }
                }
            }
        });
        self.window.on_viewer_document_link_request({
            let t = t.clone();
            move |id, url| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let url = url.to_string();
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::document_link(inst, url)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_document_comment_request({
            let t = t.clone();
            move |id, text| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let text = text.to_string();
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::document_comment(inst, text)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_document_header_request({
            let t = t.clone();
            move |id, text| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let text = text.to_string();
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::document_header(inst, text)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_document_footer_request({
            let t = t.clone();
            move |id, text| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let text = text.to_string();
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::document_footer(inst, text)
                        );
                    }
                }
            }
        });

        self.window.on_viewer_document_header_first_request({
            let t = t.clone();
            move |id, text| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let text = text.to_string();
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::document_header_first(inst, text)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_document_footer_first_request({
            let t = t.clone();
            move |id, text| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let text = text.to_string();
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::document_footer_first(inst, text)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_document_header_even_request({
            let t = t.clone();
            move |id, text| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let text = text.to_string();
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::document_header_even(inst, text)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_document_footer_even_request({
            let t = t.clone();
            move |id, text| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let text = text.to_string();
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::document_footer_even(inst, text)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_passphrase_commit({
            let t = t.clone();
            move |id, pw| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let pw = pw.to_string();
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::commit_orchid_passphrase(inst, pw)
                        );
                    }
                }
            }
        });
        self.window.on_viewer_passphrase_cancel({
            let t = t.clone();
            move |id| {
                if let Some(_c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        if let Err(e) =
                            orchid_widgets::builtin::viewer::cancel_orchid_passphrase(inst)
                        {
                            warn!(?e, "viewer passphrase cancel");
                        }
                    }
                }
            }
        });
        self.window.on_viewer_document_preview_key({
            let t = t.clone();
            move |id, key, ctrl, shift| {
                if let Some(c) = t.upgrade() {
                    if let Ok(inst) = Uuid::parse_str(id.as_str()) {
                        let tw = Arc::downgrade(&c);
                        let key = key.to_string();
                        // Clipboard needs the UI-side arboard handle.
                        if ctrl {
                            let lower = key.to_ascii_lowercase();
                            if lower == "c" || lower == "x" || lower == "v" {
                                spawn::spawn_local_compat(async move {
                                    let Some(_c) = tw.upgrade() else {
                                        return;
                                    };
                                    match lower.as_str() {
                                        "c" => {
                                            match orchid_widgets::builtin::viewer::document_preview_selection_text(
                                                inst,
                                            )
                                            .await
                                            {
                                                Ok(text) if !text.is_empty() => {
                                                    if let Ok(cb) =
                                                        crate::widgets::terminal::ArboardClipboard::new()
                                                    {
                                                        let _ = cb.copy(&text);
                                                    }
                                                }
                                                Ok(_) => {}
                                                Err(e) => {
                                                    warn!(?e, "viewer document copy");
                                                }
                                            }
                                        }
                                        "x" => {
                                            match orchid_widgets::builtin::viewer::document_preview_cut(
                                                inst,
                                            )
                                            .await
                                            {
                                                Ok(text) if !text.is_empty() => {
                                                    if let Ok(cb) =
                                                        crate::widgets::terminal::ArboardClipboard::new()
                                                    {
                                                        let _ = cb.copy(&text);
                                                    }
                                                }
                                                Ok(_) => {}
                                                Err(e) => {
                                                    warn!(?e, "viewer document cut");
                                                }
                                            }
                                        }
                                        "v" => {
                                            let cb = crate::widgets::terminal::ArboardClipboard::new().ok();
                                            if let Some((bytes, w, h)) = cb
                                                .as_ref()
                                                .and_then(|c| c.paste_image_png().ok())
                                                .flatten()
                                            {
                                                if let Err(e) = orchid_widgets::builtin::viewer::document_preview_insert_image(
                                                    inst, bytes, w, h,
                                                )
                                                .await
                                                {
                                                    warn!(?e, "viewer document paste image");
                                                }
                                                return;
                                            }
                                            let pasted = cb
                                                .and_then(|c| c.paste().ok())
                                                .unwrap_or_default();
                                            if pasted.is_empty() {
                                                return;
                                            }
                                            if let Err(e) = orchid_widgets::builtin::viewer::document_preview_paste(
                                                inst, pasted,
                                            )
                                            .await
                                            {
                                                warn!(?e, "viewer document paste");
                                            }
                                        }
                                        _ => {}
                                    }
                                });
                                return;
                            }
                        }
                        viewer_spawn!(
                            tw,
                            inst,
                            orchid_widgets::builtin::viewer::document_preview_key(
                                inst, key, ctrl, shift
                            )
                        );
                    }
                }
            }
        });
    }
}
