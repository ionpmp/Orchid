//! Destructive geometry edits that **write a sibling file** (never the original).

use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::{DynamicImage, ImageFormat as ImgFmt, RgbImage, RgbaImage};

use crate::error::{Result, ViewerError};
use crate::image::loader::{load_image_file, LoadedImage};
use crate::image::operations::{canvas_resize, crop, fit_crop_rect, resize_filtered, ResizeFilter};

/// After a crop, optionally scale so one original side is restored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CropKeep {
    #[default]
    /// Leave the cropped pixel size as-is.
    None,
    /// Scale so width matches the source image.
    Width,
    /// Scale so height matches the source image.
    Height,
}

/// How to interpret a resize target.
#[derive(Debug, Clone, PartialEq)]
pub enum ResizeSpec {
    /// Scale both sides by this factor (`0.5` = 50%).
    Percent(f32),
    /// Absolute pixels. `None` means “keep aspect from the other side”.
    Pixels {
        /// Target width.
        w: Option<u32>,
        /// Target height.
        h: Option<u32>,
    },
    /// Print size at `dpi` (default 96).
    Cm {
        /// Width in centimetres.
        w: Option<f32>,
        /// Height in centimetres.
        h: Option<f32>,
        /// Dots per inch used to convert centimetres.
        dpi: f32,
    },
}

/// One destructive edit.
#[derive(Debug, Clone, PartialEq)]
pub enum EditOp {
    /// Axis-aligned crop, optional aspect lock and keep-side scale.
    Crop {
        /// Left in source pixels.
        x: u32,
        /// Top in source pixels.
        y: u32,
        /// Width in source pixels.
        w: u32,
        /// Height in source pixels.
        h: u32,
        /// Width/height ratio, or `None` for free.
        aspect: Option<f32>,
        /// Restore one original side after the crop.
        keep: CropKeep,
    },
    /// Scale to a percent / pixel / centimetre target.
    Resize {
        /// Target size.
        spec: ResizeSpec,
        /// Resampling kernel.
        filter: ResizeFilter,
    },
    /// New canvas; source is centered.
    Canvas {
        /// Destination width.
        width: u32,
        /// Destination height.
        height: u32,
        /// RGBA fill for new pixels.
        fill: [u8; 4],
    },
    /// Map a source quad (UL, UR, LR, LL) onto a rectangle.
    Perspective {
        /// Four source corners, image pixels.
        quad: [(f32, f32); 4],
    },
    /// Rotate so the line becomes horizontal, then crop empty corners.
    Straighten {
        /// Line start, image pixels.
        x0: f32,
        /// Line start Y.
        y0: f32,
        /// Line end X.
        x1: f32,
        /// Line end Y.
        y1: f32,
    },
    /// Estimate a small horizon tilt and straighten.
    AutoStraighten,
}

impl EditOp {
    fn suffix(&self) -> &'static str {
        match self {
            Self::Crop { .. } => "crop",
            Self::Resize { .. } => "resize",
            Self::Canvas { .. } => "canvas",
            Self::Perspective { .. } => "perspective",
            Self::Straighten { .. } | Self::AutoStraighten => "straighten",
        }
    }
}

/// Apply `op` to an already-decoded image.
///
/// # Errors
///
/// Invalid geometry or a corrupt buffer.
pub fn apply_edit(src: &LoadedImage, op: &EditOp) -> Result<LoadedImage> {
    match op {
        EditOp::Crop {
            x,
            y,
            w,
            h,
            aspect,
            keep,
        } => {
            let (x, y, w, h) = fit_crop_rect(*x, *y, *w, *h, src.width, src.height, *aspect);
            let cropped = crop(src, x, y, w, h)?;
            match keep {
                CropKeep::None => Ok(cropped),
                CropKeep::Width => {
                    let th = ((cropped.height as f64) * (src.width as f64) / (cropped.width as f64))
                        .round()
                        .max(1.0) as u32;
                    resize_filtered(&cropped, src.width, th, ResizeFilter::Lanczos)
                }
                CropKeep::Height => {
                    let tw = ((cropped.width as f64) * (src.height as f64)
                        / (cropped.height as f64))
                        .round()
                        .max(1.0) as u32;
                    resize_filtered(&cropped, tw, src.height, ResizeFilter::Lanczos)
                }
            }
        }
        EditOp::Resize { spec, filter } => {
            let (tw, th) = spec.resolve(src.width, src.height)?;
            resize_filtered(src, tw, th, *filter)
        }
        EditOp::Canvas {
            width,
            height,
            fill,
        } => {
            let ox = (*width as i32 - src.width as i32) / 2;
            let oy = (*height as i32 - src.height as i32) / 2;
            canvas_resize(src, *width, *height, ox, oy, *fill)
        }
        EditOp::Perspective { quad } => perspective(src, *quad),
        EditOp::Straighten { x0, y0, x1, y1 } => {
            let deg = -((y1 - y0).atan2(x1 - x0).to_degrees());
            rotate_crop(src, deg)
        }
        EditOp::AutoStraighten => {
            let deg = auto_straighten_angle(src);
            rotate_crop(src, deg)
        }
    }
}

/// Apply `op` and write a sibling file next to `path`. Returns the new path.
///
/// # Errors
///
/// I/O or an unreadable image.
pub fn apply_edit_file(path: &Path, op: &EditOp) -> Result<PathBuf> {
    let src = load_image_file(path)?;
    let out = apply_edit(&src, op)?;
    save_sibling(path, &out, op.suffix())
}

/// Encode `img` beside `src` as `{stem}-{suffix}[ -n].{ext}`.
///
/// # Errors
///
/// I/O or encode failure.
pub fn save_sibling(src: &Path, img: &LoadedImage, suffix: &str) -> Result<PathBuf> {
    let dest = unique_sibling(src, suffix, encode_ext(img));
    let bytes = encode_loaded(img)?;
    std::fs::write(&dest, bytes)?;
    Ok(dest)
}

/// `50%`, `800x600`, `800x`, `x600`, `10cmx15cm`, optional ` filter=lanczos`.
#[must_use]
pub fn parse_resize_line(raw: &str) -> Option<(ResizeSpec, ResizeFilter)> {
    let mut filter = ResizeFilter::Lanczos;
    let mut spec_raw = raw.trim();
    if let Some((left, right)) = spec_raw.split_once("filter=") {
        spec_raw = left.trim();
        filter = ResizeFilter::parse(right)?;
    }
    let spec = parse_resize_spec(spec_raw)?;
    Some((spec, filter))
}

/// Parse a resize target without a filter suffix.
#[must_use]
pub fn parse_resize_spec(raw: &str) -> Option<ResizeSpec> {
    let s = raw.trim().replace(' ', "");
    if s.is_empty() {
        return None;
    }
    if let Some(p) = s.strip_suffix('%') {
        let v: f32 = p.parse().ok()?;
        return Some(ResizeSpec::Percent(v / 100.0));
    }
    let cm = s.to_ascii_lowercase().contains("cm");
    let body = s.replace("cm", "");
    let (a, b) = if let Some((l, r)) = body.split_once('x') {
        (l, r)
    } else if let Some((l, r)) = body.split_once('×') {
        (l, r)
    } else {
        return Some(if cm {
            ResizeSpec::Cm {
                w: body.parse().ok(),
                h: None,
                dpi: 96.0,
            }
        } else {
            ResizeSpec::Pixels {
                w: body.parse().ok(),
                h: None,
            }
        });
    };
    if cm {
        Some(ResizeSpec::Cm {
            w: parse_opt_f32(a),
            h: parse_opt_f32(b),
            dpi: 96.0,
        })
    } else {
        Some(ResizeSpec::Pixels {
            w: parse_opt_u32(a),
            h: parse_opt_u32(b),
        })
    }
}

/// `2000x1500` or `+80+40` (padding).
#[must_use]
pub fn parse_canvas_line(raw: &str, src_w: u32, src_h: u32) -> Option<(u32, u32)> {
    let s = raw.trim().replace(' ', "");
    if let Some(rest) = s.strip_prefix('+') {
        let parts: Vec<&str> = rest.split('+').collect();
        if parts.len() == 2 {
            let px: i32 = parts[0].parse().ok()?;
            let py: i32 = parts[1].parse().ok()?;
            return Some((
                src_w.saturating_add(px.unsigned_abs() * 2),
                src_h.saturating_add(py.unsigned_abs() * 2),
            ));
        }
    }
    let (a, b) = s.split_once('x')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

impl ResizeSpec {
    fn resolve(&self, sw: u32, sh: u32) -> Result<(u32, u32)> {
        let (tw, th) = match self {
            Self::Percent(p) => {
                let p = (*p as f64).clamp(0.01, 16.0);
                (
                    ((sw as f64) * p).round().max(1.0) as u32,
                    ((sh as f64) * p).round().max(1.0) as u32,
                )
            }
            Self::Pixels { w, h } => pixel_pair(sw, sh, *w, *h),
            Self::Cm { w, h, dpi } => {
                let px = |cm: f32| ((cm as f64) / 2.54 * (*dpi as f64)).round().max(1.0) as u32;
                pixel_pair(sw, sh, w.map(px), h.map(px))
            }
        };
        if tw == 0 || th == 0 {
            return Err(ViewerError::ImageDecode("resize target is zero".into()));
        }
        Ok((tw, th))
    }
}

fn pixel_pair(sw: u32, sh: u32, w: Option<u32>, h: Option<u32>) -> (u32, u32) {
    match (w, h) {
        (Some(w), Some(h)) => (w.max(1), h.max(1)),
        (Some(w), None) => {
            let h = ((sh as f64) * (w as f64) / (sw as f64)).round().max(1.0) as u32;
            (w.max(1), h)
        }
        (None, Some(h)) => {
            let w = ((sw as f64) * (h as f64) / (sh as f64)).round().max(1.0) as u32;
            (w, h.max(1))
        }
        (None, None) => (sw, sh),
    }
}

fn parse_opt_u32(s: &str) -> Option<u32> {
    if s.is_empty() {
        None
    } else {
        s.parse().ok()
    }
}

fn parse_opt_f32(s: &str) -> Option<f32> {
    if s.is_empty() {
        None
    } else {
        s.parse().ok()
    }
}

fn unique_sibling(src: &Path, suffix: &str, ext: &str) -> PathBuf {
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    let dir = src.parent().unwrap_or_else(|| Path::new("."));
    let mut dest = dir.join(format!("{stem}-{suffix}.{ext}"));
    let mut n = 2u32;
    while dest.exists() {
        dest = dir.join(format!("{stem}-{suffix}-{n}.{ext}"));
        n += 1;
    }
    dest
}

fn encode_ext(img: &LoadedImage) -> &'static str {
    match img.format {
        crate::image::loader::ImageFormat::Jpeg => "jpg",
        _ => "png",
    }
}

fn encode_loaded(img: &LoadedImage) -> Result<Vec<u8>> {
    match img.format {
        crate::image::loader::ImageFormat::Jpeg => {
            let rgb = rgba_to_rgb(img);
            let dynimg = DynamicImage::ImageRgb8(rgb);
            let mut buf = Vec::new();
            dynimg
                .write_to(&mut Cursor::new(&mut buf), ImgFmt::Jpeg)
                .map_err(|e| ViewerError::ImageDecode(e.to_string()))?;
            Ok(buf)
        }
        _ => {
            let rgba = RgbaImage::from_raw(img.width, img.height, img.rgba.to_vec())
                .ok_or_else(|| ViewerError::ImageDecode("encode buffer".into()))?;
            let dynimg = DynamicImage::ImageRgba8(rgba);
            let mut buf = Vec::new();
            dynimg
                .write_to(&mut Cursor::new(&mut buf), ImgFmt::Png)
                .map_err(|e| ViewerError::ImageDecode(e.to_string()))?;
            Ok(buf)
        }
    }
}

fn rgba_to_rgb(img: &LoadedImage) -> RgbImage {
    let mut out = Vec::with_capacity(img.width as usize * img.height as usize * 3);
    for px in img.rgba.chunks_exact(4) {
        out.extend_from_slice(&px[..3]);
    }
    RgbImage::from_raw(img.width, img.height, out)
        .unwrap_or_else(|| RgbImage::new(img.width, img.height))
}

fn rotate_crop(src: &LoadedImage, deg: f32) -> Result<LoadedImage> {
    if deg.abs() < 0.05 {
        return Ok(src.clone());
    }
    let rad = deg.to_radians();
    let (cos, sin) = (rad.cos(), rad.sin());
    let sw = src.width as f32;
    let sh = src.height as f32;
    let corners = [(0.0, 0.0), (sw, 0.0), (sw, sh), (0.0, sh)];
    let rot = |x: f32, y: f32| {
        let cx = sw * 0.5;
        let cy = sh * 0.5;
        let dx = x - cx;
        let dy = y - cy;
        (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
    };
    let mapped: Vec<(f32, f32)> = corners.iter().map(|(x, y)| rot(*x, *y)).collect();
    let min_x = mapped.iter().map(|p| p.0).fold(f32::MAX, f32::min);
    let max_x = mapped.iter().map(|p| p.0).fold(f32::MIN, f32::max);
    let min_y = mapped.iter().map(|p| p.1).fold(f32::MAX, f32::min);
    let max_y = mapped.iter().map(|p| p.1).fold(f32::MIN, f32::max);
    let dw = (max_x - min_x).ceil().max(1.0) as u32;
    let dh = (max_y - min_y).ceil().max(1.0) as u32;
    let mut out = vec![0u8; dw as usize * dh as usize * 4];
    let inv = -rad;
    let (icos, isin) = (inv.cos(), inv.sin());
    for y in 0..dh {
        for x in 0..dw {
            let wx = x as f32 + min_x + 0.5;
            let wy = y as f32 + min_y + 0.5;
            let dx = wx - sw * 0.5;
            let dy = wy - sh * 0.5;
            let sx = sw * 0.5 + dx * icos - dy * isin;
            let sy = sh * 0.5 + dx * isin + dy * icos;
            if let Some(px) = sample_bilinear(src, sx, sy) {
                let di = ((y * dw + x) * 4) as usize;
                out[di..di + 4].copy_from_slice(&px);
            }
        }
    }
    let expanded = LoadedImage {
        rgba: std::sync::Arc::new(out),
        width: dw,
        height: dh,
        format: src.format,
        original_size_bytes: src.original_size_bytes,
        color_source: src.color_source.clone(),
        color_dest: src.color_dest.clone(),
        orientation: src.orientation,
        bit_depth: src.bit_depth,
        color_model: src.color_model.clone(),
    };
    inscribed_crop(&expanded, deg)
}

fn inscribed_crop(src: &LoadedImage, deg: f32) -> Result<LoadedImage> {
    let a = deg.abs().to_radians();
    let c = a.cos();
    let s = a.sin();
    let w = src.width as f32;
    let h = src.height as f32;
    let denom = c * c - s * s;
    if denom.abs() < 0.15 {
        return Ok(src.clone());
    }
    let cw = ((w * c - h * s) / denom).abs().floor() as u32;
    let ch = ((h * c - w * s) / denom).abs().floor() as u32;
    if cw < 2 || ch < 2 || cw >= src.width || ch >= src.height {
        return Ok(src.clone());
    }
    let x = (src.width - cw) / 2;
    let y = (src.height - ch) / 2;
    crop(src, x, y, cw, ch)
}

fn perspective(src: &LoadedImage, quad: [(f32, f32); 4]) -> Result<LoadedImage> {
    let w = ((dist(quad[0], quad[1]) + dist(quad[3], quad[2])) * 0.5)
        .round()
        .clamp(2.0, 16_384.0) as u32;
    let h = ((dist(quad[0], quad[3]) + dist(quad[1], quad[2])) * 0.5)
        .round()
        .clamp(2.0, 16_384.0) as u32;
    let dest = [
        (0.0, 0.0),
        (w as f32 - 1.0, 0.0),
        (w as f32 - 1.0, h as f32 - 1.0),
        (0.0, h as f32 - 1.0),
    ];
    let hmg = homography(dest, quad)
        .ok_or_else(|| ViewerError::ImageDecode("perspective quad is degenerate".into()))?;
    let mut out = vec![0u8; w as usize * h as usize * 4];
    for y in 0..h {
        for x in 0..w {
            let (sx, sy) = apply_h(&hmg, x as f32, y as f32);
            if let Some(px) = sample_bilinear(src, sx, sy) {
                let di = ((y * w + x) * 4) as usize;
                out[di..di + 4].copy_from_slice(&px);
            }
        }
    }
    Ok(LoadedImage {
        rgba: std::sync::Arc::new(out),
        width: w,
        height: h,
        format: src.format,
        original_size_bytes: src.original_size_bytes,
        color_source: src.color_source.clone(),
        color_dest: src.color_dest.clone(),
        orientation: src.orientation,
        bit_depth: src.bit_depth,
        color_model: src.color_model.clone(),
    })
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    (dx * dx + dy * dy).sqrt()
}

fn sample_bilinear(src: &LoadedImage, x: f32, y: f32) -> Option<[u8; 4]> {
    if x < 0.0 || y < 0.0 || x >= src.width as f32 - 1.0 || y >= src.height as f32 - 1.0 {
        if x < 0.0 || y < 0.0 || x >= src.width as f32 || y >= src.height as f32 {
            return None;
        }
        let xi = (x.floor() as u32).min(src.width - 1);
        let yi = (y.floor() as u32).min(src.height - 1);
        let i = ((yi * src.width + xi) * 4) as usize;
        let s = src.rgba.get(i..i + 4)?;
        return Some([s[0], s[1], s[2], s[3]]);
    }
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let p = |xx: u32, yy: u32| {
        let i = ((yy * src.width + xx) * 4) as usize;
        let s = &src.rgba[i..i + 4];
        [s[0] as f32, s[1] as f32, s[2] as f32, s[3] as f32]
    };
    let a = p(x0, y0);
    let b = p(x1, y0);
    let c = p(x0, y1);
    let d = p(x1, y1);
    let mut out = [0u8; 4];
    for i in 0..4 {
        let top = a[i] + (b[i] - a[i]) * fx;
        let bot = c[i] + (d[i] - c[i]) * fx;
        out[i] = (top + (bot - top) * fy).round().clamp(0.0, 255.0) as u8;
    }
    Some(out)
}

fn homography(src: [(f32, f32); 4], dest: [(f32, f32); 4]) -> Option<[f64; 8]> {
    let mut a = [[0.0f64; 8]; 8];
    let mut b = [0.0f64; 8];
    for i in 0..4 {
        let (x, y) = (src[i].0 as f64, src[i].1 as f64);
        let (u, v) = (dest[i].0 as f64, dest[i].1 as f64);
        let r = i * 2;
        a[r] = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y];
        b[r] = u;
        a[r + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y];
        b[r + 1] = v;
    }
    solve8(&mut a, &mut b)
}

fn apply_h(h: &[f64; 8], x: f32, y: f32) -> (f32, f32) {
    let (x, y) = (x as f64, y as f64);
    let den = h[6] * x + h[7] * y + 1.0;
    if den.abs() < 1e-9 {
        return (0.0, 0.0);
    }
    (
        ((h[0] * x + h[1] * y + h[2]) / den) as f32,
        ((h[3] * x + h[4] * y + h[5]) / den) as f32,
    )
}

fn solve8(a: &mut [[f64; 8]; 8], b: &mut [f64; 8]) -> Option<[f64; 8]> {
    for col in 0..8 {
        let mut piv = col;
        for r in col + 1..8 {
            if a[r][col].abs() > a[piv][col].abs() {
                piv = r;
            }
        }
        if a[piv][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, piv);
        b.swap(col, piv);
        let div = a[col][col];
        for v in a[col].iter_mut().skip(col) {
            *v /= div;
        }
        b[col] /= div;
        let pivot = a[col];
        for r in 0..8 {
            if r == col {
                continue;
            }
            let f = a[r][col];
            for (v, p) in a[r].iter_mut().zip(pivot.iter()).skip(col) {
                *v -= f * p;
            }
            b[r] -= f * b[col];
        }
    }
    Some(*b)
}

fn auto_straighten_angle(src: &LoadedImage) -> f32 {
    let scale = (256.0 / src.width.max(src.height) as f32).min(1.0);
    let tw = ((src.width as f32) * scale).round().max(8.0) as u32;
    let th = ((src.height as f32) * scale).round().max(8.0) as u32;
    let small = match resize_filtered(src, tw, th, ResizeFilter::Bilinear) {
        Ok(s) => s,
        Err(_) => return 0.0,
    };
    let mut best_a = 0.0f32;
    let mut best_s = f64::MIN;
    let mut a = -12.0f32;
    while a <= 12.0 {
        let score = projection_score(&small, a);
        if score > best_s {
            best_s = score;
            best_a = a;
        }
        a += 0.5;
    }
    if best_s < 1.0 {
        0.0
    } else {
        -best_a
    }
}

fn projection_score(src: &LoadedImage, deg: f32) -> f64 {
    let rad = deg.to_radians();
    let (cos, sin) = (rad.cos(), rad.sin());
    let mut rows = vec![0.0f64; src.height as usize];
    let mut counts = vec![0u32; src.height as usize];
    for y in 1..src.height.saturating_sub(1) {
        for x in 1..src.width.saturating_sub(1) {
            let i = ((y * src.width + x) * 4) as usize;
            let l = luma(&src.rgba[i..]);
            let gx = luma(&src.rgba[i + 4..]) as i32 - luma(&src.rgba[i - 4..]) as i32;
            let gy = luma(&src.rgba[i + src.width as usize * 4..]) as i32
                - luma(&src.rgba[i - src.width as usize * 4..]) as i32;
            let mag = (gx.abs() + gy.abs()) as f64;
            if mag < 20.0 {
                continue;
            }
            let cx = x as f32 - src.width as f32 * 0.5;
            let cy = y as f32 - src.height as f32 * 0.5;
            let ry = (cx * sin + cy * cos) + src.height as f32 * 0.5;
            let yi = ry.round() as i32;
            if yi >= 0 && (yi as u32) < src.height {
                rows[yi as usize] += mag + l as f64 * 0.01;
                counts[yi as usize] += 1;
            }
        }
    }
    let mut mean = 0.0;
    let mut n = 0.0;
    for (v, c) in rows.iter().zip(&counts) {
        if *c > 0 {
            mean += *v;
            n += 1.0;
        }
    }
    if n < 4.0 {
        return 0.0;
    }
    mean /= n;
    rows.iter()
        .zip(&counts)
        .filter(|(_, c)| **c > 0)
        .map(|(v, _)| {
            let d = *v - mean;
            d * d
        })
        .sum()
}

fn luma(px: &[u8]) -> u8 {
    ((u16::from(px[0]) * 30 + u16::from(px[1]) * 59 + u16::from(px[2]) * 11) / 100) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::loader::ImageFormat;

    fn solid(w: u32, h: u32, rgb: [u8; 3]) -> LoadedImage {
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
        LoadedImage {
            rgba: std::sync::Arc::new(rgba),
            width: w,
            height: h,
            format: ImageFormat::Png,
            original_size_bytes: 0,
            ..LoadedImage::meta_defaults()
        }
    }

    fn stripe() -> LoadedImage {
        let mut img = solid(40, 20, [240, 240, 240]);
        let mut buf = img.rgba.as_ref().clone();
        for y in 8..12 {
            for x in 0..40 {
                let i = ((y * 40 + x) * 4) as usize;
                buf[i..i + 3].copy_from_slice(&[10, 10, 10]);
            }
        }
        img.rgba = std::sync::Arc::new(buf);
        img
    }

    #[test]
    fn parse_resize_variants() {
        assert!(matches!(
            parse_resize_spec("50%"),
            Some(ResizeSpec::Percent(p)) if (p - 0.5).abs() < 1e-4
        ));
        let (s, f) = parse_resize_line("800x filter=nearest").unwrap();
        assert_eq!(f, ResizeFilter::Nearest);
        assert!(matches!(
            s,
            ResizeSpec::Pixels {
                w: Some(800),
                h: None
            }
        ));
        assert!(matches!(
            parse_resize_spec("10cmx15cm"),
            Some(ResizeSpec::Cm { .. })
        ));
    }

    #[test]
    fn parse_canvas_pad() {
        assert_eq!(parse_canvas_line("+10+5", 100, 50), Some((120, 60)));
        assert_eq!(parse_canvas_line("200x80", 10, 10), Some((200, 80)));
    }

    #[test]
    fn resize_percent_keeps_aspect() {
        let src = solid(20, 10, [9, 8, 7]);
        let out = apply_edit(
            &src,
            &EditOp::Resize {
                spec: ResizeSpec::Percent(0.5),
                filter: ResizeFilter::Nearest,
            },
        )
        .unwrap();
        assert_eq!((out.width, out.height), (10, 5));
    }

    #[test]
    fn crop_keep_width_restores_side() {
        let src = solid(20, 10, [1, 2, 3]);
        let out = apply_edit(
            &src,
            &EditOp::Crop {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
                aspect: None,
                keep: CropKeep::Width,
            },
        )
        .unwrap();
        assert_eq!(out.width, 20);
        assert_eq!(out.height, 20);
    }

    #[test]
    fn canvas_centers() {
        let src = solid(4, 2, [255, 0, 0]);
        let out = apply_edit(
            &src,
            &EditOp::Canvas {
                width: 8,
                height: 6,
                fill: [0, 0, 0, 255],
            },
        )
        .unwrap();
        assert_eq!((out.width, out.height), (8, 6));
    }

    #[test]
    fn perspective_identity_keeps_size() {
        let src = solid(8, 6, [20, 40, 80]);
        let out = apply_edit(
            &src,
            &EditOp::Perspective {
                quad: [(0.0, 0.0), (7.0, 0.0), (7.0, 5.0), (0.0, 5.0)],
            },
        )
        .unwrap();
        assert!((out.width as i32 - 8).abs() <= 1);
        assert!((out.height as i32 - 6).abs() <= 1);
    }

    #[test]
    fn straighten_flat_line_is_noop() {
        let src = stripe();
        let out = apply_edit(
            &src,
            &EditOp::Straighten {
                x0: 0.0,
                y0: 10.0,
                x1: 39.0,
                y1: 10.0,
            },
        )
        .unwrap();
        assert_eq!((out.width, out.height), (src.width, src.height));
    }

    #[test]
    fn sibling_does_not_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.png");
        let src = solid(4, 4, [8, 8, 8]);
        let first = save_sibling(&path, &src, "crop").unwrap();
        std::fs::write(&path, b"orig").unwrap();
        let second = save_sibling(&path, &src, "crop").unwrap();
        assert_ne!(first, second);
        assert!(second
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .contains("crop-2"));
        assert_eq!(std::fs::read(&path).unwrap(), b"orig");
    }

    #[test]
    fn auto_straighten_on_level_stripe_is_small() {
        let src = stripe();
        let a = auto_straighten_angle(&src);
        assert!(a.abs() < 3.0, "{a}");
    }
}
