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
    pub(in crate::window::main_window) fn apply_fm_action_outcome(
        self: &Arc<Self>,
        inst: Uuid,
        outcome: orchid_widgets::builtin::file_manager::ActionOutcome,
    ) {
        match outcome {
            orchid_widgets::builtin::file_manager::ActionOutcome::Done => {
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                entry.context_menu = empty_context_menu();
                entry.confirm_dialog = empty_confirm_dialog();
                entry.conflict_dialog = empty_conflict_dialog();
                entry.rename = empty_rename_state();
                entry.find = empty_find_state();
                entry.tag = empty_tag_state();
                entry.tag_paths.clear();
                entry.batch_rename_paths.clear();
                entry.tool_action = None;
                entry.tool_paths.clear();
                entry.create_folder_parent = None;
                entry.create_item_is_file = false;
                entry.drag_active = false;
                entry.drag_paths.clear();
                entry.drag_drop_target.clear();
                entry.drag_target_pane = -1;
                drop(over);
                self.fm_patch_or_rebuild(inst);
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::NeedsConfirmation {
                message,
                action_id,
                paths,
            } => {
                let n = paths.len();
                let message_text = match message.as_str() {
                    "fm-confirm-delete"
                    | "fm-confirm-delete-permanent"
                    | "fm-confirm-copy"
                    | "fm-confirm-move"
                    | "fm-confirm-recycle-purge" => self.locale.tr_args(
                        &message,
                        &orchid_i18n::FluentArgs::new().with("n", n.to_string()),
                    ),
                    "fm-confirm-recycle-empty" => self.locale.tr(&message),
                    _ => message,
                };
                let dlg = FmConfirmDialog {
                    visible: true,
                    title: self.locale.tr("fm-confirm-title").into(),
                    message: message_text.into(),
                    confirm_label: self.locale.tr("action-confirm-yes").into(),
                    cancel_label: self.locale.tr("action-confirm-no").into(),
                    pending_action: action_id.into(),
                    pending_paths: ModelRc::new(VecModel::from(
                        paths
                            .into_iter()
                            .map(SharedString::from)
                            .collect::<Vec<_>>(),
                    )),
                };
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                entry.confirm_dialog = dlg;
                entry.context_menu = empty_context_menu();
                drop(over);
                self.fm_patch_or_rebuild(inst);
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::NeedsRename {
                path,
                current_name,
            } => {
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                entry.create_folder_parent = None;
                entry.create_item_is_file = false;
                entry.rename = FmRenameState {
                    active: true,
                    path: path.into(),
                    proposed_name: current_name.into(),
                    title: self.locale.tr("fm-rename-title").into(),
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
                entry.context_menu = empty_context_menu();
                drop(over);
                self.fm_patch_or_rebuild(inst);
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::NeedsCreateFolder { parent } => {
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                entry.create_folder_parent = Some(parent);
                entry.create_item_is_file = false;
                entry.rename = FmRenameState {
                    active: true,
                    path: SharedString::new(),
                    proposed_name: self.locale.tr("fm-action-new-folder").into(),
                    title: self.locale.tr("fm-action-new-folder").into(),
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
                entry.context_menu = empty_context_menu();
                drop(over);
                self.fm_patch_or_rebuild(inst);
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::NeedsCreateFile { parent } => {
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                entry.create_folder_parent = Some(parent);
                entry.create_item_is_file = true;
                entry.rename = FmRenameState {
                    active: true,
                    path: SharedString::new(),
                    proposed_name: self.locale.tr("fm-new-file-name").into(),
                    title: self.locale.tr("fm-action-new-file").into(),
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
                entry.context_menu = empty_context_menu();
                drop(over);
                self.fm_patch_or_rebuild(inst);
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::NeedsTag { paths } => {
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                entry.tag_paths = paths;
                entry.tag = FmTagState {
                    active: true,
                    proposed_tag: SharedString::new(),
                    title: self.locale.tr("fm-tag-add-title").into(),
                    ok_label: self.locale.tr("fm-rename-ok").into(),
                    cancel_label: self.locale.tr("fm-rename-cancel").into(),
                };
                entry.context_menu = empty_context_menu();
                drop(over);
                self.fm_patch_or_rebuild(inst);
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::NeedsSelectMask {
                op,
                filter,
            } => {
                let (title, hint) = if filter {
                    (
                        self.locale.tr("fm-select-filter-title"),
                        self.locale.tr("fm-select-filter-hint"),
                    )
                } else if matches!(op, orchid_widgets::builtin::file_manager::MaskOp::Subtract) {
                    (
                        self.locale.tr("fm-deselect-mask-title"),
                        self.locale.tr("fm-select-mask-hint"),
                    )
                } else {
                    (
                        self.locale.tr("fm-select-mask-title"),
                        self.locale.tr("fm-select-mask-hint"),
                    )
                };
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                entry.select_mask_op = Some(op);
                entry.create_folder_parent = None;
                entry.rename = FmRenameState {
                    active: true,
                    path: "orchid:select-mask".into(),
                    proposed_name: "*.*".into(),
                    title: title.into(),
                    hint: hint.into(),
                    show_filter: filter,
                    files_label: self.locale.tr("fm-select-filter-files").into(),
                    folders_label: self.locale.tr("fm-select-filter-folders").into(),
                    size_min_label: self.locale.tr("fm-select-filter-size-min").into(),
                    size_max_label: self.locale.tr("fm-select-filter-size-max").into(),
                    hidden_label: self.locale.tr("fm-select-filter-hidden").into(),
                    readonly_label: self.locale.tr("fm-select-filter-readonly").into(),
                    days_label: self.locale.tr("fm-select-filter-days").into(),
                    ok_label: self.locale.tr("fm-rename-ok").into(),
                    cancel_label: self.locale.tr("fm-rename-cancel").into(),
                };
                entry.context_menu = empty_context_menu();
                drop(over);
                self.fm_patch_or_rebuild(inst);
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::NeedsPassphrase {
                paths,
                purpose,
            } => {
                let (title, hint, ok_label) =
                    fm_passphrase_dialog_labels(self.locale.as_ref(), purpose);
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                entry.passphrase_paths = paths;
                entry.passphrase_purpose = Some(purpose);
                entry.passphrase = FmPassphraseState {
                    active: true,
                    proposed_passphrase: SharedString::new(),
                    title: title.into(),
                    hint: hint.into(),
                    ok_label: ok_label.into(),
                    cancel_label: self.locale.tr("fm-rename-cancel").into(),
                    biometric_available: self.fm_passphrase_vault.biometric_unlock_available(),
                    biometric_label: self.locale.tr("fm-passphrase-biometric").into(),
                };
                if let Err(e) = orchid_widgets::builtin::file_manager::clear_passphrase_error(inst)
                {
                    warn!(?e, "fm clear passphrase error");
                }
                entry.context_menu = empty_context_menu();
                drop(over);
                self.fm_patch_or_rebuild(inst);
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::NeedsManagedPolicy {
                path,
                policy,
            } => {
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                entry.managed_policy =
                    build_managed_policy_state(self.locale.as_ref(), &path, policy.as_ref());
                entry.context_menu = empty_context_menu();
                drop(over);
                self.fm_patch_or_rebuild(inst);
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::NeedsConflict {
                dest_name,
                can_resume,
                ..
            } => {
                let dlg = FmConflictDialog {
                    visible: true,
                    title: self.locale.tr("fm-conflict-title").into(),
                    message: self
                        .locale
                        .tr_args(
                            "fm-conflict-message",
                            &orchid_i18n::FluentArgs::new().with("name", dest_name.clone()),
                        )
                        .into(),
                    dest_name: dest_name.into(),
                    show_resume: can_resume,
                    apply_all_label: self.locale.tr("fm-conflict-apply-all").into(),
                    overwrite_label: self.locale.tr("fm-conflict-overwrite").into(),
                    skip_label: self.locale.tr("fm-conflict-skip").into(),
                    rename_label: self.locale.tr("fm-conflict-rename").into(),
                    older_label: self.locale.tr("fm-conflict-older").into(),
                    resume_label: self.locale.tr("fm-conflict-resume").into(),
                    cancel_label: self.locale.tr("fm-transfer-cancel").into(),
                };
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                entry.conflict_dialog = dlg;
                entry.confirm_dialog = empty_confirm_dialog();
                entry.context_menu = empty_context_menu();
                drop(over);
                self.fm_patch_or_rebuild(inst);
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::NeedsReport { title, body } => {
                let dlg = FmConfirmDialog {
                    visible: true,
                    title: title.into(),
                    message: body.into(),
                    confirm_label: self.locale.tr("fm-info-close").into(),
                    cancel_label: SharedString::new(),
                    pending_action: SharedString::new(),
                    pending_paths: ModelRc::new(VecModel::default()),
                };
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                entry.confirm_dialog = dlg;
                entry.rename = empty_rename_state();
                entry.tool_action = None;
                entry.tool_paths.clear();
                entry.context_menu = empty_context_menu();
                drop(over);
                self.fm_patch_or_rebuild(inst);
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::NeedsToolPrompt {
                action_id,
                paths,
                proposed,
                title,
                hint,
            } => {
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                entry.tool_action = Some(action_id);
                entry.tool_paths = paths;
                entry.create_folder_parent = None;
                entry.create_item_is_file = false;
                entry.rename = FmRenameState {
                    active: true,
                    path: "orchid:tool".into(),
                    proposed_name: proposed.into(),
                    title: title.into(),
                    hint: hint.into(),
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
                entry.context_menu = empty_context_menu();
                drop(over);
                self.fm_patch_or_rebuild(inst);
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::NeedsBatchRename { paths } => {
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                entry.batch_rename_paths = paths;
                entry.create_folder_parent = None;
                entry.create_item_is_file = false;
                entry.rename = FmRenameState {
                    active: true,
                    path: "orchid:batch-rename".into(),
                    proposed_name: "{name}_{n}{ext}".into(),
                    title: self.locale.tr("fm-batch-rename-title").into(),
                    hint: self.locale.tr("fm-batch-rename-hint").into(),
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
                entry.context_menu = empty_context_menu();
                drop(over);
                self.fm_patch_or_rebuild(inst);
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::OpenInViewer { path } => {
                let Ok(fs_path) = orchid_fs::FsPath::new(&path) else {
                    warn!(path = %path, "open in viewer: invalid path");
                    return;
                };
                let tw2 = Arc::downgrade(self);
                spawn::spawn_local_compat(async move {
                    let _ = MainWindowController::open_in_viewer_for_controller(
                        tw2, fs_path, true, false,
                    )
                    .await;
                });
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::OpenInEditor { path } => {
                let Ok(fs_path) = orchid_fs::FsPath::new(&path) else {
                    warn!(path = %path, "open in editor: invalid path");
                    return;
                };
                let tw2 = Arc::downgrade(self);
                spawn::spawn_local_compat(async move {
                    let _ = MainWindowController::open_in_viewer_for_controller(
                        tw2, fs_path, true, true,
                    )
                    .await;
                });
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::OpenFileAssociations { path } => {
                let open_path = match orchid_fs::FsPath::new(&path) {
                    Ok(fp) => fp
                        .to_local()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or(path),
                    Err(_) => path,
                };
                if let Err(e) = open_file_associations(&open_path) {
                    warn!(?e, path = %open_path, "file associations");
                }
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::OpenInViewerMany { paths } => {
                let tw2 = Arc::downgrade(self);
                spawn::spawn_local_compat(async move {
                    let mut opened = 0usize;
                    let mut skipped = 0usize;
                    for path in paths {
                        let Ok(fs_path) = orchid_fs::FsPath::new(&path) else {
                            continue;
                        };
                        if opened >= MainWindowController::VIEWER_MULTI_OPEN_CAP {
                            skipped += 1;
                            continue;
                        }
                        // One widget per path; rebuild once after the batch.
                        match MainWindowController::open_in_viewer_for_controller(
                            tw2.clone(),
                            fs_path,
                            false,
                            false,
                        )
                        .await
                        {
                            Ok((_, true)) => {
                                opened += 1;
                            }
                            Ok((_, false)) => {}
                            Err(_) => {}
                        }
                    }
                    if let Some(c) = tw2.upgrade() {
                        if skipped > 0 {
                            let title = c.locale.tr("widget-viewer-name");
                            let args = orchid_i18n::FluentArgs::new()
                                .with("opened", opened.to_string())
                                .with("skipped", skipped.to_string())
                                .with(
                                    "cap",
                                    MainWindowController::VIEWER_MULTI_OPEN_CAP.to_string(),
                                );
                            let body = c.locale.tr_args("viewer-multi-open-capped", &args);
                            c.push_notification(&title, &body, 2);
                        }
                        c.schedule_rebuild();
                    }
                });
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::PlayInAudioPlayer { paths } => {
                self.play_paths_in_audio_player(paths);
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::EnqueueInAudioPlayer {
                paths,
            } => {
                self.enqueue_paths_in_audio_player(paths);
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::PlayInVideoPlayer { paths } => {
                self.play_paths_in_video_player(paths);
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::EnqueueInVideoPlayer {
                paths,
            } => {
                self.enqueue_paths_in_video_player(paths);
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::OpenWithPicker { paths } => {
                for path in paths {
                    let open_path = match orchid_fs::FsPath::new(&path) {
                        Ok(fp) => fp
                            .to_local()
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or(path),
                        Err(_) => path,
                    };
                    if let Err(e) = open_with_application_picker(&open_path) {
                        warn!(?e, path = %open_path, "open with picker");
                    }
                }
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::NeedsFindDialog { root: _ } => {
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                entry.find = crate::slint_generated::FmFindState {
                    active: true,
                    title: self.locale.tr("fm-find-title").into(),
                    hint: self.locale.tr("fm-find-hint").into(),
                    name_label: self.locale.tr("fm-find-name").into(),
                    content_label: self.locale.tr("fm-find-content").into(),
                    name_regex_label: self.locale.tr("fm-find-name-regex").into(),
                    content_regex_label: self.locale.tr("fm-find-content-regex").into(),
                    case_label: self.locale.tr("fm-find-case").into(),
                    recursive_label: self.locale.tr("fm-find-recursive").into(),
                    archives_label: self.locale.tr("fm-find-archives").into(),
                    indexed_label: self.locale.tr("fm-find-indexed").into(),
                    save_label: self.locale.tr("fm-find-save").into(),
                    files_label: self.locale.tr("fm-find-files").into(),
                    folders_label: self.locale.tr("fm-find-folders").into(),
                    size_min_label: self.locale.tr("fm-find-size-min").into(),
                    size_max_label: self.locale.tr("fm-find-size-max").into(),
                    hidden_label: self.locale.tr("fm-find-hidden").into(),
                    readonly_label: self.locale.tr("fm-find-readonly").into(),
                    system_label: self.locale.tr("fm-find-system").into(),
                    days_label: self.locale.tr("fm-find-days").into(),
                    exif_label: self.locale.tr("fm-find-exif").into(),
                    gps_label: self.locale.tr("fm-find-gps").into(),
                    ok_label: self.locale.tr("fm-rename-ok").into(),
                    cancel_label: self.locale.tr("fm-rename-cancel").into(),
                };
                entry.rename = FmRenameState {
                    active: true,
                    path: "orchid:find".into(),
                    proposed_name: SharedString::new(),
                    title: SharedString::new(),
                    hint: SharedString::new(),
                    show_filter: false,
                    files_label: SharedString::new(),
                    folders_label: SharedString::new(),
                    size_min_label: SharedString::new(),
                    size_max_label: SharedString::new(),
                    hidden_label: SharedString::new(),
                    readonly_label: SharedString::new(),
                    days_label: SharedString::new(),
                    ok_label: SharedString::new(),
                    cancel_label: SharedString::new(),
                };
                entry.context_menu = empty_context_menu();
                drop(over);
                self.fm_patch_or_rebuild(inst);
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::NavigateSearch { path } => {
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                entry.rename = empty_rename_state();
                entry.find = empty_find_state();
                entry.context_menu = empty_context_menu();
                drop(over);
                let Ok(fs_path) = orchid_fs::FsPath::new(&path) else {
                    warn!(path = %path, "navigate search: invalid path");
                    return;
                };
                let pane = orchid_widgets::builtin::file_manager::focused_pane(inst).unwrap_or(0);
                let tw = Arc::downgrade(self);
                spawn::spawn_local_compat(async move {
                    if let Err(e) =
                        orchid_widgets::builtin::file_manager::navigate(inst, pane, fs_path).await
                    {
                        warn!(?e, "fm navigate search");
                    }
                    if let Some(c) = tw.upgrade() {
                        c.fm_patch_or_rebuild(inst);
                    }
                });
            }
            orchid_widgets::builtin::file_manager::ActionOutcome::OpenExternally { paths } => {
                for path in paths {
                    let open_path = match orchid_fs::FsPath::new(&path) {
                        Ok(fp) => fp
                            .to_local()
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or(path),
                        Err(_) => path,
                    };
                    if let Err(e) = opener::open(&open_path) {
                        warn!(?e, path = %open_path, "open file externally");
                    }
                }
            }
        }
    }
}
