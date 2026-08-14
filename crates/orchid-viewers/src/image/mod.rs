//! Image viewer.

use std::any::Any;

pub mod color;
pub mod exif;
pub mod loader;
pub mod operations;
pub mod transform;

#[cfg(windows)]
mod heic_wic;

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;

use crate::error::{Result, ViewerError};
use crate::snapshot::{ImageSnapshot, ViewerSnapshot};
use crate::viewer_trait::Viewer;

pub use loader::{
    is_image_file_extension, load_image, rgba_arc, ImageFormat, LoadedImage, IMAGE_FILE_EXTENSIONS,
};
pub use transform::{ImageBackground, ImageFitMode, ViewTransform};

/// Max image size this viewer accepts. 128 MiB.
pub const DEFAULT_SIZE_LIMIT: u64 = 128 * 1024 * 1024;

/// Image viewer.
pub struct ImageViewer {
    path: RwLock<Option<orchid_fs::FsPath>>,
    image: RwLock<Option<LoadedImage>>,
    transform: RwLock<ViewTransform>,
    viewport: RwLock<(f32, f32)>,
    fit_mode: RwLock<ImageFitMode>,
    background: RwLock<ImageBackground>,
    custom_bg: RwLock<(u8, u8, u8)>,
    chrome_hidden: RwLock<bool>,
    kiosk: RwLock<bool>,
    lens: RwLock<bool>,
    size_limit: u64,
}

impl std::fmt::Debug for ImageViewer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageViewer")
            .field(
                "path",
                &self.path.read().as_ref().map(|p| p.as_str().to_string()),
            )
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
            fit_mode: RwLock::new(ImageFitMode::Window),
            background: RwLock::new(ImageBackground::Theme),
            custom_bg: RwLock::new((26, 26, 46)),
            chrome_hidden: RwLock::new(false),
            kiosk: RwLock::new(false),
            lens: RwLock::new(false),
            size_limit: DEFAULT_SIZE_LIMIT,
        }
    }

    /// Change the viewport size the viewer fits against.
    ///
    /// When fit mode is active, re-applies fit-to-viewport so the image
    /// tracks window / frame resizes (same idea as PDF fit modes).
    pub fn set_viewport(&self, width: f32, height: f32) {
        *self.viewport.write() = (width.max(1.0), height.max(1.0));
        if self.fit_mode.read().tracks_viewport() {
            self.apply_fit_transform();
        }
    }

    /// Change zoom, anchored at `(anchor_x, anchor_y)`.
    pub fn set_zoom(&self, factor: f32, anchor_x: f32, anchor_y: f32) {
        *self.fit_mode.write() = ImageFitMode::Custom;
        self.transform.write().set_zoom(factor, anchor_x, anchor_y);
    }

    /// Pan by `(dx, dy)` pixels.
    pub fn pan(&self, dx: f32, dy: f32) {
        *self.fit_mode.write() = ImageFitMode::Custom;
        self.transform.write().pan(dx, dy);
    }

    /// Rotate 90° clockwise.
    pub fn rotate_cw(&self) {
        self.transform.write().rotate_clockwise();
        if self.fit_mode.read().tracks_viewport() {
            self.apply_fit_transform();
        }
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
        self.set_fit_mode(ImageFitMode::Window);
    }

    /// Fit so the image width matches the viewport.
    pub fn fit_to_width(&self) {
        self.set_fit_mode(ImageFitMode::Width);
    }

    /// Fit so the image height matches the viewport.
    pub fn fit_to_height(&self) {
        self.set_fit_mode(ImageFitMode::Height);
    }

    /// Fit without enlarging past 1:1.
    pub fn fit_shrink(&self) {
        self.set_fit_mode(ImageFitMode::Shrink);
    }

    /// Set the active fit mode and recompute zoom.
    pub fn set_fit_mode(&self, mode: ImageFitMode) {
        *self.fit_mode.write() = mode;
        if mode.tracks_viewport() {
            self.apply_fit_transform();
        }
    }

    fn apply_fit_transform(&self) {
        let mode = *self.fit_mode.read();
        let image = self.image.read();
        let (vw, vh) = *self.viewport.read();
        let (iw, ih) = match image.as_ref() {
            Some(i) => (i.width, i.height),
            None => (1, 1),
        };
        let rot = self.transform.read().rotation_degrees;
        let flip_h = self.transform.read().flipped_horizontal;
        let flip_v = self.transform.read().flipped_vertical;
        let mut t = ViewTransform::fit_mode(mode, iw, ih, rot, vw, vh);
        t.flipped_horizontal = flip_h;
        t.flipped_vertical = flip_v;
        *self.transform.write() = t;
    }

    /// Reset transform to 1:1 (keeps rotation / flip).
    pub fn actual_size(&self) {
        *self.fit_mode.write() = ImageFitMode::Custom;
        let mut t = self.transform.write();
        t.zoom = 1.0;
        t.pan_x = 0.0;
        t.pan_y = 0.0;
    }

    /// Cycle the canvas background.
    pub fn cycle_background(&self) {
        let next = self.background.read().next();
        *self.background.write() = next;
    }

    /// Set a custom RGB canvas color and select that background.
    pub fn set_custom_background(&self, r: u8, g: u8, b: u8) {
        *self.custom_bg.write() = (r, g, b);
        *self.background.write() = ImageBackground::Custom;
    }

    /// Toggle viewer toolbar / status chrome.
    pub fn toggle_chrome_hidden(&self) {
        let next = !*self.chrome_hidden.read();
        *self.chrome_hidden.write() = next;
    }

    /// Toggle kiosk (borderless chrome + hidden widget header).
    pub fn toggle_kiosk(&self) {
        let next = !*self.kiosk.read();
        *self.kiosk.write() = next;
        *self.chrome_hidden.write() = next;
    }

    /// Leave fullscreen / kiosk chrome (Esc).
    pub fn exit_immersive(&self) {
        *self.kiosk.write() = false;
        *self.chrome_hidden.write() = false;
    }

    /// Nudge zoom by a factor around the viewport center.
    pub fn zoom_by(&self, factor: f32) {
        let (vw, vh) = *self.viewport.read();
        self.zoom_at(factor, vw / 2.0, vh / 2.0);
    }

    /// Nudge zoom by a factor, keeping `(anchor_x, anchor_y)` fixed on screen.
    pub fn zoom_at(&self, factor: f32, anchor_x: f32, anchor_y: f32) {
        *self.fit_mode.write() = ImageFitMode::Custom;
        let z = self.transform.read().zoom * factor;
        self.transform.write().set_zoom(z, anchor_x, anchor_y);
    }

    /// Set zoom to `percent` (100 = 1:1), anchored at the viewport center.
    pub fn zoom_to_percent(&self, percent: f32) {
        let (vw, vh) = *self.viewport.read();
        self.set_zoom(percent / 100.0, vw / 2.0, vh / 2.0);
    }

    /// Zoom so the viewport rectangle fills the view.
    pub fn zoom_to_rect(&self, x0: f32, y0: f32, x1: f32, y1: f32) {
        *self.fit_mode.write() = ImageFitMode::Custom;
        let (vw, vh) = *self.viewport.read();
        self.transform.write().zoom_to_rect(x0, y0, x1, y1, vw, vh);
    }

    /// Pan so image fraction `(nx, ny)` (0–1) sits at the viewport center.
    pub fn pan_to_image_fraction(&self, nx: f32, ny: f32) {
        *self.fit_mode.write() = ImageFitMode::Custom;
        let image = self.image.read();
        let (iw, ih) = match image.as_ref() {
            Some(i) => (i.width, i.height),
            None => return,
        };
        let rot = self.transform.read().rotation_degrees;
        self.transform
            .write()
            .pan_to_image_fraction(nx, ny, iw, ih, rot);
    }

    /// Toggle the magnifier overlay.
    pub fn toggle_lens(&self) {
        let next = !*self.lens.read();
        *self.lens.write() = next;
    }

    /// Snapshot fit + transform for restore-on-switch.
    #[must_use]
    pub fn capture_view(&self) -> (ImageFitMode, ViewTransform) {
        (*self.fit_mode.read(), *self.transform.read())
    }

    /// Restore a previously captured view (same image).
    pub fn restore_view(&self, fit: ImageFitMode, transform: ViewTransform) {
        *self.fit_mode.write() = fit;
        *self.transform.write() = transform;
        if fit.tracks_viewport() {
            self.apply_fit_transform();
        }
    }

    /// Apply only a zoom factor (new image; pan reset).
    pub fn restore_zoom_only(&self, zoom: f32) {
        *self.fit_mode.write() = ImageFitMode::Custom;
        let mut t = self.transform.write();
        t.zoom = zoom.clamp(0.05, 32.0);
        t.pan_x = 0.0;
        t.pan_y = 0.0;
    }

    /// Rotate 90° counter-clockwise.
    pub fn rotate_ccw(&self) {
        self.transform.write().rotate_counter_clockwise();
        if self.fit_mode.read().tracks_viewport() {
            self.apply_fit_transform();
        }
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
        let (bg_r, bg_g, bg_b) = *self.custom_bg.read();
        ViewerSnapshot::Image(ImageSnapshot {
            path_display,
            width_px: image.width,
            height_px: image.height,
            rgba_bytes: Arc::clone(&image.rgba),
            zoom: transform.zoom,
            pan_x: transform.pan_x,
            pan_y: transform.pan_y,
            rotation_degrees: transform.rotation_degrees,
            flipped_horizontal: transform.flipped_horizontal,
            flipped_vertical: transform.flipped_vertical,
            fit_mode: self.fit_mode.read().as_u8(),
            background: self.background.read().as_u8(),
            bg_r,
            bg_g,
            bg_b,
            chrome_hidden: *self.chrome_hidden.read(),
            kiosk: *self.kiosk.read(),
            color_source: image.color_source.clone(),
            color_dest: image.color_dest.clone(),
            orientation: image.orientation,
            format_label: image.format.label().to_string(),
            size_bytes: image.original_size_bytes,
            info_text: String::new(),
            folder_index: 0,
            folder_count: 0,
            loop_folder: true,
            recent_paths: Vec::new(),
            lens: *self.lens.read(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_to_percent_is_custom() {
        let v = ImageViewer::new();
        v.set_viewport(200.0, 200.0);
        v.zoom_to_percent(250.0);
        let (fit, t) = v.capture_view();
        assert_eq!(fit, ImageFitMode::Custom);
        assert!((t.zoom - 2.5).abs() < 1e-3);
    }

    #[test]
    fn restore_zoom_only_resets_pan() {
        let v = ImageViewer::new();
        v.pan(12.0, -4.0);
        v.restore_zoom_only(2.0);
        let (fit, t) = v.capture_view();
        assert_eq!(fit, ImageFitMode::Custom);
        assert!((t.zoom - 2.0).abs() < 1e-3);
        assert!(t.pan_x.abs() < 1e-3);
        assert!(t.pan_y.abs() < 1e-3);
    }

    #[test]
    fn restore_view_reapplies_fit_mode() {
        let v = ImageViewer::new();
        v.set_viewport(200.0, 100.0);
        v.set_fit_mode(ImageFitMode::Width);
        let saved = v.capture_view();
        v.actual_size();
        v.restore_view(saved.0, saved.1);
        assert_eq!(v.capture_view().0, ImageFitMode::Width);
    }
}
