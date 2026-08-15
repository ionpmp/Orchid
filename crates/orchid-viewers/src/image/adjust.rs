//! Tone and color corrections. Results are meant to be saved as a sibling file.

use crate::error::Result;
use crate::image::edit::save_sibling;
use crate::image::loader::{load_image_file, LoadedImage};

use std::path::{Path, PathBuf};

/// One correction pass (or a packed slider set).
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum AdjustOp {
    /// Combined slider / curve / mixer payload.
    Params(AdjustParams),
    /// Per-channel 0.5–99.5% stretch.
    AutoLevels,
    /// Shared luma stretch (keeps hue).
    AutoContrast,
    /// Gray-world white balance.
    AutoColor,
    /// Rec. 601 gray.
    Grayscale,
    /// Warm brown tone.
    Sepia,
    /// Photographic negative.
    Invert,
    /// Quantize each channel to `levels` steps (2–32).
    Posterize {
        /// Number of bins.
        levels: u8,
    },
    /// Invert tones above `threshold`.
    Solarize {
        /// 0–255 cutoff.
        threshold: u8,
    },
    /// Hard black / white split.
    Threshold {
        /// 0–255 cutoff.
        threshold: u8,
    },
}

impl AdjustOp {
    /// Filename suffix for [`save_sibling`].
    #[must_use]
    pub fn suffix(&self) -> &'static str {
        match self {
            Self::Params(_) => "adjust",
            Self::AutoLevels => "autolevels",
            Self::AutoContrast => "autocontrast",
            Self::AutoColor => "autocolor",
            Self::Grayscale => "gray",
            Self::Sepia => "sepia",
            Self::Invert => "invert",
            Self::Posterize { .. } => "posterize",
            Self::Solarize { .. } => "solarize",
            Self::Threshold { .. } => "threshold",
        }
    }
}

/// Optional sliders; empty fields are skipped.
#[derive(Debug, Clone, Default, PartialEq)]
#[allow(missing_docs)]
pub struct AdjustParams {
    pub brightness: Option<f32>,
    pub contrast: Option<f32>,
    pub exposure: Option<f32>,
    pub highlights: Option<f32>,
    pub shadows: Option<f32>,
    pub temperature: Option<f32>,
    pub tint: Option<f32>,
    pub saturation: Option<f32>,
    pub vibrance: Option<f32>,
    pub hue: Option<f32>,
    pub gamma: Option<f32>,
    /// Black / mid-gamma / white in 0–255.
    pub levels: Option<(f32, f32, f32)>,
    pub curves: Option<CurveSet>,
    pub selective: Option<(SelectiveBand, [f32; 4])>,
    pub mixer: Option<[[f32; 3]; 3]>,
    pub posterize: Option<u8>,
    pub solarize: Option<u8>,
    pub threshold: Option<u8>,
}

/// RGB plus optional per-channel curves (control points 0–255).
#[derive(Debug, Clone, Default, PartialEq)]
#[allow(missing_docs)]
pub struct CurveSet {
    pub rgb: Vec<(f32, f32)>,
    pub r: Vec<(f32, f32)>,
    pub g: Vec<(f32, f32)>,
    pub b: Vec<(f32, f32)>,
}

/// Photoshop-style selective-color band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectiveBand {
    /// Reds.
    Reds,
    /// Yellows.
    Yellows,
    /// Greens.
    Greens,
    /// Cyans.
    Cyans,
    /// Blues.
    Blues,
    /// Magentas.
    Magentas,
    /// Whites.
    Whites,
    /// Neutrals.
    Neutrals,
    /// Blacks.
    Blacks,
}

impl SelectiveBand {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "reds" | "red" => Some(Self::Reds),
            "yellows" | "yellow" => Some(Self::Yellows),
            "greens" | "green" => Some(Self::Greens),
            "cyans" | "cyan" => Some(Self::Cyans),
            "blues" | "blue" => Some(Self::Blues),
            "magentas" | "magenta" => Some(Self::Magentas),
            "whites" | "white" => Some(Self::Whites),
            "neutrals" | "neutral" => Some(Self::Neutrals),
            "blacks" | "black" => Some(Self::Blacks),
            _ => None,
        }
    }
}

/// Apply a correction to a decoded image.
///
/// # Errors
///
/// Corrupt RGBA buffer.
pub fn apply_adjust(src: &LoadedImage, op: &AdjustOp) -> Result<LoadedImage> {
    match op {
        AdjustOp::Params(p) => apply_params(src, p),
        AdjustOp::AutoLevels => map_lut(src, &auto_levels_luts(src)),
        AdjustOp::AutoContrast => {
            let st = luma_stats(src);
            Ok(map_pixels(src, |px| auto_contrast_px(px, st)))
        }
        AdjustOp::AutoColor => {
            let st = luma_stats(src);
            Ok(map_pixels(src, |px| auto_color_px(px, st)))
        }
        AdjustOp::Grayscale => Ok(map_pixels(src, gray_px)),
        AdjustOp::Sepia => Ok(map_pixels(src, sepia_px)),
        AdjustOp::Invert => Ok(map_pixels(src, invert_px)),
        AdjustOp::Posterize { levels } => {
            let n = (*levels).clamp(2, 32);
            Ok(map_pixels(src, |px| posterize_px(px, n)))
        }
        AdjustOp::Solarize { threshold } => {
            let t = *threshold;
            Ok(map_pixels(src, |px| solarize_px(px, t)))
        }
        AdjustOp::Threshold { threshold } => {
            let t = *threshold;
            Ok(map_pixels(src, |px| threshold_px(px, t)))
        }
    }
}

/// Load, adjust, write a sibling. Returns the new path.
///
/// # Errors
///
/// I/O or decode.
pub fn apply_adjust_file(path: &Path, op: &AdjustOp) -> Result<PathBuf> {
    let out = if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(crate::image::raw::is_raw_file_extension)
    {
        crate::image::raw::apply_adjust_raw_file(path, op)?
    } else {
        let src = load_image_file(path)?;
        apply_adjust(&src, op)?
    };
    save_sibling(path, &out, op.suffix())
}

/// `key=value` lines, or a single token (`grayscale`, `auto-levels`, …).
#[must_use]
pub fn parse_adjust_line(raw: &str) -> Option<AdjustOp> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    let single = !t.contains('\n') && !t.contains(" | ");
    if single && !t.contains('=') {
        return match lower.as_str() {
            "grayscale" | "grey" | "gray" => Some(AdjustOp::Grayscale),
            "sepia" => Some(AdjustOp::Sepia),
            "invert" | "negative" => Some(AdjustOp::Invert),
            "auto-levels" | "autolevels" | "auto_levels" => Some(AdjustOp::AutoLevels),
            "auto-contrast" | "autocontrast" => Some(AdjustOp::AutoContrast),
            "auto-color" | "autocolor" | "auto-wb" => Some(AdjustOp::AutoColor),
            "develop" | "demosaic" | "raw" => Some(AdjustOp::Params(AdjustParams {
                exposure: Some(0.0),
                ..AdjustParams::default()
            })),
            _ => None,
        };
    }
    if single {
        if let Some(rest) = strip_key(&lower, "posterize") {
            return rest
                .parse()
                .ok()
                .map(|levels| AdjustOp::Posterize { levels });
        }
        if let Some(rest) = strip_key(&lower, "solarize") {
            return rest
                .parse()
                .ok()
                .map(|threshold| AdjustOp::Solarize { threshold });
        }
        if let Some(rest) = strip_key(&lower, "threshold") {
            return rest
                .parse()
                .ok()
                .map(|threshold| AdjustOp::Threshold { threshold });
        }
    }
    let p = unpack_params(t);
    if p == AdjustParams::default() {
        return None;
    }
    Some(AdjustOp::Params(p))
}

/// Inverse of [`unpack_params`] for the FM / viewer prompt.
#[must_use]
pub fn pack_adjust_params(p: &AdjustParams) -> String {
    let mut lines = Vec::new();
    pack_f(&mut lines, "brightness", p.brightness);
    pack_f(&mut lines, "contrast", p.contrast);
    pack_f(&mut lines, "exposure", p.exposure);
    pack_f(&mut lines, "highlights", p.highlights);
    pack_f(&mut lines, "shadows", p.shadows);
    pack_f(&mut lines, "temp", p.temperature);
    pack_f(&mut lines, "tint", p.tint);
    pack_f(&mut lines, "saturation", p.saturation);
    pack_f(&mut lines, "vibrance", p.vibrance);
    pack_f(&mut lines, "hue", p.hue);
    pack_f(&mut lines, "gamma", p.gamma);
    if let Some((b, g, w)) = p.levels {
        lines.push(format!("levels={b},{g},{w}"));
    }
    if let Some(c) = &p.curves {
        if !c.rgb.is_empty() {
            lines.push(format!("curves={}", fmt_pts(&c.rgb)));
        }
        if !c.r.is_empty() {
            lines.push(format!("curves.r={}", fmt_pts(&c.r)));
        }
        if !c.g.is_empty() {
            lines.push(format!("curves.g={}", fmt_pts(&c.g)));
        }
        if !c.b.is_empty() {
            lines.push(format!("curves.b={}", fmt_pts(&c.b)));
        }
    }
    if let Some((band, cmyk)) = p.selective {
        let name = match band {
            SelectiveBand::Reds => "reds",
            SelectiveBand::Yellows => "yellows",
            SelectiveBand::Greens => "greens",
            SelectiveBand::Cyans => "cyans",
            SelectiveBand::Blues => "blues",
            SelectiveBand::Magentas => "magentas",
            SelectiveBand::Whites => "whites",
            SelectiveBand::Neutrals => "neutrals",
            SelectiveBand::Blacks => "blacks",
        };
        lines.push(format!(
            "selective={name} {},{},{},{}",
            cmyk[0], cmyk[1], cmyk[2], cmyk[3]
        ));
    }
    if let Some(m) = p.mixer {
        lines.push(format!(
            "mixer={},{},{};{},{},{};{},{},{}",
            m[0][0], m[0][1], m[0][2], m[1][0], m[1][1], m[1][2], m[2][0], m[2][1], m[2][2]
        ));
    }
    if let Some(n) = p.posterize {
        lines.push(format!("posterize={n}"));
    }
    if let Some(n) = p.solarize {
        lines.push(format!("solarize={n}"));
    }
    if let Some(n) = p.threshold {
        lines.push(format!("threshold={n}"));
    }
    lines.join(" | ")
}

fn unpack_params(raw: &str) -> AdjustParams {
    let mut p = AdjustParams::default();
    let mut curves = CurveSet::default();
    for line in kv_chunks(raw) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim();
        match k.trim().to_ascii_lowercase().as_str() {
            "brightness" | "bright" => p.brightness = v.parse().ok(),
            "contrast" => p.contrast = v.parse().ok(),
            "exposure" | "ev" => p.exposure = v.parse().ok(),
            "highlights" | "hi" => p.highlights = v.parse().ok(),
            "shadows" | "sh" => p.shadows = v.parse().ok(),
            "temp" | "temperature" | "wb" => p.temperature = v.parse().ok(),
            "tint" => p.tint = v.parse().ok(),
            "saturation" | "sat" => p.saturation = v.parse().ok(),
            "vibrance" => p.vibrance = v.parse().ok(),
            "hue" => p.hue = v.parse().ok(),
            "gamma" => p.gamma = v.parse().ok(),
            "levels" => p.levels = parse_levels(v),
            "curves" => curves.rgb = parse_pts(v),
            "curves.r" => curves.r = parse_pts(v),
            "curves.g" => curves.g = parse_pts(v),
            "curves.b" => curves.b = parse_pts(v),
            "selective" => p.selective = parse_selective(v),
            "mixer" => p.mixer = parse_mixer(v),
            "posterize" => p.posterize = v.parse().ok(),
            "solarize" => p.solarize = v.parse().ok(),
            "threshold" => p.threshold = v.parse().ok(),
            _ => {}
        }
    }
    if !curves.rgb.is_empty()
        || !curves.r.is_empty()
        || !curves.g.is_empty()
        || !curves.b.is_empty()
    {
        p.curves = Some(curves);
    }
    p
}

fn apply_params(src: &LoadedImage, p: &AdjustParams) -> Result<LoadedImage> {
    let stats = if p.highlights.is_some() || p.shadows.is_some() {
        Some(luma_stats(src))
    } else {
        None
    };
    let rgb_lut = p.curves.as_ref().map(|c| build_lut(&c.rgb));
    let r_lut = p.curves.as_ref().map(|c| build_lut(&c.r));
    let g_lut = p.curves.as_ref().map(|c| build_lut(&c.g));
    let b_lut = p.curves.as_ref().map(|c| build_lut(&c.b));
    let levels_lut = p.levels.map(|(bk, gm, wh)| levels_lut(bk, gm, wh));
    Ok(map_pixels(src, |px| {
        let mut c = [px[0] as f32, px[1] as f32, px[2] as f32];
        if let Some(ev) = p.exposure {
            let m = 2f32.powf(ev.clamp(-4.0, 4.0));
            for v in &mut c {
                *v *= m;
            }
        }
        if let (Some(h), Some(st)) = (p.highlights, stats) {
            let w = smoothstep(st.mid, st.hi, luma3(c));
            let k = 1.0 - (h / 100.0).clamp(-1.0, 1.0) * w * 0.6;
            for v in &mut c {
                *v *= k;
            }
        }
        if let (Some(s), Some(st)) = (p.shadows, stats) {
            let w = 1.0 - smoothstep(st.lo, st.mid, luma3(c));
            let add = (s / 100.0).clamp(-1.0, 1.0) * 48.0 * w;
            for v in &mut c {
                *v += add;
            }
        }
        if let Some(lut) = levels_lut {
            for v in &mut c {
                *v = lut[(*v).round().clamp(0.0, 255.0) as usize] as f32;
            }
        }
        if let Some(lut) = &rgb_lut {
            for v in &mut c {
                *v = lut[(*v).round().clamp(0.0, 255.0) as usize] as f32;
            }
        }
        if let Some(lut) = &r_lut {
            c[0] = lut[c[0].round().clamp(0.0, 255.0) as usize] as f32;
        }
        if let Some(lut) = &g_lut {
            c[1] = lut[c[1].round().clamp(0.0, 255.0) as usize] as f32;
        }
        if let Some(lut) = &b_lut {
            c[2] = lut[c[2].round().clamp(0.0, 255.0) as usize] as f32;
        }
        if let Some(t) = p.temperature {
            let u = (t / 100.0).clamp(-1.0, 1.0);
            c[0] *= 1.0 + u * 0.35;
            c[2] *= 1.0 - u * 0.35;
        }
        if let Some(t) = p.tint {
            let u = (t / 100.0).clamp(-1.0, 1.0);
            c[1] *= 1.0 - u * 0.28;
            c[0] *= 1.0 + u * 0.12;
            c[2] *= 1.0 + u * 0.12;
        }
        if let Some(b) = p.brightness {
            let add = (b / 100.0).clamp(-1.0, 1.0) * 80.0;
            for v in &mut c {
                *v += add;
            }
        }
        if let Some(k) = p.contrast {
            let f = 1.0 + (k / 100.0).clamp(-1.0, 1.0);
            for v in &mut c {
                *v = (*v - 128.0) * f + 128.0;
            }
        }
        if p.saturation.is_some() || p.vibrance.is_some() || p.hue.is_some() {
            let (h, s, l) = rgb_to_hsl(c[0], c[1], c[2]);
            let mut hh = h;
            let mut ss = s;
            if let Some(d) = p.hue {
                hh = (hh + d / 360.0).rem_euclid(1.0);
            }
            if let Some(sat) = p.saturation {
                ss = (ss + sat / 100.0).clamp(0.0, 1.0);
            }
            if let Some(vib) = p.vibrance {
                let boost = (vib / 100.0).clamp(-1.0, 1.0) * (1.0 - ss);
                ss = (ss + boost).clamp(0.0, 1.0);
            }
            c = hsl_to_rgb(hh, ss, l);
        }
        if let Some((band, cmyk)) = p.selective {
            c = selective_px(c, band, cmyk);
        }
        if let Some(m) = p.mixer {
            let (r, g, b) = (c[0], c[1], c[2]);
            c[0] = m[0][0] * r + m[0][1] * g + m[0][2] * b;
            c[1] = m[1][0] * r + m[1][1] * g + m[1][2] * b;
            c[2] = m[2][0] * r + m[2][1] * g + m[2][2] * b;
        }
        if let Some(g) = p.gamma.filter(|g| *g > 0.05) {
            let exp = 1.0 / g;
            for v in &mut c {
                let n = (*v / 255.0).clamp(0.0, 1.0);
                *v = n.powf(exp) * 255.0;
            }
        }
        let mut out = [clamp_u8(c[0]), clamp_u8(c[1]), clamp_u8(c[2]), px[3]];
        if let Some(n) = p.posterize {
            out = posterize_px(out, n.clamp(2, 32));
        }
        if let Some(t) = p.solarize {
            out = solarize_px(out, t);
        }
        if let Some(t) = p.threshold {
            out = threshold_px(out, t);
        }
        out
    }))
}

fn kv_chunks(raw: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for line in raw.lines() {
        for part in line.split(" | ") {
            let part = part.trim();
            if !part.is_empty() {
                out.push(part);
            }
        }
    }
    out
}

fn map_pixels(src: &LoadedImage, mut f: impl FnMut([u8; 4]) -> [u8; 4]) -> LoadedImage {
    let mut out = src.rgba.as_ref().clone();
    for px in out.chunks_exact_mut(4) {
        let next = f([px[0], px[1], px[2], px[3]]);
        px.copy_from_slice(&next);
    }
    wrap(src, out)
}

fn map_lut(src: &LoadedImage, luts: &[[u8; 256]; 3]) -> Result<LoadedImage> {
    Ok(map_pixels(src, |px| {
        [
            luts[0][px[0] as usize],
            luts[1][px[1] as usize],
            luts[2][px[2] as usize],
            px[3],
        ]
    }))
}

fn wrap(src: &LoadedImage, rgba: Vec<u8>) -> LoadedImage {
    LoadedImage {
        rgba: std::sync::Arc::new(rgba),
        width: src.width,
        height: src.height,
        format: src.format,
        original_size_bytes: src.original_size_bytes,
        color_source: src.color_source.clone(),
        color_dest: src.color_dest.clone(),
        orientation: src.orientation,
        bit_depth: src.bit_depth,
        color_model: src.color_model.clone(),
    }
}

fn auto_levels_luts(src: &LoadedImage) -> [[u8; 256]; 3] {
    let mut hist = [[0u32; 256]; 3];
    for px in src.rgba.chunks_exact(4) {
        hist[0][px[0] as usize] += 1;
        hist[1][px[1] as usize] += 1;
        hist[2][px[2] as usize] += 1;
    }
    let n = (src.width as u64).saturating_mul(src.height as u64).max(1);
    let lo_n = ((n as f64) * 0.005) as u32;
    let hi_n = ((n as f64) * 0.995) as u32;
    let mut luts = [[0u8; 256]; 3];
    for ch in 0..3 {
        let (lo, hi) = percentile_span(&hist[ch], lo_n, hi_n);
        let span = (hi - lo).max(1) as f32;
        for (i, slot) in luts[ch].iter_mut().enumerate() {
            *slot = (((i as f32 - lo as f32) / span) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
    luts
}

fn percentile_span(hist: &[u32; 256], lo_n: u32, hi_n: u32) -> (u32, u32) {
    let mut acc = 0u32;
    let mut lo = 0u32;
    let mut hi = 255u32;
    for (i, c) in hist.iter().enumerate() {
        acc += *c;
        if acc >= lo_n {
            lo = i as u32;
            break;
        }
    }
    acc = 0;
    for (i, c) in hist.iter().enumerate() {
        acc += *c;
        if acc >= hi_n {
            hi = i as u32;
            break;
        }
    }
    (lo, hi.max(lo + 1))
}

#[derive(Clone, Copy)]
struct LumaStats {
    lo: f32,
    mid: f32,
    hi: f32,
    mean_r: f32,
    mean_g: f32,
    mean_b: f32,
}

fn luma_stats(src: &LoadedImage) -> LumaStats {
    let mut hist = [0u32; 256];
    let mut sr = 0f64;
    let mut sg = 0f64;
    let mut sb = 0f64;
    let mut n = 0f64;
    for px in src.rgba.chunks_exact(4) {
        hist[luma_u8(px) as usize] += 1;
        sr += f64::from(px[0]);
        sg += f64::from(px[1]);
        sb += f64::from(px[2]);
        n += 1.0;
    }
    n = n.max(1.0);
    let (lo, hi) = percentile_span(&hist, (n * 0.02) as u32, (n * 0.98) as u32);
    LumaStats {
        lo: lo as f32,
        mid: (lo + hi) as f32 * 0.5,
        hi: hi as f32,
        mean_r: (sr / n) as f32,
        mean_g: (sg / n) as f32,
        mean_b: (sb / n) as f32,
    }
}

fn auto_contrast_px(px: [u8; 4], st: LumaStats) -> [u8; 4] {
    let span = (st.hi - st.lo).max(1.0);
    let scale = 255.0 / span;
    [
        clamp_u8((f32::from(px[0]) - st.lo) * scale),
        clamp_u8((f32::from(px[1]) - st.lo) * scale),
        clamp_u8((f32::from(px[2]) - st.lo) * scale),
        px[3],
    ]
}

fn auto_color_px(px: [u8; 4], st: LumaStats) -> [u8; 4] {
    let avg = (st.mean_r + st.mean_g + st.mean_b) / 3.0;
    let sr = if st.mean_r > 1.0 {
        avg / st.mean_r
    } else {
        1.0
    };
    let sg = if st.mean_g > 1.0 {
        avg / st.mean_g
    } else {
        1.0
    };
    let sb = if st.mean_b > 1.0 {
        avg / st.mean_b
    } else {
        1.0
    };
    [
        clamp_u8(f32::from(px[0]) * sr),
        clamp_u8(f32::from(px[1]) * sg),
        clamp_u8(f32::from(px[2]) * sb),
        px[3],
    ]
}

fn gray_px(px: [u8; 4]) -> [u8; 4] {
    let y = luma_u8(&px);
    [y, y, y, px[3]]
}

fn sepia_px(px: [u8; 4]) -> [u8; 4] {
    let r = f32::from(px[0]);
    let g = f32::from(px[1]);
    let b = f32::from(px[2]);
    [
        clamp_u8(0.393 * r + 0.769 * g + 0.189 * b),
        clamp_u8(0.349 * r + 0.686 * g + 0.168 * b),
        clamp_u8(0.272 * r + 0.534 * g + 0.131 * b),
        px[3],
    ]
}

fn invert_px(px: [u8; 4]) -> [u8; 4] {
    [255 - px[0], 255 - px[1], 255 - px[2], px[3]]
}

fn posterize_px(px: [u8; 4], levels: u8) -> [u8; 4] {
    let steps = f32::from(levels.saturating_sub(1)).max(1.0);
    let q = |v: u8| ((f32::from(v) / 255.0 * steps).round() / steps * 255.0).round() as u8;
    [q(px[0]), q(px[1]), q(px[2]), px[3]]
}

fn solarize_px(px: [u8; 4], t: u8) -> [u8; 4] {
    let s = |v: u8| if v > t { 255 - v } else { v };
    [s(px[0]), s(px[1]), s(px[2]), px[3]]
}

fn threshold_px(px: [u8; 4], t: u8) -> [u8; 4] {
    let y = if luma_u8(&px) >= t { 255 } else { 0 };
    [y, y, y, px[3]]
}

fn selective_px(c: [f32; 3], band: SelectiveBand, cmyk: [f32; 4]) -> [f32; 3] {
    let mask = band_mask(c, band);
    if mask <= 0.001 {
        return c;
    }
    let (cy, m, y, k) = (
        (cmyk[0] / 100.0).clamp(-1.0, 1.0) * mask,
        (cmyk[1] / 100.0).clamp(-1.0, 1.0) * mask,
        (cmyk[2] / 100.0).clamp(-1.0, 1.0) * mask,
        (cmyk[3] / 100.0).clamp(-1.0, 1.0) * mask,
    );
    [
        c[0] * (1.0 - cy) * (1.0 - k),
        c[1] * (1.0 - m) * (1.0 - k),
        c[2] * (1.0 - y) * (1.0 - k),
    ]
}

fn band_mask(c: [f32; 3], band: SelectiveBand) -> f32 {
    let (h, s, l) = rgb_to_hsl(c[0], c[1], c[2]);
    let hue = h * 360.0;
    let near = |center: f32| {
        let d = (hue - center).abs().min(360.0 - (hue - center).abs());
        (1.0 - d / 40.0).clamp(0.0, 1.0) * s
    };
    match band {
        SelectiveBand::Reds => near(0.0).max(near(360.0)),
        SelectiveBand::Yellows => near(60.0),
        SelectiveBand::Greens => near(120.0),
        SelectiveBand::Cyans => near(180.0),
        SelectiveBand::Blues => near(240.0),
        SelectiveBand::Magentas => near(300.0),
        SelectiveBand::Whites => ((l - 0.75) / 0.25).clamp(0.0, 1.0) * (1.0 - s),
        SelectiveBand::Blacks => ((0.25 - l) / 0.25).clamp(0.0, 1.0),
        SelectiveBand::Neutrals => {
            (1.0 - (s - 0.15).abs() / 0.25).clamp(0.0, 1.0) * (1.0 - (l - 0.5).abs() * 2.0).max(0.0)
        }
    }
}

fn levels_lut(black: f32, gamma: f32, white: f32) -> [u8; 256] {
    let lo = black.clamp(0.0, 254.0);
    let hi = white.clamp(lo + 1.0, 255.0);
    let g = gamma.clamp(0.1, 4.0);
    let mut lut = [0u8; 256];
    for (i, slot) in lut.iter_mut().enumerate() {
        let n = ((i as f32 - lo) / (hi - lo)).clamp(0.0, 1.0);
        *slot = (n.powf(1.0 / g) * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    lut
}

fn build_lut(pts: &[(f32, f32)]) -> [u8; 256] {
    let mut p: Vec<(f32, f32)> = pts
        .iter()
        .map(|(x, y)| (x.clamp(0.0, 255.0), y.clamp(0.0, 255.0)))
        .collect();
    if p.is_empty() {
        p.extend([(0.0, 0.0), (255.0, 255.0)]);
    }
    p.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    if p[0].0 > 0.0 {
        p.insert(0, (0.0, p[0].1));
    }
    if p.last().map(|q| q.0).unwrap_or(0.0) < 255.0 {
        let y = p.last().map(|q| q.1).unwrap_or(255.0);
        p.push((255.0, y));
    }
    let mut lut = [0u8; 256];
    let mut i = 0usize;
    for (x, slot) in lut.iter_mut().enumerate() {
        let xf = x as f32;
        while i + 1 < p.len() && p[i + 1].0 < xf {
            i += 1;
        }
        let (x0, y0) = p[i.min(p.len() - 1)];
        let (x1, y1) = p[(i + 1).min(p.len() - 1)];
        let t = if (x1 - x0).abs() < 0.001 {
            0.0
        } else {
            ((xf - x0) / (x1 - x0)).clamp(0.0, 1.0)
        };
        *slot = (y0 + (y1 - y0) * t).round().clamp(0.0, 255.0) as u8;
    }
    lut
}

fn parse_pts(raw: &str) -> Vec<(f32, f32)> {
    raw.split([';', ' '])
        .filter_map(|pair| {
            let (a, b) = pair.split_once(',')?;
            Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
        })
        .collect()
}

fn fmt_pts(pts: &[(f32, f32)]) -> String {
    pts.iter()
        .map(|(x, y)| format!("{x},{y}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_levels(raw: &str) -> Option<(f32, f32, f32)> {
    let mut it = raw.split(',');
    Some((
        it.next()?.trim().parse().ok()?,
        it.next()?.trim().parse().ok()?,
        it.next()?.trim().parse().ok()?,
    ))
}

fn parse_selective(raw: &str) -> Option<(SelectiveBand, [f32; 4])> {
    let (band, rest) = raw.split_once(|c: char| c.is_whitespace())?;
    let b = SelectiveBand::parse(band)?;
    let nums: Vec<f32> = rest
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    if nums.len() < 4 {
        return None;
    }
    Some((b, [nums[0], nums[1], nums[2], nums[3]]))
}

fn parse_mixer(raw: &str) -> Option<[[f32; 3]; 3]> {
    let rows: Vec<&str> = raw.split(';').collect();
    if rows.len() != 3 {
        return None;
    }
    let mut m = [[0f32; 3]; 3];
    for (i, row) in rows.iter().enumerate() {
        let nums: Vec<f32> = row
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if nums.len() != 3 {
            return None;
        }
        m[i] = [nums[0], nums[1], nums[2]];
    }
    Some(m)
}

fn strip_key<'a>(lower_line: &'a str, key: &str) -> Option<&'a str> {
    let line = lower_line.lines().next()?.trim();
    line.strip_prefix(key)?.strip_prefix('=')
}

fn pack_f(lines: &mut Vec<String>, k: &str, v: Option<f32>) {
    if let Some(v) = v {
        lines.push(format!("{k}={v}"));
    }
}

fn luma_u8(px: &[u8]) -> u8 {
    ((u16::from(px[0]) * 30 + u16::from(px[1]) * 59 + u16::from(px[2]) * 11) / 100) as u8
}

fn luma3(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

fn clamp_u8(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    if (e1 - e0).abs() < 0.001 {
        return if x >= e1 { 1.0 } else { 0.0 };
    }
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let r = (r / 255.0).clamp(0.0, 1.0);
    let g = (g / 255.0).clamp(0.0, 1.0);
    let b = (b / 255.0).clamp(0.0, 1.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) * 0.5;
    let d = max - min;
    if d < 1e-6 {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    (h, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    let hue = |p: f32, q: f32, mut t: f32| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    if s < 1e-6 {
        let v = l * 255.0;
        return [v, v, v];
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    [
        hue(p, q, h + 1.0 / 3.0) * 255.0,
        hue(p, q, h) * 255.0,
        hue(p, q, h - 1.0 / 3.0) * 255.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::loader::ImageFormat;

    fn rgb(w: u32, h: u32, pix: &[[u8; 3]]) -> LoadedImage {
        let mut rgba = Vec::new();
        for p in pix {
            rgba.extend_from_slice(&[p[0], p[1], p[2], 255]);
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

    #[test]
    fn invert_flips_channels() {
        let src = rgb(1, 1, &[[10, 20, 30]]);
        let out = apply_adjust(&src, &AdjustOp::Invert).unwrap();
        assert_eq!(&out.rgba[..3], [245, 235, 225]);
    }

    #[test]
    fn grayscale_equalizes() {
        let src = rgb(1, 1, &[[255, 0, 0]]);
        let out = apply_adjust(&src, &AdjustOp::Grayscale).unwrap();
        assert_eq!(out.rgba[0], out.rgba[1]);
        assert_eq!(out.rgba[1], out.rgba[2]);
    }

    #[test]
    fn threshold_splits() {
        let src = rgb(2, 1, &[[10, 10, 10], [240, 240, 240]]);
        let out = apply_adjust(&src, &AdjustOp::Threshold { threshold: 128 }).unwrap();
        assert_eq!(&out.rgba[0..3], [0, 0, 0]);
        assert_eq!(&out.rgba[4..7], [255, 255, 255]);
    }

    #[test]
    fn posterize_two_levels() {
        let src = rgb(1, 1, &[[100, 100, 100]]);
        let out = apply_adjust(&src, &AdjustOp::Posterize { levels: 2 }).unwrap();
        assert!(out.rgba[0] == 0 || out.rgba[0] == 255);
    }

    #[test]
    fn brightness_lifts() {
        let src = rgb(1, 1, &[[20, 20, 20]]);
        let out = apply_adjust(
            &src,
            &AdjustOp::Params(AdjustParams {
                brightness: Some(50.0),
                ..AdjustParams::default()
            }),
        )
        .unwrap();
        assert!(out.rgba[0] > 20);
    }

    #[test]
    fn hue_rotates_red_toward_green() {
        let src = rgb(1, 1, &[[255, 0, 0]]);
        let out = apply_adjust(
            &src,
            &AdjustOp::Params(AdjustParams {
                hue: Some(120.0),
                ..AdjustParams::default()
            }),
        )
        .unwrap();
        assert!(out.rgba[1] > out.rgba[0]);
    }

    #[test]
    fn curves_identity() {
        let src = rgb(1, 1, &[[80, 90, 100]]);
        let out = apply_adjust(
            &src,
            &AdjustOp::Params(AdjustParams {
                curves: Some(CurveSet {
                    rgb: vec![(0.0, 0.0), (255.0, 255.0)],
                    ..CurveSet::default()
                }),
                ..AdjustParams::default()
            }),
        )
        .unwrap();
        assert_eq!(&out.rgba[..3], [80, 90, 100]);
    }

    #[test]
    fn mixer_swaps_red_blue() {
        let src = rgb(1, 1, &[[200, 10, 30]]);
        let out = apply_adjust(
            &src,
            &AdjustOp::Params(AdjustParams {
                mixer: Some([[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]]),
                ..AdjustParams::default()
            }),
        )
        .unwrap();
        assert_eq!(out.rgba[0], 30);
        assert_eq!(out.rgba[2], 200);
    }

    #[test]
    fn parse_tokens_and_pack() {
        assert!(matches!(parse_adjust_line("sepia"), Some(AdjustOp::Sepia)));
        assert!(matches!(
            parse_adjust_line("posterize=4"),
            Some(AdjustOp::Posterize { levels: 4 })
        ));
        let p = unpack_params("brightness=10\ncurves=0,0 255,255\nselective=reds 0,20,0,0");
        assert_eq!(p.brightness, Some(10.0));
        assert!(p.curves.is_some());
        assert!(p.selective.is_some());
        let packed = pack_adjust_params(&p);
        assert!(packed.contains("brightness=10"));
        let piped = unpack_params("brightness=10 | contrast=5 | mixer=0,0,1;0,1,0;1,0,0");
        assert_eq!(piped.brightness, Some(10.0));
        assert_eq!(piped.contrast, Some(5.0));
        assert!(piped.mixer.is_some());
        assert!(parse_adjust_line("nope").is_none());
        assert!(matches!(
            parse_adjust_line("develop"),
            Some(AdjustOp::Params(p)) if p.exposure == Some(0.0)
        ));
    }

    #[test]
    fn auto_levels_stretches_flat_gray() {
        let src = rgb(2, 1, &[[60, 60, 60], [80, 80, 80]]);
        let out = apply_adjust(&src, &AdjustOp::AutoLevels).unwrap();
        assert!(out.rgba[0] < 20 || out.rgba[4] > 200);
    }
}
