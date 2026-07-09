//! Image viewer.

use std::any::Any;

pub mod loader;
pub mod operations;
pub mod transform;

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;

use crate::error::{Result, ViewerError};
use crate::snapshot::{ImageSnapshot, ViewerSnapshot};
use crate::viewer_trait::Viewer;

pub use loader::{load_image, rgba_arc, ImageFormat, LoadedImage};
pub use transform::ViewTransform;

/// Max image size this viewer accepts. 128 MiB.
pub const DEFAULT_SIZE_LIMIT: u64 = 128 * 1024 * 1024;

/// Image viewer.
pub struct ImageViewer {
    path: RwLock<Option<orchid_fs::FsPath>>,
    image: RwLock<Option<LoadedImage>>,
    transform: RwLock<ViewTransform>,
    viewport: RwLock<(f32, f32)>,
    /// When true, viewport changes re-apply fit-to-viewport (like PDF fit modes).
    fit_mode: RwLock<bool>,
    size_limit: u64,
}

impl std::fmt::Debug for ImageViewer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageViewer")
            .field("path", &self.path.read().as_ref().map(|p| p.as_str().to_string()))
            .finish_non_exhaustive()
    }
}

impl Default for ImageViewer {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageViewer {
    /// Build an empty viewer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            path: RwLock::new(None),
            image: RwLock::new(None),
            transform: RwLock::new(ViewTransform::default()),
            viewport: RwLock::new((800.0, 600.0)),
            fit_mode: RwLock::new(true),
            size_limit: DEFAULT_SIZE_LIMIT,
        }
    }

    /// Change the viewport size the viewer fits against.
    ///
    /// When fit mode is active, re-applies fit-to-viewport so the image
    /// tracks window / frame resizes (same idea as PDF fit modes).
    pub fn set_viewport(&self, width: f32, height: f32) {
        *self.viewport.write() = (width.max(1.0), height.max(1.0));
        let should_fit = *self.fit_mode.read();
        if should_fit {
            self.apply_fit_transform();
        }
    }

    /// Change zoom, anchored at `(anchor_x, anchor_y)`.
    pub fn set_zoom(&self, factor: f32, anchor_x: f32, anchor_y: f32) {
        *self.fit_mode.write() = false;
        self.transform.write().set_zoom(factor, anchor_x, anchor_y);
    }

    /// Pan by `(dx, dy)` pixels.
    pub fn pan(&self, dx: f32, dy: f32) {
        *self.fit_mode.write() = false;
        self.transform.write().pan(dx, dy);
    }

    /// Rotate 90° clockwise.
    pub fn rotate_cw(&self) {
        self.transform.write().rotate_clockwise();
    }

    /// Toggle horizontal flip.
    pub fn flip_horizontal(&self) {
        let mut t = self.transform.write();
        t.flipped_horizontal = !t.flipped_horizontal;
    }

    /// Toggle vertical flip.
    pub fn flip_vertical(&self) {
        let mut t = self.transform.write();
        t.flipped_vertical = !t.flipped_vertical;
    }

    /// Reset transform to best fit.
    pub fn fit_to_viewport(&self) {
        *self.fit_mode.write() = true;
        self.apply_fit_transform();
    }

    fn apply_fit_transform(&self) {
        let image = self.image.read();
        let (vw, vh) = *self.viewport.read();
        let (iw, ih) = match image.as_ref() {
            Some(i) => (i.width, i.height),
            None => (1, 1),
        };
        *self.transform.write() = ViewTransform::fit_to_viewport(iw, ih, vw, vh);
    }

    /// Reset transform to 1:1.
    pub fn actual_size(&self) {
        *self.fit_mode.write() = false;
        self.transform.write().reset();
    }

    /// Nudge zoom by a factor around the viewport center.
    pub fn zoom_by(&self, factor: f32) {
        *self.fit_mode.write() = false;
        let (vw, vh) = *self.viewport.read();
        let anchor_x = vw / 2.0;
        let anchor_y = vh / 2.0;
        let z = self.transform.read().zoom * factor;
        self.transform.write().set_zoom(z, anchor_x, anchor_y);
    }

    /// Rotate 90° counter-clockwise.
    pub fn rotate_ccw(&self) {
        self.transform.write().rotate_counter_clockwise();
    }
}

#[async_trait]
impl Viewer for ImageViewer {
    fn type_id(&self) -> &'static str {
        "image"
    }

    async fn open(
        &mut self,
        path: orchid_fs::FsPath,
        registry: Arc<orchid_fs::FsProviderRegistry>,
    ) -> Result<()> {
        let loaded = load_image(&path, registry, self.size_limit).await?;
        *self.image.write() = Some(loaded);
        *self.path.write() = Some(path);
        self.fit_to_viewport();
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        *self.image.write() = None;
        *self.path.write() = None;
        *self.transform.write() = ViewTransform::default();
        Ok(())
    }

    fn snapshot(&self) -> ViewerSnapshot {
        let path_guard = self.path.read();
        let path_display = path_guard
            .as_ref()
            .map(|p| p.as_str().to_string())
            .unwrap_or_default();
        let image = self.image.read();
        let Some(image) = image.as_ref() else {
            return ViewerSnapshot::Loading { path_display };
        };
        let transform = *self.transform.read();
        ViewerSnapshot::Image(ImageSnapshot {
            path_display,
            width_px: image.width,
            height_px: image.height,
            rgba_bytes: Arc::new(image.rgba.clone()),
            zoom: transform.zoom,
            pan_x: transform.pan_x,
            pan_y: transform.pan_y,
            rotation_degrees: transform.rotation_degrees,
            flipped_horizontal: transform.flipped_horizontal,
            flipped_vertical: transform.flipped_vertical,
            fit_mode: *self.fit_mode.read(),
            format_label: image.format.label().to_string(),
            size_bytes: image.original_size_bytes,
            info_text: String::new(),
        })
    }

    fn current_path(&self) -> Option<&orchid_fs::FsPath> {
        // Returning a reference into a RwLock is awkward; the widget
        // instead goes through the snapshot's `path_display`. We expose
        // `None` here to avoid unsound pointer tricks — the trait contract
        // allows returning `None`.
        let _ = &ViewerError::ImageDecode(String::new());
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
