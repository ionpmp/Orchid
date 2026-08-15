//! Multi-page TIFF and multi-size ICO / CUR.

use crate::image::anim::{
    frame_from_dynamic, sequence_from_frames, AnimFrame, AnimKind, AnimSequence,
};

const MAX_PAGES: usize = 256;
const MAX_PIXELS: usize = 16 * 1024 * 1024;

/// Decode a multi-page TIFF or multi-size ICO. Returns `None` for stills.
#[must_use]
pub fn decode_pages(bytes: &[u8]) -> Option<AnimSequence> {
    if looks_like_ico(bytes) {
        return decode_ico(bytes);
    }
    if looks_like_tiff(bytes) {
        return decode_tiff(bytes);
    }
    None
}

fn looks_like_tiff(bytes: &[u8]) -> bool {
    bytes.len() >= 8
        && (bytes.starts_with(b"II*\0")
            || bytes.starts_with(b"MM\0*")
            || bytes.starts_with(b"II+\0")
            || bytes.starts_with(b"MM\0+"))
}

fn looks_like_ico(bytes: &[u8]) -> bool {
    if bytes.len() < 6 {
        return false;
    }
    let reserved = u16::from_le_bytes([bytes[0], bytes[1]]);
    let kind = u16::from_le_bytes([bytes[2], bytes[3]]);
    reserved == 0 && (kind == 1 || kind == 2)
}

fn decode_tiff(bytes: &[u8]) -> Option<AnimSequence> {
    let offsets = tiff_ifd_offsets(bytes)?;
    if offsets.len() < 2 {
        return None;
    }
    let le = bytes.starts_with(b"II");
    let mut frames = Vec::new();
    let mut pixels = 0usize;
    let mut scratch = bytes.to_vec();
    for &off in &offsets {
        if frames.len() >= MAX_PAGES {
            break;
        }
        let Some(frame) = decode_tiff_ifd(&mut scratch, off, le) else {
            continue;
        };
        let next_pixels = pixels.saturating_add(frame.width as usize * frame.height as usize);
        if !frames.is_empty() && next_pixels > MAX_PIXELS {
            break;
        }
        pixels = next_pixels;
        frames.push(frame);
    }
    (frames.len() >= 2).then(|| sequence_from_frames(frames, AnimKind::Tiff))
}

fn decode_tiff_ifd(bytes: &mut [u8], ifd_off: u32, le: bool) -> Option<AnimFrame> {
    if bytes.len() < 8 {
        return None;
    }
    write_u32(bytes, 4, ifd_off, le)?;
    let count = u16_at(bytes, ifd_off as usize, le)? as usize;
    let next_at = ifd_off as usize + 2 + count * 12;
    let saved = u32_at(bytes, next_at, le)?;
    write_u32(bytes, next_at, 0, le)?;
    let img = image::load_from_memory(bytes).ok();
    write_u32(bytes, next_at, saved, le)?;
    frame_from_dynamic(img?)
}

fn tiff_ifd_offsets(bytes: &[u8]) -> Option<Vec<u32>> {
    if bytes.len() < 8 {
        return None;
    }
    let le = match &bytes[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let magic = u16_at(bytes, 2, le)?;
    if magic != 42 {
        return None;
    }
    let mut off = u32_at(bytes, 4, le)? as usize;
    let mut out = Vec::new();
    while off >= 8 && off + 2 <= bytes.len() && out.len() < MAX_PAGES {
        if out.contains(&(off as u32)) {
            break;
        }
        out.push(off as u32);
        let count = u16_at(bytes, off, le)? as usize;
        let next_at = off + 2 + count * 12;
        off = u32_at(bytes, next_at, le)? as usize;
    }
    Some(out)
}

fn decode_ico(bytes: &[u8]) -> Option<AnimSequence> {
    if bytes.len() < 6 {
        return None;
    }
    let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    if count < 2 {
        return None;
    }
    let mut frames = Vec::new();
    let mut pixels = 0usize;
    for i in 0..count.min(MAX_PAGES) {
        let entry = 6 + i * 16;
        if entry + 16 > bytes.len() {
            break;
        }
        let data_len = u32::from_le_bytes(bytes[entry + 8..entry + 12].try_into().ok()?) as usize;
        let data_off = u32::from_le_bytes(bytes[entry + 12..entry + 16].try_into().ok()?) as usize;
        if data_off == 0 || data_len == 0 || data_off.saturating_add(data_len) > bytes.len() {
            continue;
        }
        let single = rebuild_single_ico(
            &bytes[entry..entry + 16],
            &bytes[data_off..data_off + data_len],
        )?;
        let img = image::load_from_memory(&single).ok()?;
        let Some(frame) = frame_from_dynamic(img) else {
            continue;
        };
        let next_pixels = pixels.saturating_add(frame.width as usize * frame.height as usize);
        if !frames.is_empty() && next_pixels > MAX_PIXELS {
            break;
        }
        pixels = next_pixels;
        frames.push(frame);
    }
    (frames.len() >= 2).then(|| sequence_from_frames(frames, AnimKind::Ico))
}

fn rebuild_single_ico(entry: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    if entry.len() < 16 {
        return None;
    }
    let mut out = Vec::with_capacity(22 + data.len());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&entry[..8]);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&22u32.to_le_bytes());
    out.extend_from_slice(data);
    Some(out)
}

fn u16_at(bytes: &[u8], off: usize, le: bool) -> Option<u16> {
    let b = bytes.get(off..off + 2)?;
    Some(if le {
        u16::from_le_bytes([b[0], b[1]])
    } else {
        u16::from_be_bytes([b[0], b[1]])
    })
}

fn u32_at(bytes: &[u8], off: usize, le: bool) -> Option<u32> {
    let b = bytes.get(off..off + 4)?;
    Some(if le {
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    } else {
        u32::from_be_bytes([b[0], b[1], b[2], b[3]])
    })
}

fn write_u32(bytes: &mut [u8], off: usize, value: u32, le: bool) -> Option<()> {
    let slot = bytes.get_mut(off..off + 4)?;
    let raw = if le {
        value.to_le_bytes()
    } else {
        value.to_be_bytes()
    };
    slot.copy_from_slice(&raw);
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::anim::{export_anim_frame, extract_frame_suffix};
    use image::codecs::ico::{IcoEncoder, IcoFrame};
    use image::{ExtendedColorType, Rgba, RgbaImage};
    use std::io::Cursor;

    fn solid(w: u32, h: u32, rgb: [u8; 3]) -> RgbaImage {
        let mut img = RgbaImage::new(w, h);
        for p in img.pixels_mut() {
            *p = Rgba([rgb[0], rgb[1], rgb[2], 255]);
        }
        img
    }

    fn two_page_tiff() -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut enc = tiff::encoder::TiffEncoder::new(Cursor::new(&mut buf)).unwrap();
            let red = [255u8, 0, 0].as_slice().repeat(4);
            enc.write_image::<tiff::encoder::colortype::RGB8>(2, 2, &red)
                .unwrap();
            let green = [0u8, 255, 0].as_slice().repeat(4);
            enc.write_image::<tiff::encoder::colortype::RGB8>(2, 2, &green)
                .unwrap();
        }
        buf
    }

    fn two_size_ico() -> Vec<u8> {
        let a = solid(16, 16, [255, 0, 0]);
        let b = solid(32, 32, [0, 0, 255]);
        let fa = IcoFrame::as_png(a.as_raw(), 16, 16, ExtendedColorType::Rgba8).unwrap();
        let fb = IcoFrame::as_png(b.as_raw(), 32, 32, ExtendedColorType::Rgba8).unwrap();
        let mut buf = Vec::new();
        IcoEncoder::new(&mut buf).encode_images(&[fa, fb]).unwrap();
        buf
    }

    #[test]
    fn tiff_decodes_two_pages() {
        let seq = decode_pages(&two_page_tiff()).expect("multi-page tiff");
        assert_eq!(seq.kind, AnimKind::Tiff);
        assert_eq!(seq.frames.len(), 2);
        assert_eq!(&seq.frames[0].rgba[0..3], &[255, 0, 0]);
        assert_eq!(&seq.frames[1].rgba[0..3], &[0, 255, 0]);
        assert!(!seq.kind.is_playback());
    }

    #[test]
    fn ico_decodes_two_sizes() {
        let seq = decode_pages(&two_size_ico()).expect("multi-size ico");
        assert_eq!(seq.kind, AnimKind::Ico);
        assert_eq!(seq.frames.len(), 2);
        assert_eq!(seq.frames[0].width, 16);
        assert_eq!(seq.frames[1].width, 32);
        assert_eq!(seq.thumbs[0].name, "16×16");
    }

    #[test]
    fn extract_writes_sibling() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("scan.tif");
        std::fs::write(&src, two_page_tiff()).unwrap();
        let seq = decode_pages(&std::fs::read(&src).unwrap()).unwrap();
        let suffix = extract_frame_suffix(seq.kind, 1, &seq.frames[1]);
        let dest = export_anim_frame(&src, &seq.frames[1], &suffix).unwrap();
        assert_eq!(dest.file_name().unwrap(), "scan-p002.png");
        assert!(dest.exists());
        assert!(src.exists());
    }

    #[test]
    fn static_png_is_not_pages() {
        let mut cursor = Cursor::new(Vec::new());
        solid(2, 2, [1, 2, 3])
            .write_to(&mut cursor, image::ImageFormat::Png)
            .unwrap();
        assert!(decode_pages(&cursor.into_inner()).is_none());
    }
}
