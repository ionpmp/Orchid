//! Image loading.
//!
//! Raster formats use the [`image`](image) crate's built-in decoders. SVG is
//! rasterized via [`resvg`]. RAW remains out of scope for now.

use std::sync::Arc;

use crate::error::{Result, ViewerError};
use image::GenericImageView;

/// Decoded image in RGBA8.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct LoadedImage {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
    pub original_size_bytes: u64,
}

/// Image format the loader recognised.
#[allow(missing_docs)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    WebP,
    Bmp,
    Gif,
    Tiff,
    Avif,
    Tga,
    Svg,
    Raw,
    Unknown,
}

impl ImageFormat {
    /// Short human-readable label (e.g. for the info strip).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::WebP => "WebP",
            Self::Bmp => "BMP",
            Self::Gif => "GIF",
            Self::Tiff => "TIFF",
            Self::Avif => "AVIF",
            Self::Tga => "TGA",
            Self::Svg => "SVG",
            Self::Raw => "RAW",
            Self::Unknown => "Image",
        }
    }

    fn from_image_crate(f: image::ImageFormat) -> Self {
        use image::ImageFormat::*;
        match f {
            Png => Self::Png,
            Jpeg => Self::Jpeg,
            WebP => Self::WebP,
            Bmp => Self::Bmp,
            Gif => Self::Gif,
            Tiff => Self::Tiff,
            Avif => Self::Avif,
            Tga => Self::Tga,
            _ => Self::Unknown,
        }
    }
}

/// Load an image via the given provider registry.
///
/// # Errors
///
/// * [`ViewerError::Fs`] on read failure.
/// * [`ViewerError::ImageDecode`] on decode failure.
/// * [`ViewerError::FileTooLarge`] when the file exceeds `size_limit`.
pub async fn load_image(
    path: &orchid_fs::FsPath,
    registry: Arc<orchid_fs::FsProviderRegistry>,
    size_limit_bytes: u64,
) -> Result<LoadedImage> {
    let provider = registry
        .for_path(path)
        .ok_or_else(|| orchid_fs::FsError::ProviderNotFound(path.scheme().to_string()))?;
    let bytes = provider.read(path).await?;
    let size = bytes.len() as u64;
    if size > size_limit_bytes {
        return Err(ViewerError::FileTooLarge {
            size,
            limit: size_limit_bytes,
        });
    }
    tokio::task::spawn_blocking(move || decode_bytes(&bytes, size))
        .await
        .map_err(|e| ViewerError::ImageDecode(e.to_string()))?
}

fn decode_bytes(bytes: &[u8], size: u64) -> Result<LoadedImage> {
    if looks_like_svg(bytes) {
        return decode_svg(bytes, size);
    }
    let guessed = image::guess_format(bytes)
        .map(ImageFormat::from_image_crate)
        .unwrap_or(ImageFormat::Unknown);
    let img = image::load_from_memory(bytes).map_err(|e| ViewerError::ImageDecode(e.to_string()))?;
    let (w, h) = img.dimensions();
    let rgba = img.to_rgba8().into_raw();
    Ok(LoadedImage {
        rgba,
        width: w,
        height: h,
        format: guessed,
        original_size_bytes: size,
    })
}

/// True when `bytes` look like an SVG document (XML sniff).
#[must_use]
pub fn looks_like_svg(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    let head: String = trimmed.chars().take(512).collect::<String>().to_ascii_lowercase();
    if head.starts_with("<svg") {
        return true;
    }
    if head.starts_with("<?xml") || head.starts_with("<!doctype") {
        return head.contains("<svg");
    }
    false
}

fn decode_svg(bytes: &[u8], size: u64) -> Result<LoadedImage> {
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_data(bytes, &opt)
        .map_err(|e| ViewerError::ImageDecode(format!("SVG parse: {e}")))?;

    let pixmap_size = tree.size().to_int_size();
    let width = pixmap_size.width();
    let height = pixmap_size.height();
    if width == 0 || height == 0 {
        return Err(ViewerError::ImageDecode(
            "SVG has zero width or height".into(),
        ));
    }

    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).ok_or_else(|| {
        ViewerError::ImageDecode(format!("failed to allocate {width}×{height} pixmap for SVG"))
    })?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );

    // tiny-skia stores premultiplied RGBA; convert to straight alpha for the
    // shared image pipeline (`image::RgbaImage` / DisplayedImage).
    let rgba = unpremultiply_rgba(pixmap.data());

    Ok(LoadedImage {
        rgba,
        width,
        height,
        format: ImageFormat::Svg,
        original_size_bytes: size,
    })
}

fn unpremultiply_rgba(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for px in data.chunks_exact(4) {
        let (r, g, b, a) = (px[0], px[1], px[2], px[3]);
        if a == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
        } else if a == 255 {
            out.extend_from_slice(px);
        } else {
            let a_f = f32::from(a);
            out.push(((f32::from(r) * 255.0 / a_f) + 0.5) as u8);
            out.push(((f32::from(g) * 255.0 / a_f) + 0.5) as u8);
            out.push(((f32::from(b) * 255.0 / a_f) + 0.5) as u8);
            out.push(a);
        }
    }
    out
}

/// Build an `Arc<Vec<u8>>` suitable for the snapshot's `rgba_bytes`.
#[must_use]
pub fn rgba_arc(image: &LoadedImage) -> Arc<Vec<u8>> {
    Arc::new(image.rgba.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_tiny_png() {
        // 2x2 red/green/blue/white PNG.
        let mut cursor = std::io::Cursor::new(Vec::new());
        let mut img = image::RgbaImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        img.put_pixel(1, 0, image::Rgba([0, 255, 0, 255]));
        img.put_pixel(0, 1, image::Rgba([0, 0, 255, 255]));
        img.put_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
        img.write_to(&mut cursor, image::ImageFormat::Png).unwrap();
        let bytes = cursor.into_inner();

        let loaded = decode_bytes(&bytes, bytes.len() as u64).unwrap();
        assert_eq!(loaded.width, 2);
        assert_eq!(loaded.height, 2);
        assert_eq!(loaded.format, ImageFormat::Png);
        assert_eq!(loaded.rgba[0..4], [255, 0, 0, 255]);
    }

    #[test]
    fn decodes_minimal_svg() {
        // Extra `#` delimiters so CSS `#rrggbb` does not terminate the raw string.
        let svg = br##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4" viewBox="0 0 4 4">
  <rect width="4" height="4" fill="#ff0000"/>
</svg>"##;
        assert!(looks_like_svg(svg));

        let loaded = decode_bytes(svg, svg.len() as u64).unwrap();
        assert_eq!(loaded.format, ImageFormat::Svg);
        assert_eq!(loaded.width, 4);
        assert_eq!(loaded.height, 4);
        assert_eq!(loaded.rgba.len(), 4 * 4 * 4);
        // Centre pixel should be opaque red.
        let i = ((2 * 4 + 2) * 4) as usize;
        assert_eq!(loaded.rgba[i], 255);
        assert_eq!(loaded.rgba[i + 1], 0);
        assert_eq!(loaded.rgba[i + 2], 0);
        assert_eq!(loaded.rgba[i + 3], 255);
    }

    #[test]
    fn sniffs_bare_svg_root() {
        let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"></svg>";
        assert!(looks_like_svg(svg));
    }
}
