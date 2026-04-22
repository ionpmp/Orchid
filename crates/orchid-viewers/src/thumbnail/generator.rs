//! Per-type thumbnail generators.

use std::sync::Arc;

use image::{imageops::FilterType, GenericImageView};

use crate::error::{Result, ViewerError};

use super::Thumbnail;

/// Build a thumbnail from image bytes, fitting the longest side to
/// `target_px`. PDF / video thumbnails are future tasks.
///
/// # Errors
///
/// Returns [`ViewerError::ThumbnailFailed`] on decode failure.
pub fn image_thumbnail(bytes: &[u8], target_px: u32) -> Result<Thumbnail> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| ViewerError::ThumbnailFailed(e.to_string()))?;
    let (w, h) = img.dimensions();
    let (tw, th) = fit(w, h, target_px);
    let resized = img.resize(tw, th, FilterType::Lanczos3).to_rgba8();
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
}
