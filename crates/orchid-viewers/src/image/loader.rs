//! Image loading.
//!
//! Raster formats use the [`image`](image) crate's built-in decoders. SVG is
//! rasterized via [`resvg`]. On Windows, HEIC/HEIF is decoded through WIC when
//! the OS HEIF codec is installed; otherwise we return a clear unsupported
//! error. Camera RAW demosaics via `rawler` when the camera is known, and
//! otherwise shows the largest embedded JPEG preview.

use std::sync::Arc;

use crate::error::{Result, ViewerError};
use image::GenericImageView;

/// Extensions the image viewer / FM treat as images (including pending HEIC/RAW).
pub const IMAGE_FILE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "jpe", "webp", "bmp", "gif", "tiff", "tif", "avif", "tga", "svg", "heic",
    "heif", "jxl", "dng", "cr2", "cr3", "nef", "nrw", "arw", "raf", "orf", "pef", "rw2", "srw",
    "x3f", "rwl", "dcr", "kdc", "psd", "xcf", "pcx", "ico", "cur", "pbm", "pgm", "ppm", "pnm",
    "jp2", "j2k", "jpx", "jpf", "dds", "exr", "hdr", "rgbe", "svgz", "ai", "eps", "ps", "wmf",
    "emf", "cdr",
];

/// Decode a local image path (same pipeline as the viewer, including ICC).
///
/// # Errors
///
/// I/O or an unreadable file.
pub fn load_image_file(path: &std::path::Path) -> Result<LoadedImage> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    decode_local_mmap(path, crate::image::DEFAULT_SIZE_LIMIT, ext.as_deref())
}

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
    /// Source ICC / assumed profile label for the status strip.
    pub color_source: String,
    /// Destination profile label after color management.
    pub color_dest: String,
    /// EXIF orientation that was applied (`1` = none).
    pub orientation: u32,
    /// Bits per channel before conversion to 8-bit RGBA.
    pub bit_depth: u8,
    /// `RGB`, `RGBA`, `L`, …
    pub color_model: String,
}

impl LoadedImage {
    pub(crate) fn meta_defaults() -> Self {
        Self {
            rgba: Arc::new(Vec::new()),
            width: 0,
            height: 0,
            format: ImageFormat::Unknown,
            original_size_bytes: 0,
            color_source: String::new(),
            color_dest: String::new(),
            orientation: 1,
            bit_depth: 8,
            color_model: "RGB".into(),
        }
    }
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
    Ico,
    Pnm,
    Dds,
    Hdr,
    OpenExr,
    Jxl,
    Psd,
    Xcf,
    Pcx,
    Jpeg2000,
    Ai,
    Eps,
    Wmf,
    Emf,
    Cdr,
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
            Self::Ico => "ICO",
            Self::Pnm => "PNM",
            Self::Dds => "DDS",
            Self::Hdr => "HDR",
            Self::OpenExr => "EXR",
            Self::Jxl => "JXL",
            Self::Psd => "PSD",
            Self::Xcf => "XCF",
            Self::Pcx => "PCX",
            Self::Jpeg2000 => "JPEG 2000",
            Self::Ai => "AI",
            Self::Eps => "EPS",
            Self::Wmf => "WMF",
            Self::Emf => "EMF",
            Self::Cdr => "CDR",
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
            Ico => Self::Ico,
            Pnm => Self::Pnm,
            Dds => Self::Dds,
            Hdr => Self::Hdr,
            OpenExr => Self::OpenExr,
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

/// Load an image and, when the file is animated, every composited frame.
///
/// Folder thumbs and edit pipelines should keep using [`load_image`] so a
/// long GIF is not expanded just to draw a 64 px cell.
///
/// # Errors
///
/// Same as [`load_image`].
pub async fn load_viewer_image(
    path: &orchid_fs::FsPath,
    registry: Arc<orchid_fs::FsProviderRegistry>,
    size_limit_bytes: u64,
) -> Result<(LoadedImage, Option<Arc<crate::image::anim::AnimSequence>>)> {
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
            decode_local_mmap_viewer(&os_path, size_limit_bytes, ext.as_deref())
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
    tokio::task::spawn_blocking(move || decode_bytes_viewer(&bytes, size, ext.as_deref()))
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

fn decode_local_mmap_viewer(
    os_path: &std::path::Path,
    size_limit_bytes: u64,
    extension: Option<&str>,
) -> Result<(LoadedImage, Option<Arc<crate::image::anim::AnimSequence>>)> {
    let meta = std::fs::metadata(os_path).map_err(orchid_fs::FsError::Io)?;
    let size = meta.len();
    if size > size_limit_bytes {
        return Err(ViewerError::FileTooLarge {
            size,
            limit: size_limit_bytes,
        });
    }
    let file = std::fs::File::open(os_path).map_err(orchid_fs::FsError::Io)?;
    // SAFETY: same contract as [`decode_local_mmap`].
    let map = unsafe { memmap2::Mmap::map(&file) }.map_err(orchid_fs::FsError::Io)?;
    if map.len() as u64 > size_limit_bytes {
        return Err(ViewerError::FileTooLarge {
            size: map.len() as u64,
            limit: size_limit_bytes,
        });
    }
    decode_bytes_viewer(&map, size, extension)
}

fn decode_bytes_viewer(
    bytes: &[u8],
    size: u64,
    extension: Option<&str>,
) -> Result<(LoadedImage, Option<Arc<crate::image::anim::AnimSequence>>)> {
    if let Some(seq) = crate::image::anim::decode_animation(bytes) {
        let image = seq.first_loaded(size);
        return Ok((image, Some(Arc::new(seq))));
    }
    if let Some(seq) = crate::image::pages::decode_pages(bytes) {
        let image = seq.first_loaded(size);
        return Ok((image, Some(Arc::new(seq))));
    }
    Ok((decode_bytes(bytes, size, extension)?, None))
}

fn decode_bytes(bytes: &[u8], size: u64, extension: Option<&str>) -> Result<LoadedImage> {
    if matches!(extension, Some("svgz")) {
        return crate::image::vector::decode_svgz(bytes, size);
    }
    if looks_like_svg(bytes) {
        return decode_svg(bytes, size);
    }
    if crate::image::vector::looks_like_vector(bytes)
        || extension.is_some_and(crate::image::vector::is_vector_extension)
    {
        return crate::image::vector::decode(bytes, size, extension);
    }
    if looks_like_avif(bytes) || matches!(extension, Some("avif" | "avifs")) {
        return decode_avif(bytes, size);
    }
    if looks_like_heic(bytes) || matches!(extension, Some("heic" | "heif")) {
        return decode_heic(bytes, size);
    }
    if crate::image::extra::looks_like_jxl(bytes) || matches!(extension, Some("jxl")) {
        return crate::image::extra::decode_jxl(bytes, size);
    }
    if crate::image::extra::looks_like_psd(bytes) || matches!(extension, Some("psd")) {
        return crate::image::extra::decode_psd(bytes, size);
    }
    if crate::image::extra::looks_like_xcf(bytes) || matches!(extension, Some("xcf")) {
        return crate::image::extra::decode_xcf(bytes, size);
    }
    if crate::image::extra::looks_like_pcx(bytes) || matches!(extension, Some("pcx")) {
        return crate::image::extra::decode_pcx(bytes, size);
    }
    if crate::image::extra::looks_like_jp2(bytes)
        || matches!(extension, Some("jp2" | "j2k" | "jpx" | "jpf"))
    {
        return crate::image::extra::decode_jp2(bytes, size);
    }
    if looks_like_raw(bytes) || is_raw_extension(extension) {
        return crate::image::raw::decode_raw(
            bytes,
            size,
            crate::image::raw::RawDevelop::default(),
        );
    }
    let guessed = image::guess_format(bytes)
        .map(ImageFormat::from_image_crate)
        .unwrap_or(ImageFormat::Unknown);
    let img =
        image::load_from_memory(bytes).map_err(|e| ViewerError::ImageDecode(e.to_string()))?;
    Ok(finish_decoded(img, bytes, size, guessed))
}

fn finish_decoded(
    img: image::DynamicImage,
    file_bytes: &[u8],
    size: u64,
    format: ImageFormat,
) -> LoadedImage {
    let orientation = crate::image::exif::orientation_from_bytes(file_bytes);
    let (bit_depth, color_model) = crate::image::metadata::color_type_label(&img);
    let img = crate::image::exif::apply_orientation(img, orientation);
    let (w, h) = img.dimensions();
    let mut rgba = img.into_rgba8().into_raw();
    let color = crate::image::color::apply_embedded_icc(&mut rgba, file_bytes);
    LoadedImage {
        rgba: Arc::new(rgba),
        width: w,
        height: h,
        format,
        original_size_bytes: size,
        color_source: color.source_profile,
        color_dest: color.dest_profile,
        orientation,
        bit_depth,
        color_model: color_model.to_string(),
    }
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

fn decode_avif(bytes: &[u8], size: u64) -> Result<LoadedImage> {
    #[cfg(windows)]
    {
        crate::image::heic_wic::decode_wic(bytes, size, ImageFormat::Avif)
    }
    #[cfg(not(windows))]
    {
        let _ = (bytes, size);
        Err(ViewerError::UnsupportedAvif)
    }
}

fn is_raw_extension(extension: Option<&str>) -> bool {
    extension.is_some_and(crate::image::raw::is_raw_file_extension)
}

/// True when `bytes` look like an SVG document (XML sniff).
#[must_use]
pub fn looks_like_svg(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    let head: String = trimmed
        .chars()
        .take(512)
        .collect::<String>()
        .to_ascii_lowercase();
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
    sniff_ftyp(bytes) == Some(ImageFormat::Heic)
}

/// True when `bytes` look like AVIF (`ftyp` brand `avif` / `avis`).
#[must_use]
pub fn looks_like_avif(bytes: &[u8]) -> bool {
    sniff_ftyp(bytes) == Some(ImageFormat::Avif)
}

/// True when `bytes` match a common camera-RAW magic sequence.
#[must_use]
pub fn looks_like_raw(bytes: &[u8]) -> bool {
    crate::image::raw::looks_like_raw(bytes)
}

/// Sniff HEIC/HEIF, AVIF, or common RAW containers. Returns `None` when unknown.
#[must_use]
pub fn sniff_unsupported_image(bytes: &[u8]) -> Option<ImageFormat> {
    if let Some(fmt) = sniff_ftyp(bytes) {
        return Some(fmt);
    }
    if crate::image::raw::looks_like_raw(bytes) {
        return Some(ImageFormat::Raw);
    }
    None
}

fn sniff_ftyp(bytes: &[u8]) -> Option<ImageFormat> {
    if bytes.len() < 12 || &bytes[4..8] != b"ftyp" {
        return None;
    }
    const AVIF: &[&[u8]] = &[b"avif", b"avis", b"avia"];
    const HEIF: &[&[u8]] = &[
        b"heic", b"heif", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"hevm", b"hevs",
    ];
    let mut saw_heif_generic = false;
    let mut offset = 8;
    while offset + 4 <= bytes.len() {
        let brand = &bytes[offset..offset + 4];
        if AVIF.contains(&brand) {
            return Some(ImageFormat::Avif);
        }
        if HEIF.contains(&brand) {
            return Some(ImageFormat::Heic);
        }
        if brand == b"mif1" || brand == b"msf1" {
            saw_heif_generic = true;
        }
        offset = if offset == 8 { 16 } else { offset + 4 };
        if offset > 64 {
            break;
        }
    }
    if saw_heif_generic {
        Some(ImageFormat::Heic)
    } else {
        None
    }
}

pub(crate) fn decode_svg_bytes(bytes: &[u8], size: u64) -> Result<LoadedImage> {
    decode_svg(bytes, size)
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
        ViewerError::ImageDecode(format!(
            "failed to allocate {width}×{height} pixmap for SVG"
        ))
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
        ..LoadedImage::meta_defaults()
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
        assert_eq!(sniff_unsupported_image(b"IIROxxxx"), Some(ImageFormat::Raw));
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

        let found = crate::image::raw::largest_embedded_jpeg(&container).unwrap();
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

        let loaded = crate::image::raw::decode_raw(
            &container,
            container.len() as u64,
            crate::image::raw::RawDevelop::default(),
        )
        .unwrap();
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
        assert!(is_image_file_extension("cr3"));
        assert!(is_image_file_extension("pef"));
        assert!(is_image_file_extension("x3f"));
        assert!(is_image_file_extension("svg"));
        assert!(is_image_file_extension("ai"));
        assert!(is_image_file_extension("eps"));
        assert!(is_image_file_extension("cdr"));
        assert!(!is_image_file_extension("pdf"));
    }

    #[test]
    fn avif_ftyp_is_not_heic() {
        let mut v = vec![0, 0, 0, 0x18];
        v.extend_from_slice(b"ftyp");
        v.extend_from_slice(b"avif");
        v.extend_from_slice(&[0, 0, 0, 0]);
        v.extend_from_slice(b"mif1");
        assert!(!looks_like_heic(&v));
        assert!(looks_like_avif(&v));
        assert_eq!(sniff_unsupported_image(&v), Some(ImageFormat::Avif));
    }

    fn encode_rgb(fmt: image::ImageFormat, w: u32, h: u32) -> Vec<u8> {
        let mut img = image::RgbImage::new(w, h);
        img.put_pixel(0, 0, image::Rgb([200, 10, 30]));
        if w > 1 && h > 1 {
            img.put_pixel(w - 1, h - 1, image::Rgb([10, 200, 40]));
        }
        let mut cursor = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut cursor, fmt)
            .unwrap();
        cursor.into_inner()
    }

    fn encode_rgba(fmt: image::ImageFormat, w: u32, h: u32) -> Vec<u8> {
        let mut img = image::RgbaImage::new(w, h);
        img.put_pixel(0, 0, image::Rgba([200, 10, 30, 255]));
        if w > 1 && h > 1 {
            img.put_pixel(w - 1, h - 1, image::Rgba([10, 200, 40, 255]));
        }
        let mut cursor = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut cursor, fmt)
            .unwrap();
        cursor.into_inner()
    }

    #[test]
    fn decodes_ico_pnm_hdr_exr() {
        let ico = encode_rgba(image::ImageFormat::Ico, 16, 16);
        let loaded = decode_bytes(&ico, ico.len() as u64, Some("ico")).unwrap();
        assert_eq!(loaded.format, ImageFormat::Ico);
        assert_eq!(loaded.width, 16);
        let pnm = encode_rgb(image::ImageFormat::Pnm, 8, 8);
        let loaded = decode_bytes(&pnm, pnm.len() as u64, Some("ppm")).unwrap();
        assert_eq!(loaded.format, ImageFormat::Pnm);
        assert_eq!(loaded.width, 8);
        for (fmt, ext, label) in [
            (image::ImageFormat::Hdr, "hdr", ImageFormat::Hdr),
            (image::ImageFormat::OpenExr, "exr", ImageFormat::OpenExr),
        ] {
            let mut img = image::Rgb32FImage::new(4, 4);
            img.put_pixel(0, 0, image::Rgb([1.5, 0.2, 0.1]));
            let mut cursor = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb32F(img)
                .write_to(&mut cursor, fmt)
                .unwrap();
            let bytes = cursor.into_inner();
            let loaded = decode_bytes(&bytes, bytes.len() as u64, Some(ext)).unwrap();
            assert_eq!(loaded.format, label, "{ext}");
            assert_eq!(loaded.width, 4);
        }
    }

    #[test]
    fn extra_extensions_are_images() {
        for ext in [
            "jxl", "psd", "xcf", "pcx", "ico", "cur", "jp2", "dds", "exr", "hdr", "ai", "eps",
            "wmf", "emf", "cdr", "svgz",
        ] {
            assert!(is_image_file_extension(ext), "{ext}");
        }
    }
}
