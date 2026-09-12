//! File-manager handlers for [`MainWindowController`].

#![allow(unused_imports)]

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use secrecy::ExposeSecret;
use slint::Model;
use slint::ModelRc;
use slint::SharedString;
use slint::VecModel;
use tracing::{debug, warn};
use uuid::Uuid;

use orchid_storage::LifecycleState;
use orchid_widgets::layout::PixelBounds;
use orchid_widgets::{CreateWidgetRequest, WidgetPayload};

use crate::slint_generated::{
    FmConfirmDialog, FmConflictDialog, FmPassphraseState, FmPathSuggest, FmRenameState, FmTagState,
    WidgetFrameModel,
};
use crate::window::errors::{fm_localized_error, is_passphrase_retryable};
use crate::window::models::{
    build_context_menu, build_managed_policy_state, empty_confirm_dialog, empty_conflict_dialog,
    empty_context_menu, empty_find_state, empty_fm_overlays, empty_managed_policy_state,
    empty_passphrase_state, empty_rename_state, empty_tag_state, fm_grid_rebase_slack,
    fm_grid_visible_range, fm_grid_window, fm_list_visible_range, fm_list_window,
    fm_passphrase_dialog_labels, fm_window_covers, patch_fm_selection, sync_fm_path_suggestions,
    FileManagerOverlays, FmViewport, FM_LIST_REBASE_SLACK,
};
use crate::window::spawn;

use super::super::{open_file_associations, open_with_application_picker, MainWindowController};
use super::{default_fm_overlays, parse_select_filter_commit};

impl MainWindowController {
    pub(in crate::window::main_window) fn on_fm_entry_context(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        path: &SharedString,
        x: f32,
        y: f32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let target = path.to_string();
        if let Some(clip) = orchid_widgets::builtin::file_manager::file_clipboard(inst) {
            super::super::system_file_clipboard::refresh_os_file_note(&clip);
        }
        let (actions, target_paths, info) =
            match orchid_widgets::builtin::file_manager::context_menu_for(inst, p, &target) {
                Ok(v) => v,
                Err(e) => {
                    warn!(?e, "fm context menu");
                    return;
                }
            };
        if actions.is_empty() {
            return;
        }
        let shortcuts = self.config.read().shortcuts.clone();
        let menu = build_context_menu(
            &actions,
            &target_paths,
            info.as_ref(),
            x,
            y,
            &self.locale,
            &shortcuts,
        );
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(default_fm_overlays);
        entry.context_menu = menu;
        drop(over);
        if !self.try_patch_file_manager_instance(inst) {
            self.fm_patch_or_rebuild(inst);
        }
        if target.is_empty() {
            let _ = self.try_patch_fm_selection(inst, p);
            return;
        }

        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) =
                orchid_widgets::builtin::file_manager::focus_context_target(inst, p, &target).await
            {
                warn!(?e, "fm context focus");
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_context_action(
        self: &Arc<Self>,
        fm_id: &SharedString,
        action_id: &SharedString,
        paths: &ModelRc<SharedString>,
    ) {
        let id = action_id.to_string();
        let path_vec: Vec<String> = (0..paths.row_count())
            .filter_map(|i| paths.row_data(i))
            .map(|s| s.to_string())
            .collect();
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let outcome = match orchid_widgets::builtin::file_manager::run_action_with_opts(
                inst,
                &id,
                path_vec.clone(),
                orchid_widgets::builtin::file_manager::RunActionOpts::default(),
            )
            .await
            {
                Ok(o) => o,
                Err(e) => {
                    warn!(?e, "fm action");
                    if let Some(c) = tw.upgrade() {
                        c.notify_fm_action_failed(&e);
                    }
                    return;
                }
            };

            if let Some(c) = tw.upgrade() {
                c.apply_fm_action_outcome(inst, outcome);
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_context_dismiss(
        self: &Arc<Self>,
        fm_id: &SharedString,
    ) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(default_fm_overlays);
        entry.context_menu = empty_context_menu();
        drop(over);
        self.fm_patch_or_rebuild(inst);
    }

    pub(in crate::window::main_window) fn on_fm_confirm_yes(
        self: &Arc<Self>,
        fm_id: &SharedString,
    ) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let overlay = self.fm_overlays.read().get(&inst).cloned();
        let Some(over) = overlay else {
            return;
        };
        let action = over.confirm_dialog.pending_action.to_string();
        if action.is_empty() {
            let mut over = self.fm_overlays.write();
            let entry = over.entry(inst).or_insert_with(default_fm_overlays);
            entry.confirm_dialog = empty_confirm_dialog();
            drop(over);
            self.fm_patch_or_rebuild(inst);
            return;
        }
        let path_vec: Vec<String> = (0..over.confirm_dialog.pending_paths.row_count())
            .filter_map(|i| over.confirm_dialog.pending_paths.row_data(i))
            .map(|s| s.to_string())
            .collect();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let outcome = match orchid_widgets::builtin::file_manager::run_action_with_opts(
                inst,
                &action,
                path_vec,
                orchid_widgets::builtin::file_manager::RunActionOpts { skip_confirm: true },
            )
            .await
            {
                Ok(o) => o,
                Err(e) => {
                    warn!(?e, "fm confirm action");
                    if let Some(c) = tw.upgrade() {
                        c.notify_fm_action_failed(&e);
                    }
                    return;
                }
            };
            if let Some(c) = tw.upgrade() {
                c.apply_fm_action_outcome(inst, outcome);
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_confirm_no(self: &Arc<Self>, fm_id: &SharedString) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(default_fm_overlays);
        entry.confirm_dialog = empty_confirm_dialog();
        drop(over);
        self.fm_patch_or_rebuild(inst);
    }

    pub(in crate::window::main_window) fn on_fm_rename_commit(
        self: &Arc<Self>,
        fm_id: &SharedString,
        old_path: &SharedString,
        new_name: &SharedString,
    ) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let mask_op = self
            .fm_overlays
            .read()
            .get(&inst)
            .and_then(|o| o.select_mask_op);
        if let Some(op) = mask_op {
            let packed = new_name.to_string();
            let filter = parse_select_filter_commit(&packed);
            let pane = orchid_widgets::builtin::file_manager::focused_pane(inst).unwrap_or(0);
            let tw = Arc::downgrade(self);
            spawn::spawn_local_compat(async move {
                if let Err(e) = orchid_widgets::builtin::file_manager::apply_select_filter(
                    inst, pane, op, filter,
                )
                .await
                {
                    warn!(?e, "fm select mask");
                }
                if let Some(c) = tw.upgrade() {
                    let mut over = c.fm_overlays.write();
                    let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                    entry.rename = empty_rename_state();
                    entry.select_mask_op = None;
                    drop(over);
                    c.fm_refresh_selection_ui(inst, pane);
                    c.fm_patch_or_rebuild(inst);
                }
            });
            return;
        }
        let batch_paths = self
            .fm_overlays
            .read()
            .get(&inst)
            .map(|o| o.batch_rename_paths.clone())
            .unwrap_or_default();
        if !batch_paths.is_empty() {
            let pattern = new_name.to_string();
            let tw = Arc::downgrade(self);
            spawn::spawn_local_compat(async move {
                if let Err(e) = orchid_widgets::builtin::file_manager::apply_batch_rename(
                    inst,
                    batch_paths,
                    &pattern,
                    "",
                    "",
                )
                .await
                {
                    warn!(?e, "fm batch rename");
                    if let Some(c) = tw.upgrade() {
                        c.notify_fm_action_failed(&e);
                    }
                }
                if let Some(c) = tw.upgrade() {
                    let mut over = c.fm_overlays.write();
                    let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                    entry.rename = empty_rename_state();
                    entry.batch_rename_paths.clear();
                    drop(over);
                    c.fm_patch_or_rebuild(inst);
                }
            });
            return;
        }
        if old_path.as_str() == "orchid:find" {
            let packed = new_name.to_string();
            let tw = Arc::downgrade(self);
            spawn::spawn_local_compat(async move {
                match orchid_widgets::builtin::file_manager::complete_find(inst, &packed).await {
                    Ok(outcome) => {
                        if let Some(c) = tw.upgrade() {
                            let mut over = c.fm_overlays.write();
                            let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                            entry.rename = empty_rename_state();
                            entry.find = empty_find_state();
                            drop(over);
                            c.apply_fm_action_outcome(inst, outcome);
                        }
                    }
                    Err(e) => {
                        warn!(?e, "fm find");
                        if let Some(c) = tw.upgrade() {
                            c.notify_fm_action_failed(&e);
                        }
                    }
                }
            });
            return;
        }
        if old_path.as_str() == "orchid:tool" {
            let (action, paths) = self
                .fm_overlays
                .read()
                .get(&inst)
                .map(|o| {
                    (
                        o.tool_action.clone().unwrap_or_default(),
                        o.tool_paths.clone(),
                    )
                })
                .unwrap_or_default();
            if action.is_empty() {
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                entry.rename = empty_rename_state();
                entry.tool_action = None;
                entry.tool_paths.clear();
                drop(over);
                self.fm_patch_or_rebuild(inst);
                return;
            }
            let input = new_name.to_string();
            let tw = Arc::downgrade(self);
            spawn::spawn_local_compat(async move {
                match orchid_widgets::builtin::file_manager::complete_tool(
                    inst, &action, paths, &input,
                )
                .await
                {
                    Ok(outcome) => {
                        if let Some(c) = tw.upgrade() {
                            let mut over = c.fm_overlays.write();
                            let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                            entry.rename = empty_rename_state();
                            entry.tool_action = None;
                            entry.tool_paths.clear();
                            drop(over);
                            c.apply_fm_action_outcome(inst, outcome);
                        }
                    }
                    Err(e) => {
                        warn!(?e, "fm tool prompt");
                        if let Some(c) = tw.upgrade() {
                            c.notify_fm_action_failed(&e);
                        }
                    }
                }
            });
            return;
        }
        if old_path.as_str() == "orchid:conflict-rename" {
            let newn = new_name.to_string();
            let tw = Arc::downgrade(self);
            spawn::spawn_local_compat(async move {
                let result = orchid_widgets::builtin::file_manager::apply_conflict(
                    inst,
                    orchid_widgets::builtin::file_manager::ConflictChoice::Rename,
                    false,
                    Some(newn),
                )
                .await;
                match result {
                    Ok(outcome) => {
                        if let Some(c) = tw.upgrade() {
                            let mut over = c.fm_overlays.write();
                            let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                            entry.rename = empty_rename_state();
                            drop(over);
                            c.apply_fm_action_outcome(inst, outcome);
                        }
                    }
                    Err(e) => {
                        warn!(?e, "fm conflict rename");
                        if let Some(c) = tw.upgrade() {
                            c.notify_fm_action_failed(&e);
                        }
                    }
                }
            });
            return;
        }
        let (create_parent, create_file) = self
            .fm_overlays
            .read()
            .get(&inst)
            .map(|o| (o.create_folder_parent.clone(), o.create_item_is_file))
            .unwrap_or((None, false));
        if let Some(parent) = create_parent {
            let newn = new_name.to_string();
            let tw = Arc::downgrade(self);
            spawn::spawn_local_compat(async move {
                let result = if create_file {
                    orchid_widgets::builtin::file_manager::create_file(inst, &parent, &newn).await
                } else {
                    orchid_widgets::builtin::file_manager::create_folder(inst, &parent, &newn).await
                };
                if let Err(e) = result {
                    warn!(?e, create_file = create_file, "fm create item");
                    if let Some(c) = tw.upgrade() {
                        c.notify_fm_action_failed(&e);
                    }
                }
                if let Some(c) = tw.upgrade() {
                    let mut over = c.fm_overlays.write();
                    let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                    entry.rename = empty_rename_state();
                    entry.create_folder_parent = None;
                    entry.create_item_is_file = false;
                    drop(over);
                    c.fm_patch_or_rebuild(inst);
                }
            });
            return;
        }
        let old = old_path.to_string();
        let newn = new_name.to_string();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = orchid_widgets::builtin::file_manager::rename(inst, &old, &newn).await {
                warn!(?e, "fm rename");
                if let Some(c) = tw.upgrade() {
                    c.notify_fm_action_failed(&e);
                }
            }
            if let Some(c) = tw.upgrade() {
                let mut over = c.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                entry.rename = empty_rename_state();
                drop(over);
                c.fm_patch_or_rebuild(inst);
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_rename_cancel(
        self: &Arc<Self>,
        fm_id: &SharedString,
    ) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(default_fm_overlays);
        let restore_conflict = entry.rename.path.as_str() == "orchid:conflict-rename";
        entry.rename = empty_rename_state();
        entry.find = empty_find_state();
        entry.create_folder_parent = None;
        entry.create_item_is_file = false;
        entry.select_mask_op = None;
        entry.batch_rename_paths.clear();
        entry.tool_action = None;
        entry.tool_paths.clear();
        if restore_conflict {
            entry.conflict_dialog.visible = true;
        }
        drop(over);
        self.fm_patch_or_rebuild(inst);
    }

    pub(in crate::window::main_window) fn on_fm_conflict_choice(
        self: &Arc<Self>,
        fm_id: &SharedString,
        choice: &SharedString,
        apply_all: bool,
    ) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let ch = choice.to_string();
        if ch == "cancel" {
            if let Err(e) = orchid_widgets::builtin::file_manager::cancel_transfer(inst) {
                warn!(?e, "fm conflict cancel");
            }
            let mut over = self.fm_overlays.write();
            let entry = over.entry(inst).or_insert_with(default_fm_overlays);
            entry.conflict_dialog = empty_conflict_dialog();
            drop(over);
            self.fm_patch_or_rebuild(inst);
            return;
        }
        if ch == "rename" && !apply_all {
            let dest_name = self
                .fm_overlays
                .read()
                .get(&inst)
                .map(|o| o.conflict_dialog.dest_name.clone())
                .unwrap_or_default();
            let mut over = self.fm_overlays.write();
            let entry = over.entry(inst).or_insert_with(default_fm_overlays);
            entry.conflict_dialog.visible = false;
            entry.rename = FmRenameState {
                active: true,
                path: "orchid:conflict-rename".into(),
                proposed_name: dest_name,
                title: self.locale.tr("fm-conflict-rename").into(),
                hint: SharedString::new(),
                show_filter: false,
                files_label: SharedString::new(),
                folders_label: SharedString::new(),
                size_min_label: SharedString::new(),
                size_max_label: SharedString::new(),
                hidden_label: SharedString::new(),
                readonly_label: SharedString::new(),
                days_label: SharedString::new(),
                ok_label: self.locale.tr("fm-rename-ok").into(),
                cancel_label: self.locale.tr("fm-rename-cancel").into(),
            };
            drop(over);
            self.fm_patch_or_rebuild(inst);
            return;
        }
        let mapped = match ch.as_str() {
            "overwrite" => orchid_widgets::builtin::file_manager::ConflictChoice::Overwrite,
            "skip" => orchid_widgets::builtin::file_manager::ConflictChoice::Skip,
            "older" => orchid_widgets::builtin::file_manager::ConflictChoice::OverwriteOlder,
            "resume" => orchid_widgets::builtin::file_manager::ConflictChoice::Resume,
            "rename" => orchid_widgets::builtin::file_manager::ConflictChoice::Rename,
            _ => return,
        };
        {
            let mut over = self.fm_overlays.write();
            let entry = over.entry(inst).or_insert_with(default_fm_overlays);
            entry.conflict_dialog = empty_conflict_dialog();
        }
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            match orchid_widgets::builtin::file_manager::apply_conflict(
                inst, mapped, apply_all, None,
            )
            .await
            {
                Ok(outcome) => {
                    if let Some(c) = tw.upgrade() {
                        c.apply_fm_action_outcome(inst, outcome);
                    }
                }
                Err(e) => {
                    warn!(?e, "fm conflict choice");
                    if let Some(c) = tw.upgrade() {
                        c.notify_fm_action_failed(&e);
                    }
                }
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_tag_commit(
        self: &Arc<Self>,
        fm_id: &SharedString,
        tag: &SharedString,
    ) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let paths = self
            .fm_overlays
            .read()
            .get(&inst)
            .map(|o| o.tag_paths.clone())
            .unwrap_or_default();
        let tag_str = tag.to_string();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::add_tag_to_paths(inst, paths, &tag_str)
                .await;
            if let Some(c) = tw.upgrade() {
                let mut over = c.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                entry.tag = empty_tag_state();
                entry.tag_paths.clear();
                drop(over);
                c.fm_patch_or_rebuild(inst);
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_tag_cancel(self: &Arc<Self>, fm_id: &SharedString) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(default_fm_overlays);
        entry.tag = empty_tag_state();
        entry.tag_paths.clear();
        drop(over);
        self.fm_patch_or_rebuild(inst);
    }

    pub(in crate::window::main_window) fn on_fm_passphrase_commit(
        self: &Arc<Self>,
        fm_id: &SharedString,
        passphrase: &SharedString,
    ) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let overlay = self.fm_overlays.read().get(&inst).cloned();
        let Some(over) = overlay else {
            return;
        };
        let pw = passphrase.to_string();
        if pw.trim().is_empty() {
            if let Err(e) = orchid_widgets::builtin::file_manager::report_passphrase_error(
                inst,
                "passphrase required".into(),
            ) {
                warn!(?e, "fm passphrase empty");
            }
            self.fm_patch_or_rebuild(inst);
            return;
        }
        let purpose = over
            .passphrase_purpose
            .unwrap_or(orchid_widgets::builtin::file_manager::PassphrasePurpose::Encrypt);
        let paths = over.passphrase_paths.clone();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let outcome = match orchid_widgets::builtin::file_manager::apply_passphrase(
                inst, paths, pw, purpose,
            )
            .await
            {
                Ok(o) => o,
                Err(e) => {
                    let msg = e.to_string();
                    warn!(?e, "fm passphrase");
                    if let Some(c) = tw.upgrade() {
                        if let Err(report) =
                            orchid_widgets::builtin::file_manager::report_passphrase_error(
                                inst,
                                msg.clone(),
                            )
                        {
                            warn!(?report, "fm passphrase error report");
                        }
                        if !is_passphrase_retryable(&msg) {
                            c.clear_fm_passphrase_overlay(inst);
                        } else {
                            c.fm_patch_or_rebuild(inst);
                        }
                    }
                    return;
                }
            };
            if let Some(c) = tw.upgrade() {
                c.clear_fm_passphrase_overlay(inst);
                c.apply_fm_action_outcome(inst, outcome);
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_passphrase_cancel(
        self: &Arc<Self>,
        fm_id: &SharedString,
    ) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        self.clear_fm_passphrase_overlay(inst);
        self.fm_patch_or_rebuild(inst);
    }

    pub(in crate::window::main_window) fn on_fm_managed_policy_close(
        self: &Arc<Self>,
        fm_id: &SharedString,
    ) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let mut over = self.fm_overlays.write();
        if let Some(entry) = over.get_mut(&inst) {
            entry.managed_policy = empty_managed_policy_state();
            entry.context_menu = empty_context_menu();
        }
        drop(over);
        self.fm_patch_or_rebuild(inst);
    }

    pub(in crate::window::main_window) fn on_fm_passphrase_biometric(
        self: &Arc<Self>,
        fm_id: &SharedString,
    ) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let overlay = self.fm_overlays.read().get(&inst).cloned();
        let Some(over) = overlay else {
            return;
        };
        let purpose = over
            .passphrase_purpose
            .unwrap_or(orchid_widgets::builtin::file_manager::PassphrasePurpose::Reveal);
        let paths = over.passphrase_paths.clone();
        let prompt = self.locale.tr("fm-passphrase-biometric-prompt");
        let passphrase = match self
            .fm_passphrase_vault
            .load_passphrase_after_biometric(&prompt)
        {
            Ok(p) => p.expose_secret().to_string(),
            Err(e) => {
                let msg = e.to_string();
                warn!(?e, "fm passphrase biometric");
                if let Err(report) = orchid_widgets::builtin::file_manager::report_passphrase_error(
                    inst,
                    msg.clone(),
                ) {
                    warn!(?report, "fm passphrase error report");
                }
                self.fm_patch_or_rebuild(inst);
                return;
            }
        };
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let outcome = match orchid_widgets::builtin::file_manager::apply_passphrase(
                inst, paths, passphrase, purpose,
            )
            .await
            {
                Ok(o) => o,
                Err(e) => {
                    let msg = e.to_string();
                    warn!(?e, "fm passphrase biometric apply");
                    if let Some(c) = tw.upgrade() {
                        if let Err(report) =
                            orchid_widgets::builtin::file_manager::report_passphrase_error(
                                inst,
                                msg.clone(),
                            )
                        {
                            warn!(?report, "fm passphrase error report");
                        }
                        if !is_passphrase_retryable(&msg) {
                            c.clear_fm_passphrase_overlay(inst);
                        } else {
                            c.fm_patch_or_rebuild(inst);
                        }
                    }
                    return;
                }
            };
            if let Some(c) = tw.upgrade() {
                c.clear_fm_passphrase_overlay(inst);
                c.apply_fm_action_outcome(inst, outcome);
            }
        });
    }

    pub(in crate::window::main_window) fn clear_fm_passphrase_overlay(
        self: &Arc<Self>,
        inst: Uuid,
    ) {
        let mut over = self.fm_overlays.write();
        let entry = over.entry(inst).or_insert_with(default_fm_overlays);
        entry.passphrase = empty_passphrase_state();
        entry.passphrase_paths.clear();
        entry.passphrase_purpose = None;
        drop(over);
        if let Err(e) = orchid_widgets::builtin::file_manager::clear_passphrase_error(inst) {
            warn!(?e, "fm clear passphrase error");
        }
        self.fm_patch_or_rebuild(inst);
    }
}
