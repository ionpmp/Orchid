//! Viewer widget: wraps an [`orchid_viewers::Viewer`] for any given path.

mod image_nav;

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::LazyLock;

use async_trait::async_trait;
use dashmap::DashMap;
use orchid_storage::{LifecycleState, WidgetSize};
use orchid_viewers::ViewerSnapshot;
use orchid_viewers::{
    ArchiveViewer, DocumentViewer, ImageFitMode, ImageViewer, PdfViewer, SyntaxHighlighter,
    TextViewer, ViewTransform, Viewer,
};
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
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};

/// Stable type id.
pub const TYPE_ID: &str = "viewer";

/// Persisted viewer state (path + optional floating overlay rect).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    /// Wrap folder playlist at the ends.
    #[serde(default = "default_true")]
    image_loop: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ViewerPersisted {
    fn default() -> Self {
        Self {
            path: None,
            floating: false,
            float_x: None,
            float_y: None,
            float_w: None,
            float_h: None,
            image_loop: true,
        }
    }
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

    fn from_live(
        path: Option<String>,
        floating: Option<crate::layout::PixelBounds>,
        image_loop: bool,
    ) -> Self {
        match floating {
            Some(b) => Self {
                path,
                floating: true,
                float_x: Some(b.x),
                float_y: Some(b.y),
                float_w: Some(b.width),
                float_h: Some(b.height),
                image_loop,
            },
            None => Self {
                path,
                floating: false,
                float_x: None,
                float_y: None,
                float_w: None,
                float_h: None,
                image_loop,
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
    /// After the next successful open, switch a text viewer into edit mode.
    pending_edit: AtomicBool,
    /// Floating overlay bounds when undocked from the canvas grid.
    floating: RwLock<Option<crate::layout::PixelBounds>>,
    /// Image folder playlist (next/prev, loop, recent).
    image_nav: RwLock<image_nav::ImageFolderNav>,
    /// Last zoom / fit per path (and the most recent view for new files).
    image_views: RwLock<ImageViewMemory>,
    bus: Arc<orchid_core::EventBus>,
}

#[derive(Clone, Copy)]
struct SavedImageView {
    fit: ImageFitMode,
    transform: ViewTransform,
}

struct ImageViewMemory {
    by_path: HashMap<String, SavedImageView>,
    order: VecDeque<String>,
    last: Option<SavedImageView>,
}

impl Default for ImageViewMemory {
    fn default() -> Self {
        Self {
            by_path: HashMap::new(),
            order: VecDeque::new(),
            last: None,
        }
    }
}

impl ImageViewMemory {
    fn insert(&mut self, path: String, view: SavedImageView) {
        if self.by_path.insert(path.clone(), view).is_some() {
            self.order.retain(|p| p != &path);
        }
        self.order.push_back(path);
        while self.order.len() > 32 {
            if let Some(old) = self.order.pop_front() {
                self.by_path.remove(&old);
            }
        }
        self.last = Some(view);
    }

    fn lookup(&self, path: &str) -> (Option<SavedImageView>, bool) {
        if let Some(v) = self.by_path.get(path).copied() {
            (Some(v), true)
        } else {
            (self.last, false)
        }
    }
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

    async fn remember_current_image_view(&self) {
        let path = match self.path.read().clone() {
            Some(p) if is_image_path(&p) => p,
            _ => return,
        };
        let guard = self.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return;
        };
        let Some(img) = v.as_any().downcast_ref::<ImageViewer>() else {
            return;
        };
        let (fit, transform) = img.capture_view();
        drop(guard);
        self.image_views
            .write()
            .insert(path.as_str().to_string(), SavedImageView { fit, transform });
    }

    async fn restore_image_view(&self, path: &orchid_fs::FsPath) {
        let (saved, exact) = self.image_views.read().lookup(path.as_str());
        let Some(saved) = saved else {
            return;
        };
        let guard = self.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return;
        };
        let Some(img) = v.as_any().downcast_ref::<ImageViewer>() else {
            return;
        };
        if exact {
            img.restore_view(saved.fit, saved.transform);
        } else if saved.fit.tracks_viewport() {
            img.set_fit_mode(saved.fit);
        } else {
            img.restore_zoom_only(saved.transform.zoom);
        }
    }

    /// Open a path: picks the right viewer kind, opens it, and caches the
    /// first snapshot.
    async fn open_path(&self, path: orchid_fs::FsPath) -> WidgetResult<()> {
        self.remember_current_image_view().await;
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
        if self.pending_edit.swap(false, Ordering::Relaxed) {
            if let Some(tv) = viewer.as_any().downcast_ref::<TextViewer>() {
                tv.set_mode(orchid_viewers::TextViewerMode::Edit);
            }
        }
        let snap = viewer.snapshot();
        *self.snapshot.write() = Some(snap);
        *self.viewer.lock().await = Some(viewer);
        if is_image_path(&path) {
            self.after_image_opened(&path).await;
            self.restore_image_view(&path).await;
            let guard = self.viewer.lock().await;
            if let Some(v) = guard.as_ref() {
                *self.snapshot.write() = Some(v.snapshot());
            }
        }
        self.overlay_image_nav();
        self.publish_refresh();
        Ok(())
    }

    fn overlay_image_nav(&self) {
        let Some(snap) = self.snapshot.write().take() else {
            return;
        };
        *self.snapshot.write() = Some(apply_image_nav(snap, &self.image_nav.read()));
    }

    async fn after_image_opened(&self, path: &orchid_fs::FsPath) {
        let parent = path.parent();
        let need_list = {
            let nav = self.image_nav.read();
            nav.folder.as_ref() != parent.as_ref() || !nav.siblings.iter().any(|p| p == path)
        };
        if need_list {
            if let Some(folder) = parent {
                if let Some(list) =
                    image_nav::list_image_siblings(&self.deps.registry, &folder).await
                {
                    self.image_nav.write().set_folder(folder, list, path);
                }
            }
        } else {
            self.image_nav.write().set_current(path);
        }
        self.image_nav.write().push_history(path);
    }

    async fn navigate_images(&self, step: image_nav::NavStep) -> WidgetResult<()> {
        {
            let path = self.path.read().clone();
            if let Some(path) = path.as_ref() {
                if is_image_path(path) {
                    let need = {
                        let nav = self.image_nav.read();
                        nav.siblings.is_empty()
                    };
                    if need {
                        self.after_image_opened(path).await;
                    }
                }
            }
        }
        let n = self.image_nav.read().siblings.len().max(1);
        for _ in 0..n {
            let Some(idx) = self.image_nav.read().pick(step) else {
                return Ok(());
            };
            let Some(next) = self.image_nav.read().siblings.get(idx).cloned() else {
                return Ok(());
            };
            self.open_path(next.clone()).await?;
            let failed = matches!(*self.snapshot.read(), Some(ViewerSnapshot::Error { .. }));
            if failed {
                let mut nav = self.image_nav.write();
                nav.index = idx;
                nav.mark_unreadable(&next);
                continue;
            }
            return Ok(());
        }
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
            *self.snapshot.write() = Some(apply_image_nav(v.snapshot(), &self.image_nav.read()));
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
                pending_edit: AtomicBool::new(false),
                floating: RwLock::new(None),
                image_nav: RwLock::new(image_nav::ImageFolderNav::default()),
                image_views: RwLock::new(ImageViewMemory::default()),
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
        .ok_or_else(|| WidgetError::InvalidStateForOperation("viewer widget not live".into()))?;
    inner.open_path(path).await
}

/// Open `path` and enter text-edit mode when the file is a text document.
pub async fn open_path_for_edit(instance_id: Uuid, path: orchid_fs::FsPath) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    inner.pending_edit.store(true, Ordering::Relaxed);
    inner.open_path(path).await
}

fn is_image_path(path: &orchid_fs::FsPath) -> bool {
    path.extension()
        .is_some_and(orchid_viewers::is_image_file_extension)
}

fn apply_image_nav(snap: ViewerSnapshot, nav: &image_nav::ImageFolderNav) -> ViewerSnapshot {
    match snap {
        ViewerSnapshot::Image(mut s) => {
            s.folder_index = nav.index.saturating_add(1) as u32;
            s.folder_count = nav.siblings.len() as u32;
            s.loop_folder = nav.loop_playlist;
            s.recent_paths = nav.recent_paths();
            ViewerSnapshot::Image(s)
        }
        other => other,
    }
}

/// Parent folder of the current image, when known.
#[must_use]
pub fn current_image_folder(instance_id: Uuid) -> Option<orchid_fs::FsPath> {
    let inner = VIEWER_LIVE.get(&instance_id)?;
    if let Some(folder) = inner.value().image_nav.read().folder.clone() {
        return Some(folder);
    }
    let path = inner.value().path.read().clone();
    path.as_ref().and_then(orchid_fs::FsPath::parent)
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

/// Image: zoom by `factor` around the viewport center (pinch / commands).
pub async fn image_zoom_by(instance_id: Uuid, factor: f32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
            img.zoom_by(factor);
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
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

/// Image toolbar / keyboard command (`fit-width`, `fit-height`, `fit-shrink`,
/// `bg-next`, `fullscreen`, `kiosk`, …).
pub async fn image_command(instance_id: Uuid, command: &str) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        let Some(img) = v.as_any().downcast_ref::<ImageViewer>() else {
            return Ok(());
        };
        match command {
            "fit-window" | "fit" => img.fit_to_viewport(),
            "fit-width" => img.fit_to_width(),
            "fit-height" => img.fit_to_height(),
            "fit-shrink" => img.fit_shrink(),
            "actual" => img.actual_size(),
            "bg-next" => img.cycle_background(),
            "fullscreen" => img.toggle_chrome_hidden(),
            "kiosk" => img.toggle_kiosk(),
            "exit-immersive" => img.exit_immersive(),
            "lens" => img.toggle_lens(),
            "next" | "prev" | "first" | "last" | "random" | "loop" => {}
            cmd if cmd.starts_with("goto:") || cmd.starts_with("recent:") => {}
            cmd if let Some(raw) = cmd.strip_prefix("zoom:") => {
                let raw = raw.trim().trim_end_matches('%');
                if let Ok(p) = raw.parse::<f32>() {
                    img.zoom_to_percent(p);
                }
            }
            cmd if let Some(raw) = cmd.strip_prefix("zoom-at:") => {
                let parts: Vec<&str> = raw.split(':').collect();
                if parts.len() == 3 {
                    if let (Ok(f), Ok(x), Ok(y)) = (
                        parts[0].parse::<f32>(),
                        parts[1].parse::<f32>(),
                        parts[2].parse::<f32>(),
                    ) {
                        img.zoom_at(f, x, y);
                    }
                }
            }
            cmd if let Some(raw) = cmd.strip_prefix("zoom-rect:") => {
                let parts: Vec<&str> = raw.split(':').collect();
                if parts.len() == 4 {
                    if let (Ok(x0), Ok(y0), Ok(x1), Ok(y1)) = (
                        parts[0].parse::<f32>(),
                        parts[1].parse::<f32>(),
                        parts[2].parse::<f32>(),
                        parts[3].parse::<f32>(),
                    ) {
                        img.zoom_to_rect(x0, y0, x1, y1);
                    }
                }
            }
            cmd if let Some(raw) = cmd.strip_prefix("nav-pan:") => {
                let parts: Vec<&str> = raw.split(':').collect();
                if parts.len() == 2 {
                    if let (Ok(nx), Ok(ny)) = (parts[0].parse::<f32>(), parts[1].parse::<f32>()) {
                        img.pan_to_image_fraction(nx, ny);
                    }
                }
            }
            cmd if let Some(raw) = cmd.strip_prefix("pinch:") => {
                if let Ok(f) = raw.parse::<f32>() {
                    img.zoom_by(f);
                }
            }
            _ => {}
        }
    }
    match command {
        "next" => inner.navigate_images(image_nav::NavStep::Next).await?,
        "prev" => inner.navigate_images(image_nav::NavStep::Prev).await?,
        "first" => inner.navigate_images(image_nav::NavStep::First).await?,
        "last" => inner.navigate_images(image_nav::NavStep::Last).await?,
        "random" => inner.navigate_images(image_nav::NavStep::Random).await?,
        "loop" => {
            let next = !inner.image_nav.read().loop_playlist;
            inner.image_nav.write().loop_playlist = next;
            inner.refresh_snapshot().await;
        }
        cmd if let Some(raw) = cmd.strip_prefix("goto:") => {
            if let Ok(n) = raw.parse::<usize>() {
                inner.navigate_images(image_nav::NavStep::Goto(n)).await?;
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("recent:") => {
            if let Ok(path) = orchid_fs::FsPath::new(raw) {
                inner.open_path(path).await?;
            }
        }
        _ => inner.refresh_snapshot().await,
    }
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

/// PDF: extract Unicode text for the current page (caller copies to clipboard).
pub async fn pdf_current_page_text(instance_id: Uuid) -> WidgetResult<String> {
    let inner = live_inner(instance_id)?;
    let guard = inner.viewer.lock().await;
    let Some(v) = guard.as_ref() else {
        return Err(WidgetError::InvalidStateForOperation("no viewer".into()));
    };
    let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() else {
        return Err(WidgetError::InvalidStateForOperation(
            "not a pdf viewer".into(),
        ));
    };
    pdf.current_page_text().await.map_err(map_viewer_err)
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

/// Text: toolbar action (`text` / `hex` / `bin` / `undo` / `redo` / `print`).
pub async fn text_action(instance_id: Uuid, action: String) -> WidgetResult<()> {
    use orchid_viewers::TextDisplayMode;
    let inner = live_inner(instance_id)?;
    match action.as_str() {
        "text" | "hex" | "bin" => {
            let mode = match action.as_str() {
                "hex" => TextDisplayMode::Hex,
                "bin" => TextDisplayMode::Binary,
                _ => TextDisplayMode::Text,
            };
            let guard = inner.viewer.lock().await;
            if let Some(v) = guard.as_ref() {
                if let Some(tv) = v.as_any().downcast_ref::<TextViewer>() {
                    tv.set_display_mode(mode);
                }
            }
        }
        "undo" => {
            let guard = inner.viewer.lock().await;
            if let Some(v) = guard.as_ref() {
                if let Some(tv) = v.as_any().downcast_ref::<TextViewer>() {
                    tv.undo()
                        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
                }
            }
        }
        "redo" => {
            let guard = inner.viewer.lock().await;
            if let Some(v) = guard.as_ref() {
                if let Some(tv) = v.as_any().downcast_ref::<TextViewer>() {
                    tv.redo()
                        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
                }
            }
        }
        "print" => {
            text_print_locked(&inner).await?;
            return Ok(());
        }
        _ => {}
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Text: re-decode with an explicit encoding label.
pub async fn text_set_encoding(instance_id: Uuid, label: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(tv) = v.as_any().downcast_ref::<TextViewer>() {
                tv.set_encoding(&label)
                    .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Text: find next (`forward`) / previous match.
pub async fn text_find(
    instance_id: Uuid,
    query: String,
    forward: bool,
    regex: bool,
    multiline: bool,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(tv) = v.as_any().downcast_ref::<TextViewer>() {
                tv.find(
                    &query,
                    forward,
                    orchid_viewers::FindOptions { regex, multiline },
                )
                .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Text: replace current match or all matches.
pub async fn text_replace(
    instance_id: Uuid,
    query: String,
    replacement: String,
    all: bool,
    regex: bool,
    multiline: bool,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(tv) = v.as_any().downcast_ref::<TextViewer>() {
                let opts = orchid_viewers::FindOptions { regex, multiline };
                if all {
                    tv.replace_all(&query, &replacement, opts)
                        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
                } else {
                    tv.replace_current(&query, &replacement, opts)
                        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
                }
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

async fn text_print_locked(inner: &ViewerWidgetInner) -> WidgetResult<()> {
    let text = {
        let snap = inner.snapshot.read();
        match snap.as_ref() {
            Some(ViewerSnapshot::Text(t)) => t.plain_text.to_string(),
            _ => String::new(),
        }
    };
    if text.is_empty() {
        return Err(WidgetError::InvalidStateForOperation(
            "nothing to print".into(),
        ));
    }
    let tmp = std::env::temp_dir().join(format!("orchid-print-{}.txt", inner.instance_id));
    std::fs::write(&tmp, text.as_bytes())
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    print_path(&tmp)
}

fn print_path(path: &std::path::Path) -> WidgetResult<()> {
    #[cfg(windows)]
    {
        let quoted = path.display().to_string().replace('\'', "''");
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-WindowStyle",
                "Hidden",
                "-Command",
                &format!("Start-Process -FilePath '{quoted}' -Verb Print"),
            ])
            .spawn()
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("lp")
            .arg(path)
            .spawn()
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    }
    Ok(())
}

/// Open the current viewer file in the system default app (player / browser).
pub fn open_current_externally(instance_id: Uuid) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let path = inner
        .path
        .read()
        .clone()
        .ok_or_else(|| WidgetError::InvalidStateForOperation("no file open".into()))?;
    let os = path
        .to_local()
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    opener::open(&os).map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))
}

/// Document: apply a toolbar / shortcut action (`save`, `undo`, `redo`, `bold`, …).
pub async fn document_action(instance_id: Uuid, action: String) -> WidgetResult<()> {
    use orchid_viewers::{Alignment, ListKind};

    let inner = live_inner(instance_id)?;
    match action.as_str() {
        "save" => {
            let mut guard = inner.viewer.lock().await;
            let v = guard
                .as_mut()
                .ok_or_else(|| WidgetError::InvalidStateForOperation("no viewer".into()))?;
            v.save()
                .await
                .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        }
        other => {
            let guard = inner.viewer.lock().await;
            let v = guard
                .as_ref()
                .ok_or_else(|| WidgetError::InvalidStateForOperation("no viewer".into()))?;
            let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
                return Ok(());
            };
            let result = match other {
                "undo" => doc.undo(),
                "redo" => doc.redo(),
                "bold" => doc.toggle_style_all('b'),
                "italic" => doc.toggle_style_all('i'),
                "underline" => doc.toggle_style_all('u'),
                "strikethrough" | "strike" => doc.toggle_style_all('s'),
                "highlight" => doc.toggle_style_all('h'),
                "superscript" => doc.toggle_style_all('^'),
                "subscript" => doc.toggle_style_all('_'),
                "clear-formatting" => doc.clear_formatting_selection(),
                "font-smaller" => doc.bump_font_size_selection(-1),
                "font-larger" => doc.bump_font_size_selection(1),
                "font-family-prev" => doc.bump_font_family_selection(-1),
                "font-family-next" => doc.bump_font_family_selection(1),
                "align-left" => doc.set_alignment_all(Alignment::Left),
                "align-center" => doc.set_alignment_all(Alignment::Center),
                "align-right" => doc.set_alignment_all(Alignment::Right),
                "align-justify" => doc.set_alignment_all(Alignment::Justify),
                "list-bullet" => doc.toggle_list_all(ListKind::Bullet),
                "list-numbered" => doc.toggle_list_all(ListKind::Numbered),
                "list-indent" => doc.bump_list_level_selection(1),
                "list-outdent" => doc.bump_list_level_selection(-1),
                "table-insert" => doc.preview_insert_table(2, 2),
                // "image-insert" is handled in the UI layer (clipboard bytes).
                "table-row-insert" => doc.preview_insert_table_row(),
                "table-row-delete" => doc.preview_delete_table_row(),
                "table-col-insert" => doc.preview_insert_table_column(),
                "table-col-delete" => doc.preview_delete_table_column(),
                "toggle-source" => {
                    doc.set_source_mode(!doc.source_mode());
                    Ok(())
                }
                color if color.starts_with("color-") => {
                    if let Some(rgb) = parse_toolbar_color(&color["color-".len()..]) {
                        doc.set_color_selection(rgb)
                    } else {
                        Ok(())
                    }
                }
                family if family.starts_with("font-family-") => {
                    let slug = &family["font-family-".len()..];
                    if let Some(name) = orchid_viewers::document::resolve_font_family_slug(slug) {
                        doc.set_font_family_selection(name)
                    } else {
                        Ok(())
                    }
                }
                _ => Ok(()),
            };
            result.map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: replace body from the Slint `TextInput` draft (no caret-breaking rebuild).
pub async fn document_push_edit(instance_id: Uuid, text: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() {
                doc.replace_plain_text(&text)
                    .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: find next/previous match in plain text (case-insensitive).
pub async fn document_find(instance_id: Uuid, query: String, forward: bool) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() {
                let _ = doc.preview_find(&query, forward);
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: replace current find match, or all matches when `all` is true.
pub async fn document_replace(
    instance_id: Uuid,
    query: String,
    replacement: String,
    all: bool,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() {
                if all {
                    doc.preview_replace_all(&query, &replacement)
                        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
                } else {
                    let _ = doc
                        .preview_replace_current(&query, &replacement)
                        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
                }
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: update selection from Source `TextInput` UTF-8 byte offsets.
pub async fn document_set_selection(instance_id: Uuid, anchor: i32, head: i32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() {
                doc.set_selection_plain_offsets(anchor.max(0) as usize, head.max(0) as usize);
            }
        }
    }
    // Selection changes do not need a full snapshot rebuild for caret-only moves;
    // toolbar accents update on the next format/edit refresh.
    Ok(())
}

/// Document: set preview layout width from the Slint viewport (CSS pixels).
pub async fn document_set_viewport_width(instance_id: Uuid, width_px: f32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() {
                // Leave room for page padding inside the preview image.
                let content = (width_px - 56.0).max(160.0);
                doc.set_preview_width(content);
            }
        }
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: apply or remove an external hyperlink on the selection.
///
/// Empty `url` removes the link under the caret / selection.
pub async fn document_link(instance_id: Uuid, url: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation("no viewer".into()));
        };
        let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
            return Ok(());
        };
        let result = if url.trim().is_empty() {
            doc.remove_hyperlink_selection()
        } else {
            doc.set_hyperlink_selection(&url)
        };
        result.map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: pointer on the preview canvas
/// (`phase`: 0=down, 1=move, 2=up, 3=double-click word select,
/// 4=triple-click paragraph, 5=hover; `ctrl` opens hyperlinks on down).
pub async fn document_preview_pointer(
    instance_id: Uuid,
    phase: i32,
    x: f32,
    y: f32,
    ctrl: bool,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    let mut outcome = orchid_viewers::document::PreviewPointerOutcome::default();
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() {
                outcome = doc.preview_pointer(phase.clamp(0, 5) as u8, x, y, ctrl);
            }
        }
    }
    if let Some(url) = outcome.open_url.as_deref() {
        if let Err(e) = opener::open(url) {
            warn!(error = %e, %url, "failed to open document hyperlink");
        }
    }
    // Refresh so caret / selection / link cursor tracks the pointer.
    if outcome.refresh {
        inner.refresh_snapshot().await;
    }
    Ok(())
}

/// Document: keyboard input while the preview canvas has focus.
///
/// Special keys are sent as tokens (`Backspace`, `Delete`, `Return`, `Left`,
/// `Right`, `Up`, `Down`); otherwise `key` is inserted as literal text.
///
/// Clipboard shortcuts (`c`/`x`/`v`) are handled in the UI layer (arboard).
pub async fn document_preview_key(
    instance_id: Uuid,
    key: String,
    ctrl: bool,
    shift: bool,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    if ctrl && matches!(key.as_str(), "s" | "S") {
        return document_action(instance_id, "save".into()).await;
    }
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
            return Ok(());
        };
        if doc.source_mode() {
            return Ok(());
        }
        let result = if ctrl {
            match key.as_str() {
                "a" | "A" => {
                    doc.preview_select_all();
                    Ok(())
                }
                "Home" => {
                    doc.preview_move_document_boundary(false, shift);
                    Ok(())
                }
                "End" => {
                    doc.preview_move_document_boundary(true, shift);
                    Ok(())
                }
                "Left" => {
                    doc.preview_move_by_words(-1, shift);
                    Ok(())
                }
                "Right" => {
                    doc.preview_move_by_words(1, shift);
                    Ok(())
                }
                "Backspace" => doc.preview_delete_word_backward(),
                "Delete" => doc.preview_delete_word_forward(),
                "b" | "B" => doc.toggle_style_all('b'),
                "i" | "I" => doc.toggle_style_all('i'),
                "u" | "U" => doc.toggle_style_all('u'),
                "x" | "X" if shift => doc.toggle_style_all('s'),
                "h" | "H" if shift => doc.toggle_style_all('h'),
                "=" | "+" if shift => doc.toggle_style_all('^'),
                "=" => doc.toggle_style_all('_'),
                " " => doc.clear_formatting_selection(),
                "z" | "Z" if shift => doc.redo(),
                "z" | "Z" => doc.undo(),
                "y" | "Y" => doc.redo(),
                _ => Ok(()),
            }
        } else {
            match key.as_str() {
                "Backspace" => doc.preview_delete_backward(),
                "Delete" => doc.preview_delete_forward(),
                "Return" if shift => doc.preview_insert_soft_break(),
                "Return" => doc.preview_insert_paragraph_break(),
                "Tab" if shift => {
                    if doc.selection().head.cell.is_some() {
                        let _ = doc.preview_move_table_cell(false);
                        Ok(())
                    } else {
                        doc.bump_list_level_selection(-1)
                    }
                }
                "Tab" => {
                    if doc.selection().head.cell.is_some() {
                        let _ = doc.preview_move_table_cell(true);
                        Ok(())
                    } else {
                        doc.bump_list_level_selection(1)
                    }
                }
                "Home" => {
                    doc.preview_move_line_boundary(false, shift);
                    Ok(())
                }
                "End" => {
                    doc.preview_move_line_boundary(true, shift);
                    Ok(())
                }
                "Left" => {
                    doc.preview_move_by_chars(-1, shift);
                    Ok(())
                }
                "Right" => {
                    doc.preview_move_by_chars(1, shift);
                    Ok(())
                }
                "Up" => {
                    doc.preview_move_vertical(-1, shift);
                    Ok(())
                }
                "Down" => {
                    doc.preview_move_vertical(1, shift);
                    Ok(())
                }
                other if is_printable_preview_text(other) => doc.preview_insert_text(other),
                _ => Ok(()),
            }
        };
        result.map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: plain text of the current preview selection (for clipboard copy).
pub async fn document_preview_selection_text(instance_id: Uuid) -> WidgetResult<String> {
    let inner = live_inner(instance_id)?;
    let guard = inner.viewer.lock().await;
    let Some(v) = guard.as_ref() else {
        return Ok(String::new());
    };
    let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
        return Ok(String::new());
    };
    Ok(doc.selected_plain_text())
}

/// Document: cut selection; returns the removed text for the clipboard.
pub async fn document_preview_cut(instance_id: Uuid) -> WidgetResult<String> {
    let inner = live_inner(instance_id)?;
    let text = {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(String::new());
        };
        let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
            return Ok(String::new());
        };
        doc.preview_cut_selection()
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
    };
    inner.refresh_snapshot().await;
    Ok(text)
}

/// Document: paste plain text at the preview caret.
pub async fn document_preview_paste(instance_id: Uuid, text: String) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
            return Ok(());
        };
        doc.preview_paste_plain(&text)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    }
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: insert a PNG/JPEG/… image block after the caret (from clipboard or toolbar).
pub async fn document_preview_insert_image(
    instance_id: Uuid,
    bytes: Vec<u8>,
    width_px: u32,
    height_px: u32,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() else {
            return Ok(());
        };
        doc.preview_insert_image(bytes, width_px, height_px)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    }
    inner.refresh_snapshot().await;
    Ok(())
}

fn is_printable_preview_text(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }
    !key.chars()
        .any(|c| c.is_control() || ('\u{f700}'..='\u{f7ff}').contains(&c) || c == '\u{7f}')
}

/// Parse a toolbar colour token (`RRGGBB` hex) into RGB bytes.
fn parse_toolbar_color(hex: &str) -> Option<[u8; 3]> {
    let hex = hex.trim();
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some([r, g, b])
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
            ViewerSnapshot::Document(s) => title_from(&s.path_display),
            ViewerSnapshot::Media(s) => title_from(&s.path_display),
            ViewerSnapshot::Html(s) => title_from(&s.path_display),
            ViewerSnapshot::Loading { path_display }
            | ViewerSnapshot::Error { path_display, .. } => title_from(path_display),
        };
        Some(WidgetSnapshot {
            instance_id: self.inner.instance_id,
            widget_type: TYPE_ID,
            title,
            status: WidgetStatus::Ready,
            payload: WidgetPayload::Viewer(ViewerPayload {
                snapshot: apply_image_nav(snap, &self.inner.image_nav.read()),
            }),
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
        let image_loop = self.inner.image_nav.read().loop_playlist;
        state_codec::save_state(&ViewerPersisted::from_live(path, floating, image_loop))
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
        self.inner.image_nav.write().loop_playlist = persisted.image_loop;
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
            persisted
                .path
                .as_deref()
                .and_then(|raw| orchid_fs::FsPath::new(raw).ok()),
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
        widget.inner.image_nav.write().loop_playlist = persisted.image_loop;
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
