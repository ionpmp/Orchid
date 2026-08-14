//! Zoom / pan / rotate view transform for the image viewer.

/// How the image is fitted into the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum ImageFitMode {
    /// Manual zoom / 1:1 / pan — do not re-fit on resize.
    Custom = 0,
    /// Entire image visible (letterbox). Default on open.
    #[default]
    Window = 1,
    /// Scale so the image width matches the viewport.
    Width = 2,
    /// Scale so the image height matches the viewport.
    Height = 3,
    /// Like [`Self::Window`] but never enlarge past 1:1.
    Shrink = 4,
}

impl ImageFitMode {
    /// Persist / snapshot discriminant.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Inverse of [`Self::as_u8`].
    #[must_use]
    pub const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Window,
            2 => Self::Width,
            3 => Self::Height,
            4 => Self::Shrink,
            _ => Self::Custom,
        }
    }

    /// `true` when a resize should recompute zoom.
    #[must_use]
    pub const fn tracks_viewport(self) -> bool {
        !matches!(self, Self::Custom)
    }
}

/// Canvas behind (and showing through) the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum ImageBackground {
    /// Current theme surface.
    #[default]
    Theme = 0,
    /// Solid black.
    Black = 1,
    /// Solid white.
    White = 2,
    /// Mid gray (`#808080`).
    Gray = 3,
    /// Checkerboard for inspecting alpha.
    Checkerboard = 4,
    /// User RGB stored on the viewer.
    Custom = 5,
}

impl ImageBackground {
    /// Persist / snapshot discriminant.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Inverse of [`Self::as_u8`].
    #[must_use]
    pub const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Black,
            2 => Self::White,
            3 => Self::Gray,
            4 => Self::Checkerboard,
            5 => Self::Custom,
            _ => Self::Theme,
        }
    }

    /// Cycle Theme → Black → White → Gray → Checkerboard → Custom → Theme.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Theme => Self::Black,
            Self::Black => Self::White,
            Self::White => Self::Gray,
            Self::Gray => Self::Checkerboard,
            Self::Checkerboard => Self::Custom,
            Self::Custom => Self::Theme,
        }
    }
}

/// Display transform applied on top of the loaded image.
#[derive(Debug, Clone, Copy)]
#[allow(missing_docs)]
pub struct ViewTransform {
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub rotation_degrees: f32,
    pub flipped_horizontal: bool,
    pub flipped_vertical: bool,
}

impl Default for ViewTransform {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            rotation_degrees: 0.0,
            flipped_horizontal: false,
            flipped_vertical: false,
        }
    }
}

impl ViewTransform {
    /// Axis-aligned bounding size after `rotation_degrees` (for fit math).
    #[must_use]
    pub fn oriented_size(image_w: u32, image_h: u32, rotation_degrees: f32) -> (u32, u32) {
        let deg = rotation_degrees.rem_euclid(360.0);
        if (deg - 90.0).abs() < 0.5 || (deg - 270.0).abs() < 0.5 {
            return (image_h, image_w);
        }
        if deg < 0.5 || (deg - 180.0).abs() < 0.5 {
            return (image_w, image_h);
        }
        let r = deg.to_radians();
        let (c, s) = (r.cos().abs(), r.sin().abs());
        let w = (image_w as f32 * c + image_h as f32 * s).round().max(1.0) as u32;
        let h = (image_w as f32 * s + image_h as f32 * c).round().max(1.0) as u32;
        (w, h)
    }

    /// Zoom that fits `image_w x image_h` into the viewport for `mode`.
    #[must_use]
    pub fn zoom_for_fit(
        mode: ImageFitMode,
        image_w: u32,
        image_h: u32,
        rotation_degrees: f32,
        viewport_w: f32,
        viewport_h: f32,
    ) -> f32 {
        let (iw, ih) = Self::oriented_size(image_w, image_h, rotation_degrees);
        if iw == 0 || ih == 0 {
            return 1.0;
        }
        let zx = viewport_w / iw as f32;
        let zy = viewport_h / ih as f32;
        let zoom = match mode {
            ImageFitMode::Custom => 1.0,
            ImageFitMode::Window | ImageFitMode::Shrink => zx.min(zy),
            ImageFitMode::Width => zx,
            ImageFitMode::Height => zy,
        };
        let zoom = if mode == ImageFitMode::Shrink {
            zoom.min(1.0)
        } else {
            zoom
        };
        zoom.clamp(0.05, 32.0)
    }

    /// Build a transform that fits `image_w x image_h` into
    /// `viewport_w x viewport_h`, preserving aspect ratio.
    #[must_use]
    pub fn fit_to_viewport(image_w: u32, image_h: u32, viewport_w: f32, viewport_h: f32) -> Self {
        Self::fit_mode(
            ImageFitMode::Window,
            image_w,
            image_h,
            0.0,
            viewport_w,
            viewport_h,
        )
    }

    /// Fit using `mode`, keeping rotation / flip from `base` when provided.
    #[must_use]
    pub fn fit_mode(
        mode: ImageFitMode,
        image_w: u32,
        image_h: u32,
        rotation_degrees: f32,
        viewport_w: f32,
        viewport_h: f32,
    ) -> Self {
        Self {
            zoom: Self::zoom_for_fit(
                mode,
                image_w,
                image_h,
                rotation_degrees,
                viewport_w,
                viewport_h,
            ),
            pan_x: 0.0,
            pan_y: 0.0,
            rotation_degrees,
            flipped_horizontal: false,
            flipped_vertical: false,
        }
    }

    /// Set zoom by `factor`. `anchor_*` is in image pixel coords and stays
    /// fixed under the new zoom (screen-space feel).
    pub fn set_zoom(&mut self, factor: f32, anchor_x: f32, anchor_y: f32) {
        let new_zoom = (factor).clamp(0.05, 32.0);
        if new_zoom == self.zoom {
            return;
        }
        let ratio = new_zoom / self.zoom;
        self.pan_x = anchor_x - (anchor_x - self.pan_x) * ratio;
        self.pan_y = anchor_y - (anchor_y - self.pan_y) * ratio;
        self.zoom = new_zoom;
    }

    /// Move the image by `(dx, dy)` screen-space pixels.
    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.pan_x += dx;
        self.pan_y += dy;
    }

    /// Zoom so the viewport rectangle `(x0,y0)–(x1,y1)` fills the view.
    ///
    /// Coordinates are in the same screen space as [`Self::pan_x`].
    pub fn zoom_to_rect(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        viewport_w: f32,
        viewport_h: f32,
    ) {
        let w = (x1 - x0).abs().max(8.0);
        let h = (y1 - y0).abs().max(8.0);
        let cx = (x0 + x1) * 0.5;
        let cy = (y0 + y1) * 0.5;
        let factor = (viewport_w / w).min(viewport_h / h);
        let new_zoom = (self.zoom * factor).clamp(0.05, 32.0);
        self.set_zoom(new_zoom, cx, cy);
        self.pan(viewport_w * 0.5 - cx, viewport_h * 0.5 - cy);
    }

    /// Pan so image fraction `(nx, ny)` (0–1) sits at the viewport center.
    pub fn pan_to_image_fraction(
        &mut self,
        nx: f32,
        ny: f32,
        image_w: u32,
        image_h: u32,
        rotation_degrees: f32,
    ) {
        let (iw, ih) = Self::oriented_size(image_w, image_h, rotation_degrees);
        let nx = nx.clamp(0.0, 1.0);
        let ny = ny.clamp(0.0, 1.0);
        self.pan_x = (iw as f32) * self.zoom * (0.5 - nx);
        self.pan_y = (ih as f32) * self.zoom * (0.5 - ny);
    }

    /// Rotate 90° clockwise.
    pub fn rotate_clockwise(&mut self) {
        self.rotate_by(90.0);
    }

    /// Rotate 90° counter-clockwise.
    pub fn rotate_counter_clockwise(&mut self) {
        self.rotate_by(-90.0);
    }

    /// Rotate 180°.
    pub fn rotate_180(&mut self) {
        self.rotate_by(180.0);
    }

    /// Add `delta` degrees (view-only).
    pub fn rotate_by(&mut self, delta: f32) {
        self.rotation_degrees = (self.rotation_degrees + delta).rem_euclid(360.0);
    }

    /// Set an absolute view-only angle in degrees.
    pub fn set_rotation(&mut self, degrees: f32) {
        self.rotation_degrees = degrees.rem_euclid(360.0);
    }

    /// Clear rotation and flips (keeps zoom / pan).
    pub fn reset_orientation(&mut self) {
        self.rotation_degrees = 0.0;
        self.flipped_horizontal = false;
        self.flipped_vertical = false;
    }

    /// Reset to identity (zoom 1.0, no pan, no rotation).
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_uses_smaller_axis() {
        let t = ViewTransform::fit_to_viewport(1000, 500, 200.0, 200.0);
        assert!((t.zoom - 0.2).abs() < 1e-3);
    }

    #[test]
    fn fit_width_uses_x_scale() {
        let z = ViewTransform::zoom_for_fit(ImageFitMode::Width, 1000, 500, 0.0, 200.0, 200.0);
        assert!((z - 0.2).abs() < 1e-3);
    }

    #[test]
    fn fit_height_uses_y_scale() {
        let z = ViewTransform::zoom_for_fit(ImageFitMode::Height, 1000, 500, 0.0, 200.0, 200.0);
        assert!((z - 0.4).abs() < 1e-3);
    }

    #[test]
    fn shrink_does_not_upscale() {
        let z = ViewTransform::zoom_for_fit(ImageFitMode::Shrink, 100, 80, 0.0, 400.0, 400.0);
        assert!((z - 1.0).abs() < 1e-3);
        let z2 = ViewTransform::zoom_for_fit(ImageFitMode::Window, 100, 80, 0.0, 400.0, 400.0);
        assert!(z2 > 1.5);
    }

    #[test]
    fn rotated_fit_swaps_axes() {
        let z = ViewTransform::zoom_for_fit(ImageFitMode::Window, 1000, 500, 90.0, 200.0, 200.0);
        // Oriented size is 500×1000; limiting axis is height (200/1000).
        assert!((z - 0.2).abs() < 1e-3);
    }

    #[test]
    fn zoom_around_anchor_keeps_anchor_fixed() {
        let mut t = ViewTransform::default();
        t.set_zoom(2.0, 10.0, 10.0);
        let anchor_proj_before = 10.0_f32;
        let anchor_proj_after = t.pan_x + 10.0 * t.zoom;
        assert!((anchor_proj_after - anchor_proj_before).abs() < 1e-3);
    }

    #[test]
    fn rotate_wraps_at_360() {
        let mut t = ViewTransform::default();
        for _ in 0..4 {
            t.rotate_clockwise();
        }
        assert!((t.rotation_degrees).abs() < 1e-3);
    }

    #[test]
    fn rotate_180_and_free_angle() {
        let mut t = ViewTransform::default();
        t.rotate_180();
        assert!((t.rotation_degrees - 180.0).abs() < 1e-3);
        t.set_rotation(45.0);
        assert!((t.rotation_degrees - 45.0).abs() < 1e-3);
        t.rotate_by(-15.0);
        assert!((t.rotation_degrees - 30.0).abs() < 1e-3);
    }

    #[test]
    fn reset_orientation_keeps_zoom() {
        let mut t = ViewTransform::default();
        t.zoom = 2.0;
        t.pan_x = 8.0;
        t.rotate_180();
        t.flipped_horizontal = true;
        t.reset_orientation();
        assert!((t.zoom - 2.0).abs() < 1e-3);
        assert!((t.pan_x - 8.0).abs() < 1e-3);
        assert!(t.rotation_degrees.abs() < 1e-3);
        assert!(!t.flipped_horizontal);
    }

    #[test]
    fn free_rotation_grows_oriented_box() {
        let (w, h) = ViewTransform::oriented_size(100, 50, 45.0);
        assert!(w > 100);
        assert!(h > 50);
    }

    #[test]
    fn background_cycles() {
        let mut b = ImageBackground::Theme;
        for _ in 0..6 {
            b = b.next();
        }
        assert_eq!(b, ImageBackground::Theme);
    }

    #[test]
    fn zoom_to_rect_enlarges_and_centers() {
        let mut t = ViewTransform::default();
        t.zoom = 1.0;
        t.zoom_to_rect(40.0, 40.0, 80.0, 80.0, 200.0, 200.0);
        assert!((t.zoom - 5.0).abs() < 1e-3);
    }

    #[test]
    fn pan_to_fraction_centers_requested_point() {
        let mut t = ViewTransform::default();
        t.zoom = 2.0;
        t.pan_to_image_fraction(0.25, 0.5, 100, 80, 0.0);
        assert!((t.pan_x - 50.0).abs() < 1e-3);
        assert!(t.pan_y.abs() < 1e-3);
    }
}
