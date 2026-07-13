//! Viewer widget: wraps an [`orchid_viewers::Viewer`] for any given path.

use std::sync::Arc;
use std::sync::LazyLock;

use async_trait::async_trait;
use dashmap::DashMap;
use orchid_storage::{LifecycleState, WidgetSize};
use orchid_viewers::{ArchiveViewer, ImageViewer, PdfViewer, SyntaxHighlighter, TextViewer, Viewer};
use orchid_viewers::ViewerSnapshot;
use parking_lot::RwLock;
use tokio::sync::Mutex;
use tracing::warn;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::error::WidgetError;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::ViewerPayload;
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory};

/// Stable type id.
pub const TYPE_ID: &str = "viewer";

/// Persisted viewer state (path + optional floating overlay rect).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct ViewerPersisted {
    path: Option<String>,
    /// When true, the viewer renders in the floating overlay (not the grid).
    #[serde(default)]
    floating: bool,
    #[serde(default)]
    float_x: Option<f32>,
    #[serde(default)]
    float_y: Option<f32>,
    #[serde(default)]
    float_w: Option<f32>,
    #[serde(default)]
    float_h: Option<f32>,
}

impl ViewerPersisted {
    fn floating_bounds(&self) -> Option<crate::layout::PixelBounds> {
        if !self.floating {
            return None;
        }
        Some(crate::layout::PixelBounds {
            x: self.float_x.unwrap_or(40.0),
            y: self.float_y.unwrap_or(40.0),
            width: self.float_w.unwrap_or(480.0).max(120.0),
            height: self.float_h.unwrap_or(360.0).max(120.0),
        })
    }

    fn from_live(path: Option<String>, floating: Option<crate::layout::PixelBounds>) -> Self {
        match floating {
            Some(b) => Self {
                path,
                floating: true,
                float_x: Some(b.x),
                float_y: Some(b.y),
                float_w: Some(b.width),
                float_h: Some(b.height),
            },
            None => Self {
                path,
                floating: false,
                float_x: None,
                float_y: None,
                float_w: None,
                float_h: None,
            },
        }
    }
}

/// Live viewer widget cores keyed by instance id (for UI callbacks).
static VIEWER_LIVE: LazyLock<DashMap<Uuid, Arc<ViewerWidgetInner>>> = LazyLock::new(DashMap::new);

/// Dependencies injected into every viewer instance.
#[derive(Clone)]
pub struct ViewerDeps {
    /// Filesystem provider registry.
    pub registry: Arc<orchid_fs::FsProviderRegistry>,
    /// Shared syntax highlighter (reused across text viewers).
    pub highlighter: Arc<SyntaxHighlighter>,
}

impl std::fmt::Debug for ViewerDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewerDeps").finish_non_exhaustive()
    }
}

struct ViewerWidgetInner {
    instance_id: Uuid,
    deps: ViewerDeps,
    viewer: Mutex<Option<Box<dyn Viewer>>>,
    snapshot: RwLock<Option<ViewerSnapshot>>,
    path: RwLock<Option<orchid_fs::FsPath>>,
    /// Path restored from persistence; opened in `on_create`.
    pending_path: RwLock<Option<orchid_fs::FsPath>>,
    /// Floating overlay bounds when undocked from the canvas grid.
    floating: RwLock<Option<crate::layout::PixelBounds>>,
    bus: Arc<orchid_core::EventBus>,
}

impl std::fmt::Debug for ViewerWidgetInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewerWidgetInner")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl ViewerWidgetInner {
    fn publish_refresh(&self) {
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    /// Open a path: picks the right viewer kind, opens it, and caches the
    /// first snapshot.
    async fn open_path(&self, path: orchid_fs::FsPath) -> WidgetResult<()> {
        let registry = self.deps.registry.clone();
        let highlighter = self.deps.highlighter.clone();
        *self.snapshot.write() = Some(ViewerSnapshot::Loading {
            path_display: path.as_str().to_string(),
        });
        *self.path.write() = Some(path.clone());
        self.publish_refresh();

        let select_res = orchid_viewers::select_viewer(&path, registry.clone(), highlighter).await;
        let mut viewer = match select_res {
            Ok(v) => v,
            Err(e) => {
                let path_display = path.as_str().to_string();
                warn!(path = %path_display, error = %e, "viewer dispatch failed");
                *self.snapshot.write() = Some(ViewerSnapshot::Error {
                    path_display,
                    message: e.to_string(),
                });
                self.publish_refresh();
                return Ok(());
            }
        };
        if let Err(e) = viewer.open(path.clone(), registry).await {
            warn!(error = %e, "viewer open failed");
            *self.snapshot.write() = Some(ViewerSnapshot::Error {
                path_display: path.as_str().to_string(),
                message: e.to_string(),
            });
            self.publish_refresh();
            return Ok(());
        }
        let snap = viewer.snapshot();
        *self.snapshot.write() = Some(snap);
        *self.viewer.lock().await = Some(viewer);
        self.publish_refresh();
        Ok(())
    }

    async fn close_viewer(&self) {
        let taken = self.viewer.lock().await.take();
        if let Some(mut v) = taken {
            let _ = v.close().await;
        }
        *self.snapshot.write() = None;
        *self.path.write() = None;
        self.publish_refresh();
    }

    async fn refresh_snapshot(&self) {
        let guard = self.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            *self.snapshot.write() = Some(v.snapshot());
        }
        drop(guard);
        self.publish_refresh();
    }
}

/// Viewer widget.
pub struct ViewerWidget {
    inner: Arc<ViewerWidgetInner>,
}

impl std::fmt::Debug for ViewerWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewerWidget")
            .field("instance_id", &self.inner.instance_id)
            .finish_non_exhaustive()
    }
}

impl ViewerWidget {
    /// Build an empty viewer widget.
    pub fn new(instance_id: Uuid, deps: ViewerDeps, bus: Arc<orchid_core::EventBus>) -> Self {
        Self {
            inner: Arc::new(ViewerWidgetInner {
                instance_id,
                deps,
                viewer: Mutex::new(None),
                snapshot: RwLock::new(None),
                path: RwLock::new(None),
                pending_path: RwLock::new(None),
                floating: RwLock::new(None),
                bus,
            }),
        }
    }

    /// Build a viewer that will reopen `path` on create.
    pub fn with_pending_path(
        instance_id: Uuid,
        deps: ViewerDeps,
        bus: Arc<orchid_core::EventBus>,
        path: orchid_fs::FsPath,
    ) -> Self {
        let w = Self::new(instance_id, deps, bus);
        *w.inner.pending_path.write() = Some(path);
        w
    }

    /// Build a viewer with pending path and floating overlay bounds.
    pub fn with_pending_path_and_floating(
        instance_id: Uuid,
        deps: ViewerDeps,
        bus: Arc<orchid_core::EventBus>,
        path: orchid_fs::FsPath,
        floating: crate::layout::PixelBounds,
    ) -> Self {
        let w = Self::with_pending_path(instance_id, deps, bus, path);
        *w.inner.floating.write() = Some(floating);
        w
    }

    /// Open a path on this widget instance.
    pub async fn open_path(&self, path: orchid_fs::FsPath) -> WidgetResult<()> {
        self.inner.open_path(path).await
    }

    /// Current file path when known.
    #[must_use]
    pub fn current_path(&self) -> Option<orchid_fs::FsPath> {
        self.inner.path.read().clone()
    }

    /// Floating overlay bounds when the viewer is undocked.
    #[must_use]
    pub fn floating_bounds(&self) -> Option<crate::layout::PixelBounds> {
        *self.inner.floating.read()
    }

    /// Set or clear floating overlay bounds.
    pub fn set_floating_bounds(&self, bounds: Option<crate::layout::PixelBounds>) {
        *self.inner.floating.write() = bounds;
    }
}

fn map_viewer_err(e: orchid_viewers::ViewerError) -> WidgetError {
    WidgetError::InvalidStateForOperation(e.to_string())
}

/// Approximate monospace line height used by the Slint text viewer.
const TEXT_LINE_HEIGHT_PX: f32 = 18.0;

/// Update image/PDF/text viewport size for fit/zoom/window math.
pub async fn set_viewport(instance_id: Uuid, width: f32, height: f32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let mut should_refresh = false;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
                img.set_viewport(width, height);
                should_refresh = true;
            } else if let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() {
                pdf.apply_viewport(width, height)
                    .await
                    .map_err(map_viewer_err)?;
                should_refresh = true;
            } else if let Some(tv) = v.as_any().downcast_ref::<TextViewer>() {
                let count = (height / TEXT_LINE_HEIGHT_PX).floor().max(1.0) as u32;
                // Keep the current first line; only resize the window.
                tv.set_visible_range(tv.first_visible_line(), count);
                should_refresh = true;
            }
        }
    }
    if should_refresh {
        inner.refresh_snapshot().await;
    }
    Ok(())
}

/// Open `path` on the viewer instance `instance_id`.
pub async fn open_path(instance_id: Uuid, path: orchid_fs::FsPath) -> WidgetResult<()> {
    let inner = VIEWER_LIVE
        .get(&instance_id)
        .map(|e| Arc::clone(e.value()))
        .ok_or_else(|| {
            WidgetError::InvalidStateForOperation("viewer widget not live".into())
        })?;
    inner.open_path(path).await
}

fn live_inner(instance_id: Uuid) -> WidgetResult<Arc<ViewerWidgetInner>> {
    VIEWER_LIVE
        .get(&instance_id)
        .map(|e| Arc::clone(e.value()))
        .ok_or_else(|| WidgetError::InvalidStateForOperation("viewer widget not live".into()))
}

/// Current open path for a live viewer instance, if any.
#[must_use]
pub fn current_path(instance_id: Uuid) -> Option<orchid_fs::FsPath> {
    VIEWER_LIVE
        .get(&instance_id)
        .and_then(|e| e.value().path.read().clone())
}

/// Floating overlay bounds for a live viewer, if undocked.
#[must_use]
pub fn floating_bounds(instance_id: Uuid) -> Option<crate::layout::PixelBounds> {
    VIEWER_LIVE
        .get(&instance_id)
        .and_then(|e| *e.value().floating.read())
}

/// Set or clear floating overlay bounds on a live viewer.
pub fn set_floating_bounds(
    instance_id: Uuid,
    bounds: Option<crate::layout::PixelBounds>,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    *inner.floating.write() = bounds;
    Ok(())
}

/// Find a live viewer in `instance_ids` that already has `path` open.
#[must_use]
pub fn find_instance_for_path(instance_ids: &[Uuid], path: &orchid_fs::FsPath) -> Option<Uuid> {
    for id in instance_ids {
        if current_path(*id).as_ref() == Some(path) {
            return Some(*id);
        }
    }
    None
}

/// Image toolbar: zoom in (~10%).
pub async fn image_zoom_in(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
            img.zoom_by(1.1);
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Image toolbar: zoom out (~10%).
pub async fn image_zoom_out(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
            img.zoom_by(1.0 / 1.1);
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Image toolbar: fit to viewport.
pub async fn image_fit(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
            img.fit_to_viewport();
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Image toolbar: 1:1.
pub async fn image_actual_size(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
            img.actual_size();
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Image toolbar: rotate clockwise.
pub async fn image_rotate_cw(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
            img.rotate_cw();
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Image toolbar: rotate counter-clockwise.
pub async fn image_rotate_ccw(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
            img.rotate_ccw();
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Image toolbar: toggle horizontal flip.
pub async fn image_flip_h(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
            img.flip_horizontal();
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Image toolbar: toggle vertical flip.
pub async fn image_flip_v(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
            img.flip_vertical();
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Image: pan by logical pixels.
pub async fn image_pan(instance_id: Uuid, dx: f32, dy: f32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
            img.pan(dx, dy);
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// PDF: previous page (no-op when unavailable).
pub async fn pdf_prev_page(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() {
                pdf.prev_page().await.map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// PDF: next page.
pub async fn pdf_next_page(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() {
                pdf.next_page().await.map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// PDF: fit width.
pub async fn pdf_fit_width(instance_id: Uuid, viewport_w: f32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() {
                pdf.fit_width(viewport_w).await.map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// PDF: fit page.
pub async fn pdf_fit_page(instance_id: Uuid, viewport_w: f32, viewport_h: f32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() {
                pdf.fit_page(viewport_w, viewport_h)
                    .await
                    .map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// PDF: zoom in.
pub async fn pdf_zoom_in(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() {
                pdf.zoom_in().await.map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// PDF: zoom out.
pub async fn pdf_zoom_out(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() {
                pdf.zoom_out().await.map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// PDF: go to 1-based page index.
pub async fn pdf_go_to_page(instance_id: Uuid, page: i32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() {
                pdf.go_to_page(page.max(1) as u32)
                    .await
                    .map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Archive: open folder.
pub async fn archive_navigate_into(instance_id: Uuid, path: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_mut() {
            if let Some(ar) = v.as_any_mut().downcast_mut::<ArchiveViewer>() {
                ar.navigate_into(&path).await.map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Archive: go up.
pub async fn archive_navigate_up(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_mut() {
            if let Some(ar) = v.as_any_mut().downcast_mut::<ArchiveViewer>() {
                ar.navigate_up().await.map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Archive: select file for preview.
pub async fn archive_select(instance_id: Uuid, path: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_mut() {
            if let Some(ar) = v.as_any_mut().downcast_mut::<ArchiveViewer>() {
                ar.select(&path).await.map_err(map_viewer_err)?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Archive: extract the selected file beside the archive.
pub async fn archive_extract_selected(instance_id: Uuid) -> WidgetResult<String> {
    let inner = live_inner(instance_id)?;
    let dest = {
        let mut guard = inner.viewer.lock().await;
        let v = guard
            .as_mut()
            .ok_or_else(|| WidgetError::InvalidStateForOperation("no viewer".into()))?;
        let ar = v
            .as_any_mut()
            .downcast_mut::<ArchiveViewer>()
            .ok_or_else(|| WidgetError::InvalidStateForOperation("not an archive".into()))?;
        ar.extract_selected_to_sibling()
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
    };
    inner.refresh_snapshot().await;
    Ok(dest.to_string_lossy().into_owned())
}

/// Archive: extract all entries into a sibling folder.
pub async fn archive_extract_all(instance_id: Uuid) -> WidgetResult<String> {
    let inner = live_inner(instance_id)?;
    let dest = {
        let mut guard = inner.viewer.lock().await;
        let v = guard
            .as_mut()
            .ok_or_else(|| WidgetError::InvalidStateForOperation("no viewer".into()))?;
        let ar = v
            .as_any_mut()
            .downcast_mut::<ArchiveViewer>()
            .ok_or_else(|| WidgetError::InvalidStateForOperation("not an archive".into()))?;
        ar.extract_all_to_sibling()
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
            .0
    };
    inner.refresh_snapshot().await;
    Ok(dest.to_string_lossy().into_owned())
}

/// Text: scroll by whole lines.
pub async fn text_scroll(instance_id: Uuid, delta: i32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(tv) = v.as_any().downcast_ref::<TextViewer>() {
                tv.scroll_lines(delta);
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Text: switch read / edit mode (`edit == true` → edit).
pub async fn text_set_mode(instance_id: Uuid, edit: bool) -> WidgetResult<()> {
    use orchid_viewers::TextViewerMode;
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(tv) = v.as_any().downcast_ref::<TextViewer>() {
                tv.set_mode(if edit {
                    TextViewerMode::Edit
                } else {
                    TextViewerMode::Read
                });
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Text: flip read ↔ edit. Returns `true` when the resulting mode is edit.
///
/// Leaving edit mode with unsaved changes is allowed for MVP — the dirty ●
/// indicator remains until save.
pub async fn text_toggle_edit(instance_id: Uuid) -> WidgetResult<bool> {
    use orchid_viewers::TextViewerMode;
    let inner = live_inner(instance_id)?;
    let edit = {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(false);
        };
        let Some(tv) = v.as_any().downcast_ref::<TextViewer>() else {
            return Ok(false);
        };
        let edit = tv.mode() == TextViewerMode::Read;
        tv.set_mode(if edit {
            TextViewerMode::Edit
        } else {
            TextViewerMode::Read
        });
        edit
    };
    inner.refresh_snapshot().await;
    Ok(edit)
}

/// Text: push the full document contents from the plain editor.
pub async fn text_push_edit(instance_id: Uuid, text: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(tv) = v.as_any().downcast_ref::<TextViewer>() {
                tv.replace_content(&text)
                    .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Text: save buffer to disk (clears dirty).
pub async fn text_save(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let mut guard = inner.viewer.lock().await;
        let v = guard
            .as_mut()
            .ok_or_else(|| WidgetError::InvalidStateForOperation("no viewer".into()))?;
        v.save()
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    }
    inner.refresh_snapshot().await;
    Ok(())
}

#[async_trait]
impl Widget for ViewerWidget {
    fn type_id(&self) -> &'static str {
        TYPE_ID
    }
    fn instance_id(&self) -> Uuid {
        self.inner.instance_id
    }
    async fn on_create(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        VIEWER_LIVE.insert(self.inner.instance_id, Arc::clone(&self.inner));
        let pending = self.inner.pending_path.write().take();
        if let Some(path) = pending {
            if let Err(e) = self.inner.open_path(path).await {
                warn!(error = %e, "viewer: failed to reopen persisted path");
            }
        }
        Ok(())
    }
    async fn on_activate(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }
    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }
    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.inner.close_viewer().await;
        Ok(())
    }
    async fn on_close(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.inner.close_viewer().await;
        VIEWER_LIVE.remove(&self.inner.instance_id);
        Ok(())
    }
    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }
    fn snapshot(&self) -> Option<WidgetSnapshot> {
        let snap = match self.inner.snapshot.read().clone() {
            Some(s) => s,
            None => {
                let pd = self
                    .inner
                    .path
                    .read()
                    .as_ref()
                    .map(|p| p.as_str().to_string())
                    .unwrap_or_default();
                ViewerSnapshot::Loading { path_display: pd }
            }
        };
        let title = match &snap {
            ViewerSnapshot::Image(s) => title_from(&s.path_display),
            ViewerSnapshot::Pdf(s) => title_from(&s.path_display),
            ViewerSnapshot::Text(s) => title_from(&s.path_display),
            ViewerSnapshot::Archive(s) => title_from(&s.path_display),
            ViewerSnapshot::Loading { path_display } | ViewerSnapshot::Error { path_display, .. } => {
                title_from(path_display)
            }
        };
        Some(WidgetSnapshot {
            instance_id: self.inner.instance_id,
            widget_type: TYPE_ID,
            title,
            status: WidgetStatus::Ready,
            payload: WidgetPayload::Viewer(ViewerPayload { snapshot: snap }),
        })
    }
    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        let path = self
            .inner
            .path
            .read()
            .as_ref()
            .map(|p| p.as_str().to_string());
        let floating = *self.inner.floating.read();
        state_codec::save_state(&ViewerPersisted::from_live(path, floating))
    }
    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let persisted: ViewerPersisted = state_codec::restore_state(bytes)?;
        let floating = persisted.floating_bounds();
        if let Some(raw) = persisted.path {
            match orchid_fs::FsPath::new(&raw) {
                Ok(p) => *self.inner.pending_path.write() = Some(p),
                Err(e) => warn!(error = %e, path = %raw, "viewer: invalid persisted path"),
            }
        }
        *self.inner.floating.write() = floating;
        Ok(())
    }
    fn capabilities(&self) -> WidgetCapabilities {
        WidgetCapabilities {
            supports_resize: true,
            min_size: Some(WidgetSize::Medium),
            max_size: None,
            preferred_size: Some(WidgetSize::Large),
            allows_grouping: true,
            keeps_state_when_unloaded: true,
            has_settings_panel: false,
        }
    }
}

fn title_from(path_display: &str) -> String {
    if path_display.is_empty() {
        "Viewer".into()
    } else {
        path_display
            .rsplit('/')
            .next()
            .unwrap_or(path_display)
            .to_string()
    }
}

/// Descriptor for the viewer widget. The caller injects shared deps
/// (provider registry + syntax highlighter).
#[must_use]
pub fn descriptor(deps: ViewerDeps) -> WidgetDescriptor {
    let factory: WidgetFactory = Arc::new(move |ctx: WidgetContext, state_bytes| {
        let persisted = match state_bytes {
            Some(bytes) => state_codec::restore_state::<ViewerPersisted>(bytes).unwrap_or_default(),
            None => ViewerPersisted::default(),
        };
        let widget = match (
            persisted.path.as_deref().and_then(|raw| orchid_fs::FsPath::new(raw).ok()),
            persisted.floating_bounds(),
        ) {
            (Some(path), Some(floating)) => ViewerWidget::with_pending_path_and_floating(
                ctx.instance_id,
                deps.clone(),
                ctx.bus.clone(),
                path,
                floating,
            ),
            (Some(path), None) => ViewerWidget::with_pending_path(
                ctx.instance_id,
                deps.clone(),
                ctx.bus.clone(),
                path,
            ),
            (None, floating) => {
                let w = ViewerWidget::new(ctx.instance_id, deps.clone(), ctx.bus.clone());
                if let Some(b) = floating {
                    w.set_floating_bounds(Some(b));
                }
                w
            }
        };
        Ok(Box::new(widget) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-viewer-name",
        description_key: "widget-viewer-desc",
        icon_name: "viewer",
        category: WidgetCategory::Productivity,
        default_size: WidgetSize::Large,
        min_size: Some(WidgetSize::Medium),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: true,
        factory,
    }
}
