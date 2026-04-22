//! Font-metric helpers for computing cols / rows from pixel sizes.

use crate::pty::PtySize;

/// Monospace cell dimensions as measured by the UI layer.
#[derive(Debug, Clone, Copy)]
pub struct FontMetrics {
    /// Width of a single monospace cell, in logical pixels.
    pub cell_width_px: f32,
    /// Height of a single monospace cell, in logical pixels.
    pub cell_height_px: f32,
}

impl FontMetrics {
    /// Compute the largest `PtySize` that fits into `width_px` × `height_px`,
    /// clamped to at least `1 × 1`.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_terminal::FontMetrics;
    /// let m = FontMetrics { cell_width_px: 8.0, cell_height_px: 16.0 };
    /// let s = m.fit(800.0, 600.0);
    /// assert_eq!(s.cols, 100);
    /// assert_eq!(s.rows, 37);
    /// ```
    #[must_use]
    pub fn fit(&self, width_px: f32, height_px: f32) -> PtySize {
        let cols = ((width_px / self.cell_width_px) as u16).max(1);
        let rows = ((height_px / self.cell_height_px) as u16).max(1);
        PtySize {
            cols,
            rows,
            pixel_width: width_px.max(0.0) as u16,
            pixel_height: height_px.max(0.0) as u16,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_clamps_to_at_least_one() {
        let m = FontMetrics {
            cell_width_px: 100.0,
            cell_height_px: 100.0,
        };
        let s = m.fit(50.0, 50.0);
        assert_eq!(s.cols, 1);
        assert_eq!(s.rows, 1);
    }
}
