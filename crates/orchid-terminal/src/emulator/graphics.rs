//! Sixel and Kitty inline-image decoders.
//!
//! Kitty graphics arrive as an APC string (`ESC _ ... ST`). The `vte` parser
//! discards APC, so [`ApcSplitter`] pulls those strings out before bytes are
//! handed to the parser. Sixel arrives as a DCS (`ESC P ... q`) and is
//! collected by the parser's hook / put / unhook callbacks.
//!
//! Covered Kitty subset: direct (`o` absent) 24-bit RGB, 32-bit RGBA, and PNG
//! (`f=100`). zlib payloads (`o=z`) are rejected.

/// Maximum sixel body retained for one DCS.
pub(crate) const SIXEL_CAP: usize = 1_000_000;
/// Maximum accumulated Kitty payload (decoded bytes) for one image id.
pub(crate) const KITTY_CAP: usize = 4_000_000;

const MAX_DIM: usize = 2048;
const MAX_PIXELS: usize = 2_000_000;
const APC_CAP: usize = 4 * 1024 * 1024;

/// One slice of a PTY read, in the order it arrived.
pub(crate) enum StreamEvent {
    /// Bytes for the VT parser (APC already removed).
    Vt(Vec<u8>),
    /// One completed APC payload, without the introducer or the terminator.
    Apc(Vec<u8>),
}

/// Pulls `ESC _ ... ST` payloads out of a byte stream, including across
/// `feed` calls. VT bytes and APC payloads are emitted in arrival order so a
/// picture lands at the cursor the text before it left behind.
#[derive(Debug, Default)]
pub(crate) struct ApcSplitter {
    in_apc: bool,
    held_esc: bool,
    buf: Vec<u8>,
    overflow: bool,
}

impl ApcSplitter {
    /// Split `input` into ordered VT bytes and completed APC payloads.
    pub(crate) fn push(&mut self, input: &[u8]) -> Vec<StreamEvent> {
        let mut events = Vec::new();
        let mut vt = Vec::new();
        for &b in input {
            if self.in_apc {
                self.push_apc(b, &mut events);
            } else if self.held_esc {
                self.held_esc = false;
                if b == b'_' {
                    flush_vt(&mut vt, &mut events);
                    self.in_apc = true;
                    self.buf.clear();
                    self.overflow = false;
                } else {
                    vt.push(0x1b);
                    vt.push(b);
                }
            } else if b == 0x1b {
                self.held_esc = true;
            } else {
                vt.push(b);
            }
        }
        flush_vt(&mut vt, &mut events);
        events
    }

    fn push_apc(&mut self, b: u8, events: &mut Vec<StreamEvent>) {
        if self.held_esc {
            self.held_esc = false;
            if b == b'\\' {
                self.finish(events);
                return;
            }
            self.push_byte(0x1b);
            self.push_byte(b);
            return;
        }
        if b == 0x07 {
            self.finish(events);
            return;
        }
        if b == 0x1b {
            self.held_esc = true;
            return;
        }
        self.push_byte(b);
    }

    fn push_byte(&mut self, b: u8) {
        if self.buf.len() >= APC_CAP {
            self.overflow = true;
            return;
        }
        self.buf.push(b);
    }

    fn finish(&mut self, events: &mut Vec<StreamEvent>) {
        self.in_apc = false;
        self.held_esc = false;
        let buf = std::mem::take(&mut self.buf);
        if !self.overflow {
            events.push(StreamEvent::Apc(buf));
        }
        self.overflow = false;
    }
}

fn flush_vt(vt: &mut Vec<u8>, events: &mut Vec<StreamEvent>) {
    if !vt.is_empty() {
        events.push(StreamEvent::Vt(std::mem::take(vt)));
    }
}

/// Parsed Kitty control fields (the comma-separated keys before `;`).
#[derive(Debug, Clone)]
pub(crate) struct KittyCommand {
    /// `a=` action byte (`t`, `T`, `p`, `d`, …).
    pub action: u8,
    /// `f=` pixel format. 24 = RGB, 32 = RGBA, 100 = PNG.
    pub format: u32,
    /// `s=` width in pixels. Zero when the format carries its own size.
    pub width: u32,
    /// `v=` height in pixels.
    pub height: u32,
    /// `m=1` means more chunks follow.
    pub more: bool,
    /// `i=` image id.
    pub id: u32,
    /// `C=1` moves the cursor past the image.
    pub move_cursor: bool,
    /// True when the payload is not in the supported subset (zlib).
    pub skip: bool,
}

/// Parse a Kitty graphics control header. Missing `a` defaults to transmit-only
/// (`t`), matching the protocol.
pub(crate) fn parse_kitty_command(header: &str) -> KittyCommand {
    let mut cmd = KittyCommand {
        action: b't',
        format: 32,
        width: 0,
        height: 0,
        more: false,
        id: 0,
        move_cursor: false,
        skip: false,
    };
    for part in header.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let mut kv = part.splitn(2, '=');
        let key = kv.next().unwrap_or("");
        let val = kv.next().unwrap_or("");
        match key {
            "a" => cmd.action = val.bytes().next().unwrap_or(b't'),
            "f" => cmd.format = val.parse().unwrap_or(32),
            "s" => cmd.width = val.parse().unwrap_or(0),
            "v" => cmd.height = val.parse().unwrap_or(0),
            "m" => cmd.more = val == "1",
            "i" => cmd.id = val.parse().unwrap_or(0),
            "C" => cmd.move_cursor = val == "1",
            "o" if val == "z" => cmd.skip = true,
            _ => {}
        }
    }
    cmd
}

/// Decode a finished Kitty payload into RGBA8.
pub(crate) fn decode_kitty_bytes(
    format: u32,
    width: u32,
    height: u32,
    data: &[u8],
) -> Option<(u32, u32, Vec<u8>)> {
    match format {
        24 => rgb_to_rgba(width, height, data, 3),
        32 => rgb_to_rgba(width, height, data, 4),
        100 => decode_png(data),
        _ => None,
    }
}

fn rgb_to_rgba(width: u32, height: u32, data: &[u8], stride: usize) -> Option<(u32, u32, Vec<u8>)> {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 || w > MAX_DIM || h > MAX_DIM || w.saturating_mul(h) > MAX_PIXELS {
        return None;
    }
    let need = w * h * stride;
    if data.len() < need {
        return None;
    }
    let mut rgba = Vec::with_capacity(w * h * 4);
    if stride == 4 {
        rgba.extend_from_slice(&data[..need]);
    } else {
        for px in data[..need].as_chunks::<3>().0 {
            rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }
    }
    Some((width, height, rgba))
}

fn decode_png(data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let img = image::load_from_memory(data).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 || w as usize > MAX_DIM || h as usize > MAX_DIM {
        return None;
    }
    if (w as usize).saturating_mul(h as usize) > MAX_PIXELS {
        return None;
    }
    Some((w, h, rgba.into_raw()))
}

/// Decode standard base64, ignoring ASCII whitespace. Padding is optional.
pub(crate) fn decode_base64(input: &[u8]) -> Vec<u8> {
    const TABLE: [i8; 256] = {
        let mut t = [-1i8; 256];
        let mut i = 0u8;
        while i < 26 {
            t[(b'A' + i) as usize] = i as i8;
            t[(b'a' + i) as usize] = (i + 26) as i8;
            i += 1;
        }
        let mut d = 0u8;
        while d < 10 {
            t[(b'0' + d) as usize] = (d + 52) as i8;
            d += 1;
        }
        t[b'+' as usize] = 62;
        t[b'/' as usize] = 63;
        t
    };
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &ch in input {
        if ch.is_ascii_whitespace() || ch == b'=' {
            continue;
        }
        let v = TABLE[ch as usize];
        if v < 0 {
            continue;
        }
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    out
}

/// Decode a sixel DCS body (bytes after the `q` action, before ST) into RGBA8.
/// Unplotted pixels are transparent so the terminal background shows through.
pub(crate) fn decode_sixel(data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut palette = [[255u8; 3]; 256];
    palette[0] = [0, 0, 0];
    let mut pixels: Vec<u8> = Vec::new();
    let mut w = 0usize;
    let mut h = 0usize;
    let mut x = 0usize;
    let mut y = 0usize;
    let mut color = 0usize;
    let mut i = 0usize;
    while i < data.len() {
        let b = data[i];
        i += 1;
        match b {
            b'"' => {
                let (nums, ni) = read_numbers(data, i);
                i = ni;
                if nums.len() >= 4 && w == 0 && h == 0 {
                    let pw = nums[2].min(MAX_DIM);
                    let ph = nums[3].min(MAX_DIM);
                    if pw > 0 && ph > 0 && pw.saturating_mul(ph) <= MAX_PIXELS {
                        w = pw;
                        h = ph;
                        pixels = vec![0u8; w * h * 4];
                    }
                }
            }
            b'#' => {
                let (nums, ni) = read_numbers(data, i);
                i = ni;
                if let Some(&id) = nums.first() {
                    color = id.min(255);
                    if nums.len() >= 5 && nums[1] == 2 {
                        palette[color] =
                            [scale_100(nums[2]), scale_100(nums[3]), scale_100(nums[4])];
                    }
                }
            }
            b'!' => {
                let (nums, ni) = read_numbers(data, i);
                i = ni;
                let repeat = nums.first().copied().unwrap_or(1).min(MAX_DIM);
                if i >= data.len() {
                    break;
                }
                let ch = data[i];
                i += 1;
                if is_sixel(ch) {
                    for _ in 0..repeat {
                        if !plot(&mut pixels, &mut w, &mut h, x, y, ch, palette[color]) {
                            return None;
                        }
                        x += 1;
                    }
                }
            }
            b'$' => x = 0,
            b'-' => {
                x = 0;
                y += 6;
            }
            ch if is_sixel(ch) => {
                if !plot(&mut pixels, &mut w, &mut h, x, y, ch, palette[color]) {
                    return None;
                }
                x += 1;
            }
            _ => {}
        }
    }
    if w == 0 || h == 0 {
        return None;
    }
    Some((w as u32, h as u32, pixels))
}

fn is_sixel(ch: u8) -> bool {
    (0x3f..=0x7e).contains(&ch)
}

fn scale_100(v: usize) -> u8 {
    (v.min(100) * 255 / 100) as u8
}

fn read_numbers(data: &[u8], mut i: usize) -> (Vec<usize>, usize) {
    let mut nums = Vec::new();
    if i >= data.len() || !(data[i].is_ascii_digit() || data[i] == b';') {
        return (nums, i);
    }
    loop {
        let mut n = 0usize;
        let mut any = false;
        while i < data.len() && data[i].is_ascii_digit() {
            any = true;
            n = n
                .saturating_mul(10)
                .saturating_add((data[i] - b'0') as usize);
            i += 1;
        }
        if any {
            nums.push(n);
        }
        if i < data.len() && data[i] == b';' {
            i += 1;
            continue;
        }
        break;
    }
    (nums, i)
}

fn plot(
    pixels: &mut Vec<u8>,
    w: &mut usize,
    h: &mut usize,
    x: usize,
    y: usize,
    sixel: u8,
    rgb: [u8; 3],
) -> bool {
    let bits = sixel - 0x3f;
    for bit in 0..6 {
        if bits & (1 << bit) == 0 {
            continue;
        }
        let py = y + bit;
        if !set_px(pixels, w, h, x, py, rgb) {
            return false;
        }
    }
    true
}

fn set_px(
    pixels: &mut Vec<u8>,
    w: &mut usize,
    h: &mut usize,
    x: usize,
    y: usize,
    rgb: [u8; 3],
) -> bool {
    if x >= MAX_DIM || y >= MAX_DIM {
        return false;
    }
    let need_w = x + 1;
    let need_h = y + 1;
    if need_w > *w || need_h > *h {
        let nw = (*w).max(need_w);
        let nh = (*h).max(need_h);
        if nw.saturating_mul(nh) > MAX_PIXELS {
            return false;
        }
        let mut next = vec![0u8; nw * nh * 4];
        for row in 0..*h {
            let src = row * *w * 4;
            let dst = row * nw * 4;
            next[dst..dst + *w * 4].copy_from_slice(&pixels[src..src + *w * 4]);
        }
        *pixels = next;
        *w = nw;
        *h = nh;
    }
    let i = (y * *w + x) * 4;
    pixels[i] = rgb[0];
    pixels[i + 1] = rgb[1];
    pixels[i + 2] = rgb[2];
    pixels[i + 3] = 255;
    true
}

/// Encode a 1×1 PNG. Used by tests; kept here so the decoder test does not
/// depend on UI crates.
#[cfg(test)]
fn encode_png_pixel(rgba: [u8; 4]) -> Vec<u8> {
    use image::ImageEncoder;
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(std::io::Cursor::new(&mut bytes))
        .write_image(&rgba, 1, 1, image::ExtendedColorType::Rgba8)
        .expect("png");
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apc_splitter_keeps_text_and_payload() {
        let mut s = ApcSplitter::default();
        let events = s.push(b"ab\x1b_Ghello\x1b\\cd");
        assert!(matches!(events[0], StreamEvent::Vt(ref b) if b == b"ab"));
        assert!(matches!(events[1], StreamEvent::Apc(ref b) if b == b"Ghello"));
        assert!(matches!(events[2], StreamEvent::Vt(ref b) if b == b"cd"));
    }

    #[test]
    fn apc_splitter_spans_chunks() {
        let mut s = ApcSplitter::default();
        let events = s.push(b"Z\x1b");
        assert!(matches!(events[0], StreamEvent::Vt(ref b) if b == b"Z"));
        let events = s.push(b"_Ghi\x1b\\!");
        assert!(matches!(events[0], StreamEvent::Apc(ref b) if b == b"Ghi"));
        assert!(matches!(events[1], StreamEvent::Vt(ref b) if b == b"!"));
    }

    #[test]
    fn sixel_red_pixel() {
        let (w, h, rgba) = decode_sixel(b"#1;2;100;0;0@").expect("sixel");
        assert_eq!((w, h), (1, 1));
        assert_eq!(&rgba[..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn kitty_rgba_and_png() {
        let raw = decode_base64(b"/wAA/w==");
        let (w, h, rgba) = decode_kitty_bytes(32, 1, 1, &raw).expect("rgba");
        assert_eq!((w, h), (1, 1));
        assert_eq!(rgba, vec![255, 0, 0, 255]);

        let png = encode_png_pixel([0, 255, 0, 255]);
        let (w, h, rgba) = decode_kitty_bytes(100, 0, 0, &png).expect("png");
        assert_eq!((w, h), (1, 1));
        assert_eq!(&rgba[..4], &[0, 255, 0, 255]);
    }

    #[test]
    fn kitty_zlib_is_skipped() {
        let cmd = parse_kitty_command("a=T,f=32,o=z,s=1,v=1");
        assert!(cmd.skip);
        assert!(!cmd.move_cursor);
    }
}
