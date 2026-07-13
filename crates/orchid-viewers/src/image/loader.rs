//! Image loading.
//!
//! Raster formats use the [`image`](image) crate's built-in decoders. SVG is
//! rasterized via [`resvg`]. On Windows, HEIC/HEIF is decoded through WIC when
//! the OS HEIF codec is installed; otherwise (and for RAW) we return a clear
//! unsupported error.

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
    let ext = path
        .extension()
        .map(|e| e.to_ascii_lowercase());
    tokio::task::spawn_blocking(move || decode_bytes(&bytes, size, ext.as_deref()))
        .await
        .map_err(|e| ViewerError::ImageDecode(e.to_string()))?
}

fn decode_bytes(bytes: &[u8], size: u64, extension: Option<&str>) -> Result<LoadedImage> {
    if looks_like_svg(bytes) {
        return decode_svg(bytes, size);
    }
    if looks_like_heic(bytes) || matches!(extension, Some("heic" | "heif")) {
        return decode_heic(bytes, size);
    }
    if looks_like_raw(bytes) {
        return Err(ViewerError::UnsupportedRaw);
    }
    // TIFF-based camera RAW (NEF/ARW/DNG) often lacks a unique magic; use the
    // extension so users get the dedicated unsupported message, not a generic
    // decode failure from the `image` crate.
    if let Some(err) = unsupported_by_extension(extension) {
        return Err(err);
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

fn decode_heic(bytes: &[u8], size: u64) -> Result<LoadedImage> {
    #[cfg(windows)]
    {
        return crate::image::heic_wic::decode_heic_wic(bytes, size);
    }
    #[cfg(not(windows))]
    {
        let _ = (bytes, size);
        Err(ViewerError::UnsupportedHeic)
    }
}

fn unsupported_by_extension(extension: Option<&str>) -> Option<ViewerError> {
    match extension? {
        "cr2" | "nef" | "arw" | "dng" | "raf" | "orf" | "rw2" => Some(ViewerError::UnsupportedRaw),
        _ => None,
    }
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

        let loaded = decode_bytes(&bytes, bytes.len() as u64, None).unwrap();
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
    fn tiff_based_raw_extensions_return_clear_error() {
        // Minimal TIFF header that is not Canon CR2 magic.
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
