//! HTML viewer ↔ WebView2 overlay glue.

use std::sync::Arc;

use slint::ComponentHandle;
use slint::Model;
use slint::VecModel;
use uuid::Uuid;

use orchid_viewers::ViewerSnapshot;
use orchid_widgets::WidgetPayload;

use crate::html_webview::{file_url_from_path, HtmlDocument};
use crate::slint_generated::WidgetFrameModel;

use super::MainWindowController;

impl MainWindowController {
    pub(super) fn on_html_embed_bounds(
        self: &Arc<Self>,
        id: Uuid,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        visible: bool,
    ) {
        let scale = f64::from(self.window.window().scale_factor());
        let px = |v: f32| (f64::from(v) * scale).round() as i32;
        self.html_webview.set_bounds(
            id,
            px(x),
            px(y),
            px(w),
            px(h),
            visible,
            parent_hwnd_bits(self.window.window()),
        );
    }

    pub(super) fn on_html_command(&self, id: Uuid, command: &str) {
        self.html_webview.command(id, command);
    }

    pub(super) fn sync_html_webview_document(&self, id: Uuid, snapshot: &ViewerSnapshot) {
        let document = match snapshot {
            ViewerSnapshot::Html(s) => {
                if let Some(path) = s.local_path.as_ref() {
                    file_url_from_path(path)
                        .map(HtmlDocument::Url)
                        .unwrap_or_else(|| {
                            HtmlDocument::Html(s.source_preview.as_ref().to_string())
                        })
                } else if s.source_preview.is_empty() {
                    HtmlDocument::None
                } else {
                    HtmlDocument::Html(s.source_preview.as_ref().to_string())
                }
            }
            _ => HtmlDocument::None,
        };
        self.html_webview.set_document(id, document);
    }

    pub(super) fn flush_html_webview_nav(self: &Arc<Self>) {
        let updates = self.html_webview.take_nav_updates();
        for u in updates {
            if u.surface_id.is_nil() {
                self.patch_html_nav(u.instance_id, u.can_go_back, u.can_go_forward);
                continue;
            }
            self.patch_browser_nav(
                u.instance_id,
                u.surface_id,
                u.can_go_back,
                u.can_go_forward,
                u.is_loading,
                u.zoom,
            );
            if let (Some(url), title) = (u.url.as_deref(), u.title.as_deref().unwrap_or("")) {
                orchid_widgets::builtin::browser::tab_navigated(
                    u.instance_id,
                    &u.surface_id.to_string(),
                    url,
                    title,
                );
            }
        }
        self.flush_browser_chrome();
        self.flush_browser_opens();
    }

    pub(super) fn sync_visible_html_webviews(&self) {
        let html_ids = visible_html_instance_ids(self);
        let browser_ids = visible_browser_instance_ids(self);
        let mut all = html_ids.clone();
        all.extend(browser_ids.iter().copied());
        for id in &html_ids {
            sync_html_from_cache(self, *id);
        }
        for id in &browser_ids {
            sync_browser_from_cache(self, *id);
        }
        self.html_webview.hide_except(&all);
    }

    pub(super) fn on_browser_embed_bounds(
        self: &Arc<Self>,
        id: Uuid,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        visible: bool,
    ) {
        let scale = f64::from(self.window.window().scale_factor());
        let px = |v: f32| (f64::from(v) * scale).round() as i32;
        let parent = parent_hwnd_bits(self.window.window());
        let Some(cfg) = orchid_widgets::builtin::browser::current_config(id) else {
            return;
        };
        let active_id = cfg.active_tab().id.clone();
        for tab in &cfg.tabs {
            let Ok(surface) = Uuid::parse_str(&tab.id) else {
                continue;
            };
            let is_active = tab.id == active_id;
            self.html_webview.set_bounds_surface(
                id,
                surface,
                px(x),
                px(y),
                px(w),
                px(h),
                visible && is_active,
                parent,
            );
        }
    }

    fn patch_browser_nav(
        &self,
        id: Uuid,
        surface: Uuid,
        can_go_back: bool,
        can_go_forward: bool,
        is_loading: Option<bool>,
        zoom: Option<f64>,
    ) {
        patch_browser_nav_in_model(
            &self.workspace_widgets,
            id,
            surface,
            can_go_back,
            can_go_forward,
            is_loading,
            zoom,
        );
        patch_browser_nav_in_model(
            &self.workspace_floating_widgets,
            id,
            surface,
            can_go_back,
            can_go_forward,
            is_loading,
            zoom,
        );
    }

    fn flush_browser_chrome(self: &Arc<Self>) {
        use crate::html_webview::BrowserChromeAction;
        use slint::SharedString;

        let events = self.html_webview.take_chrome_events();
        for ev in events {
            let id = SharedString::from(ev.instance_id.to_string());
            match ev.action {
                BrowserChromeAction::NewTab => self.on_browser_new_tab(&id),
                BrowserChromeAction::CloseTab => {
                    let idx = orchid_widgets::builtin::browser::current_config(ev.instance_id)
                        .map(|c| c.active_index as i32)
                        .unwrap_or(0);
                    self.on_browser_close_tab(&id, idx);
                }
                BrowserChromeAction::FocusAddress => {
                    self.bump_browser_focus_address(ev.instance_id)
                }
                BrowserChromeAction::Reload => {
                    self.on_browser_command(&id, &SharedString::from("reload"));
                }
                BrowserChromeAction::Stop => {
                    self.on_browser_command(&id, &SharedString::from("stop"));
                }
                BrowserChromeAction::Find => self.bump_browser_show_find(ev.instance_id),
                BrowserChromeAction::Bookmark => self.on_browser_toggle_bookmark(&id),
                BrowserChromeAction::Home => {
                    self.on_browser_command(&id, &SharedString::from("home"));
                }
                BrowserChromeAction::Back => {
                    self.on_browser_command(&id, &SharedString::from("back"));
                }
                BrowserChromeAction::Forward => {
                    self.on_browser_command(&id, &SharedString::from("forward"));
                }
                BrowserChromeAction::ZoomIn => {
                    self.on_browser_command(&id, &SharedString::from("zoom-in"));
                }
                BrowserChromeAction::ZoomOut => {
                    self.on_browser_command(&id, &SharedString::from("zoom-out"));
                }
                BrowserChromeAction::ZoomReset => {
                    self.on_browser_command(&id, &SharedString::from("zoom-reset"));
                }
                BrowserChromeAction::ReopenClosed => {
                    self.on_browser_command(&id, &SharedString::from("reopen-closed"));
                }
            }
        }
    }

    fn flush_browser_opens(self: &Arc<Self>) {
        let opens = self.html_webview.take_open_requests();
        for ev in opens {
            orchid_widgets::builtin::browser::new_tab_with_url(ev.instance_id, &ev.url);
            self.refresh_browser(ev.instance_id);
        }
    }

    fn patch_html_nav(&self, id: Uuid, can_go_back: bool, can_go_forward: bool) {
        patch_html_nav_in_model(&self.workspace_widgets, id, can_go_back, can_go_forward);
        patch_html_nav_in_model(
            &self.workspace_floating_widgets,
            id,
            can_go_back,
            can_go_forward,
        );
    }
}

fn patch_html_nav_in_model(
    model: &slint::ModelRc<WidgetFrameModel>,
    id: Uuid,
    can_go_back: bool,
    can_go_forward: bool,
) {
    let Some(v) = model.as_any().downcast_ref::<VecModel<WidgetFrameModel>>() else {
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
        if row.viewer.kind != 9 {
            return;
        }
        row.viewer.html.can_go_back = can_go_back;
        row.viewer.html.can_go_forward = can_go_forward;
        v.set_row_data(r, row);
        return;
    }
}

fn patch_browser_nav_in_model(
    model: &slint::ModelRc<WidgetFrameModel>,
    id: Uuid,
    surface: Uuid,
    can_go_back: bool,
    can_go_forward: bool,
    is_loading: Option<bool>,
    zoom: Option<f64>,
) {
    let Some(v) = model.as_any().downcast_ref::<VecModel<WidgetFrameModel>>() else {
        return;
    };
    let needle = id.to_string();
    let surface_s = surface.to_string();
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
        let Some(tabs) = row
            .browser
            .tabs
            .as_any()
            .downcast_ref::<VecModel<crate::slint_generated::BrowserTabEntry>>()
        else {
            return;
        };
        let active_is_surface = (0..tabs.row_count()).any(|i| {
            tabs.row_data(i)
                .is_some_and(|t| t.is_active && t.id.as_str() == surface_s)
        });
        if !active_is_surface {
            return;
        }
        row.browser.can_go_back = can_go_back;
        row.browser.can_go_forward = can_go_forward;
        if let Some(loading) = is_loading {
            row.browser.is_loading = loading;
        }
        if let Some(zoom) = zoom {
            row.browser.zoom_percent = (zoom * 100.0).round() as i32;
        }
        v.set_row_data(r, row);
        return;
    }
}

/// Collect HTML viewer instance ids currently present in the frame models.
pub(super) fn visible_html_instance_ids(c: &MainWindowController) -> Vec<Uuid> {
    let mut ids = Vec::new();
    collect_html_ids(&c.workspace_widgets, &mut ids);
    collect_html_ids(&c.workspace_floating_widgets, &mut ids);
    ids
}

fn collect_html_ids(model: &slint::ModelRc<WidgetFrameModel>, ids: &mut Vec<Uuid>) {
    let Some(v) = model.as_any().downcast_ref::<VecModel<WidgetFrameModel>>() else {
        return;
    };
    for r in 0..v.row_count() {
        let Some(row) = v.row_data(r) else {
            continue;
        };
        if row.viewer.kind != 9 {
            continue;
        }
        if let Ok(id) = Uuid::parse_str(row.instance_id.as_str()) {
            ids.push(id);
        }
    }
}

pub(super) fn sync_html_from_cache(c: &MainWindowController, id: Uuid) {
    let cache = c.widget_manager.snapshot_cache();
    let Some(ws) = cache.get(id) else {
        c.html_webview.set_document(id, HtmlDocument::None);
        return;
    };
    match &ws.payload {
        WidgetPayload::Viewer(vp) => c.sync_html_webview_document(id, &vp.snapshot),
        _ => c.html_webview.set_document(id, HtmlDocument::None),
    }
}

fn visible_browser_instance_ids(c: &MainWindowController) -> Vec<Uuid> {
    let mut ids = Vec::new();
    collect_type_ids(
        &c.workspace_widgets,
        orchid_widgets::builtin::browser::TYPE_ID,
        &mut ids,
    );
    collect_type_ids(
        &c.workspace_floating_widgets,
        orchid_widgets::builtin::browser::TYPE_ID,
        &mut ids,
    );
    ids
}

fn collect_type_ids(model: &slint::ModelRc<WidgetFrameModel>, type_id: &str, ids: &mut Vec<Uuid>) {
    let Some(v) = model.as_any().downcast_ref::<VecModel<WidgetFrameModel>>() else {
        return;
    };
    for r in 0..v.row_count() {
        let Some(row) = v.row_data(r) else {
            continue;
        };
        if row.type_id.as_str() != type_id {
            continue;
        }
        if let Ok(id) = Uuid::parse_str(row.instance_id.as_str()) {
            ids.push(id);
        }
    }
}

pub(super) fn sync_browser_from_cache(c: &MainWindowController, id: Uuid) {
    let Some(cfg) = orchid_widgets::builtin::browser::current_config(id) else {
        c.html_webview.destroy(id);
        return;
    };
    let active_id = cfg.active_tab().id.clone();
    for tab in &cfg.tabs {
        let Ok(surface) = Uuid::parse_str(&tab.id) else {
            continue;
        };
        c.html_webview
            .set_document_surface(id, surface, HtmlDocument::Url(tab.url.clone()));
        if tab.id != active_id {
            c.html_webview
                .set_bounds_surface(id, surface, 0, 0, 0, 0, false, 0);
        }
    }
}

fn parent_hwnd_bits(window: &slint::Window) -> isize {
    use slint::winit_030::WinitWindowAccessor;
    window
        .with_winit_window(|winit_window| {
            use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let handle = winit_window.window_handle().ok()?;
            match handle.as_raw() {
                RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
                _ => None,
            }
        })
        .flatten()
        .unwrap_or(0)
}
