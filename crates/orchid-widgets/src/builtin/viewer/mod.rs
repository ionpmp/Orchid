//! Viewer widget: wraps an [`orchid_viewers::Viewer`] for any given path.

mod image_browse;
mod image_inspect;
mod image_nav;
mod image_slideshow;
mod image_thumbs;
mod media_nav;
#[cfg(windows)]
pub(crate) mod smtc_publisher;

mod archive_cmds;
mod document_cmds;
mod image_cmds;
mod inner;
mod media_cmds;
mod open_cmds;
mod pdf_cmds;
mod text_cmds;

pub use archive_cmds::*;
pub use document_cmds::*;
pub use image_cmds::*;
pub use media_cmds::*;
pub use open_cmds::*;
pub use pdf_cmds::*;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::LazyLock;
pub use text_cmds::*;

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
    /// Content-addressed store for linked `.orchid` document I/O.
    pub chunk_store: Option<Arc<orchid_crypto::ChunkStore>>,
}

impl std::fmt::Debug for ViewerDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewerDeps").finish_non_exhaustive()
    }
}

/// Shared viewer core keyed from UI callbacks.
pub(crate) struct ViewerWidgetInner {
    instance_id: Uuid,
    deps: ViewerDeps,
    viewer: Mutex<Option<Box<dyn Viewer>>>,
    snapshot: RwLock<Option<ViewerSnapshot>>,
    path: RwLock<Option<orchid_fs::FsPath>>,
    /// Temp file from `.orchid` Raw unwrap; removed on close / next open.
    unwrap_temp: parking_lot::Mutex<Option<std::path::PathBuf>>,
    /// Path restored from persistence; opened in `on_create`.
    pending_path: RwLock<Option<orchid_fs::FsPath>>,
    /// After the next successful open, switch a text viewer into edit mode.
    pending_edit: AtomicBool,
    /// User path waiting for an `.orchid` passphrase unlock.
    pending_orchid_unlock: parking_lot::Mutex<Option<orchid_fs::FsPath>>,
    /// Passphrase to apply on the next document open (consumed once).
    pending_decrypt_passphrase: parking_lot::Mutex<Option<String>>,
    /// Error string for the unlock dialog (`""` when idle / first prompt).
    orchid_passphrase_error: parking_lot::Mutex<String>,
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
    /// Bumped to cancel in-flight linked `.orchid` document autosave.
    doc_autosave_gen: AtomicU64,
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
                unwrap_temp: parking_lot::Mutex::new(None),
                pending_path: RwLock::new(None),
                pending_edit: AtomicBool::new(false),
                pending_orchid_unlock: parking_lot::Mutex::new(None),
                pending_decrypt_passphrase: parking_lot::Mutex::new(None),
                orchid_passphrase_error: parking_lot::Mutex::new(String::new()),
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
                doc_autosave_gen: AtomicU64::new(0),
                playlist_panel_open: AtomicBool::new(orchid_viewers::media_playlist_panel_default()),
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

    /// Retry opening a pending encrypted `.orchid` with `passphrase`.
    pub async fn commit_orchid_passphrase(&self, passphrase: &str) -> WidgetResult<()> {
        self.inner.commit_orchid_passphrase(passphrase).await
    }

    /// Dismiss the encrypted `.orchid` unlock dialog.
    pub fn cancel_orchid_passphrase(&self) {
        self.inner.cancel_orchid_passphrase();
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

pub(crate) fn map_viewer_err(e: orchid_viewers::ViewerError) -> WidgetError {
    WidgetError::InvalidStateForOperation(e.to_string())
}

/// Approximate monospace line height used by the Slint text viewer.
pub(crate) const TEXT_LINE_HEIGHT_PX: f32 = 18.0;
pub(crate) fn is_image_path(path: &orchid_fs::FsPath) -> bool {
    path.extension()
        .is_some_and(orchid_viewers::is_image_file_extension)
}

pub(crate) fn is_media_path(path: &orchid_fs::FsPath) -> bool {
    path.extension()
        .is_some_and(orchid_viewers::is_media_file_extension)
}

pub(crate) fn apply_image_overlay(
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
                        name: p.file_name().unwrap_or_else(|| p.as_str()).to_string(),
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

pub(crate) fn live_inner(instance_id: Uuid) -> WidgetResult<Arc<ViewerWidgetInner>> {
    VIEWER_LIVE
        .get(&instance_id)
        .map(|e| Arc::clone(e.value()))
        .ok_or_else(|| WidgetError::InvalidStateForOperation("viewer widget not live".into()))
}
pub(crate) async fn text_print_locked(inner: &ViewerWidgetInner) -> WidgetResult<()> {
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

pub(crate) fn print_path(path: &std::path::Path) -> WidgetResult<()> {
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

pub(crate) async fn document_print_locked(inner: &ViewerWidgetInner) -> WidgetResult<()> {
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
                passphrase_prompt: self.inner.pending_orchid_unlock.lock().is_some(),
                passphrase_error: self.inner.orchid_passphrase_error.lock().clone(),
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
