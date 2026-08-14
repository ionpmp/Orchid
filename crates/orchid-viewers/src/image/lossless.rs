//! Lossless on-disk transforms for JPEG (DCT / jpegtran-style) and PNG.

use std::io::Cursor;

use image::{DynamicImage, ImageFormat as ImgFmt};

use crate::error::{Result, ViewerError};
use crate::image::exif::{apply_orientation, orientation_from_bytes};
use crate::image::loader::ImageFormat;

/// View-independent file transform (writes new bytes, no pixel recompress for JPEG).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LosslessOp {
    /// 90° clockwise.
    Rotate90,
    /// 180°.
    Rotate180,
    /// 90° counter-clockwise / 270° clockwise.
    Rotate270,
    /// Mirror left–right.
    FlipH,
    /// Mirror top–bottom.
    FlipV,
    /// Apply EXIF orientation in the DCT domain and reset the tag to 1.
    AutoExif,
    /// Crop to `(x, y, w, h)` in image pixels (JPEG snaps to the MCU grid).
    Crop {
        /// Left edge in image pixels.
        x: u32,
        /// Top edge in image pixels.
        y: u32,
        /// Width in image pixels.
        w: u32,
        /// Height in image pixels.
        h: u32,
    },
}

impl LosslessOp {
    /// Parse a command token (`cw`, `ccw`, `180`, `flip-h`, `flip-v`, `exif`).
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "cw" | "90" => Some(Self::Rotate90),
            "180" => Some(Self::Rotate180),
            "ccw" | "270" => Some(Self::Rotate270),
            "flip-h" | "fliph" => Some(Self::FlipH),
            "flip-v" | "flipv" => Some(Self::FlipV),
            "exif" => Some(Self::AutoExif),
            _ => None,
        }
    }
}

/// Apply `op` to encoded `bytes`. Format is taken from `format` or sniffed.
///
/// # Errors
///
/// Unsupported format, empty crop, or JPEG transform failure.
pub fn apply_lossless(bytes: &[u8], format: ImageFormat, op: LosslessOp) -> Result<Vec<u8>> {
    let format = sniff_format(bytes, format);
    match format {
        ImageFormat::Jpeg => jpeg_lossless(bytes, op),
        ImageFormat::Png => png_lossless(bytes, op),
        other => Err(ViewerError::ImageLossless(format!(
            "lossless transforms support JPEG and PNG, not {}",
            other.label()
        ))),
    }
}

fn sniff_format(bytes: &[u8], hinted: ImageFormat) -> ImageFormat {
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return ImageFormat::Jpeg;
    }
    if bytes.len() >= 8 && bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return ImageFormat::Png;
    }
    hinted
}

fn jpeg_lossless(bytes: &[u8], op: LosslessOp) -> Result<Vec<u8>> {
    use libjpeg_turbo_rs::{
        probe, transform_jpeg_with_options, CropRegion, TransformOp, TransformOptions,
    };

    let info = probe(bytes).map_err(|e| ViewerError::ImageLossless(e.to_string()))?;
    let (op_kind, crop) = match op {
        LosslessOp::Rotate90 => (TransformOp::Rot90, None),
        LosslessOp::Rotate180 => (TransformOp::Rot180, None),
        LosslessOp::Rotate270 => (TransformOp::Rot270, None),
        LosslessOp::FlipH => (TransformOp::HFlip, None),
        LosslessOp::FlipV => (TransformOp::VFlip, None),
        LosslessOp::AutoExif => {
            let orient = info.exif_orientation.unwrap_or(1).clamp(1, 8) as u8;
            if orient <= 1 {
                return Ok(bytes.to_vec());
            }
            let Some(kind) = TransformOp::from_exif_orientation(orient) else {
                return Ok(set_jpeg_exif_orientation(bytes, 1));
            };
            if matches!(kind, TransformOp::None) {
                return Ok(set_jpeg_exif_orientation(bytes, 1));
            }
            (kind, None)
        }
        LosslessOp::Crop { x, y, w, h } => {
            if w == 0 || h == 0 {
                return Err(ViewerError::ImageLossless("empty crop".into()));
            }
            let (mcu_w, mcu_h) = jpeg_mcu(info.subsampling);
            let x = (x / mcu_w) * mcu_w;
            let y = (y / mcu_h) * mcu_h;
            let w = w.max(mcu_w);
            let h = h.max(mcu_h);
            let max_w = (info.width as u32).saturating_sub(x).max(1);
            let max_h = (info.height as u32).saturating_sub(y).max(1);
            let w = w.min(max_w);
            let h = h.min(max_h);
            (
                TransformOp::None,
                Some(CropRegion {
                    x: x as usize,
                    y: y as usize,
                    width: w as usize,
                    height: h as usize,
                }),
            )
        }
    };

    let options = TransformOptions {
        op: op_kind,
        trim: true,
        crop,
        ..TransformOptions::default()
    };
    let out = transform_jpeg_with_options(bytes, &options)
        .map_err(|e| ViewerError::ImageLossless(e.to_string()))?;
    Ok(set_jpeg_exif_orientation(&out, 1))
}

fn jpeg_mcu(subsampling: impl core::fmt::Debug) -> (u32, u32) {
    let s = format!("{subsampling:?}").to_ascii_lowercase();
    if s.contains("420") || s.contains("s420") {
        (16, 16)
    } else if s.contains("422") || s.contains("s422") {
        (16, 8)
    } else if s.contains("440") || s.contains("s440") {
        (8, 16)
    } else {
        (8, 8)
    }
}

/// Set EXIF Orientation (0x0112) to `value` when an APP1 Exif IFD is present.
fn set_jpeg_exif_orientation(jpeg: &[u8], value: u16) -> Vec<u8> {
    let mut out = jpeg.to_vec();
    let Some(app1) = find_exif_app1(&out) else {
        return out;
    };
    let payload = app1 + 4;
    if payload + 6 > out.len() || &out[payload..payload + 6] != b"Exif\0\0" {
        return out;
    }
    let tiff = payload + 6;
    patch_tiff_orientation(&mut out[tiff..], value);
    out
}

fn find_exif_app1(jpeg: &[u8]) -> Option<usize> {
    if jpeg.len() < 4 || jpeg[0] != 0xFF || jpeg[1] != 0xD8 {
        return None;
    }
    let mut i = 2usize;
    while i + 4 <= jpeg.len() {
        if jpeg[i] != 0xFF {
            return None;
        }
        let marker = jpeg[i + 1];
        if marker == 0xD9 || marker == 0xDA {
            return None;
        }
        if marker == 0x00 || marker == 0xFF {
            i += 1;
            continue;
        }
        if i + 4 > jpeg.len() {
            return None;
        }
        let len = u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]) as usize;
        if len < 2 || i + 2 + len > jpeg.len() {
            return None;
        }
        if marker == 0xE1 && i + 10 <= jpeg.len() && jpeg[i + 4..i + 10].starts_with(b"Exif") {
            return Some(i);
        }
        i += 2 + len;
    }
    None
}

fn patch_tiff_orientation(tiff: &mut [u8], value: u16) -> bool {
    if tiff.len() < 8 {
        return false;
    }
    let le = match &tiff[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return false,
    };
    let read_u16 = |b: &[u8], off: usize| -> Option<u16> {
        let s = b.get(off..off + 2)?;
        Some(if le {
            u16::from_le_bytes([s[0], s[1]])
        } else {
            u16::from_be_bytes([s[0], s[1]])
        })
    };
    let read_u32 = |b: &[u8], off: usize| -> Option<u32> {
        let s = b.get(off..off + 4)?;
        Some(if le {
            u32::from_le_bytes([s[0], s[1], s[2], s[3]])
        } else {
            u32::from_be_bytes([s[0], s[1], s[2], s[3]])
        })
    };
    let write_u16 = |b: &mut [u8], off: usize, v: u16| {
        let bytes = if le { v.to_le_bytes() } else { v.to_be_bytes() };
        if let Some(slot) = b.get_mut(off..off + 2) {
            slot.copy_from_slice(&bytes);
        }
    };
    let ifd0 = read_u32(tiff, 4).unwrap_or(0) as usize;
    let count = read_u16(tiff, ifd0).unwrap_or(0) as usize;
    for n in 0..count {
        let e = ifd0 + 2 + n * 12;
        let tag = read_u16(tiff, e).unwrap_or(0);
        if tag == 0x0112 {
            write_u16(tiff, e + 8, value);
            return true;
        }
    }
    false
}

fn png_lossless(bytes: &[u8], op: LosslessOp) -> Result<Vec<u8>> {
    let img =
        image::load_from_memory(bytes).map_err(|e| ViewerError::ImageLossless(e.to_string()))?;
    let img = match op {
        LosslessOp::Rotate90 => img.rotate90(),
        LosslessOp::Rotate180 => img.rotate180(),
        LosslessOp::Rotate270 => img.rotate270(),
        LosslessOp::FlipH => img.fliph(),
        LosslessOp::FlipV => img.flipv(),
        LosslessOp::AutoExif => {
            let orient = orientation_from_bytes(bytes);
            if orient <= 1 {
                return Ok(bytes.to_vec());
            }
            apply_orientation(img, orient)
        }
        LosslessOp::Crop { x, y, w, h } => {
            use image::GenericImageView;
            let (iw, ih) = img.dimensions();
            if w == 0 || h == 0 || x >= iw || y >= ih {
                return Err(ViewerError::ImageLossless("crop outside image".into()));
            }
            let w = w.min(iw - x);
            let h = h.min(ih - y);
            img.crop_imm(x, y, w, h)
        }
    };
    encode_png(&img)
}

fn encode_png(img: &DynamicImage) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), ImgFmt::Png)
        .map_err(|e| ViewerError::ImageLossless(e.to_string()))?;
    Ok(out)
}

/// Guess format from a path extension.
#[must_use]
pub fn format_from_extension(ext: Option<&str>) -> ImageFormat {
    match ext.map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("jpg" | "jpeg") => ImageFormat::Jpeg,
        Some("png") => ImageFormat::Png,
        _ => ImageFormat::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, ImageBuffer, Rgb};

    fn tiny_rgb() -> DynamicImage {
        DynamicImage::ImageRgb8(ImageBuffer::from_fn(32, 16, |x, y| {
            if x < 16 {
                Rgb([
                    255,
                    u8::try_from(y.saturating_mul(8).min(255)).unwrap_or(0),
                    0,
                ])
            } else {
                Rgb([0, 0, 255])
            }
        }))
    }

    fn encode(img: &DynamicImage, fmt: ImgFmt) -> Vec<u8> {
        let mut out = Vec::new();
        img.write_to(&mut Cursor::new(&mut out), fmt).unwrap();
        out
    }

    #[test]
    fn jpeg_rotate_90_swaps_axes() {
        let jpeg = encode(&tiny_rgb(), ImgFmt::Jpeg);
        let out = apply_lossless(&jpeg, ImageFormat::Jpeg, LosslessOp::Rotate90).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!(decoded.dimensions(), (16, 32));
    }

    #[test]
    fn jpeg_flip_h_keeps_size() {
        let jpeg = encode(&tiny_rgb(), ImgFmt::Jpeg);
        let out = apply_lossless(&jpeg, ImageFormat::Jpeg, LosslessOp::FlipH).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!(decoded.dimensions(), (32, 16));
    }

    #[test]
    fn jpeg_crop_shrinks() {
        let jpeg = encode(&tiny_rgb(), ImgFmt::Jpeg);
        let out = apply_lossless(
            &jpeg,
            ImageFormat::Jpeg,
            LosslessOp::Crop {
                x: 0,
                y: 0,
                w: 16,
                h: 16,
            },
        )
        .unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert!(decoded.width() <= 16);
        assert!(decoded.height() <= 16);
        assert!(decoded.width() >= 8);
    }

    #[test]
    fn png_rotate_90_swaps_axes() {
        let png = encode(&tiny_rgb(), ImgFmt::Png);
        let out = apply_lossless(&png, ImageFormat::Png, LosslessOp::Rotate90).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!(decoded.dimensions(), (16, 32));
    }

    #[test]
    fn jpeg_auto_exif_noop_without_tag() {
        let jpeg = encode(&tiny_rgb(), ImgFmt::Jpeg);
        let out = apply_lossless(&jpeg, ImageFormat::Jpeg, LosslessOp::AutoExif).unwrap();
        assert_eq!(
            image::load_from_memory(&out).unwrap().dimensions(),
            (32, 16)
        );
    }

    #[test]
    fn token_parse() {
        assert_eq!(LosslessOp::from_token("cw"), Some(LosslessOp::Rotate90));
        assert_eq!(LosslessOp::from_token("exif"), Some(LosslessOp::AutoExif));
        assert_eq!(LosslessOp::from_token("nope"), None);
    }
}
