//! Media player, RSS, recent files, and universal search handlers.

use std::sync::Arc;

use slint::SharedString;
use tracing::warn;
use uuid::Uuid;

use orchid_storage::LifecycleState;
use orchid_widgets::WidgetPayload;
use orchid_widgets::builtin::search::{self as search_widget, ActionTarget};

use crate::window::errors::media_localized_error;
use crate::window::spawn;

use super::MainWindowController;

impl MainWindowController {
    pub(super) fn on_rss_item_clicked(self: &Arc<Self>, link: &SharedString) {
        let s = link.as_str();
        if s.is_empty() {
            return;
        }
        tracing::debug!(target: "orchid_ui::rss", link = %s, "opening rss item");
        if let Err(e) = orchid_widgets::builtin::rss::open_link(s) {
            warn!(?e, "failed to open RSS link");
        }
    }

    pub(super) fn on_recent_files_item_clicked(self: &Arc<Self>, path: &SharedString) {
        let s = path.as_str();
        if s.is_empty() {
            return;
        }
        let Ok(fp) = orchid_fs::FsPath::new(s) else {
            return;
        };
        let path_label = s.to_string();
        let ctrl = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            if let Err(e) = Self::open_in_viewer_for_controller(ctrl, fp, true).await {
                warn!(?e, path = %path_label, "open recent file in viewer");
            }
        });
    }

    pub(super) fn on_media_play_pause(self: &Arc<Self>) {
        let Some((inst_id, is_playing)) = self.find_active_media_widget() else {
            return;
        };
        let t = Arc::downgrade(self);
        spawn::spawn_local(async move {
            let cmd = if is_playing { "pause" } else { "play" };
            if let Err(e) = orchid_widgets::builtin::media::execute_command(inst_id, cmd).await {
                warn!(?e, "media play/pause");
                if let Some(c) = t.upgrade() {
                    c.notify_media_control_failed(&e);
                }
            }
        });
    }

    pub(super) fn on_media_command(self: &Arc<Self>, cmd: &'static str) {
        let Some((inst_id, _)) = self.find_active_media_widget() else {
            return;
        };
        let t = Arc::downgrade(self);
        spawn::spawn_local(async move {
            if let Err(e) = orchid_widgets::builtin::media::execute_command(inst_id, cmd).await {
                warn!(?e, "media command");
                if let Some(c) = t.upgrade() {
                    c.notify_media_control_failed(&e);
                }
            }
        });
    }

    pub(super) fn notify_media_control_failed(self: &Arc<Self>, err: &orchid_widgets::builtin::media::MediaError) {
        let body = media_localized_error(&self.locale, err);
        self.push_notification(&self.locale.tr("widget-media-name"), &body, 2);
    }

    pub(super) fn find_active_media_widget(&self) -> Option<(Uuid, bool)> {
        let w = self.workspace_manager.active().ok()?;
        let cache = self.widget_manager.snapshot_cache();
        for inst in self.widget_manager.instances_for_workspace(w.id) {
            if inst.type_id == "media-player" {
                let is_playing = cache
                    .get(inst.id)
                    .and_then(|s| match &s.payload {
                        orchid_widgets::WidgetPayload::MediaPlayer(p) => Some(p.is_playing),
                        _ => None,
                    })
                    .unwrap_or(false);
                return Some((inst.id, is_playing));
            }
        }
        None
    }
















    pub(super) fn on_search_query_changed(self: &Arc<Self>, inst: &SharedString, q: &SharedString) {
        let Ok(instance_id) = Uuid::parse_str(inst.as_str()) else {
            return;
        };
        search_widget::universal_search_push_query(instance_id, q.to_string());
        if q.as_str().trim().is_empty() {
            self.search_selection.write().insert(instance_id, -1);
        } else {
            self.search_selection.write().insert(instance_id, 0);
        }
        // Do not rebuild the whole workspace on every keystroke — that recreates
        // SearchView, steals focus, and races the debouncer. Snapshot updates
        // arrive via `WidgetSnapshotUpdated` and are patched through
        // `patch_workspace_frames` on the next frame.
        let wm = self.widget_manager.clone();
        spawn::spawn_local_compat(async move {
            wm.touch(instance_id);
            if let Ok(inst_ref) = wm.get_instance(instance_id) {
                if *inst_ref.lifecycle.read() == LifecycleState::Sleeping {
                    let _ = wm
                        .change_lifecycle(instance_id, LifecycleState::Active)
                        .await;
                }
            }
        });
    }

    pub(super) fn on_search_candidate_activated(self: &Arc<Self>, inst: &SharedString, cand: &SharedString) {
        let Ok(instance_id) = Uuid::parse_str(inst.as_str()) else {
            return;
        };
        let candidate_id = cand.to_string();
        let this = Arc::clone(self);
        spawn::spawn_local_compat(async move {
            this.dispatch_search_action_target(instance_id, candidate_id).await;
        });
    }

    pub(super) async fn dispatch_search_action_target(self: &Arc<Self>, instance_id: Uuid, candidate_id: String) {
        let Some(target) =
            search_widget::universal_search_action_target(instance_id, candidate_id.as_str())
        else {
            warn!(%candidate_id, "unknown search candidate");
            return;
        };
        match target {
            ActionTarget::OpenFile(path) => {
                if let Err(e) = opener::open(&path) {
                    warn!(?e, path = %path, "open file from search");
                }
            }
            ActionTarget::RunCommand(cmd_id) => {
                self.dispatch_command(&cmd_id).await;
            }
            ActionTarget::OpenSettings(section) => {
                self.open_settings(&section);
            }
            ActionTarget::CopyText(text) => {
                match crate::widgets::terminal::ArboardClipboard::new() {
                    Ok(cb) => {
                        if let Err(e) = cb.copy(&text) {
                            warn!(?e, "copy search calc result");
                        } else {
                            self.push_notification(
                                &self.locale.tr("widget-calculator-name"),
                                &self.locale.tr("calc-copied"),
                                0,
                            );
                        }
                    }
                    Err(e) => warn!(?e, "open clipboard for search calc copy"),
                }
            }
        }
    }

    pub(super) fn on_search_selection_changed(self: &Arc<Self>, inst: &SharedString, new_idx: i32) {
        let Ok(instance_id) = Uuid::parse_str(inst.as_str()) else {
            return;
        };
        let count = self
            .widget_manager
            .snapshot_cache()
            .get(instance_id)
            .and_then(|s| match &s.payload {
                WidgetPayload::UniversalSearch(p) => Some(p.candidates.len() as i32),
                _ => None,
            })
            .unwrap_or(0);
        let clamped = if count == 0 {
            -1
        } else {
            new_idx.clamp(0, (count - 1) as i32)
        };
        self.search_selection.write().insert(instance_id, clamped);
        let _ = self.patch_workspace_frames(&[instance_id]);
    }
}
