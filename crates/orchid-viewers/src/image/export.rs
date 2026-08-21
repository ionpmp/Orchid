//! Save-as / export with quality, ICO / favicon, screenshot, wallpaper, mail.

use crate::error::{Result, ViewerError};
use crate::image::loader::{load_image_file, ImageFormat, LoadedImage};
use crate::image::operations::{resize_filtered, ResizeFilter};

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{DynamicImage, ExtendedColorType, ImageEncoder, ImageFormat as ImgFmt, RgbImage};

/// Target container for [`export_file`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// JPEG.
    Jpeg,
    /// PNG.
    Png,
    /// WebP.
    WebP,
    /// BMP.
    Bmp,
    /// Multi-size ICO (16 / 32 / 48).
    Ico,
    /// Favicon ICO plus a 32×32 PNG sibling.
    Favicon,
}

impl ExportFormat {
    fn parse(raw: &str) -> Option<Self> {
        match raw
            .trim()
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str()
        {
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "png" => Some(Self::Png),
            "webp" => Some(Self::WebP),
            "bmp" => Some(Self::Bmp),
            "ico" => Some(Self::Ico),
            "favicon" | "fav" => Some(Self::Favicon),
            _ => None,
        }
    }

    /// File extension without a dot.
    #[must_use]
    pub fn ext(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::WebP => "webp",
            Self::Bmp => "bmp",
            Self::Ico | Self::Favicon => "ico",
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::Favicon => "favicon",
            Self::Ico => "ico",
            _ => "export",
        }
    }
}

/// Packed export job: format, quality, optional downscale.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportSpec {
    /// Output format.
    pub format: ExportFormat,
    /// JPEG / WebP quality 1–100.
    pub quality: u8,
    /// PNG zlib level 0–9 (mapped to Fast / Default / Best).
    pub png_level: u8,
    /// Shrink so the long edge is at most this many pixels.
    pub max_edge: Option<u32>,
}

impl Default for ExportSpec {
    fn default() -> Self {
        Self {
            format: ExportFormat::Jpeg,
            quality: 85,
            png_level: 6,
            max_edge: None,
        }
    }
}

/// `jpg | q=85 | max=1920`, `png | level=6`, `ico`, `favicon`.
#[must_use]
pub fn parse_export_line(raw: &str) -> Option<ExportSpec> {
    let mut spec = ExportSpec::default();
    let mut saw_format = false;
    for part in raw.split('|') {
        let token = part.trim();
        if token.is_empty() {
            continue;
        }
        if let Some(fmt) = ExportFormat::parse(token) {
            spec.format = fmt;
            saw_format = true;
            continue;
        }
        if let Some((k, v)) = token.split_once('=') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim();
            match key.as_str() {
                "q" | "quality" => {
                    spec.quality = val.parse().ok()?;
                    spec.quality = spec.quality.clamp(1, 100);
                }
                "level" | "compression" | "png" => {
                    spec.png_level = val.parse().ok()?;
                    spec.png_level = spec.png_level.min(9);
                }
                "max" | "max_edge" | "edge" => {
                    let n: u32 = val.parse().ok()?;
                    spec.max_edge = Some(n.clamp(16, 16_384));
                }
                _ => return None,
            }
            continue;
        }
        return None;
    }
    if !saw_format {
        return None;
    }
    Some(spec)
}

/// Write a sibling in the requested format. Never overwrites the source.
///
/// # Errors
///
/// I/O or encode.
pub fn export_file(path: &Path, spec: &ExportSpec) -> Result<PathBuf> {
    let src = load_image_file(path)?;
    export_loaded(path, &src, spec)
}

/// Encode an already-decoded image next to `hint`.
///
/// # Errors
///
/// I/O or encode.
pub fn export_loaded(hint: &Path, img: &LoadedImage, spec: &ExportSpec) -> Result<PathBuf> {
    let prepared = prepare_image(img, spec.max_edge)?;
    match spec.format {
        ExportFormat::Ico => write_ico(hint, &prepared, false),
        ExportFormat::Favicon => write_ico(hint, &prepared, true),
        other => {
            let dest = unique_dest(hint, other.suffix(), other.ext());
            let bytes = encode_with_options(&prepared, spec)?;
            std::fs::write(&dest, bytes)?;
            Ok(dest)
        }
    }
}

/// Resized JPEG sibling for email (default long edge 1920, quality 80).
///
/// # Errors
///
/// I/O or encode.
pub fn prepare_mail_attachment(path: &Path, max_edge: u32) -> Result<PathBuf> {
    let spec = ExportSpec {
        format: ExportFormat::Jpeg,
        quality: 80,
        png_level: 6,
        max_edge: Some(max_edge.clamp(320, 4096)),
    };
    let src = load_image_file(path)?;
    let prepared = prepare_image(&src, spec.max_edge)?;
    let dest = unique_dest(path, "mail", "jpg");
    let bytes = encode_with_options(&prepared, &spec)?;
    std::fs::write(&dest, bytes)?;
    Ok(dest)
}

/// Write a `.eml` with the JPEG attached so the default mail client can open it.
///
/// # Errors
///
/// I/O.
pub fn write_mail_eml(attachment: &Path) -> Result<PathBuf> {
    let bytes = std::fs::read(attachment).map_err(|e| ViewerError::ImageDecode(e.to_string()))?;
    let name = attachment
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("image.jpg");
    let b64 = encode_base64(&bytes);
    let body = format!(
        "MIME-Version: 1.0\r\n\
         Subject: {name}\r\n\
         Content-Type: multipart/mixed; boundary=\"orchid\"\r\n\
         \r\n\
         --orchid\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\
         \r\n\
         \r\n\
         --orchid\r\n\
         Content-Type: image/jpeg; name=\"{name}\"\r\n\
         Content-Transfer-Encoding: base64\r\n\
         Content-Disposition: attachment; filename=\"{name}\"\r\n\
         \r\n\
         {b64}\r\n\
         --orchid--\r\n"
    );
    let dest = unique_dest(attachment, "mail", "eml");
    std::fs::write(&dest, body).map_err(|e| ViewerError::ImageDecode(e.to_string()))?;
    Ok(dest)
}

/// Social compose URL. Local files cannot be uploaded; the caller copies pixels.
#[must_use]
pub fn share_intent_url(network: &str, label: &str) -> Option<String> {
    let text = urlencoding_lite(label);
    match network.trim().to_ascii_lowercase().as_str() {
        "twitter" | "x" => Some(format!("https://twitter.com/intent/tweet?text={text}")),
        "facebook" | "fb" => Some("https://www.facebook.com/".into()),
        "vk" | "vkontakte" => Some(format!("https://vk.com/share.php?title={text}")),
        _ => None,
    }
}

/// What to grab for [`capture_screenshot`].
#[derive(Debug, Clone, PartialEq)]
pub enum ScreenshotKind {
    /// Virtual desktop.
    Screen,
    /// Foreground window.
    Window,
    /// Absolute screen rectangle.
    Region {
        /// Left.
        x: i32,
        /// Top.
        y: i32,
        /// Width.
        w: u32,
        /// Height.
        h: u32,
    },
}

impl ScreenshotKind {
    fn parse_token(raw: &str) -> Option<Self> {
        let t = raw.trim().to_ascii_lowercase();
        if t == "screen" || t == "full" || t == "desktop" {
            return Some(Self::Screen);
        }
        if t == "window" || t == "win" {
            return Some(Self::Window);
        }
        if let Some(rest) = t.strip_prefix("region=") {
            let mut it = rest.split(',');
            let x = it.next()?.trim().parse().ok()?;
            let y = it.next()?.trim().parse().ok()?;
            let w = it.next()?.trim().parse().ok()?;
            let h = it.next()?.trim().parse().ok()?;
            if w == 0 || h == 0 {
                return None;
            }
            return Some(Self::Region { x, y, w, h });
        }
        None
    }
}

/// `screen | delay=3`, `window`, `region=100,80,800,600`.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenshotSpec {
    /// Capture target.
    pub kind: ScreenshotKind,
    /// Seconds to wait before grabbing (window / compose).
    pub delay_secs: u32,
}

impl Default for ScreenshotSpec {
    fn default() -> Self {
        Self {
            kind: ScreenshotKind::Screen,
            delay_secs: 0,
        }
    }
}

/// Parse a screenshot line. Empty → full screen, no delay.
#[must_use]
pub fn parse_screenshot_line(raw: &str) -> Option<ScreenshotSpec> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Some(ScreenshotSpec::default());
    }
    let mut spec = ScreenshotSpec::default();
    let mut saw_kind = false;
    for part in trimmed.split('|') {
        let token = part.trim();
        if token.is_empty() {
            continue;
        }
        if let Some(kind) = ScreenshotKind::parse_token(token) {
            spec.kind = kind;
            saw_kind = true;
            continue;
        }
        if let Some((k, v)) = token.split_once('=') {
            if k.trim().eq_ignore_ascii_case("delay") {
                spec.delay_secs = v.trim().parse().ok()?;
                spec.delay_secs = spec.delay_secs.min(30);
                continue;
            }
        }
        return None;
    }
    if !saw_kind && spec.delay_secs == 0 {
        return None;
    }
    Some(spec)
}

/// Capture the screen / window / region after an optional delay.
///
/// # Errors
///
/// Unsupported platform or GDI failure.
pub fn capture_screenshot(spec: &ScreenshotSpec) -> Result<LoadedImage> {
    if spec.delay_secs > 0 {
        std::thread::sleep(Duration::from_secs(u64::from(spec.delay_secs)));
    }
    capture_now(&spec.kind)
}

/// Write a timestamped PNG under `dir`.
///
/// # Errors
///
/// Capture or I/O.
pub fn write_screenshot(dir: &Path, spec: &ScreenshotSpec) -> Result<PathBuf> {
    let img = capture_screenshot(spec)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dest = unique_named(dir, &format!("orchid-shot-{stamp}"), "png");
    let bytes = encode_png(&img)?;
    std::fs::write(&dest, bytes).map_err(|e| ViewerError::ImageDecode(e.to_string()))?;
    Ok(dest)
}

/// Set the Windows desktop wallpaper. Other platforms return an error.
///
/// # Errors
///
/// I/O or the system call.
pub fn set_wallpaper(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        set_wallpaper_windows(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(ViewerError::ImageDecode(
            "set as wallpaper is only available on Windows".into(),
        ))
    }
}

/// Build a [`LoadedImage`] from raw RGBA (clipboard paste).
///
/// # Errors
///
/// Buffer size mismatch.
pub fn loaded_from_rgba(rgba: Vec<u8>, width: u32, height: u32) -> Result<LoadedImage> {
    let expect = width as usize * height as usize * 4;
    if rgba.len() != expect {
        return Err(ViewerError::ImageDecode("clipboard RGBA size".into()));
    }
    Ok(LoadedImage {
        rgba: Arc::new(rgba),
        width,
        height,
        format: ImageFormat::Png,
        original_size_bytes: expect as u64,
        color_source: "sRGB".into(),
        color_dest: "sRGB".into(),
        orientation: 1,
        bit_depth: 8,
        color_model: "RGBA".into(),
    })
}

/// PNG bytes for clipboard / ICO frames.
///
/// # Errors
///
/// Encode.
pub fn encode_png(img: &LoadedImage) -> Result<Vec<u8>> {
    let spec = ExportSpec {
        format: ExportFormat::Png,
        quality: 85,
        png_level: 6,
        max_edge: None,
    };
    encode_with_options(img, &spec)
}

/// Unique sibling path (`stem-suffix.ext`, then `-2`, `-3`, …).
#[must_use]
pub fn unique_export_dest(src: &Path, suffix: &str, ext: &str) -> PathBuf {
    unique_dest(src, suffix, ext)
}

fn prepare_image(img: &LoadedImage, max_edge: Option<u32>) -> Result<LoadedImage> {
    let Some(edge) = max_edge else {
        return Ok(img.clone());
    };
    let long = img.width.max(img.height);
    if long <= edge {
        return Ok(img.clone());
    }
    let scale = edge as f32 / long as f32;
    let tw = (img.width as f32 * scale).round().max(1.0) as u32;
    let th = (img.height as f32 * scale).round().max(1.0) as u32;
    resize_filtered(img, tw, th, ResizeFilter::Lanczos)
}

fn encode_with_options(img: &LoadedImage, spec: &ExportSpec) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    match spec.format {
        ExportFormat::Jpeg => {
            let rgb = rgba_to_rgb(img);
            let mut enc = JpegEncoder::new_with_quality(&mut buf, spec.quality);
            enc.encode(rgb.as_raw(), img.width, img.height, ExtendedColorType::Rgb8)
                .map_err(|e| ViewerError::ImageDecode(e.to_string()))?;
        }
        ExportFormat::Png => {
            let compression = match spec.png_level {
                0..=2 => CompressionType::Fast,
                7..=9 => CompressionType::Best,
                _ => CompressionType::Default,
            };
            let enc = PngEncoder::new_with_quality(&mut buf, compression, FilterType::Adaptive);
            enc.write_image(&img.rgba, img.width, img.height, ExtendedColorType::Rgba8)
                .map_err(|e| ViewerError::ImageDecode(e.to_string()))?;
        }
        ExportFormat::WebP | ExportFormat::Bmp => {
            let fmt = if spec.format == ExportFormat::WebP {
                ImgFmt::WebP
            } else {
                ImgFmt::Bmp
            };
            let rgba = image::RgbaImage::from_raw(img.width, img.height, img.rgba.to_vec())
                .ok_or_else(|| ViewerError::ImageDecode("encode buffer".into()))?;
            DynamicImage::ImageRgba8(rgba)
                .write_to(&mut Cursor::new(&mut buf), fmt)
                .map_err(|e| ViewerError::ImageDecode(e.to_string()))?;
        }
        ExportFormat::Ico | ExportFormat::Favicon => {
            return Err(ViewerError::ImageDecode("use write_ico".into()));
        }
    }
    Ok(buf)
}

fn write_ico(hint: &Path, img: &LoadedImage, favicon: bool) -> Result<PathBuf> {
    let sizes = [16u32, 32, 48];
    let mut frames = Vec::new();
    for size in sizes {
        let frame = resize_filtered(img, size, size, ResizeFilter::Lanczos)?;
        let png = encode_png(&frame)?;
        frames.push((size, png));
    }
    let bytes = pack_ico(&frames);
    let dest = unique_dest(hint, if favicon { "favicon" } else { "ico" }, "ico");
    std::fs::write(&dest, bytes).map_err(|e| ViewerError::ImageDecode(e.to_string()))?;
    if favicon {
        let png32 = frames
            .iter()
            .find(|(s, _)| *s == 32)
            .map(|(_, b)| b.clone())
            .unwrap_or_default();
        let png_dest = unique_dest(hint, "favicon-32", "png");
        std::fs::write(&png_dest, png32).map_err(|e| ViewerError::ImageDecode(e.to_string()))?;
    }
    Ok(dest)
}

fn pack_ico(frames: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let count = frames.len() as u16;
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    let mut offset = 6u32 + 16 * u32::from(count);
    for (size, png) in frames {
        let dim = if *size >= 256 { 0u8 } else { *size as u8 };
        out.push(dim);
        out.push(dim);
        out.push(0);
        out.push(0);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in frames {
        out.extend_from_slice(png);
    }
    out
}

fn unique_dest(src: &Path, suffix: &str, ext: &str) -> PathBuf {
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    let dir = src.parent().unwrap_or_else(|| Path::new("."));
    unique_named(dir, &format!("{stem}-{suffix}"), ext)
}

fn unique_named(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut dest = dir.join(format!("{stem}.{ext}"));
    let mut n = 2u32;
    while dest.exists() {
        dest = dir.join(format!("{stem}-{n}.{ext}"));
        n += 1;
    }
    dest
}

fn rgba_to_rgb(img: &LoadedImage) -> RgbImage {
    let mut out = Vec::with_capacity(img.width as usize * img.height as usize * 3);
    for px in img.rgba.chunks_exact(4) {
        out.extend_from_slice(&px[..3]);
    }
    RgbImage::from_raw(img.width, img.height, out)
        .unwrap_or_else(|| RgbImage::new(img.width, img.height))
}

fn encode_base64(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        let b1 = bytes.get(i + 1).copied();
        let b2 = bytes.get(i + 2).copied();
        out.push(T[(b0 >> 2) as usize] as char);
        out.push(T[(((b0 & 3) << 4) | (b1.unwrap_or(0) >> 4)) as usize] as char);
        match (b1, b2) {
            (Some(b1), Some(b2)) => {
                out.push(T[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
                out.push(T[(b2 & 0x3f) as usize] as char);
            }
            (Some(b1), None) => {
                out.push(T[((b1 & 0x0f) << 2) as usize] as char);
                out.push('=');
            }
            (None, _) => {
                out.push('=');
                out.push('=');
            }
        }
        i += 3;
        if out.len().is_multiple_of(76) {
            out.push_str("\r\n");
        }
    }
    out
}

fn urlencoding_lite(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn capture_now(kind: &ScreenshotKind) -> Result<LoadedImage> {
    #[cfg(windows)]
    {
        capture_windows(kind)
    }
    #[cfg(not(windows))]
    {
        let _ = kind;
        Err(ViewerError::ImageDecode(
            "screenshot capture is only available on Windows".into(),
        ))
    }
}

#[cfg(windows)]
fn set_wallpaper_windows(path: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;

    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_SETDESKWALLPAPER,
    };

    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut wide: Vec<u16> = abs.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        SystemParametersInfoW(
            SPI_SETDESKWALLPAPER,
            0,
            Some(wide.as_mut_ptr().cast()),
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        )
        .map_err(|e| ViewerError::ImageDecode(e.to_string()))
    }
}

#[cfg(windows)]
fn capture_windows(kind: &ScreenshotKind) -> Result<LoadedImage> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
        SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetSystemMetrics, GetWindowRect, SM_CXVIRTUALSCREEN,
        SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    };

    let (x, y, w, h) = match kind {
        ScreenshotKind::Screen => unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            )
        },
        ScreenshotKind::Window => unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_invalid() {
                return Err(ViewerError::ImageDecode("no foreground window".into()));
            }
            let mut rc = RECT::default();
            GetWindowRect(hwnd, &mut rc).map_err(|e| ViewerError::ImageDecode(e.to_string()))?;
            (rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top)
        },
        ScreenshotKind::Region { x, y, w, h } => (*x, *y, *w as i32, *h as i32),
    };
    if w <= 0 || h <= 0 {
        return Err(ViewerError::ImageDecode("empty screenshot region".into()));
    }

    unsafe {
        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            return Err(ViewerError::ImageDecode("GetDC failed".into()));
        }
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        if mem_dc.is_invalid() {
            let _ = ReleaseDC(None, screen_dc);
            return Err(ViewerError::ImageDecode("CreateCompatibleDC failed".into()));
        }
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as _,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let dib = match CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(h) => h,
            Err(e) => {
                let _ = DeleteDC(mem_dc);
                let _ = ReleaseDC(None, screen_dc);
                return Err(ViewerError::ImageDecode(e.to_string()));
            }
        };
        if bits.is_null() {
            let _ = DeleteObject(dib.into());
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);
            return Err(ViewerError::ImageDecode("CreateDIBSection bits".into()));
        }
        let prev = SelectObject(mem_dc, dib.into());
        let blit = BitBlt(mem_dc, 0, 0, w, h, Some(screen_dc), x, y, SRCCOPY);
        let n = (w as usize) * (h as usize) * 4;
        let bgra = std::slice::from_raw_parts(bits.cast::<u8>(), n);
        let rgba = bgra_to_rgba(bgra);
        let _ = SelectObject(mem_dc, prev);
        let _ = DeleteObject(dib.into());
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, screen_dc);
        blit.map_err(|e| ViewerError::ImageDecode(e.to_string()))?;
        loaded_from_rgba(rgba, w as u32, h as u32)
    }
}

#[cfg(windows)]
fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(bgra.len());
    for chunk in bgra.chunks_exact(4) {
        rgba.extend_from_slice(&[chunk[2], chunk[1], chunk[0], 255]);
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn solid(w: u32, h: u32, rgb: [u8; 3]) -> LoadedImage {
        let mut buf = RgbaImage::new(w, h);
        for p in buf.pixels_mut() {
            *p = Rgba([rgb[0], rgb[1], rgb[2], 255]);
        }
        loaded_from_rgba(buf.into_raw(), w, h).unwrap()
    }

    #[test]
    fn parse_export_quality_and_max() {
        let spec = parse_export_line("jpg | q=40 | max=800").unwrap();
        assert_eq!(spec.format, ExportFormat::Jpeg);
        assert_eq!(spec.quality, 40);
        assert_eq!(spec.max_edge, Some(800));
        let png = parse_export_line("png | level=9").unwrap();
        assert_eq!(png.format, ExportFormat::Png);
        assert_eq!(png.png_level, 9);
        assert!(parse_export_line("q=85").is_none());
        assert_eq!(
            parse_export_line("favicon").unwrap().format,
            ExportFormat::Favicon
        );
    }

    #[test]
    fn parse_screenshot_tokens() {
        let s = parse_screenshot_line("window | delay=3").unwrap();
        assert_eq!(s.kind, ScreenshotKind::Window);
        assert_eq!(s.delay_secs, 3);
        let r = parse_screenshot_line("region=10,20,100,80").unwrap();
        assert_eq!(
            r.kind,
            ScreenshotKind::Region {
                x: 10,
                y: 20,
                w: 100,
                h: 80
            }
        );
        assert!(parse_screenshot_line("nope").is_none());
        assert_eq!(
            parse_screenshot_line("").unwrap().kind,
            ScreenshotKind::Screen
        );
    }

    #[test]
    fn jpeg_quality_changes_size() {
        let img = solid(64, 48, [210, 40, 40]);
        let hi = encode_with_options(
            &img,
            &ExportSpec {
                format: ExportFormat::Jpeg,
                quality: 95,
                ..ExportSpec::default()
            },
        )
        .unwrap();
        let lo = encode_with_options(
            &img,
            &ExportSpec {
                format: ExportFormat::Jpeg,
                quality: 20,
                ..ExportSpec::default()
            },
        )
        .unwrap();
        assert!(lo.len() < hi.len(), "{} !< {}", lo.len(), hi.len());
    }

    #[test]
    fn ico_and_favicon_write_siblings() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("photo.png");
        let img = solid(48, 48, [10, 180, 40]);
        let png = encode_png(&img).unwrap();
        std::fs::write(&src, png).unwrap();
        let ico = export_file(
            &src,
            &ExportSpec {
                format: ExportFormat::Ico,
                ..ExportSpec::default()
            },
        )
        .unwrap();
        assert!(ico.extension().and_then(|e| e.to_str()) == Some("ico"));
        assert!(std::fs::metadata(&ico).unwrap().len() > 64);
        let fav = export_file(
            &src,
            &ExportSpec {
                format: ExportFormat::Favicon,
                ..ExportSpec::default()
            },
        )
        .unwrap();
        assert!(fav.to_string_lossy().contains("favicon"));
        let png32 = dir.path().join("photo-favicon-32.png");
        assert!(png32.exists());
    }

    #[test]
    fn mail_attachment_is_jpeg() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("wide.png");
        let img = solid(80, 40, [20, 20, 200]);
        std::fs::write(&src, encode_png(&img).unwrap()).unwrap();
        let dest = prepare_mail_attachment(&src, 40).unwrap();
        assert_eq!(dest.extension().and_then(|e| e.to_str()), Some("jpg"));
        let eml = write_mail_eml(&dest).unwrap();
        let text = std::fs::read_to_string(&eml).unwrap();
        assert!(text.contains("Content-Disposition: attachment"));
    }

    #[test]
    fn share_urls() {
        assert!(share_intent_url("twitter", "hi").unwrap().contains("tweet"));
        assert!(share_intent_url("system", "x").is_none());
    }

    #[cfg(windows)]
    #[test]
    fn screenshot_screen_has_pixels() {
        let img = capture_screenshot(&ScreenshotSpec::default()).unwrap();
        assert!(img.width > 0 && img.height > 0);
        assert_eq!(img.rgba.len(), img.width as usize * img.height as usize * 4);
    }
}
