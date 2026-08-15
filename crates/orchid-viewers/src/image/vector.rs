//! Vector formats the raster crate does not cover: AI, EPS/PS, WMF/EMF, CDR.
//!
//! SVG stays in [`super::loader`] (`resvg`). SVGZ is inflated here then handed
//! back to that path. PDF-based Illustrator files reuse Pdfium. EPS/PS open
//! via an embedded TIFF/WMF/EPSI preview or Ghostscript when installed.
//! WMF/EMF play through GDI on Windows. CorelDRAW opens an embedded preview.

use std::io::{Cursor, Read};

use crate::error::{Result, ViewerError};
use crate::image::loader::{ImageFormat, LoadedImage};

const MAX_RASTER_EDGE: u32 = 2048;
const MAX_SVGZ_UNCOMPRESSED: usize = 32 * 1024 * 1024;

/// True when `bytes` match EPS, WMF, EMF, or RIFF CDR (not ZIP CDR / PDF AI).
#[must_use]
pub fn looks_like_vector(bytes: &[u8]) -> bool {
    looks_like_eps(bytes)
        || looks_like_emf(bytes)
        || looks_like_wmf(bytes)
        || looks_like_cdr_riff(bytes)
}

/// DOS EPS binary (`C5 D0 D3 C6`) or `%!PS` / `%!`.
#[must_use]
pub fn looks_like_eps(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xC5, 0xD0, 0xD3, 0xC6])
        || bytes.starts_with(b"%!PS")
        || bytes.starts_with(b"%!\n")
        || bytes.starts_with(b"%!\r")
}

/// EMF: `EMR_HEADER` + signature `" EMF"` at offset 40.
#[must_use]
pub fn looks_like_emf(bytes: &[u8]) -> bool {
    bytes.len() >= 44 && bytes[0..4] == [0x01, 0x00, 0x00, 0x00] && &bytes[40..44] == b" EMF"
}

/// Placeable WMF (`D7 CD C6 9A`).
#[must_use]
pub fn looks_like_wmf(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xD7, 0xCD, 0xC6, 0x9A])
}

/// Legacy CorelDRAW RIFF (`RIFF….CDR*` / `cdr*`).
#[must_use]
pub fn looks_like_cdr_riff(bytes: &[u8]) -> bool {
    bytes.len() >= 12
        && bytes.starts_with(b"RIFF")
        && (bytes[8..11] == *b"CDR" || bytes[8..11] == *b"cdr")
}

/// Gzip container used by SVGZ.
#[must_use]
pub fn looks_like_svgz(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x1F, 0x8B])
}

/// Extensions that must open in the image viewer (not PDF / archive).
#[must_use]
pub fn is_vector_extension(ext: &str) -> bool {
    matches!(ext, "ai" | "eps" | "ps" | "wmf" | "emf" | "cdr" | "svgz")
}

pub(crate) fn decode(bytes: &[u8], size: u64, extension: Option<&str>) -> Result<LoadedImage> {
    match extension {
        Some("svgz") => decode_svgz(bytes, size),
        Some("ai") => decode_ai(bytes, size),
        Some("eps" | "ps") => decode_eps(bytes, size, ImageFormat::Eps),
        Some("wmf") => decode_wmf(bytes, size),
        Some("emf") => decode_emf(bytes, size),
        Some("cdr") => decode_cdr(bytes, size),
        _ if looks_like_eps(bytes) => decode_eps(bytes, size, ImageFormat::Eps),
        _ if looks_like_emf(bytes) => decode_emf(bytes, size),
        _ if looks_like_wmf(bytes) => decode_wmf(bytes, size),
        _ if looks_like_cdr_riff(bytes) => decode_cdr(bytes, size),
        _ => Err(ViewerError::ImageDecode("unrecognised vector image".into())),
    }
}

pub(crate) fn decode_svgz(bytes: &[u8], size: u64) -> Result<LoadedImage> {
    let plain = gunzip_limited(bytes)?;
    crate::image::loader::decode_svg_bytes(&plain, size)
}

fn decode_ai(bytes: &[u8], size: u64) -> Result<LoadedImage> {
    if let Some(pdf) = find_pdf_payload(bytes) {
        return rasterize_pdf(pdf, size, ImageFormat::Ai);
    }
    decode_eps(bytes, size, ImageFormat::Ai)
}

fn decode_eps(bytes: &[u8], size: u64, format: ImageFormat) -> Result<LoadedImage> {
    if let Some(pdf) = find_pdf_payload(bytes) {
        if let Ok(img) = rasterize_pdf(pdf, size, format) {
            return Ok(img);
        }
    }
    if let Some(img) = dos_eps_preview(bytes, size, format) {
        return Ok(img);
    }
    if let Some(img) = epsi_preview(bytes, size, format) {
        return Ok(img);
    }
    if let Some(img) = ghostscript_rasterize(bytes, size, format) {
        return Ok(img);
    }
    Err(ViewerError::UnsupportedEps)
}

fn decode_cdr(bytes: &[u8], size: u64) -> Result<LoadedImage> {
    if bytes.starts_with(b"PK") {
        if let Some(img) = cdr_zip_preview(bytes, size) {
            return Ok(img);
        }
    }
    if let Some(raster) = find_embedded_raster(bytes) {
        return decode_preview_raster(raster, size, ImageFormat::Cdr);
    }
    Err(ViewerError::UnsupportedCdr)
}

fn rasterize_pdf(bytes: &[u8], size: u64, format: ImageFormat) -> Result<LoadedImage> {
    let (rgba, width, height) = crate::pdf::rasterize_first_page(bytes, MAX_RASTER_EDGE)?;
    Ok(loaded(rgba, width, height, format, size))
}

fn find_pdf_payload(bytes: &[u8]) -> Option<&[u8]> {
    let hay = &bytes[..bytes.len().min(512 * 1024)];
    let pos = hay.windows(5).position(|w| w == b"%PDF-")?;
    Some(&bytes[pos..])
}

fn dos_eps_preview(bytes: &[u8], size: u64, format: ImageFormat) -> Option<LoadedImage> {
    let header = parse_dos_eps(bytes)?;
    if let Some(tiff) = header.tiff(bytes) {
        if let Ok(img) = decode_preview_raster(tiff, size, format) {
            return Some(img);
        }
    }
    if let Some(wmf) = header.wmf(bytes) {
        if let Ok(img) = decode_wmf(wmf, size) {
            let mut img = img;
            img.format = format;
            return Some(img);
        }
    }
    None
}

struct DosEps {
    wmf_off: u32,
    wmf_len: u32,
    tiff_off: u32,
    tiff_len: u32,
}

impl DosEps {
    fn tiff<'a>(&self, bytes: &'a [u8]) -> Option<&'a [u8]> {
        slice_at(bytes, self.tiff_off, self.tiff_len)
    }

    fn wmf<'a>(&self, bytes: &'a [u8]) -> Option<&'a [u8]> {
        slice_at(bytes, self.wmf_off, self.wmf_len)
    }
}

fn parse_dos_eps(bytes: &[u8]) -> Option<DosEps> {
    if bytes.len() < 30 || bytes[0..4] != [0xC5, 0xD0, 0xD3, 0xC6] {
        return None;
    }
    Some(DosEps {
        wmf_off: u32_le(bytes, 12)?,
        wmf_len: u32_le(bytes, 16)?,
        tiff_off: u32_le(bytes, 20)?,
        tiff_len: u32_le(bytes, 24)?,
    })
}

fn slice_at(bytes: &[u8], off: u32, len: u32) -> Option<&[u8]> {
    if off == 0 || len == 0 {
        return None;
    }
    let start = off as usize;
    let end = start.checked_add(len as usize)?;
    bytes.get(start..end)
}

fn u32_le(bytes: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(off..off + 4)?.try_into().ok()?,
    ))
}

/// `%%BeginPreview: w h depth lines` followed by hex rows.
fn epsi_preview(bytes: &[u8], size: u64, format: ImageFormat) -> Option<LoadedImage> {
    let text = std::str::from_utf8(bytes).ok()?;
    let marker = text.find("%%BeginPreview:")?;
    let rest = &text[marker + "%%BeginPreview:".len()..];
    let header_end = rest.find('\n')?;
    let mut nums = rest[..header_end].split_whitespace();
    let width: u32 = nums.next()?.parse().ok()?;
    let height: u32 = nums.next()?.parse().ok()?;
    let depth: u32 = nums.next()?.parse().ok()?;
    if width == 0 || height == 0 || ![1, 8, 24].contains(&depth) {
        return None;
    }
    let body = rest[header_end + 1..].split("%%EndPreview").next()?;
    let mut hex = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        if line.starts_with('%') {
            hex.extend(
                line.trim_start_matches('%')
                    .bytes()
                    .filter(u8::is_ascii_hexdigit),
            );
        } else {
            hex.extend(line.bytes().filter(u8::is_ascii_hexdigit));
        }
    }
    if hex.len() % 2 != 0 {
        return None;
    }
    let mut raw = Vec::with_capacity(hex.len() / 2);
    for pair in hex.chunks_exact(2) {
        raw.push(u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?);
    }
    let rgba = epsi_to_rgba(&raw, width, height, depth)?;
    Some(loaded(rgba, width, height, format, size))
}

fn epsi_to_rgba(raw: &[u8], width: u32, height: u32, depth: u32) -> Option<Vec<u8>> {
    let w = width as usize;
    let h = height as usize;
    let mut rgba = vec![0u8; w * h * 4];
    match depth {
        1 => {
            let stride = w.div_ceil(8);
            if raw.len() < stride * h {
                return None;
            }
            for y in 0..h {
                for x in 0..w {
                    let bit = raw[y * stride + x / 8] & (0x80 >> (x % 8));
                    let v = if bit == 0 { 255 } else { 0 };
                    let i = (y * w + x) * 4;
                    rgba[i] = v;
                    rgba[i + 1] = v;
                    rgba[i + 2] = v;
                    rgba[i + 3] = 255;
                }
            }
        }
        8 => {
            if raw.len() < w * h {
                return None;
            }
            for (i, &v) in raw.iter().take(w * h).enumerate() {
                let o = i * 4;
                rgba[o] = v;
                rgba[o + 1] = v;
                rgba[o + 2] = v;
                rgba[o + 3] = 255;
            }
        }
        24 => {
            if raw.len() < w * h * 3 {
                return None;
            }
            for y in 0..h {
                for x in 0..w {
                    let s = (y * w + x) * 3;
                    let d = (y * w + x) * 4;
                    rgba[d] = raw[s];
                    rgba[d + 1] = raw[s + 1];
                    rgba[d + 2] = raw[s + 2];
                    rgba[d + 3] = 255;
                }
            }
        }
        _ => return None,
    }
    Some(rgba)
}

fn ghostscript_rasterize(bytes: &[u8], size: u64, format: ImageFormat) -> Option<LoadedImage> {
    let gs = find_ghostscript()?;
    let stamp = format!("{}-{}", std::process::id(), size);
    let dir = std::env::temp_dir();
    let in_path = dir.join(format!("orchid-gs-{stamp}.eps"));
    let out_path = dir.join(format!("orchid-gs-{stamp}.png"));
    std::fs::write(&in_path, bytes).ok()?;
    let out_arg = format!(
        "-sOutputFile={}",
        out_path.to_string_lossy().replace('\\', "/")
    );
    let status = std::process::Command::new(gs)
        .args([
            "-dSAFER",
            "-dBATCH",
            "-dNOPAUSE",
            "-dNOPROMPT",
            "-q",
            "-sDEVICE=png16m",
            "-r144",
            "-dEPSCrop",
            &out_arg,
        ])
        .arg(&in_path)
        .status();
    let png = std::fs::read(&out_path).ok();
    let _ = std::fs::remove_file(&in_path);
    let _ = std::fs::remove_file(&out_path);
    if !status.ok()?.success() {
        return None;
    }
    decode_preview_raster(&png?, size, format).ok()
}

fn find_ghostscript() -> Option<std::path::PathBuf> {
    for name in ["gswin64c", "gswin32c", "gs"] {
        if let Ok(out) = std::process::Command::new(name).arg("-v").output() {
            if out.status.success() || !out.stderr.is_empty() || !out.stdout.is_empty() {
                return Some(std::path::PathBuf::from(name));
            }
        }
    }
    None
}

fn cdr_zip_preview(bytes: &[u8], size: u64) -> Option<LoadedImage> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    let mut best: Option<(usize, Vec<u8>)> = None;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).ok()?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().replace('\\', "/").to_ascii_lowercase();
        let hint = name.contains("preview")
            || name.contains("thumb")
            || name.ends_with(".png")
            || name.ends_with(".jpg")
            || name.ends_with(".jpeg")
            || name.ends_with(".bmp");
        if !hint {
            continue;
        }
        let mut buf = Vec::new();
        if file.read_to_end(&mut buf).is_err() || buf.len() < 32 {
            continue;
        }
        if image::guess_format(&buf).is_err() && !buf.starts_with(b"BM") {
            continue;
        }
        if best.as_ref().is_none_or(|(n, _)| buf.len() > *n) {
            best = Some((buf.len(), buf));
        }
    }
    decode_preview_raster(&best?.1, size, ImageFormat::Cdr).ok()
}

fn find_embedded_raster(data: &[u8]) -> Option<&[u8]> {
    if let Some(png) = largest_png(data) {
        return Some(png);
    }
    if let Some(jpeg) = largest_jpeg(data) {
        return Some(jpeg);
    }
    None
}

fn largest_png(data: &[u8]) -> Option<&[u8]> {
    const SIG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    const IEND: &[u8] = &[b'I', b'E', b'N', b'D'];
    let mut best: Option<&[u8]> = None;
    let mut i = 0;
    while let Some(rel) = data[i..].windows(8).position(|w| w == SIG) {
        let start = i + rel;
        let mut p = start + 8;
        let mut end = None;
        while p + 12 <= data.len() {
            let len = u32::from_be_bytes(data[p..p + 4].try_into().ok()?) as usize;
            let typ = &data[p + 4..p + 8];
            let next = p.checked_add(12)?.checked_add(len)?;
            if next > data.len() {
                break;
            }
            if typ == IEND {
                end = Some(next);
                break;
            }
            p = next;
        }
        if let Some(end) = end {
            let slice = &data[start..end];
            if best.is_none_or(|b| slice.len() > b.len()) {
                best = Some(slice);
            }
            i = end;
        } else {
            i = start + 8;
        }
    }
    best
}

fn largest_jpeg(data: &[u8]) -> Option<&[u8]> {
    const MIN: usize = 512;
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
        if slice.len() >= MIN && best.is_none_or(|b| slice.len() > b.len()) {
            best = Some(slice);
        }
        i = end;
    }
    best
}

fn decode_preview_raster(bytes: &[u8], size: u64, format: ImageFormat) -> Result<LoadedImage> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| ViewerError::ImageDecode(format!("vector preview: {e}")))?;
    let (width, height) = image::GenericImageView::dimensions(&img);
    Ok(loaded(
        img.to_rgba8().into_raw(),
        width,
        height,
        format,
        size,
    ))
}

fn gunzip_limited(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = flate2::read::GzDecoder::new(bytes);
    let mut out = Vec::new();
    let mut buf = [0u8; 16 * 1024];
    loop {
        let n = decoder
            .read(&mut buf)
            .map_err(|e| ViewerError::ImageDecode(format!("SVGZ: {e}")))?;
        if n == 0 {
            break;
        }
        if out.len().saturating_add(n) > MAX_SVGZ_UNCOMPRESSED {
            return Err(ViewerError::ImageDecode("SVGZ too large".into()));
        }
        out.extend_from_slice(&buf[..n]);
    }
    Ok(out)
}

fn decode_emf(bytes: &[u8], size: u64) -> Result<LoadedImage> {
    #[cfg(windows)]
    {
        gdi::play_emf(bytes, size, ImageFormat::Emf)
    }
    #[cfg(not(windows))]
    {
        let _ = (bytes, size);
        Err(ViewerError::UnsupportedEmf)
    }
}

fn decode_wmf(bytes: &[u8], size: u64) -> Result<LoadedImage> {
    #[cfg(windows)]
    {
        gdi::play_wmf(bytes, size)
    }
    #[cfg(not(windows))]
    {
        let _ = (bytes, size);
        Err(ViewerError::UnsupportedEmf)
    }
}

fn loaded(rgba: Vec<u8>, width: u32, height: u32, format: ImageFormat, size: u64) -> LoadedImage {
    LoadedImage {
        rgba: std::sync::Arc::new(rgba),
        width,
        height,
        format,
        original_size_bytes: size,
        ..LoadedImage::meta_defaults()
    }
}

#[cfg(windows)]
mod gdi {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteEnhMetaFile, DeleteObject,
        GetEnhMetaFileHeader, PatBlt, PlayEnhMetaFile, SelectObject, SetEnhMetaFileBits,
        BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, ENHMETAHEADER, MM_ANISOTROPIC,
        WHITENESS,
    };
    use windows::Win32::System::DataExchange::{SetWinMetaFileBits, METAFILEPICT};

    use super::{loaded, Result, ViewerError, MAX_RASTER_EDGE};
    use crate::image::loader::{ImageFormat, LoadedImage};

    pub(super) fn play_emf(bytes: &[u8], size: u64, format: ImageFormat) -> Result<LoadedImage> {
        unsafe {
            let hemf = SetEnhMetaFileBits(bytes);
            if hemf.is_invalid() {
                return Err(ViewerError::UnsupportedEmf);
            }
            let result = play_hemf(hemf, size, format);
            let _ = DeleteEnhMetaFile(Some(hemf));
            result
        }
    }

    pub(super) fn play_wmf(bytes: &[u8], size: u64) -> Result<LoadedImage> {
        let (payload, inch, box_w, box_h) = if looks_placeable(bytes) {
            let inch = u16::from_le_bytes([bytes[14], bytes[15]]).max(1);
            let left = i16::from_le_bytes([bytes[6], bytes[7]]) as i32;
            let top = i16::from_le_bytes([bytes[8], bytes[9]]) as i32;
            let right = i16::from_le_bytes([bytes[10], bytes[11]]) as i32;
            let bottom = i16::from_le_bytes([bytes[12], bytes[13]]) as i32;
            (
                bytes.get(22..).ok_or(ViewerError::UnsupportedEmf)?,
                inch,
                (right - left).unsigned_abs(),
                (bottom - top).unsigned_abs(),
            )
        } else {
            (bytes, 1440u16, 800u32, 600u32)
        };
        let x_ext = ((box_w as i32) * 2540 / i32::from(inch)).max(1);
        let y_ext = ((box_h as i32) * 2540 / i32::from(inch)).max(1);
        let pict = METAFILEPICT {
            mm: MM_ANISOTROPIC.0,
            xExt: x_ext,
            yExt: y_ext,
            hMF: Default::default(),
        };
        unsafe {
            let hemf = SetWinMetaFileBits(payload, None, Some(std::ptr::from_ref(&pict)));
            if hemf.is_invalid() {
                return Err(ViewerError::UnsupportedEmf);
            }
            let result = play_hemf(hemf, size, ImageFormat::Wmf);
            let _ = DeleteEnhMetaFile(Some(hemf));
            result
        }
    }

    fn looks_placeable(bytes: &[u8]) -> bool {
        bytes.len() >= 22 && bytes.starts_with(&[0xD7, 0xCD, 0xC6, 0x9A])
    }

    unsafe fn play_hemf(
        hemf: windows::Win32::Graphics::Gdi::HENHMETAFILE,
        size: u64,
        format: ImageFormat,
    ) -> Result<LoadedImage> {
        let mut header = ENHMETAHEADER::default();
        let got = unsafe {
            GetEnhMetaFileHeader(
                hemf,
                std::mem::size_of::<ENHMETAHEADER>() as u32,
                Some(std::ptr::from_mut(&mut header)),
            )
        };
        if got == 0 {
            return Err(ViewerError::UnsupportedEmf);
        }
        let (mut w, mut h) = size_from_header(&header);
        let long = w.max(h).max(1);
        if long > MAX_RASTER_EDGE as i32 {
            let scale = MAX_RASTER_EDGE as f32 / long as f32;
            w = ((w as f32) * scale).round().max(1.0) as i32;
            h = ((h as f32) * scale).round().max(1.0) as i32;
        }
        let mem_dc = unsafe { CreateCompatibleDC(None) };
        if mem_dc.is_invalid() {
            return Err(ViewerError::UnsupportedEmf);
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
        let dib = match unsafe {
            CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
        } {
            Ok(h) if !bits.is_null() => h,
            _ => {
                let _ = unsafe { DeleteDC(mem_dc) };
                return Err(ViewerError::UnsupportedEmf);
            }
        };
        let prev = unsafe { SelectObject(mem_dc, dib.into()) };
        let _ = unsafe { PatBlt(mem_dc, 0, 0, w, h, WHITENESS) };
        let dest = RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        let played = unsafe { PlayEnhMetaFile(mem_dc, hemf, &dest) };
        let n = (w as usize) * (h as usize) * 4;
        let bgra = unsafe { std::slice::from_raw_parts(bits.cast::<u8>(), n) };
        let rgba = bgra_to_rgba(bgra);
        let _ = unsafe { SelectObject(mem_dc, prev) };
        let _ = unsafe { DeleteObject(dib.into()) };
        let _ = unsafe { DeleteDC(mem_dc) };
        if !played.as_bool() {
            return Err(ViewerError::UnsupportedEmf);
        }
        Ok(loaded(rgba, w as u32, h as u32, format, size))
    }

    fn size_from_header(header: &ENHMETAHEADER) -> (i32, i32) {
        let fw = (header.rclFrame.right - header.rclFrame.left).unsigned_abs();
        let fh = (header.rclFrame.bottom - header.rclFrame.top).unsigned_abs();
        if fw > 0 && fh > 0 {
            let w = ((fw as f32) * 96.0 / 2540.0).round().max(1.0) as i32;
            let h = ((fh as f32) * 96.0 / 2540.0).round().max(1.0) as i32;
            return (w.min(8192), h.min(8192));
        }
        let bw = (header.rclBounds.right - header.rclBounds.left).unsigned_abs();
        let bh = (header.rclBounds.bottom - header.rclBounds.top).unsigned_abs();
        ((bw as i32).clamp(1, 8192), (bh as i32).clamp(1, 8192))
    }

    fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
        let mut rgba = Vec::with_capacity(bgra.len());
        for chunk in bgra.chunks_exact(4) {
            rgba.extend_from_slice(&[chunk[2], chunk[1], chunk[0], 255]);
        }
        rgba
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn sniffs_vector_magics() {
        assert!(looks_like_eps(b"%!PS-Adobe-3.0 EPSF-3.0\n"));
        assert!(looks_like_eps(&[0xC5, 0xD0, 0xD3, 0xC6, 0, 0]));
        assert!(looks_like_wmf(&[0xD7, 0xCD, 0xC6, 0x9A]));
        let mut emf = vec![0u8; 44];
        emf[0] = 1;
        emf[40..44].copy_from_slice(b" EMF");
        assert!(looks_like_emf(&emf));
        let mut cdr = b"RIFF\0\0\0\0CDR6xxxx".to_vec();
        assert!(looks_like_cdr_riff(&cdr));
        cdr[8..12].copy_from_slice(b"WAVE");
        assert!(!looks_like_cdr_riff(&cdr));
        assert!(is_vector_extension("ai"));
        assert!(is_vector_extension("svgz"));
    }

    #[test]
    fn dos_eps_tiff_preview() {
        let mut tiff = {
            let mut img = image::RgbImage::new(4, 4);
            img.put_pixel(0, 0, image::Rgb([200, 10, 30]));
            let mut cursor = Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(img)
                .write_to(&mut cursor, image::ImageFormat::Tiff)
                .unwrap();
            cursor.into_inner()
        };
        let mut bytes = vec![0u8; 30];
        bytes[0..4].copy_from_slice(&[0xC5, 0xD0, 0xD3, 0xC6]);
        let off = 30u32;
        bytes[20..24].copy_from_slice(&off.to_le_bytes());
        bytes[24..28].copy_from_slice(&(tiff.len() as u32).to_le_bytes());
        bytes.append(&mut tiff);
        let img = decode_eps(&bytes, bytes.len() as u64, ImageFormat::Eps).unwrap();
        assert_eq!(img.format, ImageFormat::Eps);
        assert_eq!(img.width, 4);
        assert_eq!(img.rgba[0], 200);
    }

    #[test]
    fn epsi_1bit_preview() {
        let eps =
            b"%!PS-Adobe-3.0 EPSF-3.0\n%%BeginPreview: 8 1 1 1\n%FF\n%%EndPreview\nshowpage\n";
        let img = decode_eps(eps, eps.len() as u64, ImageFormat::Eps).unwrap();
        assert_eq!(img.width, 8);
        assert_eq!(img.height, 1);
        assert_eq!(img.rgba[0], 0);
        assert_eq!(img.rgba[3], 255);
    }

    #[test]
    fn cdr_zip_uses_preview_png() {
        let png = {
            let mut img = image::RgbaImage::new(2, 2);
            img.put_pixel(0, 0, image::Rgba([1, 2, 3, 255]));
            let mut cursor = Cursor::new(Vec::new());
            img.write_to(&mut cursor, image::ImageFormat::Png).unwrap();
            cursor.into_inner()
        };
        let mut zip_buf = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut zip_buf);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("preview.png", opts).unwrap();
            zip.write_all(&png).unwrap();
            zip.finish().unwrap();
        }
        let bytes = zip_buf.into_inner();
        let img = decode_cdr(&bytes, bytes.len() as u64).unwrap();
        assert_eq!(img.format, ImageFormat::Cdr);
        assert_eq!(img.width, 2);
        assert_eq!(img.rgba[0..3], [1, 2, 3]);
    }

    #[test]
    fn svgz_roundtrip() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="3" height="3"><rect width="3" height="3" fill="#00ff00"/></svg>"##;
        let mut gz = Vec::new();
        {
            let mut enc = flate2::write::GzEncoder::new(&mut gz, flate2::Compression::default());
            enc.write_all(svg).unwrap();
            enc.finish().unwrap();
        }
        let img = decode_svgz(&gz, gz.len() as u64).unwrap();
        assert_eq!(img.format, ImageFormat::Svg);
        assert_eq!(img.width, 3);
        assert_eq!(img.rgba[0..3], [0, 255, 0]);
    }

    #[test]
    fn bare_ps_without_preview_is_clear() {
        let ps = b"%!PS-Adobe-3.0\nshowpage\n";
        match decode_eps(ps, ps.len() as u64, ImageFormat::Eps) {
            Ok(img) => assert!(img.width > 0),
            Err(ViewerError::UnsupportedEps) => {}
            Err(e) => panic!("unexpected {e:?}"),
        }
    }
}
