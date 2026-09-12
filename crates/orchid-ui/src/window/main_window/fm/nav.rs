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
    pub(in crate::window::main_window) fn on_fm_sidebar_clicked(
        self: &Arc<Self>,
        fm_id: &SharedString,
        id: &SharedString,
    ) {
        let item_id = id.to_string();
        if item_id.starts_with("section:") {
            return;
        }
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let pane = {
            let cache = self.widget_manager.snapshot_cache();
            cache
                .get(inst)
                .and_then(|s| match &s.payload {
                    WidgetPayload::FileManager(fm) => Some(fm.active_pane),
                    _ => None,
                })
                .unwrap_or(0)
        };
        self.reset_fm_pane_viewport(inst, pane);
        let tw = Arc::downgrade(self);
        let wm = self.widget_manager.clone();
        spawn::spawn_bg_then_local(
            async move {
                if let Err(e) =
                    orchid_widgets::builtin::file_manager::navigate_virtual(inst, pane, &item_id)
                        .await
                {
                    warn!(?e, "fm sidebar navigation");
                }
                let _ = wm.refresh_snapshot_cache(inst).await;
            },
            move |()| async move {
                if let Some(c) = tw.upgrade() {
                    c.fm_patch_or_rebuild(inst);
                }
            },
        );
    }

    pub(in crate::window::main_window) fn on_fm_toggle_dual_pane(
        self: &Arc<Self>,
        fm_id: &SharedString,
    ) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::toggle_dual_pane(inst).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_toggle_show_hidden(
        self: &Arc<Self>,
        fm_id: &SharedString,
    ) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::toggle_show_hidden(inst).await;
            if let Some(c) = tw.upgrade() {
                c.reset_fm_pane_viewport(inst, 0);
                c.reset_fm_pane_viewport(inst, 1);
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_toggle_click_behavior(
        self: &Arc<Self>,
        fm_id: &SharedString,
    ) {
        let Some(inst) = self.fm_prepare_instance(fm_id, None) else {
            return;
        };
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::toggle_click_behavior(inst).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_open_selected(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let entries = self.fm_selected_entries(inst, p);
        let Some((path, is_dir)) = entries.first() else {
            return;
        };
        self.fm_dispatch_open(inst, p, path.clone(), *is_dir);
    }

    pub(in crate::window::main_window) fn on_fm_move_selection(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        delta: i32,
        extend: bool,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) =
                orchid_widgets::builtin::file_manager::select_relative(inst, p, delta, extend).await
            {
                warn!(?e, "fm move selection");
                return;
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_selection_ui(inst, p);
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_pane_clicked(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        self.set_fm_focus(inst, p);
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::switch_active_pane(inst, p).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_tab_clicked(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        tab_id: &SharedString,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let tab = tab_id.to_string();
        self.reset_fm_pane_viewport(inst, p);
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::switch_to_tab(inst, p, &tab).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_tab_closed(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        tab_id: &SharedString,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let tab = tab_id.to_string();
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::close_tab(inst, p, &tab).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_tab_new(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::new_tab(inst, p).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_new_folder(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let outcome =
                match orchid_widgets::builtin::file_manager::request_new_folder(inst, p).await {
                    Ok(o) => o,
                    Err(e) => {
                        warn!(?e, "fm new folder");
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

    pub(in crate::window::main_window) fn on_fm_nav_back(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        self.reset_fm_pane_viewport(inst, p);
        let tw = Arc::downgrade(self);
        let wm = self.widget_manager.clone();
        spawn::spawn_bg_then_local(
            async move {
                let _ = orchid_widgets::builtin::file_manager::navigate_back(inst, p).await;
                let _ = wm.refresh_snapshot_cache(inst).await;
            },
            move |()| async move {
                if let Some(c) = tw.upgrade() {
                    c.fm_patch_or_rebuild(inst);
                }
            },
        );
    }

    pub(in crate::window::main_window) fn on_fm_nav_forward(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        self.reset_fm_pane_viewport(inst, p);
        let tw = Arc::downgrade(self);
        let wm = self.widget_manager.clone();
        spawn::spawn_bg_then_local(
            async move {
                let _ = orchid_widgets::builtin::file_manager::navigate_forward(inst, p).await;
                let _ = wm.refresh_snapshot_cache(inst).await;
            },
            move |()| async move {
                if let Some(c) = tw.upgrade() {
                    c.fm_patch_or_rebuild(inst);
                }
            },
        );
    }

    pub(in crate::window::main_window) fn on_fm_nav_up(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        self.reset_fm_pane_viewport(inst, p);
        let tw = Arc::downgrade(self);
        let wm = self.widget_manager.clone();
        spawn::spawn_bg_then_local(
            async move {
                let _ = orchid_widgets::builtin::file_manager::navigate_up(inst, p).await;
                let _ = wm.refresh_snapshot_cache(inst).await;
            },
            move |()| async move {
                if let Some(c) = tw.upgrade() {
                    c.fm_patch_or_rebuild(inst);
                }
            },
        );
    }

    pub(in crate::window::main_window) fn on_fm_nav_home(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        self.reset_fm_pane_viewport(inst, p);
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::navigate_home(inst, p).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_history_pick(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        path: &SharedString,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let raw = path.to_string();
        let Ok(fs_path) = orchid_fs::FsPath::new(raw) else {
            return;
        };
        self.reset_fm_pane_viewport(inst, p);
        let tw = Arc::downgrade(self);
        let wm = self.widget_manager.clone();
        spawn::spawn_bg_then_local(
            async move {
                let _ = orchid_widgets::builtin::file_manager::navigate(inst, p, fs_path).await;
                let _ = wm.refresh_snapshot_cache(inst).await;
            },
            move |()| async move {
                if let Some(c) = tw.upgrade() {
                    c.fm_patch_or_rebuild(inst);
                }
            },
        );
    }

    pub(in crate::window::main_window) fn on_fm_breadcrumb_clicked(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        path: &SharedString,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let raw = path.to_string();
        let Ok(fs_path) = orchid_fs::FsPath::new(raw) else {
            return;
        };
        self.reset_fm_pane_viewport(inst, p);
        let tw = Arc::downgrade(self);
        let wm = self.widget_manager.clone();
        spawn::spawn_bg_then_local(
            async move {
                let _ = orchid_widgets::builtin::file_manager::navigate(inst, p, fs_path).await;
                let _ = wm.refresh_snapshot_cache(inst).await;
            },
            move |()| async move {
                if let Some(c) = tw.upgrade() {
                    c.fm_patch_or_rebuild(inst);
                }
            },
        );
    }

    pub(in crate::window::main_window) fn on_fm_path_edit_changed(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        typed: &SharedString,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let typed = typed.to_string();
        if typed.is_empty() {
            self.apply_path_suggestions(inst, Vec::new());
            return;
        }
        let seq = {
            let mut map = self.fm_complete_seq.lock();
            let entry = map.entry((inst, p)).or_insert(0);
            *entry = entry.wrapping_add(1);
            *entry
        };
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            let Some(c) = tw.upgrade() else {
                return;
            };
            if c.fm_complete_seq.lock().get(&(inst, p)).copied() != Some(seq) {
                return;
            }
            let items = orchid_widgets::builtin::file_manager::complete_path(inst, &typed).await;
            if c.fm_complete_seq.lock().get(&(inst, p)).copied() != Some(seq) {
                return;
            }
            c.apply_path_suggestions(inst, items);
        });
    }

    pub(in crate::window::main_window) fn on_fm_path_edit_commit(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        path: &SharedString,
    ) {
        self.apply_path_suggestions_for_id(fm_id, Vec::new());
        let Some(fs_path) = orchid_widgets::builtin::file_manager::coerce_typed_path(path.as_str())
        else {
            return;
        };
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        self.reset_fm_pane_viewport(inst, p);
        let tw = Arc::downgrade(self);
        let wm = self.widget_manager.clone();
        spawn::spawn_bg_then_local(
            async move {
                let _ = orchid_widgets::builtin::file_manager::navigate(inst, p, fs_path).await;
                let _ = wm.refresh_snapshot_cache(inst).await;
            },
            move |()| async move {
                if let Some(c) = tw.upgrade() {
                    c.fm_patch_or_rebuild(inst);
                }
            },
        );
    }

    fn apply_path_suggestions_for_id(
        &self,
        fm_id: &SharedString,
        items: Vec<orchid_widgets::builtin::file_manager::PathCompleteItem>,
    ) {
        let Ok(inst) = Uuid::parse_str(fm_id.as_str()) else {
            return;
        };
        self.apply_path_suggestions(inst, items);
    }

    fn apply_path_suggestions(
        &self,
        inst: Uuid,
        items: Vec<orchid_widgets::builtin::file_manager::PathCompleteItem>,
    ) {
        let rows: Vec<FmPathSuggest> = items
            .into_iter()
            .map(|item| FmPathSuggest {
                path: item.path.into(),
                label: item.label.into(),
            })
            .collect();
        for model in [&self.workspace_widgets, &self.workspace_floating_widgets] {
            if Self::patch_path_suggestions_in(model, inst, &rows) {
                return;
            }
        }
    }

    fn patch_path_suggestions_in(
        model: &ModelRc<WidgetFrameModel>,
        inst: Uuid,
        rows: &[FmPathSuggest],
    ) -> bool {
        let Some(v) = model.as_any().downcast_ref::<VecModel<WidgetFrameModel>>() else {
            return false;
        };
        let needle = inst.to_string();
        for r in 0..v.row_count() {
            let Some(row) = v.row_data(r) else {
                continue;
            };
            if row.instance_id.as_str() != needle.as_str() {
                continue;
            }
            sync_fm_path_suggestions(&row.file_manager, rows.to_vec());
            return true;
        }
        false
    }

    pub(in crate::window::main_window) fn on_fm_view_mode_cycle(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        self.reset_fm_pane_viewport(inst, p);
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::cycle_view_mode(inst, p).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_sort_cycle(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        self.reset_fm_pane_viewport(inst, p);
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::cycle_sort(inst, p).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_sort_column_clicked(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        column: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let col = column.clamp(0, 3) as u8;
        self.reset_fm_pane_viewport(inst, p);
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = orchid_widgets::builtin::file_manager::set_sort_column(inst, p, col).await;
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_viewport_changed(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        y: f32,
        h: f32,
        w: f32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let pin_top = self.fm_viewport_pin_top.lock().contains(&(inst, p));
        let raw_y = y.max(0.0);
        if pin_top && raw_y <= 1.0 {
            self.fm_viewport_pin_top.lock().remove(&(inst, p));
        }
        let scroll_y = if pin_top { 0.0 } else { raw_y };
        let view_h = h.max(1.0);
        let view_w = w.max(1.0);

        let (view_mode, total) = {
            let Some(snap) = self.widget_manager.snapshot_cache().get(inst) else {
                return;
            };
            let WidgetPayload::FileManager(fm) = &snap.payload else {
                return;
            };
            let Some(pane) = fm.panes.get(p as usize) else {
                return;
            };
            let Some(tab) = pane.tabs.get(pane.active_tab as usize) else {
                return;
            };
            (tab.view_mode, tab.item_count as usize)
        };

        let large = matches!(view_mode, orchid_widgets::FmViewMode::Gallery);
        let (desired, visible, slack) = match view_mode {
            orchid_widgets::FmViewMode::Icons | orchid_widgets::FmViewMode::Gallery => (
                fm_grid_window(total, scroll_y, view_h, view_w, large),
                fm_grid_visible_range(total, scroll_y, view_h, view_w, large),
                fm_grid_rebase_slack(view_w, large),
            ),
            orchid_widgets::FmViewMode::Details => (
                fm_list_window(total, scroll_y, view_h, true),
                fm_list_visible_range(total, scroll_y, view_h, true),
                FM_LIST_REBASE_SLACK,
            ),
            orchid_widgets::FmViewMode::List => (
                fm_list_window(total, scroll_y, view_h, false),
                fm_list_visible_range(total, scroll_y, view_h, false),
                FM_LIST_REBASE_SLACK,
            ),
        };

        self.fm_viewport.lock().insert(
            (inst, p),
            FmViewport {
                scroll_y,
                view_h,
                view_w,
            },
        );

        let (first, end) = {
            let width_key = (view_w / 8.0).round() as i32;
            let mut windows = self.fm_viewport_window.lock();
            if let Some(&(c0, c1, w)) = windows.get(&(inst, p)) {
                if w == width_key && fm_window_covers((c0, c1), visible, slack, total) {
                    return;
                }
            }
            let (first, end, _, _) = desired;
            windows.insert((inst, p), (first, end, width_key));
            (first, end)
        };
        orchid_widgets::builtin::file_manager::set_viewport_window(inst, p, first, end);

        let seq = {
            let mut map = self.fm_viewport_seq.lock();
            let entry = map.entry((inst, p)).or_insert(0);
            *entry = entry.wrapping_add(1);
            *entry
        };
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            tokio::time::sleep(std::time::Duration::from_millis(16)).await;
            let Some(c) = tw.upgrade() else {
                return;
            };
            if c.fm_viewport_seq.lock().get(&(inst, p)).copied() != Some(seq) {
                return;
            }
            let _ = c.widget_manager.refresh_snapshot_cache(inst).await;
            if !c.try_patch_file_manager_instance(inst) {
                if let Err(e) = c.patch_workspace_frames(&[inst]) {
                    warn!(?e, "fm viewport patch");
                }
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_quick_filter_changed(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        q: &SharedString,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let query = q.to_string();
        let seq = {
            let mut map = self.fm_filter_seq.lock();
            let entry = map.entry((inst, p)).or_insert(0);
            *entry = entry.wrapping_add(1);
            *entry
        };
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            // Coalesce keystrokes so each character does not rebuild the full
            // Slint entry model for large directories.
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            let Some(c) = tw.upgrade() else {
                return;
            };
            if c.fm_filter_seq.lock().get(&(inst, p)).copied() != Some(seq) {
                return;
            }
            c.reset_fm_pane_viewport(inst, p);
            let _ = orchid_widgets::builtin::file_manager::set_quick_filter(inst, p, query).await;
            c.fm_refresh_ui(inst).await;
        });
    }

    pub(in crate::window::main_window) fn on_fm_entry_clicked(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        path: &SharedString,
        ctrl: bool,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        self.set_fm_focus(inst, p);
        let ps = path.to_string();

        // Workspace rebuild remounts TouchAreas between clicks, so Slint's
        // `double-clicked` often never fires. Detect a second click here.
        const DOUBLE_CLICK_MS: u128 = 500;
        let behavior = orchid_widgets::builtin::file_manager::click_behavior(inst)
            .unwrap_or(orchid_widgets::builtin::file_manager::ClickBehavior::DoubleToOpen);
        let open_from_double = if !ctrl
            && behavior == orchid_widgets::builtin::file_manager::ClickBehavior::DoubleToOpen
        {
            let now = Instant::now();
            let mut last = self.fm_last_click.lock();
            let is_double = last
                .as_ref()
                .is_some_and(|(prev_inst, prev_pane, prev_path, t)| {
                    *prev_inst == inst
                        && *prev_pane == p
                        && prev_path == &ps
                        && now.duration_since(*t).as_millis() <= DOUBLE_CLICK_MS
                });
            // Consume the pair so a third click does not open a second time.
            *last = if is_double {
                None
            } else {
                Some((inst, p, ps.clone(), now))
            };
            is_double
        } else {
            // A Ctrl+click is never half of a double click; letting it seed the
            // pair makes the next plain click open the file instead of selecting.
            *self.fm_last_click.lock() = None;
            false
        };

        if open_from_double {
            debug!(%ps, "fm entry second click -> open");
            // Cancel any drag armed by the second press; do not race a
            // select/refresh rebuild against navigate.
            self.clear_fm_drag(inst);
            let is_dir = self.fm_entry_is_dir(inst, p, &ps);
            self.fm_try_dispatch_open(inst, p, ps, is_dir);
            return;
        }

        let ps_for_select = ps.clone();
        let mode = if ctrl {
            orchid_widgets::builtin::file_manager::SelectionMode::Toggle
        } else {
            orchid_widgets::builtin::file_manager::SelectionMode::Single
        };
        // Keep selection on the UI stack (same as marquee): overlapping async
        // select/refresh tasks reorder and leave the highlight on the previous
        // entry until something like Escape forces a full paint.
        if let Err(e) =
            orchid_widgets::builtin::file_manager::select_entry_sync(inst, p, &ps_for_select, mode)
        {
            warn!(?e, "fm select entry");
        } else {
            self.fm_refresh_selection_ui(inst, p);
        }

        if ctrl || behavior != orchid_widgets::builtin::file_manager::ClickBehavior::SingleToOpen {
            return;
        }
        let is_dir = self.fm_entry_is_dir(inst, p, &ps);
        self.fm_try_dispatch_open(inst, p, ps, is_dir);
    }

    pub(in crate::window::main_window) fn on_fm_entry_shift_clicked(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        path: &SharedString,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let ps = path.to_string();
        if let Err(e) = orchid_widgets::builtin::file_manager::select_entry_sync(
            inst,
            p,
            &ps,
            orchid_widgets::builtin::file_manager::SelectionMode::Range,
        ) {
            warn!(?e, "fm select range");
        } else {
            self.fm_refresh_selection_ui(inst, p);
        }
    }

    pub(in crate::window::main_window) fn on_fm_entry_double_clicked(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        path: &SharedString,
        is_dir: bool,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let raw = path.to_string();
        let behavior = orchid_widgets::builtin::file_manager::click_behavior(inst)
            .unwrap_or(orchid_widgets::builtin::file_manager::ClickBehavior::DoubleToOpen);
        if is_dir {
            self.fm_try_dispatch_open(inst, p, raw, true);
            return;
        }
        if behavior == orchid_widgets::builtin::file_manager::ClickBehavior::DoubleToOpen {
            self.fm_try_dispatch_open(inst, p, raw, false);
        }
    }

    /// Open a path unless the same path was just opened (Slint + Rust double-click).
    pub(in crate::window::main_window) fn fm_try_dispatch_open(
        self: &Arc<Self>,
        inst: Uuid,
        pane: u8,
        path: String,
        is_dir: bool,
    ) {
        const OPEN_DEBOUNCE_MS: u128 = 400;
        {
            let now = Instant::now();
            let mut last = self.fm_last_open.lock();
            if last.as_ref().is_some_and(|(prev_inst, prev_path, t)| {
                *prev_inst == inst
                    && prev_path == &path
                    && now.duration_since(*t).as_millis() <= OPEN_DEBOUNCE_MS
            }) {
                debug!(%path, "fm open debounced");
                return;
            }
            *last = Some((inst, path.clone(), now));
        }
        self.fm_dispatch_open(inst, pane, path, is_dir);
    }

    pub(in crate::window::main_window) fn fm_dispatch_open(
        self: &Arc<Self>,
        inst: Uuid,
        pane: u8,
        path: String,
        is_dir: bool,
    ) {
        if is_dir {
            self.reset_fm_pane_viewport(inst, pane);
        }
        let tw = Arc::downgrade(self);
        let wm = self.widget_manager.clone();
        debug!(%path, is_dir, pane, %inst, "fm_dispatch_open");
        spawn::spawn_bg_then_local(
            async move {
                let t0 = Instant::now();
                let outcome =
                    orchid_widgets::builtin::file_manager::open_path(inst, pane, &path, is_dir)
                        .await;
                let elapsed_ms = t0.elapsed().as_millis();
                match &outcome {
                    Ok(_) => debug!(%path, elapsed_ms, "fm_dispatch_open ok"),
                    Err(e) => warn!(?e, %path, elapsed_ms, "fm_dispatch_open err"),
                }
                let _ = wm.refresh_snapshot_cache(inst).await;
                // Drop any dirty marks so the 60Hz tick does not remount FM while the
                // activating click is still settling (that freezes Slint on Windows).
                let _ = wm.drain_frame_dirty_ids();
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                (path, outcome)
            },
            move |(path, outcome)| async move {
                let Some(c) = tw.upgrade() else {
                    return;
                };
                match outcome {
                    Ok(orchid_widgets::builtin::file_manager::ActionOutcome::Done) => {
                        debug!(%path, "fm open applying ui");
                        c.clear_fm_drag(inst);
                        {
                            let mut over = c.fm_overlays.write();
                            let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                            entry.context_menu = empty_context_menu();
                            entry.confirm_dialog = empty_confirm_dialog();
                            entry.rename = empty_rename_state();
                            entry.tag = empty_tag_state();
                            entry.tag_paths.clear();
                            entry.create_folder_parent = None;
                            entry.create_item_is_file = false;
                        }
                        c.rebuild_pending.store(false, Ordering::Release);
                        let _ = c.widget_manager.drain_frame_dirty_ids();
                        debug!(%path, "fm open patching frames");
                        match c.patch_workspace_frames(&[inst]) {
                            Ok(()) => debug!(%path, "fm open patched"),
                            Err(e) => {
                                warn!(?e, "fm open patch frames");
                                c.schedule_rebuild();
                            }
                        }
                    }
                    Ok(o) => {
                        c.apply_fm_action_outcome(inst, o);
                    }
                    Err(e) => {
                        warn!(?e, path = %path, "fm open path");
                        c.notify_fm_action_failed(&e);
                    }
                }
            },
        );
    }

    pub(in crate::window::main_window) fn on_fm_select_all(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = orchid_widgets::builtin::file_manager::select_all_in_pane(inst, p).await
            {
                warn!(?e, "fm select all");
                return;
            }
            if let Some(c) = tw.upgrade() {
                c.fm_refresh_selection_ui(inst, p);
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_deselect_all(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let p = pane.max(0) as u8;
        if let Err(e) = orchid_widgets::builtin::file_manager::deselect_all_in_pane_sync(inst, p) {
            warn!(?e, "fm deselect all");
            return;
        }
        self.fm_refresh_selection_ui(inst, p);
    }

    pub(in crate::window::main_window) fn on_fm_delete_selected(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let paths = self.fm_selected_paths(inst, pane.max(0) as u8);
        if paths.is_empty() {
            return;
        }
        self.spawn_fm_action(inst, "fs.delete", paths);
    }

    pub(in crate::window::main_window) fn on_fm_copy_selected(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let paths = self.fm_selected_paths(inst, pane.max(0) as u8);
        if paths.is_empty() {
            return;
        }
        self.spawn_fm_action(inst, "fs.copy", paths);
    }

    pub(in crate::window::main_window) fn on_fm_paste_clipboard(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        if let Some(clip) = orchid_widgets::builtin::file_manager::file_clipboard(inst) {
            super::super::system_file_clipboard::ingest_os_files(&clip);
        }
        self.spawn_fm_action(inst, "fs.paste", Vec::new());
    }

    pub(in crate::window::main_window) fn on_fm_selection_command(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        command: &SharedString,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let cmd = command.to_string();
        match cmd.as_str() {
            "invert" => {
                let tw = Arc::downgrade(self);
                spawn::spawn_local_compat(async move {
                    if let Err(e) =
                        orchid_widgets::builtin::file_manager::invert_selection_in_pane(inst, p)
                            .await
                    {
                        warn!(?e, "fm invert");
                        return;
                    }
                    if let Some(c) = tw.upgrade() {
                        c.fm_refresh_selection_ui(inst, p);
                    }
                });
            }
            "mask-add" => self.spawn_fm_action(inst, "fs.select-mask-add", Vec::new()),
            "mask-sub" => self.spawn_fm_action(inst, "fs.select-mask-sub", Vec::new()),
            "filter" => self.spawn_fm_action(inst, "fs.select-filter", Vec::new()),
            _ => {}
        }
    }

    pub(in crate::window::main_window) fn on_fm_nav_command(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        command: &SharedString,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let cmd = command.to_string();
        match cmd.as_str() {
            "pause" => {
                if let Err(e) = orchid_widgets::builtin::file_manager::pause_transfer(inst) {
                    warn!(?e, "fm pause");
                }
                self.fm_patch_or_rebuild(inst);
                return;
            }
            "resume" => {
                if let Err(e) = orchid_widgets::builtin::file_manager::resume_transfer(inst) {
                    warn!(?e, "fm resume");
                }
                self.fm_patch_or_rebuild(inst);
                return;
            }
            "cancel" => {
                if let Err(e) = orchid_widgets::builtin::file_manager::cancel_transfer(inst) {
                    warn!(?e, "fm cancel transfer");
                }
                let mut over = self.fm_overlays.write();
                let entry = over.entry(inst).or_insert_with(default_fm_overlays);
                entry.conflict_dialog = empty_conflict_dialog();
                drop(over);
                self.fm_patch_or_rebuild(inst);
                return;
            }
            "copy-other" => {
                self.spawn_fm_action(inst, "fs.copy-to-other", self.fm_selected_paths(inst, p));
                return;
            }
            "move-other" => {
                self.spawn_fm_action(inst, "fs.move-to-other", self.fm_selected_paths(inst, p));
                return;
            }
            "delete-perm" => {
                let paths = self.fm_selected_paths(inst, p);
                if !paths.is_empty() {
                    self.spawn_fm_action(inst, "fs.delete-permanent", paths);
                }
                return;
            }
            "cut" => {
                let paths = self.fm_selected_paths(inst, p);
                if !paths.is_empty() {
                    self.spawn_fm_action(inst, "fs.cut", paths);
                }
                return;
            }
            "undo" => {
                self.spawn_fm_action(inst, "fs.undo", Vec::new());
                return;
            }
            "redo" => {
                self.spawn_fm_action(inst, "fs.redo", Vec::new());
                return;
            }
            "new-file" => {
                self.spawn_fm_action(inst, "fs.new-file", Vec::new());
                return;
            }
            "view" => {
                let paths = self.fm_selected_paths(inst, p);
                if !paths.is_empty() {
                    self.spawn_fm_action(inst, "viewer.open", paths);
                }
                return;
            }
            "edit" => {
                let paths = self.fm_selected_paths(inst, p);
                if !paths.is_empty() {
                    self.spawn_fm_action(inst, "viewer.edit", paths);
                }
                return;
            }
            "find" => {
                self.spawn_fm_action(inst, "fs.find", Vec::new());
                return;
            }
            "properties" => {
                let paths = self.fm_selected_paths(inst, p);
                self.spawn_fm_action(inst, "fs.properties", paths);
                return;
            }
            "batch-rename" => {
                let paths = self.fm_selected_paths(inst, p);
                if !paths.is_empty() {
                    self.spawn_fm_action(inst, "fs.batch-rename", paths);
                }
                return;
            }
            _ => {}
        }
        let folder = self.fm_selected_folder(inst, p);
        let tw = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let result = match cmd.as_str() {
                "drive-root" => {
                    orchid_widgets::builtin::file_manager::navigate_drive_root(inst, p).await
                }
                "branch" => {
                    orchid_widgets::builtin::file_manager::toggle_branch_view(inst, p).await
                }
                "open-tab" => {
                    orchid_widgets::builtin::file_manager::new_tab_at(inst, p, folder).await
                }
                "other-pane" => {
                    orchid_widgets::builtin::file_manager::open_in_other_pane(inst, p, folder).await
                }
                _ => Ok(()),
            };
            if let Err(e) = result {
                warn!(?e, %cmd, "fm nav-command");
                return;
            }
            if let Some(c) = tw.upgrade() {
                c.reset_fm_pane_viewport(inst, p);
                if cmd == "other-pane" {
                    c.reset_fm_pane_viewport(inst, if p == 1 { 0 } else { 1 });
                }
                c.fm_refresh_ui(inst).await;
            }
        });
    }

    pub(in crate::window::main_window) fn on_fm_marquee_select(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
        from: i32,
        to: i32,
        additive: bool,
        columns: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        // Keep this on the UI stack: an async spawn reordered patches and left
        // the rubber-band highlight lagging the pointer until a full repaint.
        if let Err(e) = orchid_widgets::builtin::file_manager::select_index_range_sync(
            inst, p, from, to, additive, columns,
        ) {
            warn!(?e, "fm marquee");
            return;
        }
        self.fm_refresh_selection_ui(inst, p);
    }

    pub(in crate::window::main_window) fn on_fm_rename_selected(
        self: &Arc<Self>,
        fm_id: &SharedString,
        pane: i32,
    ) {
        let p = pane.max(0) as u8;
        let Some(inst) = self.fm_prepare_instance(fm_id, Some(p)) else {
            return;
        };
        let paths = self.fm_selected_paths(inst, pane.max(0) as u8);
        if paths.len() != 1 {
            return;
        }
        self.spawn_fm_action(inst, "fs.rename", paths);
    }
}
