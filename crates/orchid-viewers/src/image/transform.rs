//! Zoom / pan / rotate view transform for the image viewer.

/// Display transform applied on top of the loaded image.
#[derive(Debug, Clone, Copy)]
#[allow(missing_docs)]
pub struct ViewTransform {
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub rotation_degrees: i16,
    pub flipped_horizontal: bool,
    pub flipped_vertical: bool,
}

impl Default for ViewTransform {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            rotation_degrees: 0,
            flipped_horizontal: false,
            flipped_vertical: false,
        }
    }
}

impl ViewTransform {
    /// Build a transform that fits `image_w x image_h` into
    /// `viewport_w x viewport_h`, preserving aspect ratio.
    #[must_use]
    pub fn fit_to_viewport(
        image_w: u32,
        image_h: u32,
        viewport_w: f32,
        viewport_h: f32,
    ) -> Self {
        let zoom = if image_w == 0 || image_h == 0 {
            1.0
        } else {
            let zx = viewport_w / image_w as f32;
            let zy = viewport_h / image_h as f32;
            zx.min(zy).max(0.05)
        };
        Self {
            zoom,
            ..Self::default()
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

    /// Rotate 90° clockwise.
    pub fn rotate_clockwise(&mut self) {
        self.rotation_degrees = (self.rotation_degrees + 90).rem_euclid(360);
    }

    /// Rotate 90° counter-clockwise.
    pub fn rotate_counter_clockwise(&mut self) {
        self.rotation_degrees = (self.rotation_degrees - 90).rem_euclid(360);
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
        // Fits by height (200 / 500 = 0.4) because height is the smaller limit.
        assert!((t.zoom - 0.2).abs() < 1e-3);
    }

    #[test]
    fn zoom_around_anchor_keeps_anchor_fixed() {
        let mut t = ViewTransform::default();
        t.set_zoom(2.0, 10.0, 10.0);
        // After zoom the anchor should still project to ~the same screen
        // position (pan_x + anchor*zoom == same).
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
        assert_eq!(t.rotation_degrees, 0);
    }
}
