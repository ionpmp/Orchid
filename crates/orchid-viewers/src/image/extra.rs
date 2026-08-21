//! Decoders the `image` crate does not cover: JXL, PSD, XCF, PCX, JPEG 2000.

use std::io::Cursor;

use crate::error::{Result, ViewerError};
use crate::image::loader::{ImageFormat, LoadedImage};

/// JPEG XL (`FF 0A` or ISO BMFF `JXL `).
#[must_use]
pub fn looks_like_jxl(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0x0A])
        || (bytes.len() >= 12
            && bytes.starts_with(&[0x00, 0x00, 0x00, 0x0C])
            && &bytes[4..8] == b"JXL ")
}

/// Photoshop PSD (`8BPS`).
#[must_use]
pub fn looks_like_psd(bytes: &[u8]) -> bool {
    bytes.starts_with(b"8BPS")
}

/// GIMP XCF.
#[must_use]
pub fn looks_like_xcf(bytes: &[u8]) -> bool {
    bytes.starts_with(b"gimp xcf ")
}

/// ZSoft PCX (`0x0A` + version 0–5).
#[must_use]
pub fn looks_like_pcx(bytes: &[u8]) -> bool {
    bytes.len() >= 128 && bytes[0] == 0x0A && bytes[1] <= 5 && bytes[2] <= 1
}

/// JPEG 2000 JP2 / J2K codestream.
#[must_use]
pub fn looks_like_jp2(bytes: &[u8]) -> bool {
    (bytes.len() >= 12 && bytes.starts_with(&[0x00, 0x00, 0x00, 0x0C]) && &bytes[4..8] == b"jP  ")
        || bytes.starts_with(&[0xFF, 0x4F, 0xFF, 0x51])
}

/// True when `bytes` match a format we handle outside `image::load_from_memory`.
#[must_use]
pub fn looks_like_extra_image(bytes: &[u8]) -> bool {
    looks_like_jxl(bytes)
        || looks_like_psd(bytes)
        || looks_like_xcf(bytes)
        || looks_like_pcx(bytes)
        || looks_like_jp2(bytes)
}

pub(crate) fn decode_jxl(bytes: &[u8], size: u64) -> Result<LoadedImage> {
    let image = jxl_oxide::JxlImage::builder()
        .read(Cursor::new(bytes))
        .map_err(|e| ViewerError::ImageDecode(format!("JXL: {e}")))?;
    if image.num_loaded_keyframes() == 0 {
        return Err(ViewerError::ImageDecode("JXL has no frames".into()));
    }
    let render = image
        .render_frame(0)
        .map_err(|e| ViewerError::ImageDecode(format!("JXL render: {e}")))?;
    let mut stream = render.stream();
    let width = stream.width();
    let height = stream.height();
    let channels = stream.channels() as usize;
    if width == 0 || height == 0 || !(3..=4).contains(&channels) {
        return Err(ViewerError::ImageDecode("JXL frame size".into()));
    }
    let n = width as usize * height as usize * channels;
    let mut fbuf = vec![0f32; n];
    stream.write_to_buffer(&mut fbuf);
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for px in fbuf.chunks_exact(channels) {
        rgba.push(f32_to_u8(px[0]));
        rgba.push(f32_to_u8(px[1]));
        rgba.push(f32_to_u8(px[2]));
        rgba.push(if channels == 4 { f32_to_u8(px[3]) } else { 255 });
    }
    Ok(loaded(rgba, width, height, ImageFormat::Jxl, size))
}

pub(crate) fn decode_psd(bytes: &[u8], size: u64) -> Result<LoadedImage> {
    let psd =
        psd::Psd::from_bytes(bytes).map_err(|e| ViewerError::ImageDecode(format!("PSD: {e}")))?;
    let rgba = psd.rgba();
    let width = psd.width();
    let height = psd.height();
    if rgba.len() != width as usize * height as usize * 4 {
        return Err(ViewerError::ImageDecode("PSD RGBA size".into()));
    }
    Ok(loaded(rgba, width, height, ImageFormat::Psd, size))
}

pub(crate) fn decode_xcf(bytes: &[u8], size: u64) -> Result<LoadedImage> {
    xcf::decode(bytes, size)
}

pub(crate) fn decode_pcx(bytes: &[u8], size: u64) -> Result<LoadedImage> {
    pcx::decode(bytes, size)
}

pub(crate) fn decode_jp2(bytes: &[u8], size: u64) -> Result<LoadedImage> {
    #[cfg(windows)]
    {
        crate::image::heic_wic::decode_wic(bytes, size, ImageFormat::Jpeg2000)
    }
    #[cfg(not(windows))]
    {
        let _ = (bytes, size);
        Err(ViewerError::UnsupportedJpeg2000)
    }
}

fn f32_to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
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

mod pcx {
    use super::{loaded, Result, ViewerError};
    use crate::image::loader::ImageFormat;

    pub(super) fn decode(bytes: &[u8], size: u64) -> Result<super::LoadedImage> {
        if bytes.len() < 128 || bytes[0] != 0x0A {
            return Err(ViewerError::ImageDecode("PCX header".into()));
        }
        let encoding = bytes[2];
        let bpp = bytes[3];
        let x0 = u16::from_le_bytes([bytes[4], bytes[5]]) as i32;
        let y0 = u16::from_le_bytes([bytes[6], bytes[7]]) as i32;
        let x1 = u16::from_le_bytes([bytes[8], bytes[9]]) as i32;
        let y1 = u16::from_le_bytes([bytes[10], bytes[11]]) as i32;
        let width = (x1 - x0 + 1).max(1) as u32;
        let height = (y1 - y0 + 1).max(1) as u32;
        let planes = bytes[65];
        let stride = u16::from_le_bytes([bytes[66], bytes[67]]) as usize;
        if encoding > 1 || bpp == 0 || planes == 0 || stride == 0 {
            return Err(ViewerError::ImageDecode("PCX unsupported header".into()));
        }
        let plane_bytes = stride * planes as usize;
        let mut raw = vec![0u8; plane_bytes * height as usize];
        let mut src = 128usize;
        let mut dst = 0usize;
        while dst < raw.len() && src < bytes.len() {
            if encoding == 1 {
                let b = bytes[src];
                src += 1;
                if b & 0xC0 == 0xC0 {
                    let count = (b & 0x3F) as usize;
                    if src >= bytes.len() {
                        break;
                    }
                    let val = bytes[src];
                    src += 1;
                    for _ in 0..count {
                        if dst >= raw.len() {
                            break;
                        }
                        raw[dst] = val;
                        dst += 1;
                    }
                } else {
                    raw[dst] = b;
                    dst += 1;
                }
            } else {
                raw[dst] = bytes[src];
                src += 1;
                dst += 1;
            }
        }
        let mut rgba = vec![0u8; width as usize * height as usize * 4];
        match (bpp, planes) {
            (8, 1) => {
                let pal = if bytes.len() >= 769 && bytes[bytes.len() - 769] == 0x0C {
                    &bytes[bytes.len() - 768..]
                } else {
                    &bytes[16..16 + 48]
                };
                for y in 0..height as usize {
                    for x in 0..width as usize {
                        let idx = raw[y * stride + x] as usize;
                        let o = (y * width as usize + x) * 4;
                        let p = idx.min(255) * 3;
                        if p + 2 < pal.len() {
                            rgba[o] = pal[p];
                            rgba[o + 1] = pal[p + 1];
                            rgba[o + 2] = pal[p + 2];
                        }
                        rgba[o + 3] = 255;
                    }
                }
            }
            (8, 3) => {
                for y in 0..height as usize {
                    let row = y * plane_bytes;
                    for x in 0..width as usize {
                        let o = (y * width as usize + x) * 4;
                        rgba[o] = raw[row + x];
                        rgba[o + 1] = raw[row + stride + x];
                        rgba[o + 2] = raw[row + stride * 2 + x];
                        rgba[o + 3] = 255;
                    }
                }
            }
            _ => return Err(ViewerError::ImageDecode("PCX bit depth".into())),
        }
        Ok(loaded(rgba, width, height, ImageFormat::Pcx, size))
    }

    #[cfg(test)]
    pub(super) fn encode_rgb8(w: u32, h: u32, rgb: &[u8]) -> Vec<u8> {
        let mut out = vec![0u8; 128];
        out[0] = 0x0A;
        out[1] = 5;
        out[2] = 1;
        out[3] = 8;
        let x1 = (w - 1) as u16;
        let y1 = (h - 1) as u16;
        out[8] = x1 as u8;
        out[9] = (x1 >> 8) as u8;
        out[10] = y1 as u8;
        out[11] = (y1 >> 8) as u8;
        out[65] = 3;
        let stride = w as u16;
        out[66] = stride as u8;
        out[67] = (stride >> 8) as u8;
        for y in 0..h as usize {
            for plane in 0..3 {
                for x in 0..w as usize {
                    let v = rgb[(y * w as usize + x) * 3 + plane];
                    if v < 0xC0 {
                        out.push(v);
                    } else {
                        out.push(0xC1);
                        out.push(v);
                    }
                }
            }
        }
        out
    }
}

mod xcf {
    use super::{loaded, Result, ViewerError};
    use crate::image::loader::ImageFormat;

    pub(super) fn decode(bytes: &[u8], size: u64) -> Result<super::LoadedImage> {
        let mut cur = Cursor::new(bytes);
        if !cur.take(9).eq(b"gimp xcf ") {
            return Err(ViewerError::ImageDecode("XCF magic".into()));
        }
        let ver = read_cstring(&mut cur)?;
        let version = parse_xcf_version(&ver);
        let width = read_u32(&mut cur)?;
        let height = read_u32(&mut cur)?;
        let base = read_u32(&mut cur)?;
        if version >= 4 {
            let _precision = read_u32(&mut cur)?;
        }
        let compression = skip_props(&mut cur)?;
        let layer_off = read_u32(&mut cur)? as usize;
        if layer_off == 0 || layer_off >= bytes.len() {
            return Err(ViewerError::ImageDecode("XCF has no layers".into()));
        }
        let (rgba, w, h) = decode_layer(bytes, layer_off, base, version, compression)?;
        if w == 0 || h == 0 {
            return Err(ViewerError::ImageDecode("XCF empty layer".into()));
        }
        let _ = (width, height);
        Ok(loaded(rgba, w, h, ImageFormat::Xcf, size))
    }

    struct Cursor<'a> {
        data: &'a [u8],
        pos: usize,
    }

    impl<'a> Cursor<'a> {
        fn new(data: &'a [u8]) -> Self {
            Self { data, pos: 0 }
        }
        fn take(&mut self, n: usize) -> &'a [u8] {
            let end = (self.pos + n).min(self.data.len());
            let s = &self.data[self.pos..end];
            self.pos = end;
            s
        }
        fn seek(&mut self, p: usize) {
            self.pos = p.min(self.data.len());
        }
    }

    fn parse_xcf_version(s: &str) -> u32 {
        if s == "file" {
            0
        } else {
            s.trim_start_matches('v').parse().unwrap_or(0)
        }
    }

    fn read_u32(cur: &mut Cursor<'_>) -> Result<u32> {
        let b = cur.take(4);
        if b.len() < 4 {
            return Err(ViewerError::ImageDecode("XCF truncated".into()));
        }
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_cstring(cur: &mut Cursor<'_>) -> Result<String> {
        let start = cur.pos;
        while cur.pos < cur.data.len() && cur.data[cur.pos] != 0 {
            cur.pos += 1;
        }
        let s = std::str::from_utf8(&cur.data[start..cur.pos])
            .map_err(|e| ViewerError::ImageDecode(format!("XCF: {e}")))?
            .to_string();
        if cur.pos < cur.data.len() {
            cur.pos += 1;
        }
        Ok(s)
    }

    fn skip_props(cur: &mut Cursor<'_>) -> Result<u32> {
        let mut compression = 1u32;
        loop {
            let typ = read_u32(cur)?;
            let len = read_u32(cur)? as usize;
            if typ == 0 {
                break;
            }
            if typ == 17 && len >= 1 && cur.pos < cur.data.len() {
                compression = u32::from(cur.data[cur.pos]);
            }
            cur.pos = cur.pos.saturating_add(len).min(cur.data.len());
        }
        Ok(compression)
    }

    fn decode_layer(
        data: &[u8],
        offset: usize,
        base: u32,
        version: u32,
        mut compression: u32,
    ) -> Result<(Vec<u8>, u32, u32)> {
        let mut cur = Cursor::new(data);
        cur.seek(offset);
        let width = read_u32(&mut cur)?;
        let height = read_u32(&mut cur)?;
        if version >= 11 {
            let _typ = read_u32(&mut cur)?;
        }
        let _name = read_cstring(&mut cur)?;
        let layer_comp = skip_props(&mut cur)?;
        if layer_comp != 1 {
            compression = layer_comp;
        }
        let hier = read_u32(&mut cur)? as usize;
        if hier == 0 || hier >= data.len() {
            return Err(ViewerError::ImageDecode("XCF hierarchy".into()));
        }
        cur.seek(hier);
        let hw = read_u32(&mut cur)?;
        let hh = read_u32(&mut cur)?;
        let bpp = read_u32(&mut cur)?;
        let level = read_u32(&mut cur)? as usize;
        if level == 0 || level >= data.len() {
            return Err(ViewerError::ImageDecode("XCF level".into()));
        }
        cur.seek(level);
        let _lw = read_u32(&mut cur)?;
        let _lh = read_u32(&mut cur)?;
        let tiles_x = hw.div_ceil(64);
        let tiles_y = hh.div_ceil(64);
        let n_tiles = tiles_x * tiles_y;
        let mut offsets = Vec::with_capacity(n_tiles as usize);
        for _ in 0..n_tiles {
            offsets.push(read_u32(&mut cur)? as usize);
        }
        let channels = bpp.max(1) as usize;
        let mut plane = vec![0u8; hw as usize * hh as usize * channels];
        for (i, off) in offsets.into_iter().enumerate() {
            if off == 0 || off >= data.len() {
                continue;
            }
            let tx = (i as u32) % tiles_x;
            let ty = (i as u32) / tiles_x;
            let tw = (hw - tx * 64).min(64);
            let th = (hh - ty * 64).min(64);
            decode_tile(
                data,
                off,
                &mut plane,
                hw,
                tx * 64,
                ty * 64,
                tw,
                th,
                channels,
                compression,
            )?;
        }
        let mut rgba = vec![0u8; hw as usize * hh as usize * 4];
        for i in 0..(hw * hh) as usize {
            let o = i * 4;
            match (base, channels) {
                (1, 1) => {
                    rgba[o] = plane[i];
                    rgba[o + 1] = plane[i];
                    rgba[o + 2] = plane[i];
                    rgba[o + 3] = 255;
                }
                (_, 3) => {
                    rgba[o] = plane[i * 3];
                    rgba[o + 1] = plane[i * 3 + 1];
                    rgba[o + 2] = plane[i * 3 + 2];
                    rgba[o + 3] = 255;
                }
                (_, n) if n >= 4 => {
                    rgba[o] = plane[i * n];
                    rgba[o + 1] = plane[i * n + 1];
                    rgba[o + 2] = plane[i * n + 2];
                    rgba[o + 3] = plane[i * n + 3];
                }
                _ => {
                    rgba[o + 3] = 255;
                }
            }
        }
        let _ = (width, height);
        Ok((rgba, hw, hh))
    }

    #[allow(clippy::too_many_arguments)]
    fn decode_tile(
        data: &[u8],
        offset: usize,
        dest: &mut [u8],
        stride_w: u32,
        x0: u32,
        y0: u32,
        tw: u32,
        th: u32,
        channels: usize,
        compression: u32,
    ) -> Result<()> {
        let tile_len = tw as usize * th as usize * channels;
        let raw = if compression == 0 {
            data.get(offset..offset.saturating_add(tile_len))
                .ok_or_else(|| ViewerError::ImageDecode("XCF tile".into()))?
                .to_vec()
        } else {
            rle_decode(&data[offset..], tile_len)?
        };
        if raw.len() < tile_len {
            return Err(ViewerError::ImageDecode("XCF tile short".into()));
        }
        // GIMP stores channels planar per tile.
        let plane = tw as usize * th as usize;
        for y in 0..th as usize {
            for x in 0..tw as usize {
                let di = ((y0 as usize + y) * stride_w as usize + x0 as usize + x) * channels;
                let si = y * tw as usize + x;
                for c in 0..channels {
                    dest[di + c] = raw[c * plane + si];
                }
            }
        }
        Ok(())
    }

    fn rle_decode(src: &[u8], want: usize) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(want);
        let mut i = 0;
        while out.len() < want && i < src.len() {
            let opcode = src[i];
            i += 1;
            if opcode <= 126 {
                let n = opcode as usize + 1;
                if i + n > src.len() {
                    break;
                }
                out.extend_from_slice(&src[i..i + n]);
                i += n;
            } else if opcode >= 129 {
                let n = 257 - opcode as usize;
                if i >= src.len() {
                    break;
                }
                let v = src[i];
                i += 1;
                out.extend(std::iter::repeat_n(v, n));
            }
        }
        if out.len() < want {
            out.resize(want, 0);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_extra_magics() {
        assert!(looks_like_jxl(&[0xFF, 0x0A]));
        assert!(looks_like_psd(b"8BPS"));
        assert!(looks_like_xcf(b"gimp xcf file"));
        assert!(looks_like_jp2(&[0xFF, 0x4F, 0xFF, 0x51]));
        let mut pcx = vec![0u8; 128];
        pcx[0] = 0x0A;
        pcx[1] = 5;
        assert!(looks_like_pcx(&pcx));
    }

    #[test]
    fn pcx_roundtrip_rgb() {
        let rgb = [10u8, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120];
        let bytes = pcx::encode_rgb8(2, 2, &rgb);
        let img = decode_pcx(&bytes, bytes.len() as u64).unwrap();
        assert_eq!(img.format, ImageFormat::Pcx);
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 2);
        assert_eq!(&img.rgba[0..3], &[10, 20, 30]);
        assert_eq!(&img.rgba[4..7], &[40, 50, 60]);
    }

    #[test]
    fn jp2_without_codec_is_clear() {
        let err = decode_jp2(&[0xFF, 0x4F, 0xFF, 0x51], 4).unwrap_err();
        assert!(
            matches!(err, ViewerError::UnsupportedJpeg2000)
                || matches!(err, ViewerError::UnsupportedHeic),
            "{err:?}"
        );
    }
}
