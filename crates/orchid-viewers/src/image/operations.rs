//! Destructive image operations used by the viewer's preview pipeline.
//!
//! These are preview-only — saving the result back to disk is **not** in
//! MVP scope. Document the limitation prominently in the viewer UI once
//! the editing toolbar ships.

use image::{imageops, RgbaImage};

use crate::error::{Result, ViewerError};
use crate::image::loader::LoadedImage;

fn to_rgba(image: &LoadedImage) -> Result<RgbaImage> {
    RgbaImage::from_raw(image.width, image.height, image.rgba.clone())
        .ok_or_else(|| ViewerError::ImageDecode("invalid RGBA buffer".into()))
}

fn from_rgba(src: RgbaImage, template: &LoadedImage) -> LoadedImage {
    let (w, h) = src.dimensions();
    LoadedImage {
        rgba: src.into_raw(),
        width: w,
        height: h,
        format: template.format,
        original_size_bytes: template.original_size_bytes,
    }
}

/// Rotate 90° clockwise.
#[must_use]
pub fn rotate_90_cw(src: &LoadedImage) -> LoadedImage {
    let buf = to_rgba(src).expect("rgba buffer must round-trip");
    let out = imageops::rotate90(&buf);
    from_rgba(out, src)
}

/// Rotate 180°.
#[must_use]
pub fn rotate_180(src: &LoadedImage) -> LoadedImage {
    let buf = to_rgba(src).expect("rgba buffer must round-trip");
    let out = imageops::rotate180(&buf);
    from_rgba(out, src)
}

/// Flip horizontally.
#[must_use]
pub fn flip_horizontal(src: &LoadedImage) -> LoadedImage {
    let buf = to_rgba(src).expect("rgba buffer must round-trip");
    let out = imageops::flip_horizontal(&buf);
    from_rgba(out, src)
}

/// Flip vertically.
#[must_use]
pub fn flip_vertical(src: &LoadedImage) -> LoadedImage {
    let buf = to_rgba(src).expect("rgba buffer must round-trip");
    let out = imageops::flip_vertical(&buf);
    from_rgba(out, src)
}

/// Crop to `(x, y, w, h)` in pixels. Out-of-bounds returns [`ViewerError::ImageDecode`].
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
    let mut buf = to_rgba(src)?;
    let view = imageops::crop(&mut buf, x, y, w, h).to_image();
    Ok(from_rgba(view, src))
}

/// Resize to `(target_w, target_h)` with `Lanczos3`.
#[must_use]
pub fn resize(src: &LoadedImage, target_w: u32, target_h: u32) -> LoadedImage {
    let buf = to_rgba(src).expect("rgba buffer must round-trip");
    let out = imageops::resize(&buf, target_w, target_h, imageops::FilterType::Lanczos3);
    from_rgba(out, src)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::loader::ImageFormat;

    fn two_by_two() -> LoadedImage {
        LoadedImage {
            #[rustfmt::skip]
            rgba: vec![
                // Row 0: (R, G)
                255, 0, 0, 255,   0, 255, 0, 255,
                // Row 1: (B, W)
                0, 0, 255, 255,  255, 255, 255, 255,
            ],
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
        assert_eq!(out.rgba[0..4], [0, 255, 0, 255]); // was (1, 0) = G
        assert_eq!(out.rgba[4..8], [255, 0, 0, 255]); // was (0, 0) = R
    }

    #[test]
    fn crop_rejects_oob() {
        let src = two_by_two();
        assert!(crop(&src, 0, 0, 3, 3).is_err());
    }
}
