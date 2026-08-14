//! Multi-image convert, compare, merge, and folder batch recipes.

#![allow(clippy::needless_range_loop, clippy::too_many_arguments)]

use crate::error::{Result, ViewerError};
use crate::image::loader::{load_image_file, ImageFormat, LoadedImage};
use crate::image::metadata::inspect_image_file;
use crate::image::operations::{resize_filtered, ResizeFilter};

use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::{DynamicImage, ImageFormat as ImgFmt, RgbImage, RgbaImage};

/// Encode target for convert / thumbnail export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeFormat {
    /// PNG.
    Png,
    /// JPEG.
    Jpeg,
    /// WebP.
    WebP,
    /// BMP.
    Bmp,
}

impl EncodeFormat {
    /// `jpg`, `png`, `webp`, `bmp`.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
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
            _ => None,
        }
    }

    /// File extension without a dot.
    #[must_use]
    pub fn ext(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::WebP => "webp",
            Self::Bmp => "bmp",
        }
    }

    fn img_fmt(self) -> ImgFmt {
        match self {
            Self::Png => ImgFmt::Png,
            Self::Jpeg => ImgFmt::Jpeg,
            Self::WebP => ImgFmt::WebP,
            Self::Bmp => ImgFmt::Bmp,
        }
    }
}

/// How to stack several images into one canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositeMode {
    /// Mean of aligned pixels.
    Average,
    /// Later images over earlier, 50% alpha.
    Overlay,
    /// Left-to-right strip.
    SideBySide,
    /// Top-to-bottom strip.
    StackVertical,
}

impl CompositeMode {
    /// `avg`, `overlay`, `side`, `stack`.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "avg" | "average" | "mean" => Some(Self::Average),
            "overlay" | "over" => Some(Self::Overlay),
            "side" | "sbs" | "hstack" => Some(Self::SideBySide),
            "stack" | "vstack" => Some(Self::StackVertical),
            _ => None,
        }
    }
}

/// Pixel-diff summary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffStats {
    /// Pixels whose RGB changed.
    pub changed: u32,
    /// Compared pixels.
    pub total: u32,
    /// Mean absolute channel error 0–255.
    pub mean: f32,
}

/// Named recipe stored in `orchid-batch-recipes.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(missing_docs)]
pub struct BatchRecipe {
    pub name: String,
    pub ops: String,
}

/// Convert `path` to `format` and write a sibling.
///
/// # Errors
///
/// I/O or encode.
pub fn convert_file(path: &Path, format: EncodeFormat) -> Result<PathBuf> {
    let src = load_image_file(path)?;
    save_as(path, &src, "convert", format)
}

/// Export a max-edge thumbnail sibling.
///
/// # Errors
///
/// I/O or encode.
pub fn export_thumb_file(path: &Path, max_edge: u32, format: EncodeFormat) -> Result<PathBuf> {
    let src = load_image_file(path)?;
    let edge = max_edge.clamp(16, 2048);
    let scale = edge as f32 / src.width.max(src.height).max(1) as f32;
    let tw = (src.width as f32 * scale).round().max(1.0) as u32;
    let th = (src.height as f32 * scale).round().max(1.0) as u32;
    let out = resize_filtered(&src, tw, th, ResizeFilter::Bilinear)?;
    save_as(path, &out, "thumb", format)
}

/// Side-by-side (or 2×2) compare sheet next to the first file.
///
/// # Errors
///
/// Fewer than two images, I/O.
pub fn compare_files(paths: &[&Path]) -> Result<PathBuf> {
    let imgs = load_many(paths)?;
    if imgs.len() < 2 || imgs.len() > 4 {
        return Err(ViewerError::Metadata("compare needs 2–4 images".into()));
    }
    let sheet = compare_strip(&imgs)?;
    save_as(paths[0], &sheet, "compare", EncodeFormat::Png)
}

/// Copy the chosen file as `{stem}-best.{ext}`.
///
/// # Errors
///
/// Index out of range or I/O.
pub fn pick_best_file(paths: &[&Path], index: usize) -> Result<PathBuf> {
    let src = *paths
        .get(index)
        .ok_or_else(|| ViewerError::Metadata(format!("keep index {index} is out of range")))?;
    let bytes = std::fs::read(src)?;
    let dest = unique_dest(src, "best", ext_of(src));
    std::fs::write(&dest, bytes)?;
    Ok(dest)
}

/// Absolute pixel difference of two files.
///
/// # Errors
///
/// I/O or decode.
pub fn diff_files(a: &Path, b: &Path) -> Result<(PathBuf, DiffStats)> {
    let left = load_image_file(a)?;
    let right = load_image_file(b)?;
    let (img, stats) = pixel_diff(&left, &right)?;
    let dest = save_as(a, &img, "diff", EncodeFormat::Png)?;
    Ok((dest, stats))
}

/// Composite / merge selected files.
///
/// # Errors
///
/// Fewer than two images, I/O.
pub fn composite_files(paths: &[&Path], mode: CompositeMode) -> Result<PathBuf> {
    let imgs = load_many(paths)?;
    if imgs.len() < 2 {
        return Err(ViewerError::Metadata("composite needs 2+ images".into()));
    }
    let out = composite(&imgs, mode)?;
    save_as(paths[0], &out, "merge", EncodeFormat::Png)
}

/// Horizontal panorama stitch.
///
/// # Errors
///
/// Fewer than two images, I/O.
pub fn stitch_panorama_files(paths: &[&Path]) -> Result<PathBuf> {
    let imgs = load_many(paths)?;
    if imgs.len() < 2 {
        return Err(ViewerError::Metadata("panorama needs 2+ images".into()));
    }
    let out = stitch_panorama(&imgs)?;
    save_as(paths[0], &out, "pano", EncodeFormat::Png)
}

/// Exposure-fusion HDR merge.
///
/// # Errors
///
/// Fewer than two images, I/O.
pub fn merge_hdr_files(paths: &[&Path]) -> Result<PathBuf> {
    let imgs = load_many(paths)?;
    if imgs.len() < 2 {
        return Err(ViewerError::Metadata("HDR merge needs 2+ images".into()));
    }
    let out = merge_hdr(&imgs)?;
    save_as(paths[0], &out, "hdr", EncodeFormat::Png)
}

/// Planned sibling path (does not write).
#[must_use]
pub fn planned_sibling(src: &Path, suffix: &str, ext: &str) -> PathBuf {
    unique_dest(src, suffix, ext)
}

/// Shoot-date token for image-aware rename (`YYYY-MM-DD`).
#[must_use]
pub fn image_date_token(path: &Path) -> String {
    if let Ok(ins) = inspect_image_file(path) {
        for (k, v) in ins.exif.iter().chain(ins.xmp.iter()) {
            if k.eq_ignore_ascii_case("DateTimeOriginal") || k.eq_ignore_ascii_case("DateTime") {
                if v.len() >= 10 {
                    return v[..10].replace(':', "-");
                }
                return v.clone();
            }
        }
    }
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Load `dir/orchid-batch-recipes.json`.
#[must_use]
pub fn load_batch_recipes(dir: &Path) -> Vec<BatchRecipe> {
    let path = dir.join("orchid-batch-recipes.json");
    let Ok(bytes) = std::fs::read(&path) else {
        return Vec::new();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// Insert or replace a recipe in `dir/orchid-batch-recipes.json`.
///
/// # Errors
///
/// I/O.
pub fn save_batch_recipe(dir: &Path, recipe: BatchRecipe) -> Result<()> {
    let path = dir.join("orchid-batch-recipes.json");
    let mut all = load_batch_recipes(dir);
    if let Some(existing) = all.iter_mut().find(|t| t.name == recipe.name) {
        *existing = recipe;
    } else {
        all.push(recipe);
    }
    let json = serde_json::to_vec_pretty(&all).map_err(|e| ViewerError::Metadata(e.to_string()))?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Build a compare strip (2–3 across, 4 as 2×2).
///
/// # Errors
///
/// Empty set.
pub fn compare_strip(images: &[LoadedImage]) -> Result<LoadedImage> {
    if images.is_empty() {
        return Err(ViewerError::Metadata("no images".into()));
    }
    if images.len() == 4 {
        let top = hstack(&images[0..2], 8)?;
        let bot = hstack(&images[2..4], 8)?;
        return vstack(&[top, bot], 8);
    }
    hstack(images, 8)
}

/// Absolute RGB difference. `b` is resized to `a` when sizes differ.
///
/// # Errors
///
/// Resize failure.
pub fn pixel_diff(a: &LoadedImage, b: &LoadedImage) -> Result<(LoadedImage, DiffStats)> {
    let b = if a.width != b.width || a.height != b.height {
        resize_filtered(b, a.width, a.height, ResizeFilter::Bilinear)?
    } else {
        b.clone()
    };
    let n = (a.width as usize * a.height as usize).max(1);
    let mut out = vec![0u8; n * 4];
    let mut changed = 0u32;
    let mut acc = 0u64;
    for i in 0..n {
        let o = i * 4;
        let mut mad = 0u32;
        for c in 0..3 {
            let d = a.rgba[o + c].abs_diff(b.rgba[o + c]);
            mad += u32::from(d);
            out[o + c] = d;
        }
        out[o + 3] = 255;
        acc += u64::from(mad);
        if mad > 6 {
            changed += 1;
            // Tint changed pixels so the sheet is readable.
            out[o] = out[o].saturating_add(80);
        }
    }
    let stats = DiffStats {
        changed,
        total: n as u32,
        mean: acc as f32 / (n as f32 * 3.0),
    };
    Ok((wrap(a, out, a.width, a.height), stats))
}

/// Merge images with [`CompositeMode`].
///
/// # Errors
///
/// Empty set or resize.
pub fn composite(images: &[LoadedImage], mode: CompositeMode) -> Result<LoadedImage> {
    match mode {
        CompositeMode::SideBySide => hstack(images, 0),
        CompositeMode::StackVertical => vstack(images, 0),
        CompositeMode::Average => blend_aligned(images, false),
        CompositeMode::Overlay => blend_aligned(images, true),
    }
}

/// Left-to-right stitch with a searched overlap.
///
/// # Errors
///
/// Empty set.
pub fn stitch_panorama(images: &[LoadedImage]) -> Result<LoadedImage> {
    let mut cur = images
        .first()
        .cloned()
        .ok_or_else(|| ViewerError::Metadata("no images".into()))?;
    for next in images.iter().skip(1) {
        cur = stitch_pair(&cur, next)?;
    }
    Ok(cur)
}

/// Well-exposedness fusion, then a light Reinhard compress.
///
/// # Errors
///
/// Empty set or resize.
pub fn merge_hdr(images: &[LoadedImage]) -> Result<LoadedImage> {
    let first = images
        .first()
        .ok_or_else(|| ViewerError::Metadata("no images".into()))?;
    let aligned = align_to(first, images)?;
    let n = (first.width as usize * first.height as usize).max(1);
    let mut out = vec![0u8; n * 4];
    for i in 0..n {
        let o = i * 4;
        let mut wsum = 0.0f32;
        let mut acc = [0.0f32; 3];
        for img in &aligned {
            let r = f32::from(img.rgba[o]);
            let g = f32::from(img.rgba[o + 1]);
            let b = f32::from(img.rgba[o + 2]);
            let luma = (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0;
            let x = luma - 0.5;
            let w = (-(x * x) / 0.08).exp().max(0.02);
            acc[0] += r * w;
            acc[1] += g * w;
            acc[2] += b * w;
            wsum += w;
        }
        wsum = wsum.max(1e-4);
        for c in 0..3 {
            let v = (acc[c] / wsum) / 255.0;
            let mapped = v / (1.0 + v);
            out[o + c] = (mapped * 255.0 * 1.35).clamp(0.0, 255.0) as u8;
        }
        out[o + 3] = 255;
    }
    Ok(wrap(first, out, first.width, first.height))
}

fn load_many(paths: &[&Path]) -> Result<Vec<LoadedImage>> {
    let mut out = Vec::with_capacity(paths.len());
    for p in paths {
        out.push(load_image_file(p)?);
    }
    Ok(out)
}

fn save_as(src: &Path, img: &LoadedImage, suffix: &str, format: EncodeFormat) -> Result<PathBuf> {
    let dest = unique_dest(src, suffix, format.ext());
    let bytes = encode_as(img, format)?;
    std::fs::write(&dest, bytes)?;
    Ok(dest)
}

fn encode_as(img: &LoadedImage, format: EncodeFormat) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    match format {
        EncodeFormat::Jpeg => {
            let rgb = rgba_to_rgb(img);
            DynamicImage::ImageRgb8(rgb)
                .write_to(&mut Cursor::new(&mut buf), ImgFmt::Jpeg)
                .map_err(|e| ViewerError::ImageDecode(e.to_string()))?;
        }
        other => {
            let rgba = RgbaImage::from_raw(img.width, img.height, img.rgba.to_vec())
                .ok_or_else(|| ViewerError::ImageDecode("encode buffer".into()))?;
            DynamicImage::ImageRgba8(rgba)
                .write_to(&mut Cursor::new(&mut buf), other.img_fmt())
                .map_err(|e| ViewerError::ImageDecode(e.to_string()))?;
        }
    }
    Ok(buf)
}

fn rgba_to_rgb(img: &LoadedImage) -> RgbImage {
    let mut out = Vec::with_capacity(img.width as usize * img.height as usize * 3);
    for px in img.rgba.chunks_exact(4) {
        out.extend_from_slice(&px[..3]);
    }
    RgbImage::from_raw(img.width, img.height, out)
        .unwrap_or_else(|| RgbImage::new(img.width, img.height))
}

fn unique_dest(src: &Path, suffix: &str, ext: &str) -> PathBuf {
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

fn ext_of(path: &Path) -> &str {
    path.extension().and_then(|s| s.to_str()).unwrap_or("png")
}

fn wrap(src: &LoadedImage, rgba: Vec<u8>, w: u32, h: u32) -> LoadedImage {
    LoadedImage {
        rgba: std::sync::Arc::new(rgba),
        width: w,
        height: h,
        format: ImageFormat::Png,
        original_size_bytes: src.original_size_bytes,
        color_source: src.color_source.clone(),
        color_dest: src.color_dest.clone(),
        orientation: src.orientation,
        bit_depth: src.bit_depth,
        color_model: src.color_model.clone(),
    }
}

fn fit_height(img: &LoadedImage, height: u32) -> Result<LoadedImage> {
    if img.height == height {
        return Ok(img.clone());
    }
    let w = ((img.width as f32) * (height as f32 / img.height.max(1) as f32))
        .round()
        .max(1.0) as u32;
    resize_filtered(img, w, height, ResizeFilter::Bilinear)
}

fn hstack(images: &[LoadedImage], gap: u32) -> Result<LoadedImage> {
    let h = images.iter().map(|i| i.height).min().unwrap_or(1).min(1600);
    let fitted: Vec<LoadedImage> = images
        .iter()
        .map(|i| fit_height(i, h))
        .collect::<Result<_>>()?;
    let width =
        fitted.iter().map(|i| i.width).sum::<u32>() + gap * fitted.len().saturating_sub(1) as u32;
    let mut buf = vec![20u8; (width * h * 4) as usize];
    for px in buf.chunks_exact_mut(4) {
        px[3] = 255;
    }
    let mut x = 0u32;
    for img in &fitted {
        blit(&mut buf, width, h, x, 0, img);
        x += img.width + gap;
    }
    Ok(wrap(&fitted[0], buf, width, h))
}

fn vstack(images: &[LoadedImage], gap: u32) -> Result<LoadedImage> {
    let w = images.iter().map(|i| i.width).min().unwrap_or(1).min(1600);
    let fitted: Vec<LoadedImage> = images
        .iter()
        .map(|i| {
            if i.width == w {
                Ok(i.clone())
            } else {
                let h = ((i.height as f32) * (w as f32 / i.width.max(1) as f32))
                    .round()
                    .max(1.0) as u32;
                resize_filtered(i, w, h, ResizeFilter::Bilinear)
            }
        })
        .collect::<Result<_>>()?;
    let height =
        fitted.iter().map(|i| i.height).sum::<u32>() + gap * fitted.len().saturating_sub(1) as u32;
    let mut buf = vec![20u8; (w * height * 4) as usize];
    for px in buf.chunks_exact_mut(4) {
        px[3] = 255;
    }
    let mut y = 0u32;
    for img in &fitted {
        blit(&mut buf, w, height, 0, y, img);
        y += img.height + gap;
    }
    Ok(wrap(&fitted[0], buf, w, height))
}

fn blit(buf: &mut [u8], bw: u32, bh: u32, x: u32, y: u32, src: &LoadedImage) {
    for row in 0..src.height {
        let dy = y + row;
        if dy >= bh {
            break;
        }
        for col in 0..src.width {
            let dx = x + col;
            if dx >= bw {
                break;
            }
            let di = ((dy * bw + dx) * 4) as usize;
            let si = ((row * src.width + col) * 4) as usize;
            buf[di..di + 4].copy_from_slice(&src.rgba[si..si + 4]);
        }
    }
}

fn align_to(first: &LoadedImage, images: &[LoadedImage]) -> Result<Vec<LoadedImage>> {
    let mut out = Vec::with_capacity(images.len());
    for img in images {
        if img.width == first.width && img.height == first.height {
            out.push(img.clone());
        } else {
            out.push(resize_filtered(
                img,
                first.width,
                first.height,
                ResizeFilter::Bilinear,
            )?);
        }
    }
    Ok(out)
}

fn blend_aligned(images: &[LoadedImage], overlay: bool) -> Result<LoadedImage> {
    let first = images
        .first()
        .ok_or_else(|| ViewerError::Metadata("no images".into()))?;
    let aligned = align_to(first, images)?;
    let n = (first.width as usize * first.height as usize).max(1);
    let mut buf = first.rgba.as_ref().clone();
    if overlay {
        for img in aligned.iter().skip(1) {
            for i in 0..n {
                let o = i * 4;
                for c in 0..3 {
                    buf[o + c] = ((u16::from(buf[o + c]) + u16::from(img.rgba[o + c])) / 2) as u8;
                }
            }
        }
    } else {
        let count = aligned.len() as u32;
        for i in 0..n {
            let o = i * 4;
            let mut acc = [0u32; 3];
            for img in &aligned {
                acc[0] += u32::from(img.rgba[o]);
                acc[1] += u32::from(img.rgba[o + 1]);
                acc[2] += u32::from(img.rgba[o + 2]);
            }
            buf[o] = (acc[0] / count) as u8;
            buf[o + 1] = (acc[1] / count) as u8;
            buf[o + 2] = (acc[2] / count) as u8;
            buf[o + 3] = 255;
        }
    }
    Ok(wrap(first, buf, first.width, first.height))
}

fn stitch_pair(left: &LoadedImage, right: &LoadedImage) -> Result<LoadedImage> {
    let h = left.height.min(right.height).max(1);
    let l = fit_height(left, h)?;
    let r = fit_height(right, h)?;
    let overlap = find_overlap(&l, &r);
    let width = l.width + r.width.saturating_sub(overlap);
    let mut buf = vec![0u8; (width * h * 4) as usize];
    blit(&mut buf, width, h, 0, 0, &l);
    for row in 0..h {
        for col in 0..r.width {
            let dx = l.width.saturating_sub(overlap) + col;
            if dx >= width {
                break;
            }
            let di = ((row * width + dx) * 4) as usize;
            let si = ((row * r.width + col) * 4) as usize;
            if col < overlap {
                let t = col as f32 / overlap.max(1) as f32;
                for c in 0..3 {
                    buf[di + c] = (f32::from(buf[di + c]) * (1.0 - t)
                        + f32::from(r.rgba[si + c]) * t)
                        .round() as u8;
                }
                buf[di + 3] = 255;
            } else {
                buf[di..di + 4].copy_from_slice(&r.rgba[si..si + 4]);
            }
        }
    }
    Ok(wrap(&l, buf, width, h))
}

fn find_overlap(left: &LoadedImage, right: &LoadedImage) -> u32 {
    let max_o = left.width.min(right.width).saturating_mul(8) / 10;
    let min_o = (left.width.min(right.width) / 8).max(4);
    let step = ((left.height / 16).max(1)) as usize;
    let mut best = min_o;
    let mut best_err = f32::MAX;
    let mut o = min_o;
    while o <= max_o {
        let mut acc = 0u64;
        let mut n = 0u64;
        for y in (0..left.height).step_by(step) {
            for x in (0..o).step_by(4) {
                let li = ((y * left.width + (left.width - o + x)) * 4) as usize;
                let ri = ((y * right.width + x) * 4) as usize;
                acc += u64::from(left.rgba[li].abs_diff(right.rgba[ri]));
                acc += u64::from(left.rgba[li + 1].abs_diff(right.rgba[ri + 1]));
                acc += u64::from(left.rgba[li + 2].abs_diff(right.rgba[ri + 2]));
                n += 3;
            }
        }
        let err = acc as f32 / n.max(1) as f32;
        if err < best_err {
            best_err = err;
            best = o;
        }
        o += (o / 12).max(4);
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn parse_encode_and_composite() {
        assert_eq!(EncodeFormat::parse("JPEG"), Some(EncodeFormat::Jpeg));
        assert_eq!(
            CompositeMode::parse("side"),
            Some(CompositeMode::SideBySide)
        );
        assert!(EncodeFormat::parse("heic").is_none());
    }

    #[test]
    fn compare_two_is_wider() {
        let a = rgb(4, 4, &[[10, 10, 10]; 16]);
        let b = rgb(4, 4, &[[200, 0, 0]; 16]);
        let sheet = compare_strip(&[a, b]).unwrap();
        assert!(sheet.width > 4);
        assert_eq!(sheet.height, 4);
    }

    #[test]
    fn diff_counts_changed_pixels() {
        let mut pix = [[0u8, 0, 0]; 16];
        pix[0] = [255, 0, 0];
        let a = rgb(4, 4, &[[0, 0, 0]; 16]);
        let b = rgb(4, 4, &pix);
        let (out, stats) = pixel_diff(&a, &b).unwrap();
        assert!(stats.changed >= 1);
        assert_eq!(stats.total, 16);
        assert_ne!(&out.rgba[..], &a.rgba[..]);
    }

    #[test]
    fn hdr_and_pano_change_buffer() {
        let dark = rgb(6, 4, &[[20, 20, 20]; 24]);
        let bright = rgb(6, 4, &[[220, 220, 220]; 24]);
        let hdr = merge_hdr(&[dark.clone(), bright.clone()]).unwrap();
        assert_eq!(hdr.width, 6);
        assert_ne!(&hdr.rgba[..], &dark.rgba[..]);
        let left = rgb(8, 4, &[[30, 80, 30]; 32]);
        let right = rgb(8, 4, &[[30, 80, 200]; 32]);
        let pano = stitch_panorama(&[left, right]).unwrap();
        assert!(pano.width > 8);
    }

    #[test]
    fn recipe_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        save_batch_recipe(
            dir.path(),
            BatchRecipe {
                name: "web".into(),
                ops: "resize=50% | convert=jpg".into(),
            },
        )
        .unwrap();
        let all = load_batch_recipes(dir.path());
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "web");
    }
}
