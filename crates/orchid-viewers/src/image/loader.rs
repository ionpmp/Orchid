//! Image loading.
//!
//! Raster formats use the [`image`](image) crate's built-in decoders. SVG is
//! rasterized via [`resvg`]. On Windows, HEIC/HEIF is decoded through WIC when
//! the OS HEIF codec is installed; otherwise we return a clear unsupported
//! error. Camera RAW opens via the largest embedded JPEG preview when present.

use std::sync::Arc;

use crate::error::{Result, ViewerError};
use image::GenericImageView;

/// Extensions the image viewer / FM treat as images (including pending HEIC/RAW).
pub const IMAGE_FILE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "bmp", "gif", "tiff", "tif", "avif", "tga", "svg", "heic",
    "heif", "cr2", "nef", "arw", "dng", "raf", "orf", "rw2",
];

/// True when `ext` (no leading dot) is a known image extension.
#[must_use]
pub fn is_image_file_extension(ext: &str) -> bool {
    let lower = ext.to_ascii_lowercase();
    IMAGE_FILE_EXTENSIONS.iter().any(|e| *e == lower)
}

/// Decoded image in RGBA8.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub struct LoadedImage {
    /// Shared pixel buffer so pan/zoom snapshots do not clone megabytes.
    pub rgba: Arc<Vec<u8>>,
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
    Heic,
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
            Self::Heic => "HEIC",
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
/// Local files are memory-mapped so the compressed bytes are not copied into a
/// heap `Vec` before decode. Network / archive paths still use a full read.
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
    let ext = path.extension().map(|e| e.to_ascii_lowercase());

    if path.is_local() {
        let os_path = path.to_local()?;
        let meta = tokio::fs::metadata(&os_path)
            .await
            .map_err(orchid_fs::FsError::Io)?;
        let size = meta.len();
        if size > size_limit_bytes {
            return Err(ViewerError::FileTooLarge {
                size,
                limit: size_limit_bytes,
            });
        }
        return tokio::task::spawn_blocking(move || {
            decode_local_mmap(&os_path, size_limit_bytes, ext.as_deref())
        })
        .await
        .map_err(|e| ViewerError::ImageDecode(e.to_string()))?;
    }

    let bytes = provider.read(path).await?;
    let size = bytes.len() as u64;
    if size > size_limit_bytes {
        return Err(ViewerError::FileTooLarge {
            size,
            limit: size_limit_bytes,
        });
    }
    tokio::task::spawn_blocking(move || decode_bytes(&bytes, size, ext.as_deref()))
        .await
        .map_err(|e| ViewerError::ImageDecode(e.to_string()))?
}

fn decode_local_mmap(
    os_path: &std::path::Path,
    size_limit_bytes: u64,
    extension: Option<&str>,
) -> Result<LoadedImage> {
    let meta = std::fs::metadata(os_path).map_err(orchid_fs::FsError::Io)?;
    let size = meta.len();
    if size > size_limit_bytes {
        return Err(ViewerError::FileTooLarge {
            size,
            limit: size_limit_bytes,
        });
    }
    let file = std::fs::File::open(os_path).map_err(orchid_fs::FsError::Io)?;
    // SAFETY: opened read-only; we do not truncate/write the file while mapped.
    // Concurrent writers may still change bytes on some platforms — decode may
    // then fail or produce garbage, which surfaces as ImageDecode.
    let map = unsafe { memmap2::Mmap::map(&file) }.map_err(orchid_fs::FsError::Io)?;
    if map.len() as u64 > size_limit_bytes {
        return Err(ViewerError::FileTooLarge {
            size: map.len() as u64,
            limit: size_limit_bytes,
        });
    }
    decode_bytes(&map, size, extension)
}

fn decode_bytes(bytes: &[u8], size: u64, extension: Option<&str>) -> Result<LoadedImage> {
    if looks_like_svg(bytes) {
        return decode_svg(bytes, size);
    }
    if looks_like_heic(bytes) || matches!(extension, Some("heic" | "heif")) {
        return decode_heic(bytes, size);
    }
    if looks_like_raw(bytes) || is_raw_extension(extension) {
        return decode_raw_preview(bytes, size);
    }
    let guessed = image::guess_format(bytes)
        .map(ImageFormat::from_image_crate)
        .unwrap_or(ImageFormat::Unknown);
    let img = image::load_from_memory(bytes).map_err(|e| ViewerError::ImageDecode(e.to_string()))?;
    let (w, h) = img.dimensions();
    let rgba = Arc::new(img.to_rgba8().into_raw());
    Ok(LoadedImage {
        rgba,
        width: w,
        height: h,
        format: guessed,
        original_size_bytes: size,
    })
}

fn decode_heic(bytes: &[u8], size: u64) -> Result<LoadedImage> {
    #[cfg(windows)]
    {
        crate::image::heic_wic::decode_heic_wic(bytes, size)
    }
    #[cfg(not(windows))]
    {
        let _ = (bytes, size);
        Err(ViewerError::UnsupportedHeic)
    }
}

fn is_raw_extension(extension: Option<&str>) -> bool {
    matches!(
        extension,
        Some("cr2" | "nef" | "arw" | "dng" | "raf" | "orf" | "rw2")
    )
}

/// Show camera RAW via the largest embedded JPEG preview when present.
///
/// Full demosaic is deferred; most CR2/NEF/ARW/DNG files ship a usable JPEG
/// that file managers already rely on for quick look.
fn decode_raw_preview(bytes: &[u8], size: u64) -> Result<LoadedImage> {
    let Some(jpeg) = largest_embedded_jpeg(bytes) else {
        return Err(ViewerError::UnsupportedRaw);
    };
    let img = image::load_from_memory(jpeg).map_err(|e| {
        tracing::debug!(error = %e, "RAW embedded JPEG decode failed");
        ViewerError::UnsupportedRaw
    })?;
    let (w, h) = img.dimensions();
    let rgba = Arc::new(img.to_rgba8().into_raw());
    Ok(LoadedImage {
        rgba,
        width: w,
        height: h,
        format: ImageFormat::Raw,
        original_size_bytes: size,
    })
}

/// Scan for JPEG SOI…EOI segments and return the largest one (preview > thumb).
fn largest_embedded_jpeg(data: &[u8]) -> Option<&[u8]> {
    const MIN_PREVIEW_BYTES: usize = 4 * 1024;
    let mut best: Option<&[u8]> = None;
    let mut i = 0;
    while i + 1 < data.len() {
        if data[i] != 0xFF || data[i + 1] != 0xD8 {
            i += 1;
            continue;
        }
        let start = i;
        i += 2;
        let mut end = None;
        while i + 1 < data.len() {
            if data[i] == 0xFF && data[i + 1] == 0xD9 {
                end = Some(i + 2);
                break;
            }
            i += 1;
        }
        let Some(end) = end else {
            break;
        };
        let slice = &data[start..end];
        if slice.len() >= MIN_PREVIEW_BYTES && best.is_none_or(|b| slice.len() > b.len()) {
            best = Some(slice);
        }
        i = end;
    }
    best
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

/// True when `bytes` look like HEIC/HEIF (ISO BMFF `ftyp` with a HEIF brand).
#[must_use]
pub fn looks_like_heic(bytes: &[u8]) -> bool {
    sniff_unsupported_image(bytes) == Some(ImageFormat::Heic)
}

/// True when `bytes` match a common camera-RAW magic sequence.
#[must_use]
pub fn looks_like_raw(bytes: &[u8]) -> bool {
    sniff_unsupported_image(bytes) == Some(ImageFormat::Raw)
}

/// Sniff HEIC/HEIF or common RAW containers. Returns `None` when unknown.
#[must_use]
pub fn sniff_unsupported_image(bytes: &[u8]) -> Option<ImageFormat> {
    if is_heic_ftyp(bytes) {
        return Some(ImageFormat::Heic);
    }
    if is_raw_magic(bytes) {
        return Some(ImageFormat::Raw);
    }
    None
}

fn is_heic_ftyp(bytes: &[u8]) -> bool {
    // ISO BMFF: [size:4][ftyp][major_brand:4][…] plus optional compatible brands.
    if bytes.len() < 12 || &bytes[4..8] != b"ftyp" {
        return false;
    }
    const HEIF_BRANDS: &[&[u8]] = &[
        b"heic", b"heif", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"hevm", b"hevs", b"mif1",
        b"msf1",
    ];
    // Check major brand at offset 8, then every compatible brand from offset 16.
    let mut offset = 8;
    while offset + 4 <= bytes.len() {
        let brand = &bytes[offset..offset + 4];
        if HEIF_BRANDS.contains(&brand) {
            return true;
        }
        // Skip minor_version (4 bytes) after the major brand.
        offset = if offset == 8 { 16 } else { offset + 4 };
        // Don't scan forever on huge boxes.
        if offset > 64 {
            break;
        }
    }
    false
}

fn is_raw_magic(bytes: &[u8]) -> bool {
    // Fujifilm RAF
    if bytes.starts_with(b"FUJIFILMCCD-RAW") {
        return true;
    }
    // Olympus ORF
    if bytes.starts_with(b"IIRO") || bytes.starts_with(b"MMOR") || bytes.starts_with(b"IIRS") {
        return true;
    }
    // Panasonic RW2
    if bytes.starts_with(b"IIU\0") {
        return true;
    }
    // Canon CR2: TIFF header + "CR" magic at offset 8.
    if bytes.len() >= 10
        && ((bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*")) && &bytes[8..10] == b"CR")
    {
        return true;
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
    let mut rgba = pixmap.take();
    unpremultiply_rgba_inplace(&mut rgba);

    Ok(LoadedImage {
        rgba: Arc::new(rgba),
        width,
        height,
        format: ImageFormat::Svg,
        original_size_bytes: size,
    })
}

fn unpremultiply_rgba_inplace(data: &mut [u8]) {
    for px in data.chunks_exact_mut(4) {
        let a = px[3];
        if a == 0 {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
        } else if a != 255 {
            let a_f = f32::from(a);
            px[0] = ((f32::from(px[0]) * 255.0 / a_f) + 0.5).min(255.0) as u8;
            px[1] = ((f32::from(px[1]) * 255.0 / a_f) + 0.5).min(255.0) as u8;
            px[2] = ((f32::from(px[2]) * 255.0 / a_f) + 0.5).min(255.0) as u8;
        }
    }
}

/// Shared pixel buffer for the snapshot's `rgba_bytes`.
#[must_use]
pub fn rgba_arc(image: &LoadedImage) -> Arc<Vec<u8>> {
    Arc::clone(&image.rgba)
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

        let loaded = decode_bytes(&bytes, bytes.len() as u64, None).unwrap();
        assert_eq!(loaded.width, 2);
        assert_eq!(loaded.height, 2);
        assert_eq!(loaded.format, ImageFormat::Png);
        assert_eq!(loaded.rgba[0..4], [255, 0, 0, 255]);
    }

    #[test]
    fn decode_local_mmap_reads_png_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tiny.png");
        let mut img = image::RgbaImage::new(1, 1);
        img.put_pixel(0, 0, image::Rgba([1, 2, 3, 255]));
        img.save(&path).unwrap();
        let loaded = decode_local_mmap(&path, u64::MAX, Some("png")).unwrap();
        assert_eq!(loaded.width, 1);
        assert_eq!(loaded.height, 1);
        assert_eq!(loaded.rgba[0..4], [1, 2, 3, 255]);
    }

    #[test]
    fn decodes_minimal_svg() {
        // Extra `#` delimiters so CSS `#rrggbb` does not terminate the raw string.
        let svg = br##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4" viewBox="0 0 4 4">
  <rect width="4" height="4" fill="#ff0000"/>
</svg>"##;
        assert!(looks_like_svg(svg));

        let loaded = decode_bytes(svg, svg.len() as u64, Some("svg")).unwrap();
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

    fn heic_ftyp(brand: &[u8; 4]) -> Vec<u8> {
        let mut v = vec![0, 0, 0, 0x18];
        v.extend_from_slice(b"ftyp");
        v.extend_from_slice(brand);
        v.extend_from_slice(&[0, 0, 0, 0]); // minor_version
        v.extend_from_slice(b"mif1");
        v.extend_from_slice(b"heic");
        v
    }

    #[test]
    fn sniffs_heic_ftyp_brands() {
        for brand in [b"heic", b"heif", b"mif1", b"msf1", b"heix"] {
            let bytes = heic_ftyp(brand);
            assert_eq!(sniff_unsupported_image(&bytes), Some(ImageFormat::Heic));
            assert!(looks_like_heic(&bytes));
        }
    }

    #[test]
    fn heic_decode_returns_clear_error() {
        let bytes = heic_ftyp(b"heic");
        let err = decode_bytes(&bytes, bytes.len() as u64, None).unwrap_err();
        assert!(
            matches!(err, ViewerError::UnsupportedHeic),
            "unexpected error: {err:?}"
        );
        assert_eq!(err.to_string(), "viewer-image-heic-unsupported");
    }

    #[test]
    fn heic_extension_without_ftyp_returns_clear_error() {
        let err = decode_bytes(b"not-a-heic", 10, Some("heic")).unwrap_err();
        assert!(matches!(err, ViewerError::UnsupportedHeic));
    }

    #[test]
    fn sniffs_common_raw_magics() {
        assert_eq!(
            sniff_unsupported_image(b"FUJIFILMCCD-RAW \x00rest"),
            Some(ImageFormat::Raw)
        );
        assert_eq!(
            sniff_unsupported_image(b"IIROxxxx"),
            Some(ImageFormat::Raw)
        );
        assert_eq!(
            sniff_unsupported_image(b"IIU\0rest"),
            Some(ImageFormat::Raw)
        );
        let mut cr2 = b"II*\0\0\0\0\0CR".to_vec();
        cr2.extend_from_slice(b"rest");
        assert_eq!(sniff_unsupported_image(&cr2), Some(ImageFormat::Raw));
    }

    #[test]
    fn raw_decode_returns_clear_error() {
        let bytes = b"FUJIFILMCCD-RAW \x00";
        let err = decode_bytes(bytes, bytes.len() as u64, None).unwrap_err();
        assert!(
            matches!(err, ViewerError::UnsupportedRaw),
            "unexpected error: {err:?}"
        );
        assert_eq!(err.to_string(), "viewer-image-raw-unsupported");
    }

    #[test]
    fn largest_embedded_jpeg_picks_bigger_segment() {
        let mut small = vec![0xFF, 0xD8];
        small.extend(std::iter::repeat_n(1u8, 9 * 1024));
        small.extend_from_slice(&[0xFF, 0xD9]);

        let mut big = vec![0xFF, 0xD8];
        big.extend(std::iter::repeat_n(2u8, 12 * 1024));
        big.extend_from_slice(&[0xFF, 0xD9]);

        let mut container = Vec::new();
        container.extend_from_slice(b"FUJIFILMCCD-RAW ");
        container.extend_from_slice(&small);
        container.extend_from_slice(&[0, 1, 2, 3]);
        container.extend_from_slice(&big);

        let found = largest_embedded_jpeg(&container).unwrap();
        assert_eq!(found.len(), big.len());
        assert!(found.starts_with(&[0xFF, 0xD8]));
        assert!(found.ends_with(&[0xFF, 0xD9]));
    }

    #[test]
    fn raw_preview_decodes_embedded_jpeg() {
        let mut img = image::RgbImage::new(256, 256);
        for (i, p) in img.pixels_mut().enumerate() {
            let v = (i % 256) as u8;
            *p = image::Rgb([v, 200, 40]);
        }
        let mut cursor = std::io::Cursor::new(Vec::new());
        img.write_to(&mut cursor, image::ImageFormat::Jpeg).unwrap();
        let jpeg = cursor.into_inner();
        assert!(
            jpeg.len() >= 4 * 1024,
            "fixture JPEG too small for preview floor: {} bytes",
            jpeg.len()
        );

        let mut container = Vec::new();
        container.extend_from_slice(b"II*\0\x08\0\0\0CR");
        container.extend_from_slice(&jpeg);

        let loaded = decode_raw_preview(&container, container.len() as u64).unwrap();
        assert_eq!(loaded.format, ImageFormat::Raw);
        assert_eq!(loaded.width, 256);
        assert_eq!(loaded.height, 256);
    }

    #[test]
    fn tiff_based_raw_extensions_return_clear_error() {
        // Minimal TIFF header that is not Canon CR2 magic and has no JPEG.
        let bytes = b"II*\0\x08\0\0\0not-cr2";
        for ext in ["nef", "arw", "dng"] {
            let err = decode_bytes(bytes, bytes.len() as u64, Some(ext)).unwrap_err();
            assert!(
                matches!(err, ViewerError::UnsupportedRaw),
                "ext={ext} unexpected: {err:?}"
            );
        }
    }

    #[test]
    fn image_file_extension_list_covers_heic_and_raw() {
        assert!(is_image_file_extension("HEIC"));
        assert!(is_image_file_extension("nef"));
        assert!(is_image_file_extension("svg"));
        assert!(!is_image_file_extension("pdf"));
    }

    #[test]
    fn avif_ftyp_is_not_heic() {
        let mut v = vec![0, 0, 0, 0x18];
        v.extend_from_slice(b"ftyp");
        v.extend_from_slice(b"avif");
        v.extend_from_slice(&[0, 0, 0, 0]);
        v.extend_from_slice(b"avif");
        assert!(!looks_like_heic(&v));
        assert_eq!(sniff_unsupported_image(&v), None);
    }
}
