//! Per-type thumbnail generators.

use std::sync::Arc;

use image::{imageops::FilterType, GenericImageView};

use crate::error::{Result, ViewerError};

use super::Thumbnail;

/// Build a thumbnail from image bytes, fitting the longest side to
/// `target_px`. Prefers an EXIF / embedded JPEG preview so a 40 MP file
/// does not need a full decode. PDF / video thumbnails are future tasks.
///
/// # Errors
///
/// Returns [`ViewerError::ThumbnailFailed`] on decode failure.
pub fn image_thumbnail(bytes: &[u8], target_px: u32) -> Result<Thumbnail> {
    if let Some(preview) = super::exif_preview::exif_jpeg_thumbnail(bytes) {
        if let Ok(thumb) = decode_fit(preview, target_px) {
            return Ok(thumb);
        }
    }
    if let Some(preview) = super::exif_preview::embedded_jpeg_preview(bytes) {
        if preview.len() < bytes.len() {
            if let Ok(thumb) = decode_fit(preview, target_px) {
                return Ok(thumb);
            }
        }
    }
    decode_fit(bytes, target_px)
}

fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF
}

/// Largest libjpeg IDCT denom in {8,4,2,1} that still covers `target` on the
/// long edge, so we decode ~1/64th of the pixels for a 256px thumb of a 4K JPEG.
fn jpeg_scale_denom(src_long: u32, target: u32) -> u32 {
    let target = target.max(1);
    for denom in [8, 4, 2] {
        if src_long / denom >= target {
            return denom;
        }
    }
    1
}

fn decode_jpeg_fit(bytes: &[u8], target_px: u32) -> Result<Thumbnail> {
    let mut dec = libjpeg_turbo_rs::Decoder::new(bytes)
        .map_err(|e| ViewerError::ThumbnailFailed(e.to_string()))?;
    let hdr = dec.header();
    let w = u32::from(hdr.width).max(1);
    let h = u32::from(hdr.height).max(1);
    let denom = jpeg_scale_denom(w.max(h), target_px);
    if denom > 1 {
        dec.set_scale(libjpeg_turbo_rs::ScalingFactor::new(1, denom));
    }
    dec.set_output_format(libjpeg_turbo_rs::PixelFormat::Rgba);
    let img = dec
        .decode_image()
        .map_err(|e| ViewerError::ThumbnailFailed(e.to_string()))?;
    let width = u32::try_from(img.width).unwrap_or(1).max(1);
    let height = u32::try_from(img.height).unwrap_or(1).max(1);
    let rgba = image::RgbaImage::from_raw(width, height, img.data)
        .ok_or_else(|| ViewerError::ThumbnailFailed("rgba size mismatch".into()))?;
    fit_rgba(rgba, target_px)
}

fn fit_rgba(rgba: image::RgbaImage, target_px: u32) -> Result<Thumbnail> {
    let (tw, th) = fit(rgba.width(), rgba.height(), target_px);
    if rgba.width() == tw && rgba.height() == th {
        return Ok(Thumbnail {
            rgba: Arc::new(rgba.into_raw()),
            width: tw,
            height: th,
        });
    }
    let resized = image::DynamicImage::ImageRgba8(rgba)
        .resize(tw, th, FilterType::Triangle)
        .into_rgba8();
    let (fw, fh) = resized.dimensions();
    Ok(Thumbnail {
        rgba: Arc::new(resized.into_raw()),
        width: fw,
        height: fh,
    })
}

fn decode_fit(bytes: &[u8], target_px: u32) -> Result<Thumbnail> {
    if is_jpeg(bytes) {
        if let Ok(thumb) = decode_jpeg_fit(bytes, target_px) {
            return Ok(thumb);
        }
    }
    let img =
        image::load_from_memory(bytes).map_err(|e| ViewerError::ThumbnailFailed(e.to_string()))?;
    let (w, h) = img.dimensions();
    let (tw, th) = fit(w, h, target_px);
    let resized = img.resize(tw, th, FilterType::Triangle).into_rgba8();
    let (fw, fh) = resized.dimensions();
    Ok(Thumbnail {
        rgba: Arc::new(resized.into_raw()),
        width: fw,
        height: fh,
    })
}

fn fit(w: u32, h: u32, target: u32) -> (u32, u32) {
    if w == 0 || h == 0 {
        return (target, target);
    }
    if w >= h {
        (target, (h * target / w).max(1))
    } else {
        ((w * target / h).max(1), target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnail_from_png_respects_target_box() {
        let mut img = image::RgbaImage::new(1000, 500);
        for p in img.pixels_mut() {
            *p = image::Rgba([0, 0, 0, 255]);
        }
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        let thumb = image_thumbnail(&buf.into_inner(), 100).unwrap();
        assert!(thumb.width.max(thumb.height) == 100);
    }

    #[test]
    fn jpeg_dct_scale_fits_target_box() {
        let img = image::RgbImage::from_pixel(800, 400, image::Rgb([10, 20, 30]));
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Jpeg).unwrap();
        let thumb = image_thumbnail(&buf.into_inner(), 100).unwrap();
        assert_eq!(thumb.width.max(thumb.height), 100);
    }

    #[test]
    fn jpeg_scale_denom_picks_eighth_for_large_source() {
        assert_eq!(jpeg_scale_denom(4000, 256), 8);
        assert_eq!(jpeg_scale_denom(400, 256), 1);
        assert_eq!(jpeg_scale_denom(800, 256), 2);
    }

    #[test]
    fn prefers_embedded_jpeg_preview() {
        let preview = image::RgbImage::from_pixel(16, 8, image::Rgb([9, 8, 7]));
        let mut preview_buf = std::io::Cursor::new(Vec::new());
        preview
            .write_to(&mut preview_buf, image::ImageFormat::Jpeg)
            .unwrap();
        let preview_jpeg = preview_buf.into_inner();
        let mut container = vec![0xFF, 0xD8, 0x00, 0x01];
        container.extend_from_slice(&preview_jpeg);
        let thumb = image_thumbnail(&container, 16).unwrap();
        assert!(thumb.width <= 16 && thumb.height <= 16);
        assert!(thumb.width > 0 && thumb.height > 0);
    }
}
