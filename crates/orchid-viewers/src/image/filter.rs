//! Convolution and stylize filters. Results are meant to be saved as a sibling file.

use crate::error::{Result, ViewerError};
use crate::image::edit::save_sibling;
use crate::image::loader::{load_image_file, LoadedImage};

use std::path::{Path, PathBuf};

/// One filter, a stack, or a named look.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterOp {
    /// High-pass sharpen (`amount` 0–4, default 1).
    Sharpen {
        /// Mix versus the original.
        amount: f32,
    },
    /// Unsharp mask.
    Unsharp {
        /// Blur radius in pixels.
        radius: f32,
        /// How hard to push the difference.
        amount: f32,
        /// Ignore diffs below this (0–255).
        threshold: u8,
    },
    /// Gaussian blur.
    Blur {
        /// Sigma-ish radius (0.3–12).
        radius: f32,
    },
    /// Directional streak.
    Motion {
        /// Length in pixels.
        length: f32,
        /// Degrees, 0 = right.
        angle: f32,
    },
    /// Per-channel median (noise reduction).
    Median {
        /// Neighborhood radius 1–3.
        radius: u8,
    },
    /// Replace salt-and-pepper outliers.
    Despeckle,
    /// Relief / bevel.
    Emboss,
    /// Sobel magnitude.
    Edges,
    /// Oil-paint bins.
    Oil {
        /// Brush radius 1–4.
        radius: u8,
    },
    /// Soft wash + ink edges.
    Watercolor,
    /// Quantize + ink outline.
    Cartoon,
    /// Pencil / color-dodge sketch.
    Sketch,
    /// Film grain.
    Grain {
        /// 0–100.
        amount: f32,
    },
    /// Darken the corners.
    Vignette {
        /// 0–100.
        amount: f32,
    },
    /// Barrel (`k < 0`) / pincushion (`k > 0`) correction.
    Lens {
        /// Typical range −0.4…0.4.
        k: f32,
    },
    /// Radial R/B shift (chromatic aberration).
    Chromatic {
        /// Pixels at the corners.
        amount: f32,
    },
    /// Desaturate red-eye; optional circle `x,y,r`.
    RedEye {
        /// Restrict to a circle, or `None` for a full scan.
        region: Option<(u32, u32, u32)>,
    },
    /// Median on skin-colored pixels.
    Skin {
        /// 0–100 blend.
        amount: f32,
    },
    /// Apply left-to-right.
    Stack(Vec<FilterOp>),
}

impl FilterOp {
    /// Filename suffix for [`save_sibling`].
    #[must_use]
    pub fn suffix(&self) -> &'static str {
        match self {
            Self::Sharpen { .. } => "sharpen",
            Self::Unsharp { .. } => "unsharp",
            Self::Blur { .. } => "blur",
            Self::Motion { .. } => "motion",
            Self::Median { .. } => "median",
            Self::Despeckle => "despeckle",
            Self::Emboss => "emboss",
            Self::Edges => "edges",
            Self::Oil { .. } => "oil",
            Self::Watercolor => "watercolor",
            Self::Cartoon => "cartoon",
            Self::Sketch => "sketch",
            Self::Grain { .. } => "grain",
            Self::Vignette { .. } => "vignette",
            Self::Lens { .. } => "lens",
            Self::Chromatic { .. } => "ca",
            Self::RedEye { .. } => "redeye",
            Self::Skin { .. } => "skin",
            Self::Stack(_) => "filter",
        }
    }
}

/// Named look stored in `orchid-filter-presets.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct FilterPreset {
    pub name: String,
    pub ops: String,
}

/// Built-in one-click looks (`look=vivid`, …).
#[must_use]
pub fn builtin_looks() -> &'static [(&'static str, &'static str)] {
    &[
        ("vivid", "unsharp=1,1.2,0 | vignette=18"),
        ("soft", "blur=0.8 | skin=45"),
        ("drama", "sharpen=1.4 | vignette=45 | grain=12"),
        ("clean", "despeckle | median=1 | unsharp=0.8,0.6,3"),
        ("fade", "vignette=30 | grain=8"),
        ("comic", "cartoon"),
        ("pencil", "sketch"),
    ]
}

/// Apply a filter to a decoded image.
///
/// # Errors
///
/// Corrupt RGBA buffer.
pub fn apply_filter(src: &LoadedImage, op: &FilterOp) -> Result<LoadedImage> {
    if src.width == 0 || src.height == 0 || src.rgba.len() < 4 {
        return Ok(src.clone());
    }
    match op {
        FilterOp::Sharpen { amount } => Ok(sharpen(src, *amount)),
        FilterOp::Unsharp {
            radius,
            amount,
            threshold,
        } => Ok(unsharp(src, *radius, *amount, *threshold)),
        FilterOp::Blur { radius } => Ok(gaussian(src, *radius)),
        FilterOp::Motion { length, angle } => Ok(motion(src, *length, *angle)),
        FilterOp::Median { radius } => Ok(median(src, *radius)),
        FilterOp::Despeckle => Ok(despeckle(src)),
        FilterOp::Emboss => Ok(convolve3(
            src,
            &[-2.0, -1.0, 0.0, -1.0, 1.0, 1.0, 0.0, 1.0, 2.0],
            128.0,
            1.0,
        )),
        FilterOp::Edges => Ok(sobel(src)),
        FilterOp::Oil { radius } => Ok(oil(src, *radius)),
        FilterOp::Watercolor => Ok(watercolor(src)),
        FilterOp::Cartoon => Ok(cartoon(src)),
        FilterOp::Sketch => Ok(sketch(src)),
        FilterOp::Grain { amount } => Ok(grain(src, *amount)),
        FilterOp::Vignette { amount } => Ok(vignette(src, *amount)),
        FilterOp::Lens { k } => Ok(lens(src, *k)),
        FilterOp::Chromatic { amount } => Ok(chromatic(src, *amount)),
        FilterOp::RedEye { region } => Ok(redeye(src, *region)),
        FilterOp::Skin { amount } => Ok(skin(src, *amount)),
        FilterOp::Stack(ops) => {
            let mut cur = src.clone();
            for step in ops {
                cur = apply_filter(&cur, step)?;
            }
            Ok(cur)
        }
    }
}

/// Load, filter, write a sibling. Returns the new path.
///
/// # Errors
///
/// I/O or decode.
pub fn apply_filter_file(path: &Path, op: &FilterOp) -> Result<PathBuf> {
    let src = load_image_file(path)?;
    let out = apply_filter(&src, op)?;
    save_sibling(path, &out, op.suffix())
}

/// Tokens, `key=value`, stacks (` | `), and `look=` / `preset=`.
#[must_use]
pub fn parse_filter_line(raw: &str) -> Option<FilterOp> {
    parse_filter_line_in(raw, None)
}

/// Like [`parse_filter_line`], also resolving folder presets.
#[must_use]
pub fn parse_filter_line_in(raw: &str, preset_dir: Option<&Path>) -> Option<FilterOp> {
    let chunks = kv_chunks(raw);
    if chunks.is_empty() {
        return None;
    }
    let mut ops = Vec::new();
    for chunk in chunks {
        match parse_one(chunk, preset_dir)? {
            FilterOp::Stack(inner) => ops.extend(inner),
            op => ops.push(op),
        }
    }
    if ops.len() == 1 {
        Some(ops.remove(0))
    } else {
        Some(FilterOp::Stack(ops))
    }
}

/// Load `dir/orchid-filter-presets.json`.
#[must_use]
pub fn load_filter_presets(dir: &Path) -> Vec<FilterPreset> {
    let path = dir.join("orchid-filter-presets.json");
    let Ok(bytes) = std::fs::read(&path) else {
        return Vec::new();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// Insert or replace a look in `dir/orchid-filter-presets.json`.
///
/// # Errors
///
/// I/O.
pub fn save_filter_preset(dir: &Path, preset: FilterPreset) -> Result<()> {
    let path = dir.join("orchid-filter-presets.json");
    let mut all = load_filter_presets(dir);
    if let Some(existing) = all.iter_mut().find(|t| t.name == preset.name) {
        *existing = preset;
    } else {
        all.push(preset);
    }
    let json = serde_json::to_vec_pretty(&all).map_err(|e| ViewerError::Metadata(e.to_string()))?;
    std::fs::write(path, json)?;
    Ok(())
}

fn parse_one(raw: &str, preset_dir: Option<&Path>) -> Option<FilterOp> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    let (key, val) = match t.split_once('=') {
        Some((k, v)) => (k.trim().to_ascii_lowercase(), Some(v.trim())),
        None => (t.to_ascii_lowercase(), None),
    };
    match key.as_str() {
        "sharpen" => Some(FilterOp::Sharpen {
            amount: val.and_then(|v| v.parse().ok()).unwrap_or(1.0),
        }),
        "unsharp" | "unsharp-mask" => {
            let (radius, amount, threshold) = parse3(val.unwrap_or("1,1,0"), 1.0, 1.0, 0.0);
            Some(FilterOp::Unsharp {
                radius,
                amount,
                threshold: threshold.clamp(0.0, 255.0) as u8,
            })
        }
        "blur" | "gaussian" | "gaussian-blur" => Some(FilterOp::Blur {
            radius: val.and_then(|v| v.parse().ok()).unwrap_or(1.5),
        }),
        "motion" | "motion-blur" => {
            let (length, angle, _) = parse3(val.unwrap_or("12,0"), 12.0, 0.0, 0.0);
            Some(FilterOp::Motion { length, angle })
        }
        "median" => Some(FilterOp::Median {
            radius: val.and_then(|v| v.parse().ok()).unwrap_or(1),
        }),
        "despeckle" => Some(FilterOp::Despeckle),
        "emboss" => Some(FilterOp::Emboss),
        "edge" | "edges" | "edge-detect" => Some(FilterOp::Edges),
        "oil" | "oil-paint" => Some(FilterOp::Oil {
            radius: val.and_then(|v| v.parse().ok()).unwrap_or(3),
        }),
        "watercolor" | "watercolour" => Some(FilterOp::Watercolor),
        "cartoon" => Some(FilterOp::Cartoon),
        "sketch" | "pencil" => Some(FilterOp::Sketch),
        "grain" | "noise" => Some(FilterOp::Grain {
            amount: val.and_then(|v| v.parse().ok()).unwrap_or(20.0),
        }),
        "vignette" => Some(FilterOp::Vignette {
            amount: val.and_then(|v| v.parse().ok()).unwrap_or(40.0),
        }),
        "lens" | "barrel" | "pincushion" => Some(FilterOp::Lens {
            k: val.and_then(|v| v.parse().ok()).unwrap_or(-0.12),
        }),
        "ca" | "chromatic" | "aberration" => Some(FilterOp::Chromatic {
            amount: val.and_then(|v| v.parse().ok()).unwrap_or(2.0),
        }),
        "redeye" | "red-eye" => Some(FilterOp::RedEye {
            region: val.and_then(parse_region),
        }),
        "skin" | "smooth" => Some(FilterOp::Skin {
            amount: val.and_then(|v| v.parse().ok()).unwrap_or(50.0),
        }),
        "look" | "preset" => {
            let name = val.unwrap_or("").to_ascii_lowercase();
            resolve_look(&name, preset_dir)
        }
        _ => None,
    }
}

fn resolve_look(name: &str, preset_dir: Option<&Path>) -> Option<FilterOp> {
    if let Some(dir) = preset_dir {
        if let Some(p) = load_filter_presets(dir)
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
        {
            return parse_filter_line_in(&p.ops, None);
        }
    }
    let recipe = builtin_looks()
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, ops)| *ops)?;
    parse_filter_line(recipe)
}

fn parse3(raw: &str, a: f32, b: f32, c: f32) -> (f32, f32, f32) {
    let mut it = raw.split(',');
    (
        it.next().and_then(|s| s.trim().parse().ok()).unwrap_or(a),
        it.next().and_then(|s| s.trim().parse().ok()).unwrap_or(b),
        it.next().and_then(|s| s.trim().parse().ok()).unwrap_or(c),
    )
}

fn parse_region(raw: &str) -> Option<(u32, u32, u32)> {
    let mut it = raw.split(',');
    Some((
        it.next()?.trim().parse().ok()?,
        it.next()?.trim().parse().ok()?,
        it.next()?.trim().parse().ok()?,
    ))
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

fn px_at(src: &LoadedImage, x: i32, y: i32) -> [u8; 4] {
    let x = x.clamp(0, src.width.saturating_sub(1) as i32) as u32;
    let y = y.clamp(0, src.height.saturating_sub(1) as i32) as u32;
    let i = ((y * src.width + x) * 4) as usize;
    let p = &src.rgba[i..i + 4];
    [p[0], p[1], p[2], p[3]]
}

fn bilinear(src: &LoadedImage, x: f32, y: f32) -> [u8; 4] {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    let a = px_at(src, x0, y0);
    let b = px_at(src, x0 + 1, y0);
    let c = px_at(src, x0, y0 + 1);
    let d = px_at(src, x0 + 1, y0 + 1);
    let mix = |a: u8, b: u8, c: u8, d: u8| {
        let top = f32::from(a) * (1.0 - tx) + f32::from(b) * tx;
        let bot = f32::from(c) * (1.0 - tx) + f32::from(d) * tx;
        (top * (1.0 - ty) + bot * ty).round().clamp(0.0, 255.0) as u8
    };
    [
        mix(a[0], b[0], c[0], d[0]),
        mix(a[1], b[1], c[1], d[1]),
        mix(a[2], b[2], c[2], d[2]),
        mix(a[3], b[3], c[3], d[3]),
    ]
}

fn map_xy(src: &LoadedImage, mut f: impl FnMut(u32, u32, [u8; 4]) -> [u8; 4]) -> LoadedImage {
    let mut out = src.rgba.as_ref().clone();
    let w = src.width;
    for y in 0..src.height {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let next = f(x, y, [out[i], out[i + 1], out[i + 2], out[i + 3]]);
            out[i..i + 4].copy_from_slice(&next);
        }
    }
    wrap(src, out)
}

fn convolve3(src: &LoadedImage, k: &[f32; 9], bias: f32, mix: f32) -> LoadedImage {
    let mut out = vec![0u8; src.rgba.len()];
    let w = src.width as i32;
    let h = src.height as i32;
    for y in 0..h {
        for x in 0..w {
            let mut acc = [bias; 3];
            let mut ki = 0usize;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let p = px_at(src, x + dx, y + dy);
                    let wgt = k[ki];
                    acc[0] += f32::from(p[0]) * wgt;
                    acc[1] += f32::from(p[1]) * wgt;
                    acc[2] += f32::from(p[2]) * wgt;
                    ki += 1;
                }
            }
            let orig = px_at(src, x, y);
            let i = ((y as u32 * src.width + x as u32) * 4) as usize;
            for c in 0..3 {
                let v = f32::from(orig[c]) * (1.0 - mix) + acc[c] * mix;
                out[i + c] = v.round().clamp(0.0, 255.0) as u8;
            }
            out[i + 3] = orig[3];
        }
    }
    wrap(src, out)
}

fn sharpen(src: &LoadedImage, amount: f32) -> LoadedImage {
    let a = amount.clamp(0.0, 4.0);
    convolve3(
        src,
        &[0.0, -1.0, 0.0, -1.0, 5.0, -1.0, 0.0, -1.0, 0.0],
        0.0,
        a.min(1.0) + (a - 1.0).max(0.0) * 0.35,
    )
}

fn gaussian_kernel(sigma: f32) -> Vec<f32> {
    let s = sigma.clamp(0.3, 12.0);
    let r = (s * 3.0).ceil() as i32;
    let r = r.clamp(1, 16);
    let mut k = Vec::with_capacity((r * 2 + 1) as usize);
    let mut sum = 0.0f32;
    for x in -r..=r {
        let v = (-0.5 * (x as f32 / s).powi(2)).exp();
        k.push(v);
        sum += v;
    }
    for v in &mut k {
        *v /= sum;
    }
    k
}

fn convolve_1d(src: &LoadedImage, k: &[f32], horizontal: bool) -> LoadedImage {
    let mut out = vec![0u8; src.rgba.len()];
    let w = src.width as i32;
    let h = src.height as i32;
    let r = (k.len() as i32 - 1) / 2;
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0f32; 3];
            for (i, wgt) in k.iter().enumerate() {
                let d = i as i32 - r;
                let p = if horizontal {
                    px_at(src, x + d, y)
                } else {
                    px_at(src, x, y + d)
                };
                acc[0] += f32::from(p[0]) * wgt;
                acc[1] += f32::from(p[1]) * wgt;
                acc[2] += f32::from(p[2]) * wgt;
            }
            let orig = px_at(src, x, y);
            let i = ((y as u32 * src.width + x as u32) * 4) as usize;
            out[i] = acc[0].round().clamp(0.0, 255.0) as u8;
            out[i + 1] = acc[1].round().clamp(0.0, 255.0) as u8;
            out[i + 2] = acc[2].round().clamp(0.0, 255.0) as u8;
            out[i + 3] = orig[3];
        }
    }
    wrap(src, out)
}

fn gaussian(src: &LoadedImage, radius: f32) -> LoadedImage {
    let k = gaussian_kernel(radius);
    let tmp = convolve_1d(src, &k, true);
    convolve_1d(&tmp, &k, false)
}

fn unsharp(src: &LoadedImage, radius: f32, amount: f32, threshold: u8) -> LoadedImage {
    let blur = gaussian(src, radius.max(0.3));
    let amt = amount.clamp(0.0, 4.0);
    let t = i16::from(threshold);
    let mut out = src.rgba.as_ref().clone();
    for (o, (s, b)) in out
        .chunks_exact_mut(4)
        .zip(src.rgba.chunks_exact(4).zip(blur.rgba.chunks_exact(4)))
    {
        for c in 0..3 {
            let diff = i16::from(s[c]) - i16::from(b[c]);
            if diff.abs() <= t {
                continue;
            }
            o[c] = (f32::from(s[c]) + f32::from(diff) * amt)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
    wrap(src, out)
}

fn motion(src: &LoadedImage, length: f32, angle: f32) -> LoadedImage {
    let n = length.clamp(1.0, 32.0).round() as i32;
    let rad = angle.to_radians();
    let dx = rad.cos();
    let dy = rad.sin();
    let mut out = vec![0u8; src.rgba.len()];
    for y in 0..src.height {
        for x in 0..src.width {
            let mut acc = [0.0f32; 3];
            for i in 0..n {
                let t = i as f32 - (n as f32 - 1.0) * 0.5;
                let p = bilinear(src, x as f32 + dx * t, y as f32 + dy * t);
                acc[0] += f32::from(p[0]);
                acc[1] += f32::from(p[1]);
                acc[2] += f32::from(p[2]);
            }
            let den = n as f32;
            let i = ((y * src.width + x) * 4) as usize;
            let orig = px_at(src, x as i32, y as i32);
            out[i] = (acc[0] / den).round().clamp(0.0, 255.0) as u8;
            out[i + 1] = (acc[1] / den).round().clamp(0.0, 255.0) as u8;
            out[i + 2] = (acc[2] / den).round().clamp(0.0, 255.0) as u8;
            out[i + 3] = orig[3];
        }
    }
    wrap(src, out)
}

fn median(src: &LoadedImage, radius: u8) -> LoadedImage {
    let r = i32::from(radius.clamp(1, 3));
    let mut out = vec![0u8; src.rgba.len()];
    let mut bucket = [0u8; 49];
    for y in 0..src.height as i32 {
        for x in 0..src.width as i32 {
            let orig = px_at(src, x, y);
            let i = ((y as u32 * src.width + x as u32) * 4) as usize;
            for c in 0..3 {
                let mut n = 0usize;
                for dy in -r..=r {
                    for dx in -r..=r {
                        bucket[n] = px_at(src, x + dx, y + dy)[c];
                        n += 1;
                    }
                }
                bucket[..n].sort_unstable();
                out[i + c] = bucket[n / 2];
            }
            out[i + 3] = orig[3];
        }
    }
    wrap(src, out)
}

fn despeckle(src: &LoadedImage) -> LoadedImage {
    let mut out = src.rgba.as_ref().clone();
    for y in 0..src.height as i32 {
        for x in 0..src.width as i32 {
            let orig = px_at(src, x, y);
            let i = ((y as u32 * src.width + x as u32) * 4) as usize;
            for c in 0..3 {
                let mut vals = [0u8; 9];
                let mut n = 0usize;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        vals[n] = px_at(src, x + dx, y + dy)[c];
                        n += 1;
                    }
                }
                vals[..9].sort_unstable();
                let med = vals[4];
                let lo = vals[0];
                let hi = vals[8];
                if (orig[c] == lo || orig[c] == hi)
                    && (i16::from(orig[c]) - i16::from(med)).abs() > 18
                {
                    out[i + c] = med;
                }
            }
        }
    }
    wrap(src, out)
}

fn sobel(src: &LoadedImage) -> LoadedImage {
    let mut out = vec![0u8; src.rgba.len()];
    for y in 0..src.height as i32 {
        for x in 0..src.width as i32 {
            let mut gx = 0.0f32;
            let mut gy = 0.0f32;
            const KX: [f32; 9] = [-1.0, 0.0, 1.0, -2.0, 0.0, 2.0, -1.0, 0.0, 1.0];
            const KY: [f32; 9] = [-1.0, -2.0, -1.0, 0.0, 0.0, 0.0, 1.0, 2.0, 1.0];
            let mut ki = 0usize;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let yv = f32::from(luma_u8(&px_at(src, x + dx, y + dy)));
                    gx += yv * KX[ki];
                    gy += yv * KY[ki];
                    ki += 1;
                }
            }
            let mag = (gx * gx + gy * gy).sqrt().min(255.0);
            let i = ((y as u32 * src.width + x as u32) * 4) as usize;
            let v = mag.round() as u8;
            out[i] = v;
            out[i + 1] = v;
            out[i + 2] = v;
            out[i + 3] = 255;
        }
    }
    wrap(src, out)
}

fn oil(src: &LoadedImage, radius: u8) -> LoadedImage {
    let r = i32::from(radius.clamp(1, 4));
    const BINS: usize = 16;
    let mut out = vec![0u8; src.rgba.len()];
    for y in 0..src.height as i32 {
        for x in 0..src.width as i32 {
            let mut count = [0u32; BINS];
            let mut rs = [0u32; BINS];
            let mut gs = [0u32; BINS];
            let mut bs = [0u32; BINS];
            for dy in -r..=r {
                for dx in -r..=r {
                    let p = px_at(src, x + dx, y + dy);
                    let bin = (usize::from(luma_u8(&p)) * BINS / 256).min(BINS - 1);
                    count[bin] += 1;
                    rs[bin] += u32::from(p[0]);
                    gs[bin] += u32::from(p[1]);
                    bs[bin] += u32::from(p[2]);
                }
            }
            let best = count
                .iter()
                .enumerate()
                .max_by_key(|(_, n)| *n)
                .map(|(i, _)| i)
                .unwrap_or(0);
            let n = count[best].max(1);
            let i = ((y as u32 * src.width + x as u32) * 4) as usize;
            out[i] = (rs[best] / n) as u8;
            out[i + 1] = (gs[best] / n) as u8;
            out[i + 2] = (bs[best] / n) as u8;
            out[i + 3] = px_at(src, x, y)[3];
        }
    }
    wrap(src, out)
}

fn watercolor(src: &LoadedImage) -> LoadedImage {
    let soft = gaussian(&median(src, 2), 1.1);
    let edges = sobel(&soft);
    let mut out = soft.rgba.as_ref().clone();
    for (o, e) in out.chunks_exact_mut(4).zip(edges.rgba.chunks_exact(4)) {
        let ink = f32::from(e[0]) / 255.0;
        for v in o.iter_mut().take(3) {
            *v = (f32::from(*v) * (1.0 - ink * 0.55))
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
    wrap(src, out)
}

fn cartoon(src: &LoadedImage) -> LoadedImage {
    let flat = median(src, 2);
    let mut out = flat.rgba.as_ref().clone();
    for px in out.chunks_exact_mut(4) {
        for v in px.iter_mut().take(3) {
            *v = (u16::from(*v) / 36 * 36).min(255) as u8;
        }
    }
    let quant = wrap(src, out);
    let edges = sobel(&flat);
    let mut merged = quant.rgba.as_ref().clone();
    for (o, e) in merged.chunks_exact_mut(4).zip(edges.rgba.chunks_exact(4)) {
        if e[0] > 70 {
            o[0] = 20;
            o[1] = 20;
            o[2] = 20;
        }
    }
    wrap(src, merged)
}

fn sketch(src: &LoadedImage) -> LoadedImage {
    let mut gray = src.rgba.as_ref().clone();
    for px in gray.chunks_exact_mut(4) {
        let y = luma_u8(px);
        px[0] = y;
        px[1] = y;
        px[2] = y;
    }
    let gimg = wrap(src, gray);
    let mut inv = gimg.rgba.as_ref().clone();
    for px in inv.chunks_exact_mut(4) {
        px[0] = 255 - px[0];
        px[1] = 255 - px[1];
        px[2] = 255 - px[2];
    }
    let blur = gaussian(&wrap(src, inv), 2.4);
    let mut out = gimg.rgba.as_ref().clone();
    for (o, b) in out.chunks_exact_mut(4).zip(blur.rgba.chunks_exact(4)) {
        for c in 0..3 {
            let den = 255u16.saturating_sub(u16::from(b[c])).max(1);
            o[c] = ((u16::from(o[c]) * 255) / den).min(255) as u8;
        }
    }
    wrap(src, out)
}

fn grain(src: &LoadedImage, amount: f32) -> LoadedImage {
    let amp = (amount.clamp(0.0, 100.0) / 100.0) * 48.0;
    map_xy(src, |x, y, px| {
        let n = hash_u32(x, y, 0x9E37_79B9);
        let v = (n as f32 / u32::MAX as f32) * 2.0 - 1.0;
        [
            clamp_add(px[0], v * amp),
            clamp_add(px[1], v * amp * 0.92),
            clamp_add(px[2], v * amp * 0.88),
            px[3],
        ]
    })
}

fn vignette(src: &LoadedImage, amount: f32) -> LoadedImage {
    let a = (amount.clamp(0.0, 100.0) / 100.0) * 0.85;
    let cx = (src.width as f32 - 1.0) * 0.5;
    let cy = (src.height as f32 - 1.0) * 0.5;
    let max_r = (cx * cx + cy * cy).sqrt().max(1.0);
    map_xy(src, |x, y, px| {
        let dx = x as f32 - cx;
        let dy = y as f32 - cy;
        let t = ((dx * dx + dy * dy).sqrt() / max_r).clamp(0.0, 1.0);
        let fall = 1.0 - a * t.powf(1.6);
        [
            clamp_mul(px[0], fall),
            clamp_mul(px[1], fall),
            clamp_mul(px[2], fall),
            px[3],
        ]
    })
}

fn lens(src: &LoadedImage, k: f32) -> LoadedImage {
    let k = k.clamp(-0.6, 0.6);
    let cx = (src.width as f32 - 1.0) * 0.5;
    let cy = (src.height as f32 - 1.0) * 0.5;
    let scale = cx.max(cy).max(1.0);
    let mut out = vec![0u8; src.rgba.len()];
    for y in 0..src.height {
        for x in 0..src.width {
            let nx = (x as f32 - cx) / scale;
            let ny = (y as f32 - cy) / scale;
            let r2 = nx * nx + ny * ny;
            let f = 1.0 + k * r2;
            let p = bilinear(src, cx + nx * f * scale, cy + ny * f * scale);
            let i = ((y * src.width + x) * 4) as usize;
            out[i..i + 4].copy_from_slice(&p);
        }
    }
    wrap(src, out)
}

fn chromatic(src: &LoadedImage, amount: f32) -> LoadedImage {
    let amt = amount.clamp(-8.0, 8.0);
    let cx = (src.width as f32 - 1.0) * 0.5;
    let cy = (src.height as f32 - 1.0) * 0.5;
    let max_r = cx.hypot(cy).max(1.0);
    let mut out = vec![0u8; src.rgba.len()];
    for y in 0..src.height {
        for x in 0..src.width {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let r = dx.hypot(dy) / max_r;
            let s = amt * r;
            let ux = if r > 1e-4 { dx / (r * max_r) } else { 0.0 };
            let uy = if r > 1e-4 { dy / (r * max_r) } else { 0.0 };
            let pr = bilinear(src, x as f32 + ux * s, y as f32 + uy * s);
            let pg = px_at(src, x as i32, y as i32);
            let pb = bilinear(src, x as f32 - ux * s, y as f32 - uy * s);
            let i = ((y * src.width + x) * 4) as usize;
            out[i] = pr[0];
            out[i + 1] = pg[1];
            out[i + 2] = pb[2];
            out[i + 3] = pg[3];
        }
    }
    wrap(src, out)
}

fn redeye(src: &LoadedImage, region: Option<(u32, u32, u32)>) -> LoadedImage {
    map_xy(src, |x, y, px| {
        if let Some((cx, cy, r)) = region {
            let dx = x as i32 - cx as i32;
            let dy = y as i32 - cy as i32;
            if dx * dx + dy * dy > (r as i32).pow(2) {
                return px;
            }
        }
        if is_red_eye(px) {
            let g = u16::from(px[1]) + u16::from(px[2]);
            let nr = (g / 2).min(255) as u8;
            [nr, px[1], px[2], px[3]]
        } else {
            px
        }
    })
}

fn is_red_eye(px: [u8; 4]) -> bool {
    px[0] > 140 && px[0] > px[1].saturating_add(35) && px[0] > px[2].saturating_add(35) && {
        let y = luma_u8(&px);
        (40..230).contains(&y)
    }
}

fn skin(src: &LoadedImage, amount: f32) -> LoadedImage {
    let smooth = median(src, 2);
    let mix = (amount.clamp(0.0, 100.0) / 100.0) * 0.85;
    let mut out = src.rgba.as_ref().clone();
    for (o, s) in out.chunks_exact_mut(4).zip(smooth.rgba.chunks_exact(4)) {
        if !is_skin([o[0], o[1], o[2], o[3]]) {
            continue;
        }
        for c in 0..3 {
            o[c] = (f32::from(o[c]) * (1.0 - mix) + f32::from(s[c]) * mix).round() as u8;
        }
    }
    wrap(src, out)
}

fn is_skin(px: [u8; 4]) -> bool {
    let (h, s, l) = rgb_to_hsl(f32::from(px[0]), f32::from(px[1]), f32::from(px[2]));
    let hue = h * 360.0;
    (hue <= 50.0 || hue >= 340.0) && (0.15..=0.72).contains(&s) && (0.28..=0.88).contains(&l)
}

fn luma_u8(px: &[u8]) -> u8 {
    ((u16::from(px[0]) * 30 + u16::from(px[1]) * 59 + u16::from(px[2]) * 11) / 100) as u8
}

fn clamp_add(v: u8, d: f32) -> u8 {
    (f32::from(v) + d).round().clamp(0.0, 255.0) as u8
}

fn clamp_mul(v: u8, m: f32) -> u8 {
    (f32::from(v) * m).round().clamp(0.0, 255.0) as u8
}

fn hash_u32(x: u32, y: u32, seed: u32) -> u32 {
    let mut n = x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263) ^ seed;
    n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    n ^ (n >> 16)
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let r = r / 255.0;
    let g = g / 255.0;
    let b = b / 255.0;
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
    let h = if (max - r).abs() < 1e-6 {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if (max - g).abs() < 1e-6 {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } / 6.0;
    (h, s, l)
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
    fn blur_softens_neighbor() {
        let src = rgb(2, 1, &[[0, 0, 0], [255, 255, 255]]);
        let out = apply_filter(&src, &FilterOp::Blur { radius: 1.2 }).unwrap();
        assert!(out.rgba[0] > 0);
        assert!(out.rgba[4] < 255);
    }

    #[test]
    fn invert_like_emboss_shifts() {
        let src = rgb(
            3,
            3,
            &[
                [0, 0, 0],
                [0, 0, 0],
                [0, 0, 0],
                [0, 0, 0],
                [255, 255, 255],
                [0, 0, 0],
                [0, 0, 0],
                [0, 0, 0],
                [0, 0, 0],
            ],
        );
        let out = apply_filter(&src, &FilterOp::Emboss).unwrap();
        assert_ne!(&out.rgba[..], &src.rgba[..]);
    }

    #[test]
    fn sketch_is_gray() {
        let src = rgb(
            2,
            2,
            &[[200, 10, 10], [10, 200, 10], [10, 10, 200], [80, 80, 80]],
        );
        let out = apply_filter(&src, &FilterOp::Sketch).unwrap();
        assert_eq!(out.rgba[0], out.rgba[1]);
        assert_eq!(out.rgba[1], out.rgba[2]);
    }

    #[test]
    fn vignette_darkens_corner() {
        let pix = [[200u8, 200, 200]; 16];
        let src = rgb(4, 4, &pix);
        let out = apply_filter(&src, &FilterOp::Vignette { amount: 80.0 }).unwrap();
        let center = &out.rgba[24..27];
        let corner = &out.rgba[0..3];
        assert!(corner[0] < center[0]);
    }

    #[test]
    fn redeye_kills_red() {
        let src = rgb(1, 1, &[[220, 20, 20]]);
        let out = apply_filter(&src, &FilterOp::RedEye { region: None }).unwrap();
        assert!(out.rgba[0] < 80);
    }

    #[test]
    fn grain_changes_pixels() {
        let src = rgb(2, 2, &[[80, 80, 80]; 4]);
        let out = apply_filter(&src, &FilterOp::Grain { amount: 80.0 }).unwrap();
        assert_ne!(&out.rgba[..], &src.rgba[..]);
    }

    #[test]
    fn lens_identity() {
        let src = rgb(
            2,
            2,
            &[[10, 20, 30], [40, 50, 60], [70, 80, 90], [11, 22, 33]],
        );
        let out = apply_filter(&src, &FilterOp::Lens { k: 0.0 }).unwrap();
        assert_eq!(&out.rgba[..], &src.rgba[..]);
    }

    #[test]
    fn parse_stack_and_look() {
        assert!(matches!(
            parse_filter_line("sharpen=1.2"),
            Some(FilterOp::Sharpen { .. })
        ));
        assert!(matches!(
            parse_filter_line("despeckle | vignette=20"),
            Some(FilterOp::Stack(_))
        ));
        let look = parse_filter_line("look=vivid").unwrap();
        assert!(matches!(look, FilterOp::Stack(_)));
        assert!(parse_filter_line("nope").is_none());
    }

    #[test]
    fn preset_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        save_filter_preset(
            dir.path(),
            FilterPreset {
                name: "portrait".into(),
                ops: "skin=40 | vignette=10".into(),
            },
        )
        .unwrap();
        let all = load_filter_presets(dir.path());
        assert_eq!(all[0].name, "portrait");
        let op = parse_filter_line_in("look=portrait", Some(dir.path())).unwrap();
        assert!(matches!(op, FilterOp::Stack(_)));
    }
}
