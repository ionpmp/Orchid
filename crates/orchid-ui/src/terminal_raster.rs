//! Rasterize a [`orchid_widgets::TerminalPayload`] with the same `fontdue::Font` used for
//! [`orchid_terminal::FontMetrics`], then return a Slint `Image` for a single `Image` view
//! (one draw path, no per-cell `Text` / Skia mismatch). When the monospace face has no outline for
//! a code point, an optional `glyph_fallback` (e.g. a system UI / symbol font) is used; if that
//! also misses, we try U+FFFD and finally a small cell-center dot so the cell is not blank.
//!
//! Retained buffers ([`RetainedRaster`]) remember which cells each bitmap holds, so an update
//! repaints only the rows whose cells differ instead of reallocating and filling the whole bitmap.

use std::collections::HashMap;

use fontdue::Font;
use orchid_widgets::TerminalImage;
use orchid_widgets::TerminalPayload;
use orchid_widgets::TerminalPayloadCell;
use parking_lot::Mutex;
use slint::Image;
use slint::Rgba8Pixel;
use slint::SharedPixelBuffer;

type GlyphRaster = Option<(fontdue::Metrics, Box<[u8]>)>;

/// Cached glyph coverage for `(font identity, char, size_bucket)`.
struct GlyphCache {
    /// Primary font pointer identity (stable for process lifetime of loaded fonts).
    primary_ptr: usize,
    fallback_ptr: usize,
    /// Size quantized to 0.25 px to keep the key space small.
    size_q: u32,
    map: HashMap<char, GlyphRaster>,
}

impl GlyphCache {
    fn new(primary: &Font, fallback: Option<&Font>, size_draw: f32) -> Self {
        Self {
            primary_ptr: font_id(primary),
            fallback_ptr: fallback.map(font_id).unwrap_or(0),
            size_q: size_bucket(size_draw),
            map: HashMap::with_capacity(512),
        }
    }

    fn matches(&self, primary: &Font, fallback: Option<&Font>, size_draw: f32) -> bool {
        self.primary_ptr == font_id(primary)
            && self.fallback_ptr == fallback.map(font_id).unwrap_or(0)
            && self.size_q == size_bucket(size_draw)
    }

    fn glyph(
        &mut self,
        primary: &Font,
        glyph_fallback: Option<&Font>,
        ch: char,
        size: f32,
    ) -> Option<&(fontdue::Metrics, Box<[u8]>)> {
        self.map
            .entry(ch)
            .or_insert_with(|| best_raster_for_cell(primary, glyph_fallback, ch, size))
            .as_ref()
    }
}

fn font_id(f: &Font) -> usize {
    f as *const Font as usize
}

fn size_bucket(size_draw: f32) -> u32 {
    (size_draw * 4.0).round() as u32
}

fn glyph_cache() -> &'static Mutex<Option<GlyphCache>> {
    static CACHE: Mutex<Option<GlyphCache>> = Mutex::new(None);
    &CACHE
}

/// Alpha-blend `fg` (straight) over the RGBA pixel `dst` with coverage `a` (0..=255).
#[inline]
fn blend_coverage(dst: &mut [u8; 4], fg: [u8; 4], a: u8) {
    let a = u32::from(a);
    let inv = 255 - a;
    for (d, f) in dst.iter_mut().zip(fg) {
        *d = ((u32::from(f) * a + u32::from(*d) * inv + 127) / 255) as u8;
    }
}

/// Alpha-blend `fg` (straight) over `dst` using `alpha` 0.0..=1.0.
fn blend_over_rgba(dst: &mut [u8], i: usize, fg: [u8; 4], alpha: f32) {
    if alpha <= 0.0 {
        return;
    }
    let t = alpha.clamp(0.0, 1.0);
    for c in 0..4 {
        let d = dst[i + c] as f32;
        let f = fg[c] as f32;
        dst[i + c] = (f * t + d * (1.0 - t)) as u8;
    }
}

/// Blend straight-RGBA `layer` with alpha `a` over `dst` (for cursor tint).
fn blend_straight_over(dst: &mut [u8], i: usize, layer: [u8; 4], a: f32) {
    let t = a.clamp(0.0, 1.0) * (layer[3] as f32 / 255.0);
    if t <= 0.0 {
        return;
    }
    for c in 0..3 {
        let d = dst[i + c] as f32;
        let f = layer[c] as f32;
        dst[i + c] = (f * t + d * (1.0 - t)) as u8;
    }
    let d = dst[i + 3] as f32;
    dst[i + 3] = (t * 255.0 + d * (1.0 - t)) as u8;
}

/// `font.rasterize` with no coverage (missing glyph) returns an empty mask; treat as missing.
fn try_raster_glyph(f: &Font, ch: char, size: f32) -> GlyphRaster {
    let (m, coverage) = f.rasterize(ch, size);
    if !coverage.is_empty() && m.width > 0 && m.height > 0 {
        return Some((m, coverage.into_boxed_slice()));
    }
    None
}

fn best_raster_for_cell(
    primary: &Font,
    glyph_fallback: Option<&Font>,
    ch: char,
    size: f32,
) -> GlyphRaster {
    try_raster_glyph(primary, ch, size)
        .or_else(|| glyph_fallback.and_then(|fb| try_raster_glyph(fb, ch, size)))
        .or_else(|| try_raster_glyph(primary, '\u{FFFD}', size))
        .or_else(|| glyph_fallback.and_then(|fb| try_raster_glyph(fb, '\u{FFFD}', size)))
}

/// 2×2–3×3 block in the cell so undefined points are visible even with no TTF.
#[allow(clippy::too_many_arguments)]
fn draw_missing_glyphs_marker(
    p: &mut [u8],
    tw: u32,
    th: u32,
    col: u32,
    row: u32,
    cell_w: u32,
    cell_h: u32,
    fg: [u8; 4],
) {
    let x0 = col * cell_w + (cell_w.saturating_sub(3)) / 2;
    let y0 = row * cell_h + (cell_h.saturating_sub(3)) / 2;
    for dy in 0..2u32 {
        for dx in 0..2u32 {
            let px = x0 + dx;
            let py = y0 + dy;
            if px >= tw || py >= th {
                continue;
            }
            let oi = (py * tw + px) as usize * 4;
            if oi + 3 < p.len() {
                blend_over_rgba(p, oi, fg, 0.7);
            }
        }
    }
}

/// One bitmap plus the cells and cursor it currently shows.
struct PaintedBuffer {
    pixels: SharedPixelBuffer<Rgba8Pixel>,
    /// Row-major copy of the cells last painted into `pixels`.
    cells: Vec<TerminalPayloadCell>,
    /// Cursor cell tinted into `pixels`, if any.
    cursor: Option<(u16, u16)>,
    /// Images last blitted into `pixels`.
    image_stamps: Vec<ImageStamp>,
}

/// Retained RGBA buffers for incremental terminal updates.
///
/// Two independent pixel buffers ping-pong so `Image::from_rgba8` never
/// shares the buffer we paint next. A cheap `clone()` of `SharedPixelBuffer`
/// only bumps a refcount; the next `make_mut_slice` would otherwise detach
/// and copy the whole frame (~3.5 MiB at 120×40×2×).
///
/// Each buffer is diffed against the incoming cells rather than trusting the
/// emulator's dirty-line list: that list is drained per snapshot, so rows are
/// lost whenever two snapshots land between rasters.
pub struct RetainedRaster {
    buffers: [PaintedBuffer; 2],
    /// Index last handed to Slint as an `Image`.
    front: usize,
    cols: u16,
    rows: u16,
    cell_wp: u32,
    cell_hp: u32,
    size_q: u32,
    font: usize,
    fallback: usize,
    cursor_color: [u8; 4],
}

type ImageStamp = (u16, i32, u32, u32, usize);

impl RetainedRaster {
    #[allow(clippy::too_many_arguments)]
    fn matches(
        &self,
        cols: u16,
        rows: u16,
        cell_wp: u32,
        cell_hp: u32,
        size_q: u32,
        font: usize,
        fallback: usize,
        cursor_color: [u8; 4],
    ) -> bool {
        self.cols == cols
            && self.rows == rows
            && self.cell_wp == cell_wp
            && self.cell_hp == cell_hp
            && self.size_q == size_q
            && self.font == font
            && self.fallback == fallback
            && self.cursor_color == cursor_color
    }
}

/// Geometry shared by every paint call for one raster.
struct PaintCtx<'a> {
    cols: u16,
    rows: u16,
    font: &'a Font,
    glyph_fallback: Option<&'a Font>,
    size_draw: f32,
    cell_wp: u32,
    cell_hp: u32,
    tw: u32,
    th: u32,
    ascent: f32,
}

fn paint_rows(
    buffer: &mut SharedPixelBuffer<Rgba8Pixel>,
    ctx: &PaintCtx<'_>,
    cells: &[TerminalPayloadCell],
    row_list: &[u16],
) {
    let PaintCtx {
        cols,
        rows,
        cell_wp,
        cell_hp,
        tw,
        th,
        ..
    } = *ctx;
    if row_list.is_empty() {
        return;
    }
    {
        let sbuf = buffer.make_mut_slice();
        for &r in row_list.iter().filter(|&&r| r < rows) {
            for c in 0..cols {
                let i = (r as usize) * (cols as usize) + c as usize;
                let b = cells.get(i).unwrap_or(&FALLBACK_CELL).bg_rgba;
                let px = Rgba8Pixel {
                    r: b[0],
                    g: b[1],
                    b: b[2],
                    a: b[3],
                };
                let cx = c as u32 * cell_wp;
                let cy = r as u32 * cell_hp;
                for yy in 0..cell_hp {
                    let start = ((cy + yy) * tw + cx) as usize;
                    sbuf[start..start + cell_wp as usize].fill(px);
                }
            }
        }
    }
    let mut glyphs = glyph_cache().lock();
    if glyphs
        .as_ref()
        .is_none_or(|g| !g.matches(ctx.font, ctx.glyph_fallback, ctx.size_draw))
    {
        *glyphs = Some(GlyphCache::new(ctx.font, ctx.glyph_fallback, ctx.size_draw));
    }
    let glyphs = glyphs.as_mut().expect("glyph cache just initialized");
    let p = buffer.make_mut_bytes();
    let row_stride = tw as usize * 4;
    for &r in row_list.iter().filter(|&&r| r < rows) {
        for c in 0..cols {
            let i = (r as usize) * (cols as usize) + c as usize;
            let cell = cells.get(i).unwrap_or(&FALLBACK_CELL);
            if cell.ch == '\0' || cell.ch == ' ' {
                continue;
            }
            let fg = cell.fg_rgba;
            let Some((m, coverage)) =
                glyphs.glyph(ctx.font, ctx.glyph_fallback, cell.ch, ctx.size_draw)
            else {
                draw_missing_glyphs_marker(p, tw, th, c as u32, r as u32, cell_wp, cell_hp, fg);
                continue;
            };
            let w = m.width;
            let bounds = m.bounds;
            let cx = c as f32 * cell_wp as f32;
            let cy = r as f32 * cell_hp as f32;
            let baseline = cy + ctx.ascent;
            let y_top = baseline - (bounds.ymin + bounds.height);
            let x_left = cx + (cell_wp as f32 - m.advance_width).max(0.0) * 0.5 + m.xmin as f32;
            let ox = x_left.round() as i64;
            let oy = y_top.round() as i64;
            let x0 = (-ox).clamp(0, w as i64) as usize;
            let x1 = (i64::from(tw) - ox).clamp(0, w as i64) as usize;
            let y0 = (-oy).clamp(0, m.height as i64) as usize;
            let y1 = (i64::from(th) - oy).clamp(0, m.height as i64) as usize;
            if x0 >= x1 {
                continue;
            }
            for y in y0..y1 {
                let src = &coverage[y * w + x0..y * w + x1];
                let dst_start =
                    (oy + y as i64) as usize * row_stride + (ox + x0 as i64) as usize * 4;
                let dst = &mut p[dst_start..dst_start + src.len() * 4];
                for (px, &a) in dst.as_chunks_mut::<4>().0.iter_mut().zip(src) {
                    if a != 0 {
                        blend_coverage(px, fg, a);
                    }
                }
            }
        }
    }
}

fn paint_cursor(
    buffer: &mut SharedPixelBuffer<Rgba8Pixel>,
    ctx: &PaintCtx<'_>,
    cursor: (u16, u16),
    cursor_color: [u8; 4],
) {
    let PaintCtx {
        cell_wp,
        cell_hp,
        tw,
        th,
        ..
    } = *ctx;
    let (cursor_col, cursor_row) = cursor;
    let cx = cursor_col as u32 * cell_wp;
    let cy = cursor_row as u32 * cell_hp;
    let a = 0.35f32;
    let p2 = buffer.make_mut_bytes();
    for yy in 0..cell_hp {
        for xx in 0..cell_wp {
            let px = cx + xx;
            let py = cy + yy;
            if px < tw && py < th {
                let oi = (py * tw + px) as usize * 4;
                if oi + 3 < p2.len() {
                    blend_straight_over(p2, oi, cursor_color, a);
                }
            }
        }
    }
}

/// Cells padded / truncated to exactly `cols * rows`, as painted.
fn normalized_cells(cells: &[TerminalPayloadCell], len: usize) -> Vec<TerminalPayloadCell> {
    let mut out = Vec::with_capacity(len);
    out.extend_from_slice(&cells[..cells.len().min(len)]);
    out.resize(len, FALLBACK_CELL);
    out
}

/// Raster terminal cells, patching into a retained buffer when geometry matches:
/// only rows whose cells differ from what that buffer shows are repainted.
#[allow(clippy::too_many_arguments)]
pub fn render_terminal_cells_retained(
    retained: &mut Option<RetainedRaster>,
    cols: u16,
    rows: u16,
    cells: &[TerminalPayloadCell],
    cursor_col: u16,
    cursor_row: u16,
    cursor_visible: bool,
    font: &Font,
    glyph_fallback: Option<&Font>,
    size_px: f32,
    cell_w: u32,
    cell_h: u32,
    content_scale: f32,
    cursor_color: [u8; 4],
    images: &[TerminalImage],
) -> Option<Image> {
    if cols == 0 || rows == 0 {
        *retained = None;
        return Some(Image::default());
    }
    let s = if content_scale.is_finite() && content_scale > 0.0 {
        content_scale.clamp(1.0, 4.0)
    } else {
        1.0
    };
    let size_draw = size_px * s;
    let size_q = size_bucket(size_draw);
    let cell_wp = (cell_w as f32 * s).round().max(1.0) as u32;
    let cell_hp = (cell_h as f32 * s).round().max(1.0) as u32;
    let tw = cols as u32 * cell_wp;
    let th = rows as u32 * cell_hp;
    if tw == 0 || th == 0 {
        return None;
    }
    let line = font.horizontal_line_metrics(size_draw)?;
    let ctx = PaintCtx {
        cols,
        rows,
        font,
        glyph_fallback,
        size_draw,
        cell_wp,
        cell_hp,
        tw,
        th,
        ascent: line.ascent,
    };
    let font_ptr = font_id(font);
    let fallback_ptr = glyph_fallback.map(font_id).unwrap_or(0);
    let cursor = (cursor_visible && cursor_col < cols && cursor_row < rows)
        .then_some((cursor_col, cursor_row));
    let n_cells = cols as usize * rows as usize;

    let reusable = retained.as_ref().is_some_and(|r| {
        r.matches(
            cols,
            rows,
            cell_wp,
            cell_hp,
            size_q,
            font_ptr,
            fallback_ptr,
            cursor_color,
        )
    });

    if !reusable {
        let all_rows: Vec<u16> = (0..rows).collect();
        let mut a = SharedPixelBuffer::new(tw, th);
        paint_rows(&mut a, &ctx, cells, &all_rows);
        if let Some(cur) = cursor {
            paint_cursor(&mut a, &ctx, cur, cursor_color);
        }
        blit_images(&mut a, cell_wp, cell_hp, s, images);
        let b = SharedPixelBuffer::clone_from_slice(a.as_bytes(), tw, th);
        let painted = normalized_cells(cells, n_cells);
        let image = Image::from_rgba8(a.clone());
        *retained = Some(RetainedRaster {
            buffers: [
                PaintedBuffer {
                    pixels: a,
                    cells: painted.clone(),
                    cursor,
                    image_stamps: image_stamps(images),
                },
                PaintedBuffer {
                    pixels: b,
                    cells: painted,
                    cursor,
                    image_stamps: image_stamps(images),
                },
            ],
            front: 0,
            cols,
            rows,
            cell_wp,
            cell_hp,
            size_q,
            font: font_ptr,
            fallback: fallback_ptr,
            cursor_color,
        });
        return Some(image);
    }

    let rast = retained.as_mut().expect("checked above");
    let stamps = image_stamps(images);
    let back = 1 - rast.front;
    let buf = &mut rast.buffers[back];
    let images_changed = buf.image_stamps != stamps;
    let cols_us = cols as usize;
    let mut paint: Vec<u16> = if images_changed {
        (0..rows).collect()
    } else {
        (0..rows)
            .filter(|&r| {
                let start = r as usize * cols_us;
                let end = start + cols_us;
                cells
                    .get(start..end)
                    .is_none_or(|src| src != &buf.cells[start..end])
            })
            .collect()
    };
    // Repaint old + new cursor rows so the tint is cleared / redrawn.
    if buf.cursor != cursor {
        paint.extend(buf.cursor.map(|(_, r)| r));
        paint.extend(cursor.map(|(_, r)| r));
        paint.sort_unstable();
        paint.dedup();
    }

    if !paint.is_empty() {
        paint_rows(&mut buf.pixels, &ctx, cells, &paint);
        if let Some(cur) = cursor.filter(|(_, r)| paint.binary_search(r).is_ok()) {
            paint_cursor(&mut buf.pixels, &ctx, cur, cursor_color);
        }
        blit_images(&mut buf.pixels, cell_wp, cell_hp, s, images);
        buf.image_stamps = stamps;
        for &r in &paint {
            let start = r as usize * cols_us;
            let end = start + cols_us;
            let dst = &mut buf.cells[start..end];
            match cells.get(start..end) {
                Some(src) => dst.clone_from_slice(src),
                None => {
                    for (i, d) in dst.iter_mut().enumerate() {
                        *d = cells.get(start + i).unwrap_or(&FALLBACK_CELL).clone();
                    }
                }
            }
        }
    }
    buf.cursor = cursor;
    let image = Image::from_rgba8(buf.pixels.clone());
    rast.front = back;
    Some(image)
}

fn image_stamps(images: &[TerminalImage]) -> Vec<ImageStamp> {
    images
        .iter()
        .map(|img| {
            (
                img.col,
                img.row,
                img.width,
                img.height,
                std::sync::Arc::as_ptr(&img.rgba) as usize,
            )
        })
        .collect()
}

fn blit_images(
    buffer: &mut SharedPixelBuffer<Rgba8Pixel>,
    cell_wp: u32,
    cell_hp: u32,
    scale: f32,
    images: &[TerminalImage],
) {
    if images.is_empty() {
        return;
    }
    let tw = buffer.width() as i32;
    let th = buffer.height() as i32;
    if tw <= 0 || th <= 0 {
        return;
    }
    let stride = tw as usize * 4;
    let bytes = buffer.make_mut_bytes();
    for img in images {
        if img.width == 0 || img.height == 0 {
            continue;
        }
        let need = img.width as usize * img.height as usize * 4;
        if img.rgba.len() < need {
            continue;
        }
        let dw = ((img.width as f32) * scale).round().max(1.0) as i32;
        let dh = ((img.height as f32) * scale).round().max(1.0) as i32;
        let x0 = i32::from(img.col) * cell_wp as i32;
        let y0 = img.row * cell_hp as i32;
        for dy in 0..dh {
            let dst_y = y0 + dy;
            if dst_y < 0 || dst_y >= th {
                continue;
            }
            let sy = (dy as u64 * u64::from(img.height) / dh as u64) as u32;
            for dx in 0..dw {
                let dst_x = x0 + dx;
                if dst_x < 0 || dst_x >= tw {
                    continue;
                }
                let sx = (dx as u64 * u64::from(img.width) / dw as u64) as u32;
                let si = (sy * img.width + sx) as usize * 4;
                let a = img.rgba[si + 3] as u32;
                if a == 0 {
                    continue;
                }
                let di = dst_y as usize * stride + dst_x as usize * 4;
                if a == 255 {
                    bytes[di..di + 4].copy_from_slice(&img.rgba[si..si + 4]);
                } else {
                    for c in 0..3 {
                        let dst = bytes[di + c] as u32;
                        bytes[di + c] =
                            ((u32::from(img.rgba[si + c]) * a + dst * (255 - a)) / 255) as u8;
                    }
                    bytes[di + 3] = 255;
                }
            }
        }
    }
}

/// Raster terminal cells to an RGBA image in **physical** pixels (full redraw).
#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
pub fn render_terminal_cells(
    cols: u16,
    rows: u16,
    cells: &[TerminalPayloadCell],
    cursor_col: u16,
    cursor_row: u16,
    cursor_visible: bool,
    font: &Font,
    glyph_fallback: Option<&Font>,
    size_px: f32,
    cell_w: u32,
    cell_h: u32,
    content_scale: f32,
    cursor_color: [u8; 4],
    images: &[TerminalImage],
) -> Option<Image> {
    let mut retained = None;
    render_terminal_cells_retained(
        &mut retained,
        cols,
        rows,
        cells,
        cursor_col,
        cursor_row,
        cursor_visible,
        font,
        glyph_fallback,
        size_px,
        cell_w,
        cell_h,
        content_scale,
        cursor_color,
        images,
    )
}

/// Convenience wrapper around [`render_terminal_cells`] for a full [`TerminalPayload`].
#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
pub fn render_terminal(
    t: &TerminalPayload,
    font: &Font,
    glyph_fallback: Option<&Font>,
    size_px: f32,
    cell_w: u32,
    cell_h: u32,
    content_scale: f32,
    cursor_color: [u8; 4],
) -> Option<Image> {
    render_terminal_cells(
        t.cols,
        t.rows,
        &t.cells,
        t.cursor_col,
        t.cursor_row,
        t.cursor_visible,
        font,
        glyph_fallback,
        size_px,
        cell_w,
        cell_h,
        content_scale,
        cursor_color,
        &t.images,
    )
}

const FALLBACK_CELL: TerminalPayloadCell = TerminalPayloadCell {
    ch: ' ',
    fg_rgba: [0xE6, 0xEB, 0xF0, 0xFF],
    bg_rgba: [0x12, 0x14, 0x18, 0xFF],
    bold: false,
    italic: false,
    underline: false,
};

#[cfg(test)]
mod tests {
    use super::*;

    const COLS: u16 = 24;
    const ROWS: u16 = 6;
    const CURSOR: [u8; 4] = [0x40, 0x90, 0xFF, 0xFF];

    fn system_mono_font() -> Option<Font> {
        [
            "C:\\Windows\\Fonts\\consola.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "/System/Library/Fonts/Menlo.ttc",
        ]
        .iter()
        .find_map(|p| std::fs::read(p).ok())
        .and_then(|bytes| Font::from_bytes(bytes, fontdue::FontSettings::default()).ok())
    }

    fn screen(lines: &[&str]) -> Vec<TerminalPayloadCell> {
        let mut cells = vec![FALLBACK_CELL; COLS as usize * ROWS as usize];
        for (r, line) in lines.iter().enumerate() {
            for (c, ch) in line.chars().take(COLS as usize).enumerate() {
                let cell = &mut cells[r * COLS as usize + c];
                cell.ch = ch;
                if ch.is_ascii_digit() {
                    cell.fg_rgba = [0xFF, 0x80, 0x40, 0xFF];
                    cell.bg_rgba = [0x20, 0x30, 0x40, 0xFF];
                }
            }
        }
        cells
    }

    fn render(
        retained: &mut Option<RetainedRaster>,
        font: &Font,
        cells: &[TerminalPayloadCell],
        cursor: (u16, u16, bool),
    ) -> Vec<u8> {
        render_terminal_cells_retained(
            retained,
            COLS,
            ROWS,
            cells,
            cursor.0,
            cursor.1,
            cursor.2,
            font,
            None,
            14.0,
            8,
            16,
            1.25,
            CURSOR,
            &[],
        )
        .and_then(|img| img.to_rgba8())
        .expect("raster")
        .as_bytes()
        .to_vec()
    }

    #[test]
    fn incremental_updates_match_full_redraw() {
        let Some(font) = system_mono_font() else {
            return;
        };
        let frames = [
            (screen(&["$ ls", "a b c"]), (4, 0, true)),
            (screen(&["$ ls", "a b c", "$ echo 42"]), (9, 2, true)),
            (screen(&["$ ls", "a b c", "$ echo 42", "42"]), (0, 4, false)),
            (screen(&["x", "", "$ echo 42", "42", "$ _"]), (2, 4, true)),
            (screen(&["x", "", "", "", "", "tail 7"]), (2, 4, true)),
        ];
        let mut retained = None;
        for (cells, cursor) in &frames {
            let incremental = render(&mut retained, &font, cells, *cursor);
            let full = render(&mut None, &font, cells, *cursor);
            assert!(incremental == full, "incremental raster diverged");
        }
    }

    #[test]
    fn skipped_frames_and_shared_key_stay_correct() {
        let Some(font) = system_mono_font() else {
            return;
        };
        let a = screen(&["terminal a", "1111"]);
        let b = screen(&["terminal b", "", "2222 2222"]);
        let mut retained = None;
        render(&mut retained, &font, &a, (0, 0, true));
        // Two terminals sharing one retained slot, and frames whose
        // intermediate states were never rastered.
        for (cells, cursor) in [(&b, (3, 2, true)), (&a, (1, 1, true)), (&b, (3, 2, false))] {
            let incremental = render(&mut retained, &font, cells, cursor);
            let full = render(&mut None, &font, cells, cursor);
            assert!(incremental == full, "incremental raster diverged");
        }
    }
}
