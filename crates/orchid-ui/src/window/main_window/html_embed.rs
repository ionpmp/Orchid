//! HTML viewer ↔ WebView2 overlay glue.

use std::sync::Arc;

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
                        .unwrap_or_else(|| HtmlDocument::Html(s.source_preview.as_ref().to_string()))
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

    pub(super) fn flush_html_webview_nav(&self) {
        let updates = self.html_webview.take_nav_updates();
        if updates.is_empty() {
            return;
        }
        for u in updates {
            self.patch_html_nav(u.instance_id, u.can_go_back, u.can_go_forward);
        }
    }

    pub(super) fn sync_visible_html_webviews(&self) {
        let ids = visible_html_instance_ids(self);
        for id in &ids {
            sync_html_from_cache(self, *id);
        }
        self.html_webview.hide_except(&ids);
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
