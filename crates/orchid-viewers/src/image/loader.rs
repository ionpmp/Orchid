//! Image loading.
//!
//! Uses the [`image`](image) crate's built-in decoders. RAW and SVG are
//! explicitly out of MVP scope — adding them is a future task that either
//! pulls in `rawloader` + `resvg` or delegates to a helper binary.

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
}
