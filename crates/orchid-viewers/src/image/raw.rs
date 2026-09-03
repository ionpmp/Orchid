//! Camera RAW: embedded JPEG preview, full demosaic, and basic develop.

use std::path::Path;

use rawler::decoders::RawDecodeParams;
use rawler::rawsource::RawSource;
use rawler::{RawImage, RawImageData};

use crate::error::{Result, ViewerError};
use crate::image::adjust::{AdjustOp, AdjustParams};
use crate::image::loader::{ImageFormat, LoadedImage};

/// Extensions treated as camera RAW (preview and/or demosaic).
pub const RAW_FILE_EXTENSIONS: &[&str] = &[
    "cr2", "cr3", "nef", "nrw", "arw", "raf", "orf", "pef", "rw2", "srw", "x3f", "rwl", "dng",
    "dcr", "kdc",
];

/// True when `ext` (no leading dot) is a known RAW extension.
#[must_use]
pub fn is_raw_file_extension(ext: &str) -> bool {
    let lower = ext.to_ascii_lowercase();
    RAW_FILE_EXTENSIONS.iter().any(|e| *e == lower)
}

/// Exposure EV plus optional temperature / tint sliders (−100…100, same as adjust).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawDevelop {
    /// Stops; `2^ev` on linear RGB.
    pub exposure_ev: f32,
    /// Warm / cool slider used by the adjust panel.
    pub temperature: f32,
    /// Green / magenta slider used by the adjust panel.
    pub tint: f32,
}

impl Default for RawDevelop {
    fn default() -> Self {
        Self {
            exposure_ev: 0.0,
            temperature: 0.0,
            tint: 0.0,
        }
    }
}

impl RawDevelop {
    /// Pull EV / WB from an adjust op (`develop` is a no-op develop).
    #[must_use]
    pub fn from_adjust(op: &AdjustOp) -> Self {
        match op {
            AdjustOp::Params(p) => Self {
                exposure_ev: p.exposure.unwrap_or(0.0),
                temperature: p.temperature.unwrap_or(0.0),
                tint: p.tint.unwrap_or(0.0),
            },
            _ => Self::default(),
        }
    }
}

/// Decode a RAW file: demosaic when `rawler` understands the camera, else the
/// largest embedded JPEG preview.
pub(crate) fn decode_raw(bytes: &[u8], size: u64, develop: RawDevelop) -> Result<LoadedImage> {
    match develop_demosaic(bytes, size, develop) {
        Ok(img) => Ok(img),
        Err(e) => {
            tracing::debug!(error = %e, "RAW demosaic failed, trying embedded JPEG");
            decode_embedded_preview(bytes, size)
        }
    }
}

/// Demosaic `path` with `develop` (no JPEG fallback).
///
/// # Errors
///
/// I/O or an unsupported / unreadable RAW.
pub fn develop_raw_file(path: &Path, develop: RawDevelop) -> Result<LoadedImage> {
    let bytes = std::fs::read(path).map_err(orchid_fs::FsError::Io)?;
    let size = bytes.len() as u64;
    develop_demosaic(&bytes, size, develop).or_else(|e| {
        tracing::debug!(error = %e, "RAW develop failed, using embedded JPEG");
        decode_embedded_preview(&bytes, size)
    })
}

pub(crate) fn decode_embedded_preview(bytes: &[u8], size: u64) -> Result<LoadedImage> {
    let Some(jpeg) = largest_embedded_jpeg(bytes) else {
        return Err(ViewerError::UnsupportedRaw);
    };
    let img = image::load_from_memory(jpeg).map_err(|e| {
        tracing::debug!(error = %e, "RAW embedded JPEG decode failed");
        ViewerError::UnsupportedRaw
    })?;
    let orientation = crate::image::exif::orientation_from_bytes(jpeg);
    let img = crate::image::exif::apply_orientation(img, orientation);
    let rgba = img.into_rgba8();
    let (w, h) = rgba.dimensions();
    Ok(LoadedImage {
        rgba: std::sync::Arc::new(rgba.into_raw()),
        width: w,
        height: h,
        format: ImageFormat::Raw,
        original_size_bytes: size,
        color_source: "Embedded JPEG".into(),
        color_dest: "sRGB".into(),
        orientation,
        bit_depth: 8,
        color_model: "RGB".into(),
    })
}

/// Scan for JPEG SOI…EOI segments and return the largest one (preview > thumb).
pub(crate) fn largest_embedded_jpeg(data: &[u8]) -> Option<&[u8]> {
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

/// True when `bytes` match a common camera-RAW magic sequence.
#[must_use]
pub fn looks_like_raw(bytes: &[u8]) -> bool {
    is_raw_magic(bytes)
}

fn is_raw_magic(bytes: &[u8]) -> bool {
    if looks_like_cr3(bytes) {
        return true;
    }
    if bytes.starts_with(b"FOVb") {
        return true;
    }
    if bytes.starts_with(b"FUJIFILMCCD-RAW") {
        return true;
    }
    if bytes.starts_with(b"IIRO") || bytes.starts_with(b"MMOR") || bytes.starts_with(b"IIRS") {
        return true;
    }
    if bytes.starts_with(b"IIU\0") {
        return true;
    }
    if bytes.len() >= 10
        && ((bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*")) && &bytes[8..10] == b"CR")
    {
        return true;
    }
    false
}

fn looks_like_cr3(bytes: &[u8]) -> bool {
    if bytes.len() < 16 || &bytes[4..8] != b"ftyp" {
        return false;
    }
    let mut offset = 8;
    while offset + 4 <= bytes.len().min(64) {
        let brand = &bytes[offset..offset + 4];
        if brand == b"crx " || brand == b"crx\0" || brand == b"CRX " {
            return true;
        }
        offset = if offset == 8 { 16 } else { offset + 4 };
    }
    false
}

fn develop_demosaic(bytes: &[u8], size: u64, develop: RawDevelop) -> Result<LoadedImage> {
    let src = RawSource::new_from_slice(bytes);
    let mut raw = rawler::decode(&src, &RawDecodeParams::default())
        .map_err(|e| ViewerError::ImageDecode(format!("RAW: {e}")))?;
    if let Err(e) = raw.apply_scaling() {
        tracing::debug!(error = %e, "RAW apply_scaling skipped");
    }
    let (rgb, width, height) = rasterize_raw(&raw, develop)?;
    let orientation = exif_orientation(&raw);
    let dynimg = image::DynamicImage::ImageRgb32F(
        image::Rgb32FImage::from_raw(width, height, rgb)
            .ok_or_else(|| ViewerError::ImageDecode("RAW RGB buffer".into()))?,
    );
    let dynimg = crate::image::exif::apply_orientation(dynimg, orientation);
    let rgba = dynimg.into_rgba8();
    let (w, h) = rgba.dimensions();
    Ok(LoadedImage {
        rgba: std::sync::Arc::new(rgba.into_raw()),
        width: w,
        height: h,
        format: ImageFormat::Raw,
        original_size_bytes: size,
        color_source: format!("RAW demosaic {}", raw.clean_model),
        color_dest: "sRGB".into(),
        orientation,
        bit_depth: raw.bps.max(8) as u8,
        color_model: "RGB".into(),
    })
}

fn rasterize_raw(raw: &RawImage, develop: RawDevelop) -> Result<(Vec<f32>, u32, u32)> {
    let w = raw.width;
    let h = raw.height;
    if w == 0 || h == 0 {
        return Err(ViewerError::ImageDecode("RAW empty".into()));
    }
    let plane = linear_plane(raw, w, h)?;
    let rgb = if raw.cpp >= 3 || raw.is_monochrome() {
        packed_to_rgb(&plane, w, h, raw.cpp)
    } else {
        let cfa = raw.cropped_cfa();
        if !cfa.is_valid() {
            return Err(ViewerError::ImageDecode("RAW CFA".into()));
        }
        demosaic_bilinear(&plane, w, h, |row, col| cfa.color_at(row, col))
    };
    let rgb = apply_wb_matrix_gamma(&rgb, raw, develop);
    let (rgb, cw, ch) = crop_rgb(rgb, w, h, raw);
    Ok((rgb, cw, ch))
}

fn linear_plane(raw: &RawImage, w: usize, h: usize) -> Result<Vec<f32>> {
    let n = w
        .checked_mul(h)
        .and_then(|px| px.checked_mul(raw.cpp.max(1)))
        .ok_or_else(|| ViewerError::ImageDecode("RAW size".into()))?;
    match &raw.data {
        RawImageData::Float(v) => {
            if v.len() < n {
                return Err(ViewerError::ImageDecode("RAW float plane".into()));
            }
            Ok(v[..n].to_vec())
        }
        RawImageData::Integer(v) => {
            if v.len() < n {
                return Err(ViewerError::ImageDecode("RAW int plane".into()));
            }
            let scale = if raw.bps >= 16 {
                1.0 / 65535.0
            } else {
                1.0 / ((1u32 << raw.bps.min(16)) - 1) as f32
            };
            Ok(v[..n].iter().map(|s| f32::from(*s) * scale).collect())
        }
    }
}

fn packed_to_rgb(plane: &[f32], w: usize, h: usize, cpp: usize) -> Vec<f32> {
    let mut rgb = vec![0f32; w * h * 3];
    if cpp >= 3 {
        for i in 0..w * h {
            let s = i * cpp;
            rgb[i * 3] = plane[s];
            rgb[i * 3 + 1] = plane[s + 1];
            rgb[i * 3 + 2] = plane[s + 2];
        }
    } else {
        for i in 0..w * h {
            let v = plane[i];
            rgb[i * 3] = v;
            rgb[i * 3 + 1] = v;
            rgb[i * 3 + 2] = v;
        }
    }
    rgb
}

/// Bilinear CFA demosaic. `color_at(row, col)` returns 0=R, 1=G, 2=B (or 3=G2).
pub(crate) fn demosaic_bilinear(
    plane: &[f32],
    w: usize,
    h: usize,
    color_at: impl Fn(usize, usize) -> usize,
) -> Vec<f32> {
    let mut rgb = vec![0f32; w * h * 3];
    let at = |x: isize, y: isize| -> Option<f32> {
        if x < 0 || y < 0 {
            return None;
        }
        let (x, y) = (x as usize, y as usize);
        if x >= w || y >= h {
            return None;
        }
        plane.get(y * w + x).copied()
    };
    let avg = |vals: &[Option<f32>]| {
        let mut s = 0.0;
        let mut n = 0.0;
        for v in vals.iter().flatten() {
            s += *v;
            n += 1.0;
        }
        if n > 0.0 {
            s / n
        } else {
            0.0
        }
    };
    for y in 0..h {
        for x in 0..w {
            let c = color_at(y, x) % 3;
            let v = plane[y * w + x];
            let (r, g, b) = match c {
                0 => {
                    let g = avg(&[
                        at(x as isize - 1, y as isize),
                        at(x as isize + 1, y as isize),
                        at(x as isize, y as isize - 1),
                        at(x as isize, y as isize + 1),
                    ]);
                    let b = avg(&[
                        at(x as isize - 1, y as isize - 1),
                        at(x as isize + 1, y as isize - 1),
                        at(x as isize - 1, y as isize + 1),
                        at(x as isize + 1, y as isize + 1),
                    ]);
                    (v, g, b)
                }
                2 => {
                    let g = avg(&[
                        at(x as isize - 1, y as isize),
                        at(x as isize + 1, y as isize),
                        at(x as isize, y as isize - 1),
                        at(x as isize, y as isize + 1),
                    ]);
                    let r = avg(&[
                        at(x as isize - 1, y as isize - 1),
                        at(x as isize + 1, y as isize - 1),
                        at(x as isize - 1, y as isize + 1),
                        at(x as isize + 1, y as isize + 1),
                    ]);
                    (r, g, v)
                }
                _ => {
                    let r = avg(&[
                        at(x as isize - 1, y as isize),
                        at(x as isize + 1, y as isize),
                    ]);
                    let b = avg(&[
                        at(x as isize, y as isize - 1),
                        at(x as isize, y as isize + 1),
                    ]);
                    // G row vs G column: if horizontal neighbors are R, use them as R.
                    let horiz = color_at(y, x.saturating_sub(1).min(w - 1));
                    if horiz.is_multiple_of(3) {
                        (r, v, b)
                    } else {
                        (b, v, r)
                    }
                }
            };
            let o = (y * w + x) * 3;
            rgb[o] = r;
            rgb[o + 1] = g;
            rgb[o + 2] = b;
        }
    }
    rgb
}

fn apply_wb_matrix_gamma(rgb: &[f32], raw: &RawImage, develop: RawDevelop) -> Vec<f32> {
    let mut wb = raw.wb_coeffs;
    if !wb[0].is_finite() || wb[0] <= 0.0 {
        wb = raw.neutralwb();
    }
    let g = if wb[1].is_finite() && wb[1] > 1e-6 {
        wb[1]
    } else {
        1.0
    };
    let mut wr = (wb[0] / g).clamp(0.1, 8.0);
    let mut wg = 1.0f32;
    let mut wb_b = (wb[2] / g).clamp(0.1, 8.0);
    let tu = (develop.temperature / 100.0).clamp(-1.0, 1.0);
    wr *= 1.0 + tu * 0.35;
    wb_b *= 1.0 - tu * 0.35;
    let nu = (develop.tint / 100.0).clamp(-1.0, 1.0);
    wg *= 1.0 - nu * 0.28;
    wr *= 1.0 + nu * 0.12;
    wb_b *= 1.0 + nu * 0.12;
    let ev = 2f32.powf(develop.exposure_ev.clamp(-4.0, 4.0));
    let cam_xyz = raw.cam_to_xyz_normalized();
    let use_matrix = cam_xyz.iter().flatten().any(|v| v.abs() > 1e-6);
    let mut out = vec![0f32; rgb.len()];
    for (i, px) in rgb.chunks_exact(3).enumerate() {
        let mut r = px[0] * wr * ev;
        let mut g = px[1] * wg * ev;
        let mut b = px[2] * wb_b * ev;
        if use_matrix {
            let x = cam_xyz[0][0] * r + cam_xyz[0][1] * g + cam_xyz[0][2] * b;
            let y = cam_xyz[1][0] * r + cam_xyz[1][1] * g + cam_xyz[1][2] * b;
            let z = cam_xyz[2][0] * r + cam_xyz[2][1] * g + cam_xyz[2][2] * b;
            r = 3.2406 * x - 1.5372 * y - 0.4986 * z;
            g = -0.9689 * x + 1.8758 * y + 0.0415 * z;
            b = 0.0557 * x - 0.2040 * y + 1.0570 * z;
        }
        let o = i * 3;
        out[o] = srgb_gamma(r);
        out[o + 1] = srgb_gamma(g);
        out[o + 2] = srgb_gamma(b);
    }
    out
}

fn srgb_gamma(v: f32) -> f32 {
    let v = v.max(0.0);
    if v <= 0.0031308 {
        (12.92 * v).clamp(0.0, 1.0)
    } else {
        (1.055 * v.powf(1.0 / 2.4) - 0.055).clamp(0.0, 1.0)
    }
}

fn crop_rgb(rgb: Vec<f32>, w: usize, h: usize, raw: &RawImage) -> (Vec<f32>, u32, u32) {
    let Some(rect) = raw.crop_area.or(raw.active_area) else {
        return (rgb, w as u32, h as u32);
    };
    let x = rect.x();
    let y = rect.y();
    let cw = rect.width().min(w.saturating_sub(x));
    let ch = rect.height().min(h.saturating_sub(y));
    if cw == 0 || ch == 0 || (cw == w && ch == h && x == 0 && y == 0) {
        return (rgb, w as u32, h as u32);
    }
    let mut out = vec![0f32; cw * ch * 3];
    for row in 0..ch {
        let src = ((y + row) * w + x) * 3;
        let dst = row * cw * 3;
        out[dst..dst + cw * 3].copy_from_slice(&rgb[src..src + cw * 3]);
    }
    (out, cw as u32, ch as u32)
}

fn exif_orientation(raw: &RawImage) -> u32 {
    match raw.orientation.to_u16() {
        2..=8 => u32::from(raw.orientation.to_u16()),
        _ => 1,
    }
}

/// Apply adjust on a RAW path: re-develop exposure/WB from the sensor, then
/// the remaining 8-bit corrections.
pub(crate) fn apply_adjust_raw_file(path: &Path, op: &AdjustOp) -> Result<LoadedImage> {
    let develop = RawDevelop::from_adjust(op);
    let developed = develop_raw_file(path, develop)?;
    match op {
        AdjustOp::Params(p) => {
            let rest = AdjustParams {
                exposure: None,
                temperature: None,
                tint: None,
                ..p.clone()
            };
            if rest == AdjustParams::default() {
                Ok(developed)
            } else {
                crate::image::adjust::apply_adjust(&developed, &AdjustOp::Params(rest))
            }
        }
        other => crate::image::adjust::apply_adjust(&developed, other),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn raw_extensions_cover_requested_cameras() {
        for ext in [
            "cr2", "cr3", "nef", "arw", "raf", "orf", "pef", "rw2", "srw", "x3f", "rwl", "dng",
            "dcr",
        ] {
            assert!(is_raw_file_extension(ext), "{ext}");
        }
        assert!(!is_raw_file_extension("jpg"));
    }

    #[test]
    fn sniffs_cr3_x3f_raf() {
        let mut cr3 = vec![0, 0, 0, 0x18];
        cr3.extend_from_slice(b"ftyp");
        cr3.extend_from_slice(b"crx ");
        cr3.extend_from_slice(&[0, 0, 0, 0]);
        cr3.extend_from_slice(b"isom");
        assert!(looks_like_raw(&cr3));
        assert!(looks_like_raw(b"FOVb\0\0"));
        assert!(looks_like_raw(b"FUJIFILMCCD-RAW "));
    }

    #[test]
    fn bilinear_rggb_recovers_primaries() {
        // 2×2 RGGB: R=1, G=0.5, G=0.5, B=0.25
        let plane = [1.0f32, 0.5, 0.5, 0.25];
        let cfa = rawler::CFA::new("RGGB");
        let rgb = demosaic_bilinear(&plane, 2, 2, |r, c| cfa.color_at(r, c));
        assert!((rgb[0] - 1.0).abs() < 1e-5, "R at (0,0)");
        assert!((rgb[4] - 0.5).abs() < 1e-5, "G at (0,1) {}", rgb[4]);
        assert!((rgb[11] - 0.25).abs() < 1e-5, "B at (1,1)");
    }

    #[test]
    fn develop_from_adjust_reads_ev_and_wb() {
        let op = AdjustOp::Params(AdjustParams {
            exposure: Some(0.5),
            temperature: Some(-20.0),
            tint: Some(10.0),
            ..AdjustParams::default()
        });
        let d = RawDevelop::from_adjust(&op);
        assert_eq!(d.exposure_ev, 0.5);
        assert_eq!(d.temperature, -20.0);
        assert_eq!(d.tint, 10.0);
    }

    #[test]
    fn embedded_jpeg_preview_roundtrip() {
        let mut img = image::RgbImage::new(256, 256);
        for (i, p) in img.pixels_mut().enumerate() {
            let v = (i % 256) as u8;
            *p = image::Rgb([v, 200, 40]);
        }
        let mut cursor = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut cursor, image::ImageFormat::Jpeg)
            .unwrap();
        let jpeg = cursor.into_inner();
        let mut container = b"II*\0\x08\0\0\0CR".to_vec();
        container.extend_from_slice(&jpeg);
        let loaded = decode_embedded_preview(&container, container.len() as u64).unwrap();
        assert_eq!(loaded.format, ImageFormat::Raw);
        assert_eq!(loaded.color_source, "Embedded JPEG");
        assert_eq!(loaded.width, 256);
    }
}
