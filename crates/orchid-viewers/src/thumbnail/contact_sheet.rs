//! Contact-sheet compositor: a grid of thumbnails as one RGBA image.

use std::sync::Arc;

use super::Thumbnail;

/// Lay out `thumbs` into a padded grid. Empty input yields a 1×1 transparent pixel.
#[must_use]
pub fn compose_contact_sheet(
    thumbs: &[Thumbnail],
    columns: u32,
    cell_px: u32,
    padding: u32,
) -> Thumbnail {
    let columns = columns.max(1);
    let cell_px = cell_px.max(8);
    let padding = padding.max(1);
    if thumbs.is_empty() {
        return Thumbnail {
            rgba: Arc::new(vec![0, 0, 0, 0]),
            width: 1,
            height: 1,
        };
    }
    let n = thumbs.len() as u32;
    let rows = n.div_ceil(columns);
    let width = columns * cell_px + (columns + 1) * padding;
    let height = rows * cell_px + (rows + 1) * padding;
    let mut rgba = vec![0x2A_u8; (width as usize) * (height as usize) * 4];
    for (i, thumb) in thumbs.iter().enumerate() {
        let col = (i as u32) % columns;
        let row = (i as u32) / columns;
        let cell_x = padding + col * (cell_px + padding);
        let cell_y = padding + row * (cell_px + padding);
        blit_fit(&mut rgba, width, height, cell_x, cell_y, cell_px, thumb);
    }
    Thumbnail {
        rgba: Arc::new(rgba),
        width,
        height,
    }
}

fn blit_fit(
    dest: &mut [u8],
    dest_w: u32,
    dest_h: u32,
    cell_x: u32,
    cell_y: u32,
    cell_px: u32,
    thumb: &Thumbnail,
) {
    if thumb.width == 0 || thumb.height == 0 {
        return;
    }
    let (tw, th) = if thumb.width >= thumb.height {
        let w = cell_px.min(thumb.width);
        let h = (thumb.height * w / thumb.width).max(1);
        (w, h)
    } else {
        let h = cell_px.min(thumb.height);
        let w = (thumb.width * h / thumb.height).max(1);
        (w, h)
    };
    let ox = cell_x + (cell_px.saturating_sub(tw)) / 2;
    let oy = cell_y + (cell_px.saturating_sub(th)) / 2;
    for y in 0..th {
        for x in 0..tw {
            let sx = (x * thumb.width / tw).min(thumb.width - 1);
            let sy = (y * thumb.height / th).min(thumb.height - 1);
            let si = ((sy * thumb.width + sx) * 4) as usize;
            let dx = ox + x;
            let dy = oy + y;
            if dx >= dest_w || dy >= dest_h {
                continue;
            }
            let di = ((dy * dest_w + dx) * 4) as usize;
            if di + 3 < dest.len() && si + 3 < thumb.rgba.len() {
                dest[di..di + 4].copy_from_slice(&thumb.rgba[si..si + 4]);
            }
        }
    }
}

/// Encode a contact sheet as PNG bytes.
///
/// # Errors
///
/// PNG encode failure.
pub fn encode_contact_sheet_png(sheet: &Thumbnail) -> crate::error::Result<Vec<u8>> {
    use image::ImageEncoder;
    let mut buf = Vec::new();
    image::codecs::png::PngEncoder::new(&mut buf)
        .write_image(
            sheet.rgba.as_slice(),
            sheet.width,
            sheet.height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| crate::error::ViewerError::ThumbnailFailed(e.to_string()))?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, r: u8) -> Thumbnail {
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for px in rgba.chunks_exact_mut(4) {
            px[0] = r;
            px[3] = 255;
        }
        Thumbnail {
            rgba: Arc::new(rgba),
            width: w,
            height: h,
        }
    }

    #[test]
    fn grid_size_matches_cols_and_padding() {
        let thumbs = vec![solid(10, 10, 255), solid(10, 8, 128), solid(8, 10, 64)];
        let sheet = compose_contact_sheet(&thumbs, 2, 20, 2);
        assert_eq!(sheet.width, 2 * 20 + 3 * 2);
        assert_eq!(sheet.height, 2 * 20 + 3 * 2);
        assert_eq!(sheet.rgba.len(), (sheet.width * sheet.height * 4) as usize);
    }

    #[test]
    fn empty_sheet_is_placeholder() {
        let sheet = compose_contact_sheet(&[], 4, 64, 4);
        assert_eq!(sheet.width, 1);
        assert_eq!(sheet.height, 1);
    }
}
