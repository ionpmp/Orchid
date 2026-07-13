//! Destructive image operations used by the viewer's preview pipeline.
//!
//! These are preview-only — saving the result back to disk is **not** in
//! MVP scope. Document the limitation prominently in the viewer UI once
//! the editing toolbar ships.

use image::{imageops, ImageBuffer, Rgba, RgbaImage};

use crate::error::{Result, ViewerError};
use crate::image::loader::LoadedImage;

type RgbaView<'a> = ImageBuffer<Rgba<u8>, &'a [u8]>;

/// Borrowed RGBA view — avoids cloning the full buffer for read-only ops.
fn rgba_view(image: &LoadedImage) -> Result<RgbaView<'_>> {
    ImageBuffer::from_raw(image.width, image.height, image.rgba.as_slice())
        .ok_or_else(|| ViewerError::ImageDecode("invalid RGBA buffer".into()))
}

fn from_rgba(src: RgbaImage, template: &LoadedImage) -> LoadedImage {
    let (w, h) = src.dimensions();
    LoadedImage {
        rgba: std::sync::Arc::new(src.into_raw()),
        width: w,
        height: h,
        format: template.format,
        original_size_bytes: template.original_size_bytes,
    }
}

/// Rotate 90° clockwise.
#[must_use]
pub fn rotate_90_cw(src: &LoadedImage) -> LoadedImage {
    let view = rgba_view(src).expect("rgba buffer must round-trip");
    let out = imageops::rotate90(&view);
    from_rgba(out, src)
}

/// Rotate 180°.
#[must_use]
pub fn rotate_180(src: &LoadedImage) -> LoadedImage {
    let view = rgba_view(src).expect("rgba buffer must round-trip");
    let out = imageops::rotate180(&view);
    from_rgba(out, src)
}

/// Flip horizontally.
#[must_use]
pub fn flip_horizontal(src: &LoadedImage) -> LoadedImage {
    let view = rgba_view(src).expect("rgba buffer must round-trip");
    let out = imageops::flip_horizontal(&view);
    from_rgba(out, src)
}

/// Flip vertically.
#[must_use]
pub fn flip_vertical(src: &LoadedImage) -> LoadedImage {
    let view = rgba_view(src).expect("rgba buffer must round-trip");
    let out = imageops::flip_vertical(&view);
    from_rgba(out, src)
}

/// Crop to `(x, y, w, h)` in pixels. Out-of-bounds returns [`ViewerError::ImageDecode`].
///
/// Copies only the cropped region (not the full source buffer).
///
/// # Errors
///
/// Returns [`ViewerError::ImageDecode`] if the rect falls outside the image.
pub fn crop(src: &LoadedImage, x: u32, y: u32, w: u32, h: u32) -> Result<LoadedImage> {
    if x + w > src.width || y + h > src.height || w == 0 || h == 0 {
        return Err(ViewerError::ImageDecode(
            "crop rect outside image bounds".into(),
        ));
    }
    let mut out = Vec::with_capacity((w as usize).saturating_mul(h as usize).saturating_mul(4));
    let row_bytes = (w as usize).saturating_mul(4);
    for row in y..y.saturating_add(h) {
        let start = ((row as usize) * (src.width as usize) + (x as usize)).saturating_mul(4);
        let end = start + row_bytes;
        out.extend_from_slice(
            src.rgba
                .get(start..end)
                .ok_or_else(|| ViewerError::ImageDecode("crop row out of range".into()))?,
        );
    }
    Ok(LoadedImage {
        rgba: std::sync::Arc::new(out),
        width: w,
        height: h,
        format: src.format,
        original_size_bytes: src.original_size_bytes,
    })
}

/// Resize to `(target_w, target_h)` with `Lanczos3`.
#[must_use]
pub fn resize(src: &LoadedImage, target_w: u32, target_h: u32) -> LoadedImage {
    let view = rgba_view(src).expect("rgba buffer must round-trip");
    let out = imageops::resize(&view, target_w, target_h, imageops::FilterType::Lanczos3);
    from_rgba(out, src)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::loader::ImageFormat;

    fn two_by_two() -> LoadedImage {
        LoadedImage {
            #[rustfmt::skip]
            rgba: std::sync::Arc::new(vec![
                // Row 0: (R, G)
                255, 0, 0, 255,   0, 255, 0, 255,
                // Row 1: (B, W)
                0, 0, 255, 255,  255, 255, 255, 255,
            ]),
            width: 2,
            height: 2,
            format: ImageFormat::Png,
            original_size_bytes: 0,
        }
    }

    #[test]
    fn rotate_90_cw_rearranges_pixels() {
        let src = two_by_two();
        let out = rotate_90_cw(&src);
        // Expected rotation:
        //   (B, R)
        //   (W, G)
        assert_eq!(out.width, 2);
        assert_eq!(out.height, 2);
        assert_eq!(out.rgba[0..4], [0, 0, 255, 255]); // top-left = original B
        assert_eq!(out.rgba[4..8], [255, 0, 0, 255]); // top-right = original R
    }

    #[test]
    fn flip_horizontal_swaps_columns() {
        let src = two_by_two();
        let out = flip_horizontal(&src);
        assert_eq!(out.rgba[0..4], [0, 255, 0, 255]); // was G
        assert_eq!(out.rgba[4..8], [255, 0, 0, 255]); // was R
    }

    #[test]
    fn crop_rejects_oob() {
        let src = two_by_two();
        assert!(crop(&src, 0, 0, 3, 3).is_err());
    }

    #[test]
    fn crop_copies_only_region() {
        let src = two_by_two();
        let out = crop(&src, 1, 0, 1, 1).unwrap();
        assert_eq!(out.width, 1);
        assert_eq!(out.height, 1);
        assert_eq!(out.rgba.as_slice(), [0, 255, 0, 255]);
    }
}
