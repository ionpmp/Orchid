//! Animated GIF, APNG, and animated WebP decode, playback helpers, and frame export.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::codecs::gif::GifDecoder;
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::{AnimationDecoder, Frame};

use crate::error::{Result, ViewerError};
use crate::image::edit::save_sibling;
use crate::image::loader::{ImageFormat, LoadedImage};
use crate::snapshot::ImageThumbItem;

/// Cap decoded frames so a long loop cannot fill RAM.
const MAX_FRAMES: usize = 256;
/// ~64 MiB of RGBA across all frames.
const MAX_PIXELS: usize = 16 * 1024 * 1024;
const THUMB_EDGE: u32 = 48;
const MAX_THUMBS: usize = 64;
const DEFAULT_DELAY_MS: u32 = 100;
const MIN_DELAY_MS: u32 = 20;

/// Kind of animated container we decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimKind {
    /// GIF89a.
    Gif,
    /// Animated PNG (`acTL`).
    Apng,
    /// Animated WebP.
    WebP,
    /// Multi-page TIFF.
    Tiff,
    /// Multi-size ICO / CUR.
    Ico,
}

impl AnimKind {
    /// Short status label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Gif => "GIF",
            Self::Apng => "APNG",
            Self::WebP => "WebP",
            Self::Tiff => "TIFF",
            Self::Ico => "ICO",
        }
    }

    /// True when frames should auto-play (GIF / APNG / WebP).
    #[must_use]
    pub fn is_playback(self) -> bool {
        matches!(self, Self::Gif | Self::Apng | Self::WebP)
    }

    fn image_format(self) -> ImageFormat {
        match self {
            Self::Gif => ImageFormat::Gif,
            Self::Apng => ImageFormat::Png,
            Self::WebP => ImageFormat::WebP,
            Self::Tiff => ImageFormat::Tiff,
            Self::Ico => ImageFormat::Ico,
        }
    }
}

/// One composited animation frame (full canvas, RGBA8).
#[derive(Debug, Clone)]
pub struct AnimFrame {
    /// Shared pixels so the viewer can swap frames without copying.
    pub rgba: Arc<Vec<u8>>,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Display duration after this frame, milliseconds.
    pub delay_ms: u32,
}

/// Decoded multi-frame sequence (`frames.len() >= 2`).
#[derive(Debug, Clone)]
pub struct AnimSequence {
    /// Composited frames in playback order.
    pub frames: Vec<AnimFrame>,
    /// Container kind.
    pub kind: AnimKind,
    /// Small strip thumbs (at most [`MAX_THUMBS`]).
    pub thumbs: Vec<ImageThumbItem>,
}

impl AnimSequence {
    /// First frame as a still [`LoadedImage`].
    #[must_use]
    pub fn first_loaded(&self, size: u64) -> LoadedImage {
        let frame = &self.frames[0];
        LoadedImage {
            rgba: Arc::clone(&frame.rgba),
            width: frame.width,
            height: frame.height,
            format: self.kind.image_format(),
            original_size_bytes: size,
            ..LoadedImage::meta_defaults()
        }
    }
}

/// True when `ext` (no leading dot) might contain animation.
#[must_use]
pub fn is_animation_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "gif" | "png" | "apng" | "webp" | "tif" | "tiff" | "ico" | "cur"
    )
}

/// Decode GIF / APNG / animated WebP. Returns `None` for stills or unknown bytes.
#[must_use]
pub fn decode_animation(bytes: &[u8]) -> Option<AnimSequence> {
    let fmt = image::guess_format(bytes).ok()?;
    match fmt {
        image::ImageFormat::Gif => decode_gif(bytes),
        image::ImageFormat::Png => decode_apng(bytes),
        image::ImageFormat::WebP => decode_webp(bytes),
        _ => None,
    }
}

/// Decode animation from a local path (best-effort).
#[must_use]
pub fn load_animation_file(path: &Path) -> Option<Arc<AnimSequence>> {
    let bytes = std::fs::read(path).ok()?;
    decode_animation(&bytes)
        .or_else(|| crate::image::pages::decode_pages(&bytes))
        .map(Arc::new)
}

/// Write each frame as a sibling PNG `{stem}-f001.png`, never overwriting `src`.
///
/// # Errors
///
/// I/O or encode failure. Empty input is an error.
pub fn export_anim_frames(src: &Path, frames: &[AnimFrame]) -> Result<Vec<PathBuf>> {
    if frames.is_empty() {
        return Err(ViewerError::ImageDecode("no animation frames".into()));
    }
    let mut out = Vec::with_capacity(frames.len());
    for (i, frame) in frames.iter().enumerate() {
        let img = LoadedImage {
            rgba: Arc::clone(&frame.rgba),
            width: frame.width,
            height: frame.height,
            format: ImageFormat::Png,
            original_size_bytes: frame.rgba.len() as u64,
            ..LoadedImage::meta_defaults()
        };
        out.push(save_sibling(src, &img, &format!("f{:03}", i + 1))?);
    }
    Ok(out)
}

/// Write one frame as a sibling PNG `{stem}-{suffix}.png`, never overwriting `src`.
///
/// # Errors
///
/// I/O or encode failure.
pub fn export_anim_frame(src: &Path, frame: &AnimFrame, suffix: &str) -> Result<PathBuf> {
    let img = LoadedImage {
        rgba: Arc::clone(&frame.rgba),
        width: frame.width,
        height: frame.height,
        format: ImageFormat::Png,
        original_size_bytes: frame.rgba.len() as u64,
        ..LoadedImage::meta_defaults()
    };
    save_sibling(src, &img, suffix)
}

/// Suffix for extracting frame `index` (0-based).
#[must_use]
pub fn extract_frame_suffix(kind: AnimKind, index: usize, frame: &AnimFrame) -> String {
    match kind {
        AnimKind::Ico => format!("{}x{}", frame.width, frame.height),
        _ => format!("p{:03}", index + 1),
    }
}

/// Browser-style delay: 0 → 100 ms, anything under 20 ms → 20 ms.
#[must_use]
pub fn clamp_delay_ms(ms: u32) -> u32 {
    if ms == 0 {
        DEFAULT_DELAY_MS
    } else {
        ms.max(MIN_DELAY_MS)
    }
}

fn delay_of(frame: &Frame) -> u32 {
    let (num, den) = frame.delay().numer_denom_ms();
    if den == 0 {
        return DEFAULT_DELAY_MS;
    }
    clamp_delay_ms(num / den)
}

fn collect_frames<'a, D: AnimationDecoder<'a>>(decoder: D) -> Option<Vec<AnimFrame>> {
    let mut frames = Vec::new();
    let mut pixels = 0usize;
    for item in decoder.into_frames() {
        let frame = item.ok()?;
        let delay_ms = delay_of(&frame);
        let buf = frame.into_buffer();
        let (width, height) = buf.dimensions();
        if width == 0 || height == 0 {
            continue;
        }
        let next_pixels = pixels.saturating_add(width as usize * height as usize);
        if !frames.is_empty() && (frames.len() >= MAX_FRAMES || next_pixels > MAX_PIXELS) {
            break;
        }
        pixels = next_pixels;
        frames.push(AnimFrame {
            rgba: Arc::new(buf.into_raw()),
            width,
            height,
            delay_ms,
        });
    }
    (frames.len() >= 2).then_some(frames)
}

fn with_thumbs(frames: Vec<AnimFrame>, kind: AnimKind) -> AnimSequence {
    let thumbs = frames
        .iter()
        .take(MAX_THUMBS)
        .enumerate()
        .map(|(i, frame)| {
            let (rgba, width, height) = scale_rgba(frame, THUMB_EDGE);
            ImageThumbItem {
                path: format!("anim:{}", i + 1),
                name: thumb_name(kind, i, frame),
                size_bytes: 0,
                date_text: if kind.is_playback() {
                    format!("{}ms", frame.delay_ms)
                } else {
                    format!("{}×{}", frame.width, frame.height)
                },
                rating: 0,
                rgba: Some(rgba),
                width,
                height,
                selected: false,
                index: (i + 1) as u32,
                taken_ms: 0,
                has_gps: false,
                gps_lat: 0.0,
                gps_lon: 0.0,
            }
        })
        .collect();
    AnimSequence {
        frames,
        kind,
        thumbs,
    }
}

fn thumb_name(kind: AnimKind, index: usize, frame: &AnimFrame) -> String {
    match kind {
        AnimKind::Ico => format!("{}×{}", frame.width, frame.height),
        _ => (index + 1).to_string(),
    }
}

pub(crate) fn sequence_from_frames(frames: Vec<AnimFrame>, kind: AnimKind) -> AnimSequence {
    with_thumbs(frames, kind)
}

pub(crate) fn frame_from_dynamic(img: image::DynamicImage) -> Option<AnimFrame> {
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    Some(AnimFrame {
        rgba: Arc::new(rgba.into_raw()),
        width,
        height,
        delay_ms: 0,
    })
}

fn decode_gif(bytes: &[u8]) -> Option<AnimSequence> {
    let decoder = GifDecoder::new(Cursor::new(bytes)).ok()?;
    let frames = collect_frames(decoder)?;
    Some(with_thumbs(frames, AnimKind::Gif))
}

fn decode_apng(bytes: &[u8]) -> Option<AnimSequence> {
    let decoder = PngDecoder::new(Cursor::new(bytes)).ok()?;
    if !decoder.is_apng().ok()? {
        return None;
    }
    let frames = collect_frames(decoder.apng().ok()?)?;
    Some(with_thumbs(frames, AnimKind::Apng))
}

fn decode_webp(bytes: &[u8]) -> Option<AnimSequence> {
    let decoder = WebPDecoder::new(Cursor::new(bytes)).ok()?;
    if !decoder.has_animation() {
        return None;
    }
    let frames = collect_frames(decoder)?;
    Some(with_thumbs(frames, AnimKind::WebP))
}

fn scale_rgba(frame: &AnimFrame, max_edge: u32) -> (Arc<Vec<u8>>, u32, u32) {
    let src_w = frame.width.max(1);
    let src_h = frame.height.max(1);
    let scale = (max_edge as f32 / src_w.max(src_h) as f32).min(1.0);
    let dst_w = ((src_w as f32 * scale).round() as u32).max(1);
    let dst_h = ((src_h as f32 * scale).round() as u32).max(1);
    if dst_w == src_w && dst_h == src_h {
        return (Arc::clone(&frame.rgba), src_w, src_h);
    }
    let mut out = vec![0u8; dst_w as usize * dst_h as usize * 4];
    for y in 0..dst_h {
        let sy = y * src_h / dst_h;
        for x in 0..dst_w {
            let sx = x * src_w / dst_w;
            let src = (sy as usize * src_w as usize + sx as usize) * 4;
            let dst = (y as usize * dst_w as usize + x as usize) * 4;
            if let (Some(s), Some(d)) = (frame.rgba.get(src..src + 4), out.get_mut(dst..dst + 4)) {
                d.copy_from_slice(s);
            }
        }
    }
    (Arc::new(out), dst_w, dst_h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Rgba, RgbaImage};

    fn solid(w: u32, h: u32, rgb: [u8; 3]) -> RgbaImage {
        let mut img = RgbaImage::new(w, h);
        for p in img.pixels_mut() {
            *p = Rgba([rgb[0], rgb[1], rgb[2], 255]);
        }
        img
    }

    fn two_frame_gif() -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut enc = GifEncoder::new(&mut buf);
            enc.set_repeat(Repeat::Infinite).unwrap();
            enc.encode_frame(image::Frame::from_parts(
                solid(2, 2, [255, 0, 0]),
                0,
                0,
                Delay::from_numer_denom_ms(80, 1),
            ))
            .unwrap();
            enc.encode_frame(image::Frame::from_parts(
                solid(2, 2, [0, 255, 0]),
                0,
                0,
                Delay::from_numer_denom_ms(120, 1),
            ))
            .unwrap();
        }
        buf
    }

    #[test]
    fn clamp_delay_zero_is_100() {
        assert_eq!(clamp_delay_ms(0), 100);
        assert_eq!(clamp_delay_ms(10), 20);
        assert_eq!(clamp_delay_ms(80), 80);
    }

    #[test]
    fn gif_decodes_two_frames() {
        let seq = decode_animation(&two_frame_gif()).expect("animated gif");
        assert_eq!(seq.kind, AnimKind::Gif);
        assert_eq!(seq.frames.len(), 2);
        assert_eq!(seq.frames[0].width, 2);
        assert_eq!(&seq.frames[0].rgba[0..4], &[255, 0, 0, 255]);
        assert_eq!(&seq.frames[1].rgba[0..4], &[0, 255, 0, 255]);
        assert!(seq.frames[0].delay_ms >= 20);
        assert_eq!(seq.thumbs.len(), 2);
    }

    #[test]
    fn static_png_is_not_animation() {
        let mut cursor = std::io::Cursor::new(Vec::new());
        solid(2, 2, [1, 2, 3])
            .write_to(&mut cursor, image::ImageFormat::Png)
            .unwrap();
        assert!(decode_animation(&cursor.into_inner()).is_none());
    }

    #[test]
    fn export_writes_sibling_pngs() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("loop.gif");
        std::fs::write(&src, two_frame_gif()).unwrap();
        let seq = decode_animation(&std::fs::read(&src).unwrap()).unwrap();
        let paths = export_anim_frames(&src, &seq.frames).unwrap();
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].file_name().unwrap(), "loop-f001.png");
        assert_eq!(paths[1].file_name().unwrap(), "loop-f002.png");
        assert!(src.exists());
        for p in &paths {
            assert!(p.exists());
            assert_ne!(p, &src);
        }
    }

    #[test]
    fn animation_extensions() {
        assert!(is_animation_extension("GIF"));
        assert!(is_animation_extension("apng"));
        assert!(is_animation_extension("webp"));
        assert!(!is_animation_extension("jpg"));
    }
}
