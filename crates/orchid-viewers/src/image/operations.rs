//! Destructive image operations (crop / resize / canvas).
//!
//! Pixel ops live here; save-as (never overwrite) is [`crate::image::edit`].

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
        color_source: template.color_source.clone(),
        color_dest: template.color_dest.clone(),
        orientation: template.orientation,
        bit_depth: template.bit_depth,
        color_model: template.color_model.clone(),
    }
}

/// Rotate 90° clockwise.
///
/// # Errors
///
/// Returns [`ViewerError::ImageDecode`] when the RGBA buffer is corrupt.
pub fn rotate_90_cw(src: &LoadedImage) -> Result<LoadedImage> {
    let view = rgba_view(src)?;
    let out = imageops::rotate90(&view);
    Ok(from_rgba(out, src))
}

/// Rotate 180°.
///
/// # Errors
///
/// Returns [`ViewerError::ImageDecode`] when the RGBA buffer is corrupt.
pub fn rotate_180(src: &LoadedImage) -> Result<LoadedImage> {
    let view = rgba_view(src)?;
    let out = imageops::rotate180(&view);
    Ok(from_rgba(out, src))
}

/// Flip horizontally.
///
/// # Errors
///
/// Returns [`ViewerError::ImageDecode`] when the RGBA buffer is corrupt.
pub fn flip_horizontal(src: &LoadedImage) -> Result<LoadedImage> {
    let view = rgba_view(src)?;
    let out = imageops::flip_horizontal(&view);
    Ok(from_rgba(out, src))
}

/// Flip vertically.
///
/// # Errors
///
/// Returns [`ViewerError::ImageDecode`] when the RGBA buffer is corrupt.
pub fn flip_vertical(src: &LoadedImage) -> Result<LoadedImage> {
    let view = rgba_view(src)?;
    let out = imageops::flip_vertical(&view);
    Ok(from_rgba(out, src))
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
        color_source: src.color_source.clone(),
        color_dest: src.color_dest.clone(),
        orientation: src.orientation,
        bit_depth: src.bit_depth,
        color_model: src.color_model.clone(),
    })
}

/// Resize filter for [`resize_filtered`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResizeFilter {
    /// Nearest neighbour.
    Nearest,
    /// Triangle / bilinear.
    Bilinear,
    /// Catmull-Rom (bicubic).
    Bicubic,
    /// Lanczos3 (default).
    #[default]
    Lanczos,
}

impl ResizeFilter {
    fn to_image(self) -> imageops::FilterType {
        match self {
            Self::Nearest => imageops::FilterType::Nearest,
            Self::Bilinear => imageops::FilterType::Triangle,
            Self::Bicubic => imageops::FilterType::CatmullRom,
            Self::Lanczos => imageops::FilterType::Lanczos3,
        }
    }

    /// `nearest` / `bilinear` / `bicubic` / `lanczos`.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "nearest" | "nn" | "point" => Some(Self::Nearest),
            "bilinear" | "linear" | "triangle" => Some(Self::Bilinear),
            "bicubic" | "cubic" | "catmull" => Some(Self::Bicubic),
            "lanczos" | "lanczos3" | "" => Some(Self::Lanczos),
            _ => None,
        }
    }
}

/// Resize to `(target_w, target_h)` with `Lanczos3`.
///
/// # Errors
///
/// Returns [`ViewerError::ImageDecode`] when the RGBA buffer is corrupt.
pub fn resize(src: &LoadedImage, target_w: u32, target_h: u32) -> Result<LoadedImage> {
    resize_filtered(src, target_w, target_h, ResizeFilter::Lanczos)
}

/// Resize with an explicit filter.
///
/// # Errors
///
/// Returns [`ViewerError::ImageDecode`] when the RGBA buffer is corrupt or the
/// target size is zero.
pub fn resize_filtered(
    src: &LoadedImage,
    target_w: u32,
    target_h: u32,
    filter: ResizeFilter,
) -> Result<LoadedImage> {
    if target_w == 0 || target_h == 0 {
        return Err(ViewerError::ImageDecode("resize target is zero".into()));
    }
    let view = rgba_view(src)?;
    let out = imageops::resize(&view, target_w, target_h, filter.to_image());
    Ok(from_rgba(out, src))
}

/// Expand or shrink the canvas. The source is pasted at `(ox, oy)`; new pixels
/// use `fill` (RGBA).
///
/// # Errors
///
/// Zero destination size.
pub fn canvas_resize(
    src: &LoadedImage,
    dest_w: u32,
    dest_h: u32,
    ox: i32,
    oy: i32,
    fill: [u8; 4],
) -> Result<LoadedImage> {
    if dest_w == 0 || dest_h == 0 {
        return Err(ViewerError::ImageDecode("canvas size is zero".into()));
    }
    let mut out = vec![0u8; dest_w as usize * dest_h as usize * 4];
    for px in out.chunks_exact_mut(4) {
        px.copy_from_slice(&fill);
    }
    let src_w = src.width as i32;
    let src_h = src.height as i32;
    for sy in 0..src_h {
        let dy = sy + oy;
        if dy < 0 || dy >= dest_h as i32 {
            continue;
        }
        for sx in 0..src_w {
            let dx = sx + ox;
            if dx < 0 || dx >= dest_w as i32 {
                continue;
            }
            let si = ((sy as u32 * src.width + sx as u32) * 4) as usize;
            let di = ((dy as u32 * dest_w + dx as u32) * 4) as usize;
            if let (Some(s), Some(d)) = (src.rgba.get(si..si + 4), out.get_mut(di..di + 4)) {
                d.copy_from_slice(s);
            }
        }
    }
    Ok(LoadedImage {
        rgba: std::sync::Arc::new(out),
        width: dest_w,
        height: dest_h,
        format: src.format,
        original_size_bytes: src.original_size_bytes,
        color_source: src.color_source.clone(),
        color_dest: src.color_dest.clone(),
        orientation: src.orientation,
        bit_depth: src.bit_depth,
        color_model: src.color_model.clone(),
    })
}

/// Shrink `(x, y, w, h)` so its aspect matches `aspect` (width/height), staying
/// inside the image. `None` leaves the rect unchanged (still clamped).
#[must_use]
pub fn fit_crop_rect(
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    img_w: u32,
    img_h: u32,
    aspect: Option<f32>,
) -> (u32, u32, u32, u32) {
    let mut x = x.min(img_w.saturating_sub(1));
    let mut y = y.min(img_h.saturating_sub(1));
    let mut w = w.min(img_w.saturating_sub(x)).max(1);
    let mut h = h.min(img_h.saturating_sub(y)).max(1);
    if let Some(a) = aspect.filter(|a| *a > 0.05) {
        let cur = w as f32 / h as f32;
        if cur > a {
            let nw = ((h as f32) * a).round().max(1.0) as u32;
            x += (w.saturating_sub(nw)) / 2;
            w = nw.min(img_w.saturating_sub(x)).max(1);
        } else if cur < a {
            let nh = ((w as f32) / a).round().max(1.0) as u32;
            y += (h.saturating_sub(nh)) / 2;
            h = nh.min(img_h.saturating_sub(y)).max(1);
        }
    }
    (x, y, w, h)
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
            ..LoadedImage::meta_defaults()
        }
    }

    #[test]
    fn rotate_90_cw_rearranges_pixels() {
        let src = two_by_two();
        let out = rotate_90_cw(&src).unwrap();
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
        let out = flip_horizontal(&src).unwrap();
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

    #[test]
    fn canvas_expands_and_fills() {
        let src = two_by_two();
        let out = canvas_resize(&src, 4, 3, 1, 1, [1, 2, 3, 255]).unwrap();
        assert_eq!(out.width, 4);
        assert_eq!(out.height, 3);
        assert_eq!(out.rgba[0..4], [1, 2, 3, 255]);
        assert_eq!(out.rgba[(1 + 4) * 4..(1 + 4) * 4 + 4], [255, 0, 0, 255]);
    }

    #[test]
    fn fit_crop_locks_square() {
        let (x, y, w, h) = fit_crop_rect(0, 0, 4, 2, 4, 2, Some(1.0));
        assert_eq!((w, h), (2, 2));
        assert_eq!(x, 1);
        assert_eq!(y, 0);
    }
}
