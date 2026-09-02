//! Viewer widget: wraps an [`orchid_viewers::Viewer`] for any given path.

mod image_browse;
mod image_inspect;
mod image_nav;
mod image_slideshow;
mod image_thumbs;
mod media_nav;
#[cfg(windows)]
pub(crate) mod smtc_publisher;

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::LazyLock;

use async_trait::async_trait;
use dashmap::DashMap;
use orchid_storage::{LifecycleState, WidgetSize};
use orchid_viewers::ViewerSnapshot;
use orchid_viewers::{
    apply_adjust, apply_edit, apply_filter, apply_lossless, encode_png, export_file,
    format_from_extension, is_animation_extension, load_animation_file, parse_adjust_line,
    parse_annotate_line, parse_canvas_line, parse_export_line, parse_filter_line_in,
    parse_print_line, parse_resize_line, parse_screenshot_line, prepare_mail_attachment,
    set_wallpaper, share_intent_url, unique_export_dest, write_mail_eml, write_screenshot,
    AdjustOp, AnnotateOp, ArchiveViewer, CropKeep, DocumentViewer, EditOp, ExportFormat,
    ExportSpec, FilterOp, HistMode, ImageFitMode, ImageThumbItem, ImageViewer, LosslessOp,
    MediaViewer, PdfViewer, SlideTransition, SyntaxHighlighter, TextViewer, ThumbnailService,
    ThumbnailSize, ViewTransform, Viewer,
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
    /// 0 hidden, 1 bottom, 2 top.
    #[serde(default = "default_thumb_strip")]
    thumb_strip: u8,
    #[serde(default)]
    thumb_grid: bool,
    /// 0 small, 1 medium, 2 large.
    #[serde(default = "default_thumb_size")]
    thumb_size: u8,
    #[serde(default = "default_true")]
    thumb_meta: bool,
    #[serde(default = "default_preload_n")]
    preload_n: u8,
    #[serde(default)]
    browse_mode: u8,
    #[serde(default = "default_true")]
    overlay_autohide: bool,
    #[serde(default = "default_slide_interval")]
    slide_interval_ms: u32,
    #[serde(default)]
    slide_random: bool,
    #[serde(default = "default_slide_trans")]
    slide_transition: u8,
    #[serde(default = "default_slide_trans_ms")]
    slide_transition_ms: u32,
    #[serde(default = "default_true")]
    slide_overlay: bool,
    #[serde(default)]
    meta_overlay: bool,
    #[serde(default)]
    hist_mode: u8,
}

fn default_true() -> bool {
    true
}

fn default_thumb_strip() -> u8 {
    1
}

fn default_thumb_size() -> u8 {
    1
}

fn default_preload_n() -> u8 {
    2
}

fn default_slide_interval() -> u32 {
    4000
}

fn default_slide_trans() -> u8 {
    1
}

fn default_slide_trans_ms() -> u32 {
    500
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
            thumb_strip: 1,
            thumb_grid: false,
            thumb_size: 1,
            thumb_meta: true,
            preload_n: 2,
            browse_mode: 0,
            overlay_autohide: true,
            slide_interval_ms: 4000,
            slide_random: false,
            slide_transition: 1,
            slide_transition_ms: 500,
            slide_overlay: true,
            meta_overlay: false,
            hist_mode: 0,
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
        thumbs: &image_thumbs::ImageThumbState,
        slide: &image_slideshow::SlideshowState,
        inspect: &image_inspect::InspectState,
    ) -> Self {
        let (floating_on, float_x, float_y, float_w, float_h) = match floating {
            Some(b) => (true, Some(b.x), Some(b.y), Some(b.width), Some(b.height)),
            None => (false, None, None, None, None),
        };
        Self {
            path,
            floating: floating_on,
            float_x,
            float_y,
            float_w,
            float_h,
            image_loop,
            thumb_strip: thumbs.strip,
            thumb_grid: thumbs.grid,
            thumb_size: thumbs.size.as_u8(),
            thumb_meta: thumbs.show_meta,
            preload_n: thumbs.preload_n,
            browse_mode: thumbs.browse,
            overlay_autohide: thumbs.overlay_autohide,
            slide_interval_ms: slide.interval_ms,
            slide_random: slide.random,
            slide_transition: slide.transition.as_u8(),
            slide_transition_ms: slide.transition_ms,
            slide_overlay: slide.overlay,
            meta_overlay: inspect.overlay,
            hist_mode: inspect.hist_mode.as_u8(),
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
    /// Shared disk-backed thumbnail cache (same root as the file manager).
    pub thumbnails: Option<Arc<ThumbnailService>>,
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
    /// Media folder playlist (next/prev, loop).
    media_nav: RwLock<media_nav::MediaFolderNav>,
    /// Last zoom / fit per path (and the most recent view for new files).
    image_views: RwLock<ImageViewMemory>,
    /// Thumbnail strip / grid prefs and generated cells.
    image_thumbs: RwLock<image_thumbs::ImageThumbState>,
    /// Next-N decoded images for instant next/prev.
    image_preload: RwLock<image_thumbs::ImagePreloadCache>,
    /// Bumped to cancel in-flight thumb / preload jobs.
    thumb_gen: AtomicU64,
    slideshow: RwLock<image_slideshow::SlideshowState>,
    slide_tick: AtomicU64,
    anim_tick: AtomicU64,
    media_tick: AtomicU64,
    /// Side playlist panel visibility (Q toggles).
    playlist_panel_open: AtomicBool,
    /// Last media widget viewport (CSS px) for re-applying blit size on panel toggle.
    media_viewport: RwLock<(f32, f32)>,
    music_child: parking_lot::Mutex<Option<std::process::Child>>,
    inspect: RwLock<image_inspect::InspectState>,
    inspect_gen: AtomicU64,
    bus: Arc<orchid_core::EventBus>,
}

#[derive(Clone, Copy)]
struct SavedImageView {
    fit: ImageFitMode,
    transform: ViewTransform,
}

#[derive(Default)]
struct ImageViewMemory {
    by_path: HashMap<String, SavedImageView>,
    order: VecDeque<String>,
    last: Option<SavedImageView>,
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

    fn forget(&mut self, path: &str) {
        self.by_path.remove(path);
        self.order.retain(|p| p != path);
        self.last = None;
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
        self.anim_tick.fetch_add(1, Ordering::Relaxed);
        self.media_tick.fetch_add(1, Ordering::Relaxed);
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
        if is_image_path(&path) {
            let preloaded = self.image_preload.write().take(path.as_str());
            if let Some(loaded) = preloaded {
                if let Some(img) = viewer.as_any_mut().downcast_mut::<ImageViewer>() {
                    img.open_loaded(path.clone(), loaded);
                }
            } else if let Err(e) = viewer.open(path.clone(), registry).await {
                warn!(error = %e, "viewer open failed");
                *self.snapshot.write() = Some(ViewerSnapshot::Error {
                    path_display: path.as_str().to_string(),
                    message: e.to_string(),
                });
                self.publish_refresh();
                return Ok(());
            }
        } else if let Err(e) = viewer.open(path.clone(), registry).await {
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
            self.attach_animation_if_needed(&path).await;
            let guard = self.viewer.lock().await;
            if let Some(v) = guard.as_ref() {
                *self.snapshot.write() = Some(v.snapshot());
            }
            self.schedule_thumbs_and_preload();
        }
        if is_media_path(&path) {
            crate::builtin::audio_player::pause_all();
            crate::builtin::video_player::pause_all();
            self.after_media_opened(&path).await;
            self.schedule_media_ticks();
            #[cfg(windows)]
            smtc_publisher::set_active(self.instance_id);
        } else {
            #[cfg(windows)]
            smtc_publisher::clear_active(self.instance_id);
        }
        self.overlay_image_nav();
        self.publish_refresh();
        Ok(())
    }

    fn overlay_image_nav(&self) {
        let Some(snap) = self.snapshot.write().take() else {
            return;
        };
        *self.snapshot.write() = Some(apply_image_overlay(
            snap,
            &self.image_nav.read(),
            Some(&self.image_thumbs.read()),
            Some(&self.slideshow.read()),
            Some(&self.inspect.read()),
            Some(&self.media_nav.read()),
            self.playlist_panel_open.load(Ordering::Relaxed),
        ));
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
        if self.slideshow.read().overlay {
            self.slideshow.write().overlay_text = image_slideshow::overlay_for_path(path);
        }
        self.schedule_inspect(path);
    }

    async fn after_media_opened(&self, path: &orchid_fs::FsPath) {
        let parent = path.parent();
        let need_list = {
            let nav = self.media_nav.read();
            nav.folder.as_ref() != parent.as_ref() || !nav.siblings.iter().any(|p| p == path)
        };
        if need_list {
            if let Some(folder) = parent {
                if let Some(list) =
                    media_nav::list_media_siblings(&self.deps.registry, &folder).await
                {
                    self.media_nav.write().set_folder(folder, list, path);
                }
            }
        } else {
            self.media_nav.write().set_current(path);
        }
        self.media_nav.write().push_history(path);
        self.apply_media_playlist_overlay().await;
    }

    async fn apply_media_playlist_overlay(&self) {
        let (idx, count, shuffle, loop_playlist) = {
            let nav = self.media_nav.read();
            (
                nav.index as u32,
                nav.siblings.len() as u32,
                nav.shuffle,
                nav.loop_playlist,
            )
        };
        let guard = self.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(media) = v.as_any().downcast_ref::<MediaViewer>() {
                media.set_playlist_info(idx, count, shuffle, loop_playlist);
            }
        }
    }

    async fn navigate_media(&self, step: media_nav::MediaNavStep) -> WidgetResult<()> {
        {
            let path = self.path.read().clone();
            if let Some(path) = path.as_ref() {
                if is_media_path(path) {
                    let need = self.media_nav.read().siblings.is_empty();
                    if need {
                        self.after_media_opened(path).await;
                    }
                }
            }
        }
        let idx = self.media_nav.read().pick(step);
        let Some(idx) = idx else {
            return Ok(());
        };
        let Some(next) = self.media_nav.read().siblings.get(idx).cloned() else {
            return Ok(());
        };
        self.media_nav.write().index = idx;
        self.open_path(next).await
    }

    fn schedule_media_ticks(&self) {
        let gen = self.media_tick.fetch_add(1, Ordering::Relaxed) + 1;
        let inner = {
            let Some(entry) = VIEWER_LIVE.get(&self.instance_id) else {
                return;
            };
            Arc::clone(entry.value())
        };
        tokio::spawn(async move {
            loop {
                if inner.media_tick.load(Ordering::Relaxed) != gen {
                    return;
                }
                let tick = {
                    let guard = inner.viewer.lock().await;
                    if let Some(v) = guard.as_ref() {
                        if let Some(media) = v.as_any().downcast_ref::<MediaViewer>() {
                            Some((
                                media.take_dirty(),
                                media.take_eof(),
                                media.is_playing(),
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };
                let Some((dirty, eof, playing)) = tick else {
                    return;
                };
                if eof {
                    let _ = inner
                        .navigate_media(media_nav::MediaNavStep::Next)
                        .await;
                    continue;
                }
                if dirty {
                        // Re-apply playlist chrome then publish.
                        let (idx, count, shuffle, loop_playlist) = {
                            let nav = inner.media_nav.read();
                            (
                                nav.index as u32,
                                nav.siblings.len() as u32,
                                nav.shuffle,
                                nav.loop_playlist,
                            )
                        };
                        {
                            let guard = inner.viewer.lock().await;
                            if let Some(v) = guard.as_ref() {
                                if let Some(media) = v.as_any().downcast_ref::<MediaViewer>() {
                                    media.set_playlist_info(idx, count, shuffle, loop_playlist);
                                }
                                let snap = apply_image_overlay(
                                    v.snapshot(),
                                    &inner.image_nav.read(),
                                    Some(&inner.image_thumbs.read()),
                                    Some(&inner.slideshow.read()),
                                    Some(&inner.inspect.read()),
                                    Some(&inner.media_nav.read()),
                                    inner.playlist_panel_open.load(Ordering::Relaxed),
                                );
                                #[cfg(windows)]
                                if let ViewerSnapshot::Media(ref m) = snap {
                                    smtc_publisher::publish(inner.instance_id, m);
                                }
                                *inner.snapshot.write() = Some(snap);
                            }
                        }
                        inner.publish_refresh();
                }
                // ~30 Hz while playing (frames + progress); idle slower when paused.
                let wait_ms = if playing { 33 } else { 200 };
                tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                if inner.media_tick.load(Ordering::Relaxed) != gen {
                    return;
                }
            }
        });
    }

    fn schedule_inspect(&self, path: &orchid_fs::FsPath) {
        let gen = self.inspect_gen.fetch_add(1, Ordering::Relaxed) + 1;
        let path = path.clone();
        let inner = {
            let Some(entry) = VIEWER_LIVE.get(&self.instance_id) else {
                return;
            };
            Arc::clone(entry.value())
        };
        let snap = match self.snapshot.read().clone() {
            Some(ViewerSnapshot::Image(s)) => Some(s),
            _ => None,
        };
        tokio::task::spawn_blocking(move || {
            if inner.inspect_gen.load(Ordering::Relaxed) != gen {
                return;
            }
            let inspect = match image_inspect::inspect_local(&path) {
                Ok(v) => v,
                Err(e) => {
                    warn!(error = %e, "image inspect failed");
                    return;
                }
            };
            if inner.inspect_gen.load(Ordering::Relaxed) != gen {
                return;
            }
            let hist = snap.as_ref().map(image_inspect::histogram_from_snap);
            {
                let mut st = inner.inspect.write();
                st.apply_inspect(path.as_str(), inspect, snap.as_ref());
                if let Some(h) = hist {
                    st.set_histogram(h);
                }
            }
            inner.publish_refresh();
        });
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
        self.capture_slide_prev();
        let use_shuffle = {
            let sl = self.slideshow.read();
            sl.playing && sl.random && matches!(step, image_nav::NavStep::Next)
        };
        let n = self.image_nav.read().siblings.len().max(1);
        for _ in 0..n {
            let idx = if use_shuffle {
                let nav = self.image_nav.read().clone();
                self.slideshow.write().next_shuffled(&nav)
            } else {
                self.image_nav.read().pick(step)
            };
            let Some(idx) = idx else {
                if self.slideshow.read().playing && !self.image_nav.read().loop_playlist {
                    self.stop_slideshow();
                }
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
        self.stop_slideshow();
        #[cfg(windows)]
        smtc_publisher::clear_active(self.instance_id);
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
            *self.snapshot.write() = Some(apply_image_overlay(
                v.snapshot(),
                &self.image_nav.read(),
                Some(&self.image_thumbs.read()),
                Some(&self.slideshow.read()),
                Some(&self.inspect.read()),
                Some(&self.media_nav.read()),
                self.playlist_panel_open.load(Ordering::Relaxed),
            ));
        }
        drop(guard);
        self.publish_refresh();
    }

    fn schedule_thumbs_and_preload(&self) {
        let gen = self.thumb_gen.fetch_add(1, Ordering::Relaxed) + 1;
        let inner = {
            let Some(entry) = VIEWER_LIVE.get(&self.instance_id) else {
                return;
            };
            Arc::clone(entry.value())
        };
        tokio::spawn(async move {
            inner.refresh_thumbs(gen).await;
            inner.preload_ahead(gen).await;
        });
    }

    async fn refresh_thumbs(&self, gen: u64) {
        let Some(service) = self.deps.thumbnails.clone() else {
            return;
        };
        if self.thumb_gen.load(Ordering::Relaxed) != gen {
            return;
        }
        let (siblings, current, size) = {
            let nav = self.image_nav.read();
            let thumbs = self.image_thumbs.read();
            (
                nav.siblings.clone(),
                nav.siblings.get(nav.index).cloned(),
                thumbs.size,
            )
        };
        let current_key = current.as_ref().map(|p| p.as_str().to_string());
        let mut items = Vec::with_capacity(siblings.len().min(256));
        for (i, path) in siblings.iter().take(256).enumerate() {
            if self.thumb_gen.load(Ordering::Relaxed) != gen {
                return;
            }
            let meta = image_thumbs::sibling_meta(&self.deps.registry, path).await;
            let key = ThumbnailService::cache_key(path, meta.modified_ms);
            let thumb =
                image_thumbs::load_one_thumb(&service, &self.deps.registry, path, key, size).await;
            items.push(ImageThumbItem {
                path: path.as_str().to_string(),
                name: path.file_name().unwrap_or_default().to_string(),
                size_bytes: meta.size_bytes,
                date_text: meta.date_text,
                rating: meta.rating,
                rgba: thumb.as_ref().map(|t| Arc::clone(&t.rgba)),
                width: thumb.as_ref().map(|t| t.width).unwrap_or(0),
                height: thumb.as_ref().map(|t| t.height).unwrap_or(0),
                selected: current_key.as_deref() == Some(path.as_str()),
                index: (i + 1) as u32,
                taken_ms: meta.taken_ms,
                has_gps: meta.has_gps,
                gps_lat: meta.gps_lat,
                gps_lon: meta.gps_lon,
            });
            if items.len() % 8 == 0 {
                if self.thumb_gen.load(Ordering::Relaxed) != gen {
                    return;
                }
                self.image_thumbs.write().items = items.clone();
                self.overlay_image_nav();
                self.publish_refresh();
            }
        }
        if self.thumb_gen.load(Ordering::Relaxed) != gen {
            return;
        }
        self.image_thumbs.write().items = items;
        self.overlay_image_nav();
        self.publish_refresh();
    }

    async fn preload_ahead(&self, gen: u64) {
        if self.thumb_gen.load(Ordering::Relaxed) != gen {
            return;
        }
        let (n, paths) = {
            let thumbs = self.image_thumbs.read();
            let nav = self.image_nav.read();
            (
                thumbs.preload_n as usize,
                image_thumbs::preload_paths(&nav, thumbs.preload_n as usize),
            )
        };
        if n == 0 {
            return;
        }
        let cap = n.saturating_mul(2).saturating_add(2);
        for path in paths {
            if self.thumb_gen.load(Ordering::Relaxed) != gen {
                return;
            }
            if self.image_preload.read().contains(path.as_str()) {
                continue;
            }
            let registry = Arc::clone(&self.deps.registry);
            if let Some((key, img)) = image_thumbs::preload_one(registry, path).await {
                if self.thumb_gen.load(Ordering::Relaxed) != gen {
                    return;
                }
                self.image_preload.write().insert(key, img, cap);
            }
        }
    }

    async fn write_contact_sheet(&self) -> WidgetResult<()> {
        let Some(service) = self.deps.thumbnails.clone() else {
            return Err(WidgetError::InvalidStateForOperation(
                "thumbnail cache unavailable".into(),
            ));
        };
        let nav = self.image_nav.read().clone();
        let size = self.image_thumbs.read().size;
        let dest = image_thumbs::write_contact_sheet(&service, &self.deps.registry, &nav, size)
            .await
            .map_err(WidgetError::InvalidStateForOperation)?;
        self.open_path(dest).await
    }

    fn forget_thumb_memory(&self, path: &orchid_fs::FsPath) {
        self.image_preload.write().forget(path.as_str());
        self.image_thumbs
            .write()
            .items
            .retain(|t| t.path != path.as_str());
    }

    fn capture_slide_prev(&self) {
        if !self.slideshow.read().playing {
            return;
        }
        let Some(ViewerSnapshot::Image(s)) = self.snapshot.read().clone() else {
            return;
        };
        let mut sl = self.slideshow.write();
        sl.prev_rgba = Some(s.rgba_bytes);
        sl.prev_w = s.width_px;
        sl.prev_h = s.height_px;
        sl.gen = 0;
    }

    fn patch_slide_clock(&self) {
        let gen = self.slideshow.read().gen;
        if let Some(ViewerSnapshot::Image(s)) = self.snapshot.write().as_mut() {
            s.slideshow_gen = gen;
        }
        self.publish_refresh();
    }

    fn stop_slideshow(&self) {
        self.slide_tick.fetch_add(1, Ordering::Relaxed);
        {
            let mut sl = self.slideshow.write();
            sl.playing = false;
            sl.paused = false;
            sl.prev_rgba = None;
        }
        image_slideshow::stop_music(&mut self.music_child.lock());
    }

    fn schedule_slideshow_ticks(&self) {
        let gen = self.slide_tick.fetch_add(1, Ordering::Relaxed) + 1;
        let inner = {
            let Some(entry) = VIEWER_LIVE.get(&self.instance_id) else {
                return;
            };
            Arc::clone(entry.value())
        };
        tokio::spawn(async move {
            let mut elapsed = 0u32;
            loop {
                if inner.slide_tick.load(Ordering::Relaxed) != gen {
                    return;
                }
                let (playing, paused, interval, trans_ms) = {
                    let sl = inner.slideshow.read();
                    (sl.playing, sl.paused, sl.interval_ms, sl.transition_ms)
                };
                if !playing {
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                if inner.slide_tick.load(Ordering::Relaxed) != gen {
                    return;
                }
                if paused {
                    continue;
                }
                elapsed = elapsed.saturating_add(50);
                let bumped = {
                    let mut sl = inner.slideshow.write();
                    if sl.gen.saturating_mul(50) < trans_ms.max(1) {
                        sl.gen = sl.gen.saturating_add(1);
                        true
                    } else {
                        false
                    }
                };
                if bumped {
                    inner.patch_slide_clock();
                }
                if elapsed >= interval.max(400) {
                    elapsed = 0;
                    if let Err(e) = inner.navigate_images(image_nav::NavStep::Next).await {
                        warn!(error = %e, "slideshow advance failed");
                        return;
                    }
                }
            }
        });
    }

    async fn attach_animation_if_needed(&self, path: &orchid_fs::FsPath) {
        let has_anim = {
            let guard = self.viewer.lock().await;
            guard
                .as_ref()
                .and_then(|v| v.as_any().downcast_ref::<ImageViewer>())
                .is_some_and(|img| img.anim_count() >= 2)
        };
        if !has_anim {
            let ext = path.extension().unwrap_or_default();
            if is_animation_extension(ext) {
                let os = path.to_local().ok();
                let seq = if let Some(os) = os {
                    tokio::task::spawn_blocking(move || load_animation_file(&os))
                        .await
                        .ok()
                        .flatten()
                } else {
                    None
                };
                let guard = self.viewer.lock().await;
                if let Some(v) = guard.as_ref() {
                    if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
                        img.attach_anim(seq);
                    }
                }
            }
        }
        self.schedule_anim_ticks();
    }

    fn schedule_anim_ticks(&self) {
        let gen = self.anim_tick.fetch_add(1, Ordering::Relaxed) + 1;
        let inner = {
            let Some(entry) = VIEWER_LIVE.get(&self.instance_id) else {
                return;
            };
            Arc::clone(entry.value())
        };
        tokio::spawn(async move {
            loop {
                if inner.anim_tick.load(Ordering::Relaxed) != gen {
                    return;
                }
                let delay = {
                    if inner.slideshow.read().playing {
                        None
                    } else {
                        let guard = inner.viewer.lock().await;
                        guard.as_ref().and_then(|v| {
                            let img = v.as_any().downcast_ref::<ImageViewer>()?;
                            if img.anim_count() < 2 || !img.anim_playing() {
                                None
                            } else {
                                Some(img.anim_delay_ms())
                            }
                        })
                    }
                };
                match delay {
                    None => {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        continue;
                    }
                    Some(ms) => {
                        tokio::time::sleep(std::time::Duration::from_millis(u64::from(ms.max(20))))
                            .await;
                    }
                }
                if inner.anim_tick.load(Ordering::Relaxed) != gen {
                    return;
                }
                if inner.slideshow.read().playing {
                    continue;
                }
                let advanced = {
                    let guard = inner.viewer.lock().await;
                    if let Some(v) = guard.as_ref() {
                        if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
                            if img.anim_playing() {
                                img.anim_advance();
                                true
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                };
                if advanced {
                    inner.refresh_snapshot().await;
                }
            }
        });
    }

    async fn start_slideshow(&self) -> WidgetResult<()> {
        {
            let path = self.path.read().clone();
            if let Some(path) = path.as_ref() {
                if is_image_path(path) {
                    let need = self.image_nav.read().siblings.is_empty();
                    if need {
                        self.after_image_opened(path).await;
                    }
                }
            }
        }
        let music = {
            let existing = self.slideshow.read().music_path.clone();
            let current = self.path.read().clone();
            if existing.is_some() {
                existing
            } else if let Some(path) = current {
                image_slideshow::first_folder_audio(&self.deps.registry, &path)
                    .await
                    .map(|p| p.as_str().to_string())
            } else {
                None
            }
        };
        {
            let guard = self.viewer.lock().await;
            if let Some(v) = guard.as_ref() {
                if let Some(img) = v.as_any().downcast_ref::<ImageViewer>() {
                    img.set_anim_playing(false);
                }
            }
        }
        {
            let nav = self.image_nav.read().clone();
            let mut sl = self.slideshow.write();
            sl.playing = true;
            sl.paused = false;
            sl.music_path = music.clone();
            if sl.random {
                sl.rebuild_shuffle(&nav);
            }
            if sl.overlay {
                if let Some(path) = self.path.read().as_ref() {
                    sl.overlay_text = image_slideshow::overlay_for_path(path);
                }
            }
        }
        if let Some(m) = music {
            image_slideshow::start_music(&m, &mut self.music_child.lock());
        }
        self.schedule_slideshow_ticks();
        self.refresh_snapshot().await;
        Ok(())
    }

    async fn toggle_slideshow(&self) -> WidgetResult<()> {
        if self.slideshow.read().playing {
            self.stop_slideshow();
            self.refresh_snapshot().await;
            Ok(())
        } else {
            self.start_slideshow().await
        }
    }

    async fn export_slideshow(&self, kind: &str) -> WidgetResult<()> {
        let nav = self.image_nav.read().clone();
        let slide = self.slideshow.read().clone();
        let dest = match kind {
            "video" => {
                tokio::task::spawn_blocking(move || image_slideshow::write_video(&nav, &slide))
                    .await
                    .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
            }
            _ => tokio::task::spawn_blocking(move || image_slideshow::write_pack(&nav, &slide))
                .await
                .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?,
        }
        .map_err(WidgetError::InvalidStateForOperation)?;
        let _ = opener::open(&dest);
        Ok(())
    }

    async fn apply_lossless_path(
        &self,
        path: &orchid_fs::FsPath,
        op: LosslessOp,
    ) -> WidgetResult<()> {
        let provider = self.deps.registry.for_path(path).ok_or_else(|| {
            WidgetError::InvalidStateForOperation(format!("no provider for {}", path.as_str()))
        })?;
        let bytes = provider
            .read(path)
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        let fmt = format_from_extension(path.extension());
        let out = tokio::task::spawn_blocking(move || apply_lossless(&bytes, fmt, op))
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
            .map_err(map_viewer_err)?;
        provider
            .write(path, &out)
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        Ok(())
    }

    async fn reopen_after_lossless(&self, path: orchid_fs::FsPath) -> WidgetResult<()> {
        self.image_views.write().forget(path.as_str());
        self.forget_thumb_memory(&path);
        *self.path.write() = None;
        self.open_path(path).await
    }

    async fn apply_lossless_current(&self, op: LosslessOp) -> WidgetResult<()> {
        let Some(path) = self.path.read().clone() else {
            return Ok(());
        };
        self.apply_lossless_path(&path, op).await?;
        self.reopen_after_lossless(path).await
    }

    async fn apply_lossless_folder(&self, op: LosslessOp) -> WidgetResult<()> {
        let siblings = self.image_nav.read().siblings.clone();
        let current = self.path.read().clone();
        self.image_preload.write().clear();
        self.image_thumbs.write().items.clear();
        let mut ok = 0usize;
        let mut last_err: Option<WidgetError> = None;
        for path in &siblings {
            match self.apply_lossless_path(path, op).await {
                Ok(()) => ok += 1,
                Err(e) => last_err = Some(e),
            }
        }
        if let Some(path) = current {
            self.reopen_after_lossless(path).await?;
        }
        if ok == 0 {
            if let Some(e) = last_err {
                return Err(e);
            }
        }
        Ok(())
    }

    async fn apply_lossless_crop(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> WidgetResult<()> {
        let crop = {
            let guard = self.viewer.lock().await;
            let Some(v) = guard.as_ref() else {
                return Ok(());
            };
            let Some(img) = v.as_any().downcast_ref::<ImageViewer>() else {
                return Ok(());
            };
            img.viewport_rect_to_image_crop(x0, y0, x1, y1)
        };
        let Some((x, y, w, h)) = crop else {
            return Ok(());
        };
        self.apply_lossless_current(LosslessOp::Crop { x, y, w, h })
            .await
    }

    async fn apply_edit_current(&self, op: EditOp) -> WidgetResult<()> {
        let Some(src) = self.path.read().clone() else {
            return Ok(());
        };
        let img = {
            let guard = self.viewer.lock().await;
            let Some(v) = guard.as_ref() else {
                return Ok(());
            };
            let Some(viewer) = v.as_any().downcast_ref::<ImageViewer>() else {
                return Ok(());
            };
            viewer.clone_loaded()
        };
        let Some(img) = img else {
            return Ok(());
        };
        let os = src
            .to_local()
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        let suffix = match &op {
            EditOp::Crop { .. } => "crop",
            EditOp::Resize { .. } => "resize",
            EditOp::Canvas { .. } => "canvas",
            EditOp::Perspective { .. } => "perspective",
            EditOp::Straighten { .. } | EditOp::AutoStraighten => "straighten",
        };
        let dest = tokio::task::spawn_blocking(move || {
            let out = apply_edit(&img, &op)?;
            orchid_viewers::save_sibling(&os, &out, suffix)
        })
        .await
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
        .map_err(map_viewer_err)?;
        let next = orchid_fs::FsPath::from_local(&dest)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        self.open_path(next).await
    }

    async fn export_anim_frames(&self) -> WidgetResult<()> {
        let plan = {
            let guard = self.viewer.lock().await;
            let Some(v) = guard.as_ref() else {
                return Ok(());
            };
            let Some(img) = v.as_any().downcast_ref::<ImageViewer>() else {
                return Ok(());
            };
            img.anim_export_plan()
        };
        let Some((os, seq)) = plan else {
            return Ok(());
        };
        tokio::task::spawn_blocking(move || orchid_viewers::export_anim_frames(&os, &seq.frames))
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
            .map_err(map_viewer_err)?;
        self.refresh_snapshot().await;
        Ok(())
    }

    async fn extract_anim_frame(&self) -> WidgetResult<()> {
        let plan = {
            let guard = self.viewer.lock().await;
            let Some(v) = guard.as_ref() else {
                return Ok(());
            };
            let Some(img) = v.as_any().downcast_ref::<ImageViewer>() else {
                return Ok(());
            };
            img.anim_extract_plan()
        };
        let Some((os, frame, suffix)) = plan else {
            return Ok(());
        };
        tokio::task::spawn_blocking(move || {
            orchid_viewers::export_anim_frame(&os, &frame, &suffix)
        })
        .await
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
        .map_err(map_viewer_err)?;
        self.refresh_snapshot().await;
        Ok(())
    }

    async fn apply_adjust_current(&self, op: AdjustOp) -> WidgetResult<()> {
        let Some(src) = self.path.read().clone() else {
            return Ok(());
        };
        let img = {
            let guard = self.viewer.lock().await;
            let Some(v) = guard.as_ref() else {
                return Ok(());
            };
            let Some(viewer) = v.as_any().downcast_ref::<ImageViewer>() else {
                return Ok(());
            };
            viewer.clone_loaded()
        };
        let Some(img) = img else {
            return Ok(());
        };
        let os = src
            .to_local()
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        let suffix = op.suffix();
        let dest = tokio::task::spawn_blocking(move || {
            if os
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(orchid_viewers::is_raw_file_extension)
            {
                let _ = img;
                orchid_viewers::apply_adjust_file(&os, &op)
            } else {
                let out = apply_adjust(&img, &op)?;
                orchid_viewers::save_sibling(&os, &out, suffix)
            }
        })
        .await
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
        .map_err(map_viewer_err)?;
        let next = orchid_fs::FsPath::from_local(&dest)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        self.open_path(next).await
    }

    async fn apply_filter_current(&self, op: FilterOp) -> WidgetResult<()> {
        let Some(src) = self.path.read().clone() else {
            return Ok(());
        };
        let img = {
            let guard = self.viewer.lock().await;
            let Some(v) = guard.as_ref() else {
                return Ok(());
            };
            let Some(viewer) = v.as_any().downcast_ref::<ImageViewer>() else {
                return Ok(());
            };
            viewer.clone_loaded()
        };
        let Some(img) = img else {
            return Ok(());
        };
        let os = src
            .to_local()
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        let suffix = op.suffix();
        let dest = tokio::task::spawn_blocking(move || {
            let out = apply_filter(&img, &op)?;
            orchid_viewers::save_sibling(&os, &out, suffix)
        })
        .await
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
        .map_err(map_viewer_err)?;
        let next = orchid_fs::FsPath::from_local(&dest)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        self.open_path(next).await
    }

    async fn apply_annotate_current(&self, op: AnnotateOp) -> WidgetResult<()> {
        let Some(src) = self.path.read().clone() else {
            return Ok(());
        };
        let os = src
            .to_local()
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        let dest =
            tokio::task::spawn_blocking(move || orchid_viewers::apply_annotate_file(&os, &op))
                .await
                .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
                .map_err(map_viewer_err)?;
        let next = orchid_fs::FsPath::from_local(&dest)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        self.open_path(next).await
    }

    async fn print_job(&self, raw: &str, preview: bool, folder: bool) -> WidgetResult<()> {
        let spec = if raw.trim().is_empty() {
            orchid_viewers::PrintSpec::default()
        } else {
            parse_print_line(raw).ok_or_else(|| {
                WidgetError::InvalidStateForOperation("could not parse print spec".into())
            })?
        };
        let paths = if folder {
            self.image_nav.read().siblings.clone()
        } else {
            self.path.read().clone().into_iter().collect()
        };
        let os: Vec<std::path::PathBuf> = paths.iter().filter_map(|p| p.to_local().ok()).collect();
        if os.is_empty() {
            return Ok(());
        }
        let hint = os[0].clone();
        let dests = tokio::task::spawn_blocking(move || {
            let refs: Vec<&std::path::Path> = os.iter().map(std::path::PathBuf::as_path).collect();
            if preview {
                orchid_viewers::write_print_preview(&refs, &spec, &hint).map(|p| vec![p])
            } else {
                orchid_viewers::write_print_temps(&refs, &spec)
            }
        })
        .await
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
        .map_err(map_viewer_err)?;
        if preview {
            if let Some(dest) = dests.first() {
                let next = orchid_fs::FsPath::from_local(dest)
                    .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
                return self.open_path(next).await;
            }
            return Ok(());
        }
        for dest in dests {
            orchid_viewers::send_to_printer(&dest).map_err(map_viewer_err)?;
        }
        Ok(())
    }

    async fn export_current(&self, raw: &str) -> WidgetResult<()> {
        let spec = if raw.trim().is_empty() {
            ExportSpec::default()
        } else {
            parse_export_line(raw).ok_or_else(|| {
                WidgetError::InvalidStateForOperation("could not parse export spec".into())
            })?
        };
        let Some(src) = self.path.read().clone() else {
            return Ok(());
        };
        let os = src
            .to_local()
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        let dest = tokio::task::spawn_blocking(move || export_file(&os, &spec))
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
            .map_err(map_viewer_err)?;
        let next = orchid_fs::FsPath::from_local(&dest)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        self.open_path(next).await
    }

    async fn copy_image(&self) -> WidgetResult<()> {
        let img = {
            let guard = self.viewer.lock().await;
            guard.as_ref().and_then(|v| {
                v.as_any()
                    .downcast_ref::<ImageViewer>()
                    .and_then(|img| img.clone_loaded())
            })
        };
        let Some(img) = img else {
            return Ok(());
        };
        tokio::task::spawn_blocking(move || crate::builtin::file_manager::copy_loaded(&img))
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))??;
        Ok(())
    }

    async fn paste_image(&self) -> WidgetResult<()> {
        let hint = self
            .path
            .read()
            .as_ref()
            .and_then(|p| p.to_local().ok())
            .unwrap_or_else(|| {
                std::env::var_os("USERPROFILE")
                    .map(|h| {
                        std::path::PathBuf::from(h)
                            .join("Pictures")
                            .join("clipboard.png")
                    })
                    .unwrap_or_else(|| std::env::temp_dir().join("clipboard.png"))
            });
        let dest = tokio::task::spawn_blocking(move || {
            let img = crate::builtin::file_manager::paste_loaded()?;
            let dest = unique_export_dest(&hint, "paste", "png");
            let bytes = encode_png(&img)
                .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
            std::fs::write(&dest, bytes)
                .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
            Ok::<_, WidgetError>(dest)
        })
        .await
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))??;
        let next = orchid_fs::FsPath::from_local(&dest)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        self.open_path(next).await
    }

    async fn set_current_wallpaper(&self) -> WidgetResult<()> {
        let Some(src) = self.path.read().clone() else {
            return Ok(());
        };
        let os = src
            .to_local()
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        tokio::task::spawn_blocking(move || {
            let wall = {
                let ext = os
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if matches!(ext.as_str(), "jpg" | "jpeg" | "bmp") {
                    os
                } else {
                    export_file(
                        &os,
                        &ExportSpec {
                            format: ExportFormat::Jpeg,
                            quality: 92,
                            ..ExportSpec::default()
                        },
                    )
                    .map_err(map_viewer_err)?
                }
            };
            set_wallpaper(&wall).map_err(map_viewer_err)
        })
        .await
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))??;
        Ok(())
    }

    async fn email_current(&self, raw: &str) -> WidgetResult<()> {
        let max = raw
            .split('|')
            .find_map(|p| p.trim().strip_prefix("max=")?.trim().parse::<u32>().ok())
            .or_else(|| raw.trim().parse().ok())
            .unwrap_or(1920);
        let Some(src) = self.path.read().clone() else {
            return Ok(());
        };
        let os = src
            .to_local()
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        let eml = tokio::task::spawn_blocking(move || {
            let jpeg = prepare_mail_attachment(&os, max)?;
            write_mail_eml(&jpeg)
        })
        .await
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
        .map_err(map_viewer_err)?;
        let _ = opener::open(&eml);
        Ok(())
    }

    async fn share_current(&self, raw: &str) -> WidgetResult<()> {
        let network = raw
            .split('|')
            .next()
            .unwrap_or(raw)
            .trim()
            .to_ascii_lowercase();
        self.copy_image().await?;
        let label = self
            .path
            .read()
            .as_ref()
            .and_then(|p| p.file_name())
            .unwrap_or("image")
            .to_string();
        if let Some(url) = share_intent_url(&network, &label) {
            let _ = opener::open(url);
        } else if let Some(src) = self.path.read().clone() {
            if let Ok(os) = src.to_local() {
                let _ = opener::open(os);
            }
        }
        Ok(())
    }

    async fn screenshot_current(&self, raw: &str) -> WidgetResult<()> {
        let spec = parse_screenshot_line(raw).ok_or_else(|| {
            WidgetError::InvalidStateForOperation("could not parse screenshot spec".into())
        })?;
        let dir = self
            .path
            .read()
            .as_ref()
            .and_then(|p| p.to_local().ok())
            .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
            .or_else(|| {
                std::env::var_os("USERPROFILE")
                    .map(|h| std::path::PathBuf::from(h).join("Pictures"))
            })
            .unwrap_or_else(std::env::temp_dir);
        let dest = tokio::task::spawn_blocking(move || write_screenshot(&dir, &spec))
            .await
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?
            .map_err(map_viewer_err)?;
        let next = orchid_fs::FsPath::from_local(&dest)
            .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
        self.open_path(next).await
    }

    async fn annotate_from_view(&self, raw: &str) -> WidgetResult<()> {
        let Some((kind, rest)) = raw.split_once(':') else {
            return Ok(());
        };
        let (coords, tail) = rest.split_once(" | ").unwrap_or((rest, ""));
        let img_pts = {
            let guard = self.viewer.lock().await;
            let Some(v) = guard.as_ref() else {
                return Ok(());
            };
            let Some(img) = v.as_any().downcast_ref::<ImageViewer>() else {
                return Ok(());
            };
            let mut out = Vec::new();
            if kind == "pen" || kind == "poly" {
                for pair in coords.split([';', ' ']) {
                    let Some((x, y)) = pair.split_once(',') else {
                        continue;
                    };
                    if let (Ok(x), Ok(y)) = (x.parse::<f32>(), y.parse::<f32>()) {
                        if let Some(p) = img.viewport_to_image(x, y) {
                            out.push(p);
                        }
                    }
                }
            } else {
                let nums: Vec<f32> = coords.split(':').filter_map(|s| s.parse().ok()).collect();
                for pair in nums.chunks(2) {
                    if pair.len() == 2 {
                        if let Some(p) = img.viewport_to_image(pair[0], pair[1]) {
                            out.push(p);
                        }
                    }
                }
            }
            out
        };
        let packed = match kind {
            "line" | "arrow" if img_pts.len() >= 2 => format!(
                "{kind}={},{},{},{}{tail_part}",
                img_pts[0].0,
                img_pts[0].1,
                img_pts[1].0,
                img_pts[1].1,
                tail_part = if tail.is_empty() {
                    String::new()
                } else {
                    format!(" | {tail}")
                }
            ),
            "rect" | "ellipse" | "highlight" | "privacy" if img_pts.len() >= 2 => {
                let x = img_pts[0].0.min(img_pts[1].0);
                let y = img_pts[0].1.min(img_pts[1].1);
                let w = (img_pts[0].0 - img_pts[1].0).abs();
                let h = (img_pts[0].1 - img_pts[1].1).abs();
                let extra = if tail.is_empty() {
                    String::new()
                } else {
                    format!(" | {tail}")
                };
                format!("{kind}={x},{y},{w},{h}{extra}")
            }
            "text" | "callout" if !img_pts.is_empty() => {
                let extra = if tail.is_empty() {
                    String::new()
                } else {
                    format!(" | {tail}")
                };
                format!("x={} | y={}{extra}", img_pts[0].0, img_pts[0].1)
            }
            "pen" | "poly" if img_pts.len() >= 2 => {
                let pts = img_pts
                    .iter()
                    .map(|(x, y)| format!("{x},{y}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                let extra = if tail.is_empty() {
                    String::new()
                } else {
                    format!(" | {tail}")
                };
                format!("{kind}={pts}{extra}")
            }
            _ => return Ok(()),
        };
        if let Some(op) = parse_annotate_line(&packed) {
            self.apply_annotate_current(op).await?;
        }
        Ok(())
    }

    async fn edit_crop_from_view(
        &self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        aspect: f32,
        keep: u8,
    ) -> WidgetResult<()> {
        let crop = {
            let guard = self.viewer.lock().await;
            let Some(v) = guard.as_ref() else {
                return Ok(());
            };
            let Some(img) = v.as_any().downcast_ref::<ImageViewer>() else {
                return Ok(());
            };
            img.viewport_rect_to_image_crop(x0, y0, x1, y1)
        };
        let Some((x, y, w, h)) = crop else {
            return Ok(());
        };
        self.apply_edit_current(EditOp::Crop {
            x,
            y,
            w,
            h,
            aspect: (aspect > 0.05).then_some(aspect),
            keep: match keep {
                1 => CropKeep::Width,
                2 => CropKeep::Height,
                _ => CropKeep::None,
            },
        })
        .await
    }

    async fn edit_line_from_view(
        &self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        perspective: bool,
        extra: &[(f32, f32)],
    ) -> WidgetResult<()> {
        let pts = {
            let guard = self.viewer.lock().await;
            let Some(v) = guard.as_ref() else {
                return Ok(());
            };
            let Some(img) = v.as_any().downcast_ref::<ImageViewer>() else {
                return Ok(());
            };
            let mut out = Vec::new();
            for (x, y) in [(x0, y0), (x1, y1)]
                .into_iter()
                .chain(extra.iter().copied())
            {
                if let Some(p) = img.viewport_to_image(x, y) {
                    out.push(p);
                }
            }
            out
        };
        if perspective {
            if pts.len() < 4 {
                return Ok(());
            }
            return self
                .apply_edit_current(EditOp::Perspective {
                    quad: [pts[0], pts[1], pts[2], pts[3]],
                })
                .await;
        }
        if pts.len() < 2 {
            return Ok(());
        }
        self.apply_edit_current(EditOp::Straighten {
            x0: pts[0].0,
            y0: pts[0].1,
            x1: pts[1].0,
            y1: pts[1].1,
        })
        .await
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
                media_nav: RwLock::new(media_nav::MediaFolderNav::default()),
                image_views: RwLock::new(ImageViewMemory::default()),
                image_thumbs: RwLock::new(image_thumbs::ImageThumbState::default()),
                image_preload: RwLock::new(image_thumbs::ImagePreloadCache::default()),
                thumb_gen: AtomicU64::new(0),
                slideshow: RwLock::new(image_slideshow::SlideshowState::default()),
                slide_tick: AtomicU64::new(0),
                anim_tick: AtomicU64::new(0),
                media_tick: AtomicU64::new(0),
                playlist_panel_open: AtomicBool::new(
                    orchid_viewers::media_playlist_panel_default(),
                ),
                media_viewport: RwLock::new((0.0, 0.0)),
                music_child: parking_lot::Mutex::new(None),
                inspect: RwLock::new(image_inspect::InspectState::default()),
                inspect_gen: AtomicU64::new(0),
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
            } else if let Some(media) = v.as_any().downcast_ref::<MediaViewer>() {
                *inner.media_viewport.write() = (width, height);
                let panel = inner.playlist_panel_open.load(Ordering::Relaxed);
                media.set_viewport(width, height, panel);
                // No immediate refresh — next frame blit uses the new target size.
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

fn is_media_path(path: &orchid_fs::FsPath) -> bool {
    path.extension()
        .is_some_and(orchid_viewers::is_media_file_extension)
}

fn apply_image_overlay(
    snap: ViewerSnapshot,
    nav: &image_nav::ImageFolderNav,
    thumbs: Option<&image_thumbs::ImageThumbState>,
    slide: Option<&image_slideshow::SlideshowState>,
    inspect: Option<&image_inspect::InspectState>,
    media_nav: Option<&media_nav::MediaFolderNav>,
    playlist_panel_open: bool,
) -> ViewerSnapshot {
    match snap {
        ViewerSnapshot::Image(mut s) => {
            s.folder_index = nav.index.saturating_add(1) as u32;
            s.folder_count = nav.siblings.len() as u32;
            s.loop_folder = nav.loop_playlist;
            s.recent_paths = nav.recent_paths();
            if let Some(th) = thumbs {
                s.thumbs = th.items.clone();
                s.thumb_strip = th.strip;
                s.thumb_grid = th.grid;
                s.thumb_size = th.size.as_u8();
                s.thumb_show_meta = th.show_meta;
                s.browse_mode = th.browse;
                s.overlay_autohide = th.overlay_autohide;
                s.timeline = image_browse::timeline_items(&th.items);
                s.map_pins = image_browse::map_pins(&th.items);
                let (cy, cm) = if th.cal_year == 0 || th.cal_month == 0 {
                    let date = th
                        .items
                        .iter()
                        .find(|t| t.selected)
                        .map(|t| t.date_text.as_str())
                        .unwrap_or("");
                    image_browse::month_from_date(date)
                } else {
                    (th.cal_year, u32::from(th.cal_month))
                };
                let (title, days) = image_browse::calendar_days(&th.items, cy, cm);
                s.cal_title = title;
                s.cal_year = cy;
                s.cal_month = cm as u8;
                s.cal_days = days;
            }
            if let Some(sl) = slide {
                s.slideshow_playing = sl.playing;
                s.slideshow_paused = sl.paused;
                s.slideshow_interval_ms = sl.interval_ms;
                s.slideshow_random = sl.random;
                s.slideshow_transition = sl.transition.as_u8();
                s.slideshow_transition_ms = sl.transition_ms;
                s.slideshow_overlay = sl.overlay;
                s.slideshow_overlay_text = sl.overlay_text.clone();
                s.slideshow_music = sl.music_path.clone().unwrap_or_default();
                s.slideshow_gen = sl.gen;
                s.prev_rgba = sl.prev_rgba.clone();
                s.prev_width = sl.prev_w;
                s.prev_height = sl.prev_h;
            }
            if let Some(ins) = inspect {
                s.meta_panel = ins.panel;
                s.meta_overlay = ins.overlay;
                s.meta_text = ins.report.clone();
                s.meta_overlay_text = ins.overlay_text.clone();
                s.hist_rgba = ins.hist_rgba.clone();
                s.hist_width = if ins.hist_rgba.is_some() { 256 } else { 0 };
                s.hist_height = if ins.hist_rgba.is_some() { 72 } else { 0 };
                s.hist_mode = ins.hist_mode.as_u8();
                s.probe_text = ins.probe.clone();
                s.gps_label = ins
                    .inspect
                    .as_ref()
                    .and_then(|i| i.gps)
                    .map(|g| g.label())
                    .unwrap_or_default();
                s.has_gps = ins.inspect.as_ref().and_then(|i| i.gps).is_some();
                if let Some(i) = ins.inspect.as_ref() {
                    let e = orchid_viewers::inspect_to_edit(i);
                    s.meta_edit_title = e.title.unwrap_or_default();
                    s.meta_edit_creator = e.creator.unwrap_or_default();
                    s.meta_edit_copyright = e.copyright.unwrap_or_default();
                    s.meta_edit_keywords = e.keywords.unwrap_or_default();
                    s.meta_edit_description = e.description.unwrap_or_default();
                    s.meta_edit_date = e.date.flatten().unwrap_or_default();
                    s.meta_edit_gps = e
                        .gps
                        .flatten()
                        .map(|g| format!("{},{}", g.lat, g.lon))
                        .unwrap_or_default();
                }
            }
            ViewerSnapshot::Image(s)
        }
        ViewerSnapshot::Media(mut m) => {
            if let Some(nav) = media_nav {
                m.playlist_items = nav
                    .siblings
                    .iter()
                    .enumerate()
                    .map(|(i, p)| orchid_viewers::MediaPlaylistItem {
                        name: p
                            .file_name()
                            .unwrap_or_else(|| p.as_str())
                            .to_string(),
                        index: i as u32,
                        selected: i == nav.index,
                    })
                    .collect();
                m.playlist_panel_open = playlist_panel_open && !m.playlist_items.is_empty();
            }
            ViewerSnapshot::Media(m)
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

/// Pause every live media viewer that is currently playing.
pub async fn pause_all_media() {
    let ids: Vec<Uuid> = VIEWER_LIVE.iter().map(|e| *e.key()).collect();
    for id in ids {
        let Some(inner) = VIEWER_LIVE.get(&id).map(|e| Arc::clone(e.value())) else {
            continue;
        };
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            continue;
        };
        let Some(media) = v.as_any().downcast_ref::<MediaViewer>() else {
            continue;
        };
        if media.is_playing() {
            media.pause();
        }
    }
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

fn print_uses_folder(raw: &str) -> bool {
    raw.split(" | ").any(|p| {
        matches!(
            p.trim().to_ascii_lowercase().as_str(),
            "folder" | "sheet" | "index" | "contact"
        )
    })
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
            "rotate-180" => img.rotate_180(),
            "reset-transform" => img.reset_transforms(),
            "next"
            | "prev"
            | "first"
            | "last"
            | "random"
            | "loop"
            | "thumbs"
            | "thumb-grid"
            | "thumb-size"
            | "thumb-meta"
            | "thumb-refresh"
            | "contact-sheet"
            | "browse-timeline"
            | "browse-map"
            | "browse-calendar"
            | "browse-off"
            | "cal-prev"
            | "cal-next"
            | "overlay-autohide"
            | "slideshow"
            | "slideshow-stop"
            | "slideshow-pause"
            | "slideshow-faster"
            | "slideshow-slower"
            | "slideshow-random"
            | "slideshow-transition"
            | "slideshow-trans-ms"
            | "slideshow-overlay"
            | "slideshow-music"
            | "slideshow-export-html"
            | "slideshow-export-video"
            | "slideshow-export-exe"
            | "slideshow-export-scr"
            | "meta-panel"
            | "meta-overlay"
            | "hist-mode"
            | "gps-map"
            | "meta-strip"
            | "meta-strip-gps"
            | "meta-export-csv"
            | "meta-export-xml"
            | "edit-auto-straighten"
            | "copy-image"
            | "paste-image"
            | "wallpaper"
            | "screenshot"
            | "anim-export"
            | "anim-extract" => {}
            "anim-play" => img.set_anim_playing(true),
            "anim-pause" => img.set_anim_playing(false),
            "anim-toggle" => img.toggle_anim(),
            "anim-next" => img.anim_step(1),
            "anim-prev" => img.anim_step(-1),
            "anim-first" => img.anim_goto(1),
            "anim-last" => img.anim_goto(img.anim_count()),
            cmd if let Some(raw) = cmd.strip_prefix("anim-goto:") => {
                if let Ok(n) = raw.parse::<usize>() {
                    img.anim_goto(n);
                }
            }
            cmd if cmd.starts_with("goto:")
                || cmd.starts_with("recent:")
                || cmd.starts_with("lossless")
                || cmd.starts_with("preload:")
                || cmd.starts_with("open-thumb:")
                || cmd.starts_with("slideshow-interval:")
                || cmd.starts_with("slideshow-music:")
                || cmd.starts_with("probe:")
                || cmd.starts_with("meta-save:")
                || cmd.starts_with("edit-")
                || cmd.starts_with("adjust:")
                || cmd.starts_with("filter:")
                || cmd.starts_with("annotate")
                || cmd.starts_with("print")
                || cmd.starts_with("export")
                || cmd.starts_with("screenshot")
                || cmd.starts_with("email")
                || cmd.starts_with("share")
                || cmd.starts_with("save-as")
                || cmd.starts_with("anim-") => {}
            cmd if let Some(raw) = cmd.strip_prefix("rotate:") => {
                if let Ok(deg) = raw.parse::<f32>() {
                    img.set_rotation(deg);
                }
            }
            cmd if let Some(raw) = cmd.strip_prefix("rotate-by:") => {
                if let Ok(delta) = raw.parse::<f32>() {
                    img.rotate_by(delta);
                }
            }
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
    if matches!(command, "anim-play" | "anim-toggle") {
        inner.schedule_anim_ticks();
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
        cmd if let Some(raw) = cmd.strip_prefix("lossless-folder:") => {
            if let Some(op) = LosslessOp::from_token(raw) {
                inner.apply_lossless_folder(op).await?;
            }
        }
        "edit-auto-straighten" => inner.apply_edit_current(EditOp::AutoStraighten).await?,
        cmd if let Some(raw) = cmd.strip_prefix("adjust:") => {
            if let Some(op) = parse_adjust_line(raw) {
                inner.apply_adjust_current(op).await?;
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("filter:") => {
            let dir = inner.path.read().as_ref().and_then(|p| {
                p.to_local()
                    .ok()
                    .and_then(|os| os.parent().map(std::path::Path::to_path_buf))
            });
            if let Some(op) = parse_filter_line_in(raw, dir.as_deref()) {
                inner.apply_filter_current(op).await?;
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("annotate-view:") => {
            inner.annotate_from_view(raw).await?;
        }
        cmd if let Some(raw) = cmd.strip_prefix("annotate:") => {
            if let Some(op) = parse_annotate_line(raw) {
                inner.apply_annotate_current(op).await?;
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("print-preview:") => {
            let folder = print_uses_folder(raw);
            inner.print_job(raw, true, folder).await?;
        }
        cmd if let Some(raw) = cmd.strip_prefix("print-sheet:") => {
            let mut line = raw.to_string();
            if !line.contains("sheet") {
                line = format!("sheet | {line}");
            }
            inner.print_job(&line, false, true).await?;
        }
        cmd if let Some(raw) = cmd.strip_prefix("print:") => {
            inner.print_job(raw, false, print_uses_folder(raw)).await?;
        }
        cmd if let Some(raw) = cmd.strip_prefix("export:") => {
            inner.export_current(raw).await?;
        }
        cmd if let Some(raw) = cmd.strip_prefix("save-as:") => {
            inner.export_current(raw).await?;
        }
        "copy-image" => inner.copy_image().await?,
        "paste-image" => inner.paste_image().await?,
        "wallpaper" => inner.set_current_wallpaper().await?,
        cmd if let Some(raw) = cmd.strip_prefix("email:") => {
            inner.email_current(raw).await?;
        }
        cmd if let Some(raw) = cmd.strip_prefix("share:") => {
            inner.share_current(raw).await?;
        }
        cmd if let Some(raw) = cmd.strip_prefix("screenshot:") => {
            inner.screenshot_current(raw).await?;
        }
        "screenshot" => inner.screenshot_current("").await?,
        cmd if let Some(raw) = cmd.strip_prefix("edit-resize:") => {
            if let Some((spec, filter)) = parse_resize_line(raw) {
                inner
                    .apply_edit_current(EditOp::Resize { spec, filter })
                    .await?;
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("edit-canvas:") => {
            let size = {
                let guard = inner.viewer.lock().await;
                guard.as_ref().and_then(|v| {
                    v.as_any()
                        .downcast_ref::<ImageViewer>()
                        .and_then(|img| img.clone_loaded())
                        .map(|i| (i.width, i.height))
                })
            };
            if let Some((sw, sh)) = size {
                if let Some((w, h)) = parse_canvas_line(raw, sw, sh) {
                    inner
                        .apply_edit_current(EditOp::Canvas {
                            width: w,
                            height: h,
                            fill: [0, 0, 0, 255],
                        })
                        .await?;
                }
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("edit-crop:") => {
            let parts: Vec<&str> = raw.split(':').collect();
            if parts.len() >= 4 {
                if let (Ok(x0), Ok(y0), Ok(x1), Ok(y1)) = (
                    parts[0].parse::<f32>(),
                    parts[1].parse::<f32>(),
                    parts[2].parse::<f32>(),
                    parts[3].parse::<f32>(),
                ) {
                    let aspect = parts.get(4).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                    let keep = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0u8);
                    inner
                        .edit_crop_from_view(x0, y0, x1, y1, aspect, keep)
                        .await?;
                }
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("edit-straighten:") => {
            let parts: Vec<&str> = raw.split(':').collect();
            if parts.len() == 4 {
                if let (Ok(x0), Ok(y0), Ok(x1), Ok(y1)) = (
                    parts[0].parse::<f32>(),
                    parts[1].parse::<f32>(),
                    parts[2].parse::<f32>(),
                    parts[3].parse::<f32>(),
                ) {
                    inner
                        .edit_line_from_view(x0, y0, x1, y1, false, &[])
                        .await?;
                }
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("edit-perspective:") => {
            let parts: Vec<&str> = raw.split(':').collect();
            if parts.len() == 8 {
                let nums: Result<Vec<f32>, _> = parts.iter().map(|s| s.parse()).collect();
                if let Ok(n) = nums {
                    inner
                        .edit_line_from_view(
                            n[0],
                            n[1],
                            n[2],
                            n[3],
                            true,
                            &[(n[4], n[5]), (n[6], n[7])],
                        )
                        .await?;
                }
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("lossless-crop:") => {
            let parts: Vec<&str> = raw.split(':').collect();
            if parts.len() == 4 {
                if let (Ok(x0), Ok(y0), Ok(x1), Ok(y1)) = (
                    parts[0].parse::<f32>(),
                    parts[1].parse::<f32>(),
                    parts[2].parse::<f32>(),
                    parts[3].parse::<f32>(),
                ) {
                    inner.apply_lossless_crop(x0, y0, x1, y1).await?;
                }
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("lossless-") => {
            if let Some(op) = LosslessOp::from_token(raw) {
                inner.apply_lossless_current(op).await?;
            }
        }
        "thumbs" => {
            inner.image_thumbs.write().cycle_strip();
            inner.refresh_snapshot().await;
        }
        "thumb-grid" => {
            let next = !inner.image_thumbs.read().grid;
            inner.image_thumbs.write().grid = next;
            inner.refresh_snapshot().await;
        }
        "thumb-size" => {
            inner.image_thumbs.write().cycle_size();
            inner.schedule_thumbs_and_preload();
            inner.refresh_snapshot().await;
        }
        "thumb-meta" => {
            let next = !inner.image_thumbs.read().show_meta;
            inner.image_thumbs.write().show_meta = next;
            inner.refresh_snapshot().await;
        }
        "thumb-refresh" => {
            inner.image_thumbs.write().items.clear();
            inner.schedule_thumbs_and_preload();
            inner.refresh_snapshot().await;
        }
        "contact-sheet" => {
            inner.write_contact_sheet().await?;
        }
        "browse-timeline" => {
            inner
                .image_thumbs
                .write()
                .toggle_browse(image_browse::BROWSE_TIMELINE);
            inner.refresh_snapshot().await;
        }
        "browse-map" => {
            inner
                .image_thumbs
                .write()
                .toggle_browse(image_browse::BROWSE_MAP);
            inner.refresh_snapshot().await;
        }
        "browse-calendar" => {
            inner
                .image_thumbs
                .write()
                .toggle_browse(image_browse::BROWSE_CALENDAR);
            inner.refresh_snapshot().await;
        }
        "browse-off" => {
            inner.image_thumbs.write().browse = image_browse::BROWSE_PHOTO;
            inner.refresh_snapshot().await;
        }
        "cal-prev" => {
            {
                let mut th = inner.image_thumbs.write();
                let (y, m) =
                    image_browse::shift_month(th.cal_year, u32::from(th.cal_month.max(1)), -1);
                th.cal_year = y;
                th.cal_month = m as u8;
            }
            inner.refresh_snapshot().await;
        }
        "cal-next" => {
            {
                let mut th = inner.image_thumbs.write();
                let (y, m) =
                    image_browse::shift_month(th.cal_year, u32::from(th.cal_month.max(1)), 1);
                th.cal_year = y;
                th.cal_month = m as u8;
            }
            inner.refresh_snapshot().await;
        }
        "overlay-autohide" => {
            let next = !inner.image_thumbs.read().overlay_autohide;
            inner.image_thumbs.write().overlay_autohide = next;
            inner.refresh_snapshot().await;
        }
        cmd if let Some(raw) = cmd.strip_prefix("preload:") => {
            if let Ok(n) = raw.parse::<u8>() {
                inner.image_thumbs.write().preload_n = n.min(8);
                inner.schedule_thumbs_and_preload();
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("open-thumb:") => {
            if let Ok(path) = orchid_fs::FsPath::new(raw) {
                inner.image_thumbs.write().grid = false;
                inner.image_thumbs.write().browse = image_browse::BROWSE_PHOTO;
                inner.open_path(path).await?;
            }
        }
        "anim-export" => inner.export_anim_frames().await?,
        "anim-extract" => inner.extract_anim_frame().await?,
        "slideshow" => inner.toggle_slideshow().await?,
        "slideshow-stop" => {
            inner.stop_slideshow();
            inner.refresh_snapshot().await;
        }
        "slideshow-pause" => {
            if inner.slideshow.read().playing {
                let next = !inner.slideshow.read().paused;
                inner.slideshow.write().paused = next;
                inner.refresh_snapshot().await;
            }
        }
        "slideshow-faster" => {
            inner.slideshow.write().cycle_interval(true);
            inner.refresh_snapshot().await;
        }
        "slideshow-slower" => {
            inner.slideshow.write().cycle_interval(false);
            inner.refresh_snapshot().await;
        }
        "slideshow-random" => {
            let next = !inner.slideshow.read().random;
            {
                let nav = inner.image_nav.read().clone();
                let mut sl = inner.slideshow.write();
                sl.random = next;
                if next {
                    sl.rebuild_shuffle(&nav);
                }
            }
            inner.refresh_snapshot().await;
        }
        "slideshow-transition" => {
            let next = inner.slideshow.read().transition.cycle();
            inner.slideshow.write().transition = next;
            inner.refresh_snapshot().await;
        }
        "slideshow-trans-ms" => {
            inner.slideshow.write().cycle_transition_ms();
            inner.refresh_snapshot().await;
        }
        "slideshow-overlay" => {
            let next = !inner.slideshow.read().overlay;
            inner.slideshow.write().overlay = next;
            if next {
                if let Some(path) = inner.path.read().as_ref() {
                    inner.slideshow.write().overlay_text = image_slideshow::overlay_for_path(path);
                }
            }
            inner.refresh_snapshot().await;
        }
        "slideshow-music" => {
            let current = inner.slideshow.read().music_path.clone();
            let path = inner.path.read().clone();
            let next = if let Some(path) = path {
                image_slideshow::next_folder_audio(&inner.deps.registry, &path, current.as_deref())
                    .await
            } else {
                None
            };
            inner.slideshow.write().music_path = next.as_ref().map(|p| p.as_str().to_string());
            if inner.slideshow.read().playing {
                match &next {
                    Some(p) => {
                        image_slideshow::start_music(p.as_str(), &mut inner.music_child.lock())
                    }
                    None => image_slideshow::stop_music(&mut inner.music_child.lock()),
                }
            }
            inner.refresh_snapshot().await;
        }
        "slideshow-export-html" | "slideshow-export-exe" | "slideshow-export-scr" => {
            inner.export_slideshow("pack").await?;
        }
        "slideshow-export-video" => inner.export_slideshow("video").await?,
        cmd if let Some(raw) = cmd.strip_prefix("slideshow-interval:") => {
            if let Ok(sec) = raw.parse::<u32>() {
                inner.slideshow.write().interval_ms = (sec * 1000).clamp(1000, 30_000);
                inner.refresh_snapshot().await;
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("slideshow-music:") => {
            inner.slideshow.write().music_path = if raw.is_empty() {
                None
            } else {
                Some(raw.to_string())
            };
            if inner.slideshow.read().playing {
                if let Some(m) = inner.slideshow.read().music_path.clone() {
                    image_slideshow::start_music(&m, &mut inner.music_child.lock());
                }
            }
            inner.refresh_snapshot().await;
        }
        "meta-panel" => {
            let next = !inner.inspect.read().panel;
            inner.inspect.write().panel = next;
            inner.refresh_snapshot().await;
        }
        "meta-overlay" => {
            let next = !inner.inspect.read().overlay;
            inner.inspect.write().overlay = next;
            inner.refresh_snapshot().await;
        }
        "hist-mode" => {
            inner.inspect.write().cycle_hist_mode();
            inner.refresh_snapshot().await;
        }
        "gps-map" => {
            if let Some(url) = inner.inspect.read().gps_url() {
                let _ = opener::open(url);
            }
        }
        "meta-strip" => apply_viewer_meta(
            &inner,
            &orchid_viewers::EditableMeta {
                strip_all: true,
                ..orchid_viewers::EditableMeta::default()
            },
        )?,
        "meta-strip-gps" => apply_viewer_meta(
            &inner,
            &orchid_viewers::EditableMeta {
                strip_gps: true,
                ..orchid_viewers::EditableMeta::default()
            },
        )?,
        "meta-export-csv" => export_viewer_meta(&inner, false)?,
        "meta-export-xml" => export_viewer_meta(&inner, true)?,
        cmd if let Some(raw) = cmd.strip_prefix("meta-save:") => {
            apply_viewer_meta(&inner, &orchid_viewers::unpack_editable_meta(raw))?;
        }
        cmd if let Some(raw) = cmd.strip_prefix("probe:") => {
            let parts: Vec<&str> = raw.split(':').collect();
            if parts.len() == 2 {
                if let (Ok(x), Ok(y)) = (parts[0].parse::<i32>(), parts[1].parse::<i32>()) {
                    let changed = {
                        let Some(ViewerSnapshot::Image(s)) = inner.snapshot.read().clone() else {
                            return Ok(());
                        };
                        inner.inspect.write().probe_pixel(&s, x, y)
                    };
                    if changed {
                        if let Some(ViewerSnapshot::Image(s)) = inner.snapshot.write().as_mut() {
                            s.probe_text = inner.inspect.read().probe.clone();
                        }
                        inner.publish_refresh();
                    }
                }
            }
        }
        _ => inner.refresh_snapshot().await,
    }
    Ok(())
}

fn apply_viewer_meta(
    inner: &ViewerWidgetInner,
    edit: &orchid_viewers::EditableMeta,
) -> WidgetResult<()> {
    let Some(path) = inner.path.read().clone() else {
        return Ok(());
    };
    let os = path
        .to_local()
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    orchid_viewers::apply_editable_meta(&os, edit)
        .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    inner.schedule_inspect(&path);
    Ok(())
}

fn export_viewer_meta(inner: &ViewerWidgetInner, xml: bool) -> WidgetResult<()> {
    let Some(path) = inner.path.read().clone() else {
        return Ok(());
    };
    let os = path
        .to_local()
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    let body = if xml {
        orchid_viewers::export_metadata_xml(std::slice::from_ref(&os))
    } else {
        orchid_viewers::export_metadata_csv(std::slice::from_ref(&os))
    }
    .map_err(|e| WidgetError::InvalidStateForOperation(format!("{e}")))?;
    let dest = if xml {
        os.with_extension("metadata.xml")
    } else {
        os.with_extension("metadata.csv")
    };
    std::fs::write(&dest, body)
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    let _ = opener::open(&dest);
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

/// PDF: write the current page as a sibling PNG.
pub async fn pdf_extract_page(instance_id: Uuid) -> WidgetResult<String> {
    let inner = live_inner(instance_id)?;
    let dest = {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Err(WidgetError::InvalidStateForOperation("no viewer".into()));
        };
        let Some(pdf) = v.as_any().downcast_ref::<PdfViewer>() else {
            return Err(WidgetError::InvalidStateForOperation(
                "not a pdf viewer".into(),
            ));
        };
        pdf.extract_current_page().map_err(map_viewer_err)?
    };
    Ok(dest.to_string_lossy().into_owned())
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

async fn document_print_locked(inner: &ViewerWidgetInner) -> WidgetResult<()> {
    let text = {
        let snap = inner.snapshot.read();
        match snap.as_ref() {
            Some(ViewerSnapshot::Document(d)) => d.plain_text.to_string(),
            _ => String::new(),
        }
    };
    if text.is_empty() {
        return Err(WidgetError::InvalidStateForOperation(
            "nothing to print".into(),
        ));
    }
    let tmp = std::env::temp_dir().join(format!("orchid-doc-print-{}.txt", inner.instance_id));
    std::fs::write(&tmp, text.as_bytes())
        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
    print_path(&tmp)
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

/// Media viewer transport / playlist command.
pub async fn media_command(instance_id: Uuid, command: &str) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    match command {
        "next" => {
            inner
                .navigate_media(media_nav::MediaNavStep::Next)
                .await?;
            return Ok(());
        }
        "prev" => {
            inner
                .navigate_media(media_nav::MediaNavStep::Prev)
                .await?;
            return Ok(());
        }
        "first" => {
            inner
                .navigate_media(media_nav::MediaNavStep::First)
                .await?;
            return Ok(());
        }
        "last" => {
            inner
                .navigate_media(media_nav::MediaNavStep::Last)
                .await?;
            return Ok(());
        }
        "loop" => {
            let next = !inner.media_nav.read().loop_playlist;
            inner.media_nav.write().loop_playlist = next;
            inner.refresh_snapshot().await;
            return Ok(());
        }
        "shuffle" => {
            let next = !inner.media_nav.read().shuffle;
            inner.media_nav.write().shuffle = next;
            inner.refresh_snapshot().await;
            return Ok(());
        }
        "playlist-toggle" => {
            let next = !inner.playlist_panel_open.load(Ordering::Relaxed);
            inner
                .playlist_panel_open
                .store(next, Ordering::Relaxed);
            orchid_viewers::persist_media_playlist_panel(next);
            {
                let (w, h) = *inner.media_viewport.read();
                if w > 0.0 && h > 0.0 {
                    let guard = inner.viewer.lock().await;
                    if let Some(v) = guard.as_ref() {
                        if let Some(media) = v.as_any().downcast_ref::<MediaViewer>() {
                            media.set_viewport(w, h, next);
                        }
                    }
                }
            }
            inner.refresh_snapshot().await;
            return Ok(());
        }
        "random" => {
            inner
                .navigate_media(media_nav::MediaNavStep::Random)
                .await?;
            return Ok(());
        }
        cmd if let Some(raw) = cmd.strip_prefix("goto:") => {
            if let Ok(n) = raw.parse::<usize>() {
                inner
                    .navigate_media(media_nav::MediaNavStep::Goto(n))
                    .await?;
            }
            return Ok(());
        }
        "fullscreen" | "kiosk" | "exit-immersive" | "next-monitor" => {
            // Handled at the UI window layer; still accept no-op here.
            return Ok(());
        }
        _ => {}
    }
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        let Some(media) = v.as_any().downcast_ref::<MediaViewer>() else {
            return Ok(());
        };
        if command == "play-pause" && !media.is_playing() {
            crate::builtin::audio_player::pause_all();
            crate::builtin::video_player::pause_all();
        } else if command == "play" {
            crate::builtin::audio_player::pause_all();
            crate::builtin::video_player::pause_all();
        }
        media.apply_command(command);
    }
    inner.schedule_media_ticks();
    inner.refresh_snapshot().await;
    Ok(())
}

/// Seek media to a 0..1 progress fraction.
pub async fn media_seek_frac(instance_id: Uuid, frac: f32) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        let Some(v) = guard.as_ref() else {
            return Ok(());
        };
        let Some(media) = v.as_any().downcast_ref::<MediaViewer>() else {
            return Ok(());
        };
        media.seek_fraction(f64::from(frac));
    }
    inner.schedule_media_ticks();
    inner.refresh_snapshot().await;
    Ok(())
}

/// Document: apply a toolbar / shortcut action (`save`, `undo`, `redo`, `bold`, …).
pub async fn document_action(instance_id: Uuid, action: String) -> WidgetResult<()> {
    use orchid_viewers::{Alignment, ListKind};

    let inner = live_inner(instance_id)?;
    match action.as_str() {
        "print" => {
            document_print_locked(&inner).await?;
            return Ok(());
        }
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
                "all-caps" | "caps" => doc.toggle_style_all('a'),
                "small-caps" => doc.toggle_style_all('m'),
                "vanish" | "hidden" => doc.toggle_style_all('v'),
                "shadow" => doc.toggle_style_all('w'),
                "highlight" => doc.toggle_style_all('h'),
                "shade" => doc.toggle_paragraph_shade_selection(),
                "border-bottom" => doc.toggle_paragraph_border_bottom_selection(),
                "keep-next" => doc.toggle_keep_next_selection(),
                "keep-lines" => doc.toggle_keep_lines_selection(),
                "widow-control" => doc.toggle_widow_control_selection(),
                "contextual-spacing" => doc.toggle_contextual_spacing_selection(),
                "bidi" => doc.toggle_bidi_selection(),
                "suppress-auto-hyphens" => doc.toggle_suppress_auto_hyphens_selection(),
                "insert-bookmark" => doc.insert_bookmark_at_selection().map(|_| ()),
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
                "space-after-more" => doc.bump_paragraph_spacing_selection(0, 120),
                "space-after-less" => doc.bump_paragraph_spacing_selection(0, -120),
                "space-before-more" => doc.bump_paragraph_spacing_selection(120, 0),
                "space-before-less" => doc.bump_paragraph_spacing_selection(-120, 0),
                "line-spacing-more" => doc.bump_line_spacing_selection(1),
                "line-spacing-less" => doc.bump_line_spacing_selection(-1),
                "margin-more" => doc.bump_page_margins(180),
                "margin-less" => doc.bump_page_margins(-180),
                "indent-more" => doc.bump_indent_left_selection(720),
                "indent-less" => doc.bump_indent_left_selection(-720),
                "indent-right-more" => doc.bump_indent_right_selection(720),
                "indent-right-less" => doc.bump_indent_right_selection(-720),
                "first-line-more" => doc.bump_indent_first_line_selection(360),
                "first-line-less" => doc.bump_indent_first_line_selection(-360),
                "page-size-cycle" => doc.cycle_page_size(),
                "page-orientation-toggle" => doc.toggle_page_orientation(),
                "zoom-in" => doc.bump_preview_zoom(1),
                "zoom-out" => doc.bump_preview_zoom(-1),
                "zoom-reset" => doc.reset_preview_zoom(),
                "table-insert" => doc.preview_insert_table(2, 2),
                // "image-insert" is handled in the UI layer (clipboard bytes).
                "table-row-insert" => doc.preview_insert_table_row(),
                "table-row-delete" => doc.preview_delete_table_row(),
                "table-col-insert" => doc.preview_insert_table_column(),
                "table-col-delete" => doc.preview_delete_table_column(),
                "table-merge" => doc.preview_merge_table_cells(),
                "table-unmerge" => doc.preview_unmerge_table_cells(),
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

/// Document: find next/previous match in plain text.
pub async fn document_find(
    instance_id: Uuid,
    query: String,
    forward: bool,
    match_case: bool,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() {
                let _ = doc.preview_find(&query, forward, match_case);
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
    match_case: bool,
) -> WidgetResult<()> {
    let inner = live_inner(instance_id)?;
    {
        let guard = inner.viewer.lock().await;
        if let Some(v) = guard.as_ref() {
            if let Some(doc) = v.as_any().downcast_ref::<DocumentViewer>() {
                if all {
                    doc.preview_replace_all(&query, &replacement, match_case)
                        .map_err(|e| WidgetError::InvalidStateForOperation(e.to_string()))?;
                } else {
                    let _ = doc
                        .preview_replace_current(&query, &replacement, match_case)
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
                doc.set_preview_viewport_width(width_px.max(200.0));
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
    if ctrl && matches!(key.as_str(), "p" | "P") {
        return document_action(instance_id, "print".into()).await;
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
                "0" => doc.reset_preview_zoom(),
                "Return" => doc.preview_insert_page_break(),
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
                snapshot: apply_image_overlay(
                    snap,
                    &self.inner.image_nav.read(),
                    Some(&self.inner.image_thumbs.read()),
                    Some(&self.inner.slideshow.read()),
                    Some(&self.inner.inspect.read()),
                    Some(&self.inner.media_nav.read()),
                    self.inner.playlist_panel_open.load(Ordering::Relaxed),
                ),
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
        let thumbs = self.inner.image_thumbs.read().clone();
        let slide = self.inner.slideshow.read().clone();
        let inspect = self.inner.inspect.read().clone();
        state_codec::save_state(&ViewerPersisted::from_live(
            path, floating, image_loop, &thumbs, &slide, &inspect,
        ))
    }
    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let persisted: ViewerPersisted = state_codec::restore_state(bytes)?;
        let floating = persisted.floating_bounds();
        if let Some(ref raw) = persisted.path {
            match orchid_fs::FsPath::new(raw.as_str()) {
                Ok(p) => *self.inner.pending_path.write() = Some(p),
                Err(e) => warn!(error = %e, path = %raw, "viewer: invalid persisted path"),
            }
        }
        *self.inner.floating.write() = floating;
        self.inner.image_nav.write().loop_playlist = persisted.image_loop;
        apply_persisted_thumbs(&self.inner, &persisted);
        apply_persisted_slideshow(&self.inner, &persisted);
        apply_persisted_inspect(&self.inner, &persisted);
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

fn apply_persisted_thumbs(inner: &ViewerWidgetInner, persisted: &ViewerPersisted) {
    let mut thumbs = inner.image_thumbs.write();
    thumbs.strip = persisted.thumb_strip.min(2);
    thumbs.grid = persisted.thumb_grid;
    thumbs.size = ThumbnailSize::from_u8(persisted.thumb_size);
    thumbs.show_meta = persisted.thumb_meta;
    thumbs.preload_n = persisted.preload_n.min(8);
    thumbs.browse = persisted.browse_mode.min(3);
    thumbs.overlay_autohide = persisted.overlay_autohide;
}

fn apply_persisted_slideshow(inner: &ViewerWidgetInner, persisted: &ViewerPersisted) {
    let mut sl = inner.slideshow.write();
    sl.interval_ms = persisted.slide_interval_ms.clamp(1000, 30_000);
    sl.random = persisted.slide_random;
    sl.transition = SlideTransition::from_u8(persisted.slide_transition);
    sl.transition_ms = persisted.slide_transition_ms.clamp(80, 3000);
    sl.overlay = persisted.slide_overlay;
}

fn apply_persisted_inspect(inner: &ViewerWidgetInner, persisted: &ViewerPersisted) {
    let mut ins = inner.inspect.write();
    ins.overlay = persisted.meta_overlay;
    ins.hist_mode = HistMode::from_u8(persisted.hist_mode);
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
        apply_persisted_thumbs(&widget.inner, &persisted);
        apply_persisted_slideshow(&widget.inner, &persisted);
        apply_persisted_inspect(&widget.inner, &persisted);
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
