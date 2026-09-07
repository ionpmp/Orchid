//! Browser widget handlers for [`MainWindowController`].

use std::sync::Arc;

use slint::SharedString;
use uuid::Uuid;

use crate::window::spawn;

use super::MainWindowController;

impl MainWindowController {
    fn parse_browser_id(id: &SharedString) -> Option<Uuid> {
        Uuid::parse_str(id.as_str()).ok()
    }

    pub(super) fn on_browser_select_tab(self: &Arc<Self>, id: &SharedString, index: i32) {
        let Some(inst) = Self::parse_browser_id(id) else {
            return;
        };
        orchid_widgets::builtin::browser::select_tab(inst, index);
        self.refresh_browser(inst);
    }

    pub(super) fn on_browser_new_tab(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_browser_id(id) else {
            return;
        };
        orchid_widgets::builtin::browser::new_tab(inst);
        self.refresh_browser(inst);
    }

    pub(super) fn on_browser_close_tab(self: &Arc<Self>, id: &SharedString, index: i32) {
        let Some(inst) = Self::parse_browser_id(id) else {
            return;
        };
        let closed_surface = orchid_widgets::builtin::browser::current_config(inst)
            .and_then(|cfg| cfg.tabs.get(index as usize).map(|t| t.id.clone()));
        orchid_widgets::builtin::browser::close_tab(inst, index);
        if let Some(tab_id) = closed_surface {
            if let Ok(surface) = Uuid::parse_str(&tab_id) {
                self.html_webview.destroy_surface(inst, surface);
            }
        }
        self.refresh_browser(inst);
    }

    pub(super) fn on_browser_navigate(self: &Arc<Self>, id: &SharedString, url: &SharedString) {
        let Some(inst) = Self::parse_browser_id(id) else {
            return;
        };
        orchid_widgets::builtin::browser::navigate(inst, url.as_str());
        self.refresh_browser(inst);
    }

    pub(super) fn on_browser_command(self: &Arc<Self>, id: &SharedString, command: &SharedString) {
        let Some(inst) = Self::parse_browser_id(id) else {
            return;
        };
        if command.as_str() == "home" {
            orchid_widgets::builtin::browser::go_home(inst);
            self.refresh_browser(inst);
            return;
        }
        if command.as_str() == "reopen-closed" {
            orchid_widgets::builtin::browser::reopen_closed_tab(inst);
            self.refresh_browser(inst);
            return;
        }
        if command.as_str() == "next-tab" {
            orchid_widgets::builtin::browser::cycle_tab(inst, 1);
            self.refresh_browser(inst);
            return;
        }
        if command.as_str() == "prev-tab" {
            orchid_widgets::builtin::browser::cycle_tab(inst, -1);
            self.refresh_browser(inst);
            return;
        }
        if let Some(rest) = command.as_str().strip_prefix("move-tab:") {
            let mut parts = rest.split(':');
            if let (Some(from), Some(to)) = (parts.next(), parts.next()) {
                if let (Ok(from), Ok(to)) = (from.parse::<i32>(), to.parse::<i32>()) {
                    orchid_widgets::builtin::browser::move_tab(inst, from, to);
                    self.refresh_browser(inst);
                }
            }
            return;
        }
        if let Some(dl) = command.as_str().strip_prefix("download-cancel:") {
            if let Ok(id) = Uuid::parse_str(dl) {
                self.html_webview.cancel_download(id);
            }
            return;
        }
        if let Some(dl) = command.as_str().strip_prefix("download-open:") {
            if let Some(path) = orchid_widgets::builtin::browser::download_path(inst, dl) {
                if let Err(e) = opener::open(&path) {
                    tracing::warn!(?e, %path, "browser download open");
                }
            }
            return;
        }
        if let Some(dl) = command.as_str().strip_prefix("download-show:") {
            if let Some(path) = orchid_widgets::builtin::browser::download_path(inst, dl) {
                let folder = std::path::Path::new(&path)
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new(&path));
                if let Ok(fs) = orchid_fs::FsPath::from_local(folder) {
                    self.reveal_folder_in_fm(fs);
                }
            }
            return;
        }
        if let Some(idx) = command.as_str().strip_prefix("download-remove:") {
            if let Ok(index) = idx.parse::<i32>() {
                orchid_widgets::builtin::browser::remove_download(inst, index);
                self.refresh_browser(inst);
            }
            return;
        }
        let Some(tab_id) = orchid_widgets::builtin::browser::active_tab_id(inst) else {
            return;
        };
        let Ok(surface) = Uuid::parse_str(&tab_id) else {
            return;
        };
        self.html_webview
            .command_surface(inst, surface, command.as_str());
    }

    pub(super) fn on_browser_find(
        self: &Arc<Self>,
        id: &SharedString,
        query: &SharedString,
        forward: bool,
    ) {
        let Some(inst) = Self::parse_browser_id(id) else {
            return;
        };
        let Some(tab_id) = orchid_widgets::builtin::browser::active_tab_id(inst) else {
            return;
        };
        let Ok(surface) = Uuid::parse_str(&tab_id) else {
            return;
        };
        self.html_webview
            .find_in_page(inst, surface, query.as_str(), forward);
    }

    pub(super) fn on_browser_toggle_bookmark(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_browser_id(id) else {
            return;
        };
        orchid_widgets::builtin::browser::toggle_bookmark(inst);
        self.refresh_browser(inst);
    }

    pub(super) fn on_browser_open_bookmark(self: &Arc<Self>, id: &SharedString, index: i32) {
        let Some(inst) = Self::parse_browser_id(id) else {
            return;
        };
        orchid_widgets::builtin::browser::open_bookmark(inst, index);
        self.refresh_browser(inst);
    }

    pub(super) fn on_browser_remove_bookmark(self: &Arc<Self>, id: &SharedString, index: i32) {
        let Some(inst) = Self::parse_browser_id(id) else {
            return;
        };
        orchid_widgets::builtin::browser::remove_bookmark(inst, index);
        self.refresh_browser(inst);
    }

    pub(super) fn bump_browser_focus_address(&self, inst: Uuid) {
        bump_browser_gen(&self.workspace_widgets, inst, 0);
        bump_browser_gen(&self.workspace_floating_widgets, inst, 0);
    }

    pub(super) fn bump_browser_show_find(&self, inst: Uuid) {
        bump_browser_gen(&self.workspace_widgets, inst, 1);
        bump_browser_gen(&self.workspace_floating_widgets, inst, 1);
    }

    pub(super) fn bump_browser_show_downloads(&self, inst: Uuid) {
        bump_browser_gen(&self.workspace_widgets, inst, 2);
        bump_browser_gen(&self.workspace_floating_widgets, inst, 2);
    }

    pub(super) fn on_browser_open_external(self: &Arc<Self>, id: &SharedString) {
        let Some(inst) = Self::parse_browser_id(id) else {
            return;
        };
        let Some(cfg) = orchid_widgets::builtin::browser::current_config(inst) else {
            return;
        };
        let url = cfg.active_tab().url.clone();
        if url.is_empty() || url == "about:blank" {
            return;
        }
        if let Err(e) = opener::open(&url) {
            tracing::warn!(?e, %url, "browser open external");
            let title = self.locale.tr("widget-browser-name");
            self.push_notification(&title, &e.to_string(), 3);
        }
    }

    pub(super) fn refresh_browser(self: &Arc<Self>, inst_id: Uuid) {
        let wm = self.widget_manager.clone();
        let t = Arc::downgrade(self);
        spawn::spawn_local_compat(async move {
            let _ = wm.refresh_snapshot_cache(inst_id).await;
            if let Some(c) = t.upgrade() {
                c.schedule_instance_patch(inst_id);
            }
        });
    }
}

fn bump_browser_gen(
    model: &slint::ModelRc<crate::slint_generated::WidgetFrameModel>,
    id: Uuid,
    kind: u8,
) {
    use slint::Model;
    use slint::VecModel;

    let Some(v) = model
        .as_any()
        .downcast_ref::<VecModel<crate::slint_generated::WidgetFrameModel>>()
    else {
        return;
    };
    let needle = id.to_string();
    for r in 0..v.row_count() {
        let Some(mut row) = v.row_data(r) else {
            continue;
        };
        if row.instance_id.as_str() != needle {
            continue;
        }
        if row.type_id.as_str() != orchid_widgets::builtin::browser::TYPE_ID {
            return;
        }
        match kind {
            0 => row.browser.focus_address_gen = row.browser.focus_address_gen.saturating_add(1),
            2 => row.browser.show_downloads_gen = row.browser.show_downloads_gen.saturating_add(1),
            _ => row.browser.show_find_gen = row.browser.show_find_gen.saturating_add(1),
        }
        v.set_row_data(r, row);
        return;
    }
}
