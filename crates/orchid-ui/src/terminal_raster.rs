//! Rasterize a [`orchid_widgets::TerminalPayload`] with the same `fontdue::Font` used for
//! [`orchid_terminal::FontMetrics`], then return a Slint `Image` for a single `Image` view
//! (one draw path, no per-cell `Text` / Skia mismatch). When the monospace face has no outline for
//! a code point, an optional `glyph_fallback` (e.g. a system UI / symbol font) is used; if that
//! also misses, we try U+FFFD and finally a small cell-center dot so the cell is not blank.
//!
//! Retained buffers ([`RetainedRaster`]) allow dirty-line patches instead of reallocating and
//! filling the entire bitmap on every PTY update.

use std::collections::HashMap;
use std::sync::Arc;

use fontdue::Font;
use orchid_widgets::TerminalPayload;
use orchid_widgets::TerminalPayloadCell;
use parking_lot::Mutex;
use slint::Image;
use slint::Rgba8Pixel;
use slint::SharedPixelBuffer;

type GlyphRaster = Option<(fontdue::Metrics, Arc<[u8]>)>;

/// Cached glyph coverage for `(font identity, char, size_bucket)`.
struct GlyphCache {
    /// Primary font pointer identity (stable for process lifetime of loaded fonts).
    primary_ptr: usize,
    fallback_ptr: usize,
    /// Size quantized to 0.25 px to keep the key space small.
    size_q: u32,
    map: HashMap<(char, bool), GlyphRaster>,
}

impl GlyphCache {
    fn new(primary: &Font, fallback: Option<&Font>, size_draw: f32) -> Self {
        Self {
            primary_ptr: primary as *const Font as usize,
            fallback_ptr: fallback.map(|f| f as *const Font as usize).unwrap_or(0),
            size_q: (size_draw * 4.0).round() as u32,
            map: HashMap::with_capacity(512),
        }
    }

    fn matches(&self, primary: &Font, fallback: Option<&Font>, size_draw: f32) -> bool {
        self.primary_ptr == primary as *const Font as usize
            && self.fallback_ptr == fallback.map(|f| f as *const Font as usize).unwrap_or(0)
            && self.size_q == (size_draw * 4.0).round() as u32
    }
}

fn glyph_cache() -> &'static Mutex<Option<GlyphCache>> {
    static CACHE: Mutex<Option<GlyphCache>> = Mutex::new(None);
    &CACHE
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
fn try_raster_glyph(f: &Font, ch: char, size: f32) -> Option<(fontdue::Metrics, Arc<[u8]>)> {
    let (m, coverage) = f.rasterize(ch, size);
    if !coverage.is_empty() && m.width > 0 && m.height > 0 {
        return Some((m, Arc::from(coverage)));
    }
    None
}

fn best_raster_for_cell(
    primary: &Font,
    glyph_fallback: Option<&Font>,
    ch: char,
    size: f32,
) -> Option<(fontdue::Metrics, Arc<[u8]>)> {
    try_raster_glyph(primary, ch, size)
        .or_else(|| glyph_fallback.and_then(|fb| try_raster_glyph(fb, ch, size)))
        .or_else(|| try_raster_glyph(primary, '\u{FFFD}', size))
        .or_else(|| glyph_fallback.and_then(|fb| try_raster_glyph(fb, '\u{FFFD}', size)))
}

fn cached_raster_for_cell(
    primary: &Font,
    glyph_fallback: Option<&Font>,
    ch: char,
    size: f32,
) -> Option<(fontdue::Metrics, Arc<[u8]>)> {
    let mut guard = glyph_cache().lock();
    let need_new = guard
        .as_ref()
        .is_none_or(|c| !c.matches(primary, glyph_fallback, size));
    if need_new {
        *guard = Some(GlyphCache::new(primary, glyph_fallback, size));
    }
    let cache = guard.as_mut().expect("glyph cache just initialized");
    let key = (ch, false);
    if let Some(hit) = cache.map.get(&key) {
        return hit.as_ref().map(|(m, cov)| (*m, Arc::clone(cov)));
    }
    let rendered = best_raster_for_cell(primary, glyph_fallback, ch, size);
    cache
        .map
        .insert(key, rendered.as_ref().map(|(m, cov)| (*m, Arc::clone(cov))));
    rendered
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

/// Retained RGBA buffer for dirty-line terminal updates.
pub struct RetainedRaster {
    buffer: SharedPixelBuffer<Rgba8Pixel>,
    cols: u16,
    rows: u16,
    cell_wp: u32,
    cell_hp: u32,
    size_q: u32,
    cursor_col: u16,
    cursor_row: u16,
    cursor_visible: bool,
}

impl RetainedRaster {
    fn matches_geometry(
        &self,
        cols: u16,
        rows: u16,
        cell_wp: u32,
        cell_hp: u32,
        size_q: u32,
    ) -> bool {
        self.cols == cols
            && self.rows == rows
            && self.cell_wp == cell_wp
            && self.cell_hp == cell_hp
            && self.size_q == size_q
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_rows(
    buffer: &mut SharedPixelBuffer<Rgba8Pixel>,
    cols: u16,
    rows: u16,
    cells: &[TerminalPayloadCell],
    row_indices: impl Iterator<Item = u16>,
    font: &Font,
    glyph_fallback: Option<&Font>,
    size_draw: f32,
    cell_wp: u32,
    cell_hp: u32,
    tw: u32,
    th: u32,
    ascent: f32,
) {
    let row_list: Vec<u16> = row_indices.filter(|&r| r < rows).collect();
    if row_list.is_empty() {
        return;
    }
    {
        let sbuf = buffer.make_mut_slice();
        for &r in &row_list {
            for c in 0..cols {
                let i = (r * cols + c) as usize;
                let cell: &TerminalPayloadCell = cells.get(i).unwrap_or(&FALLBACK_CELL);
                let b = cell.bg_rgba;
                let px = Rgba8Pixel {
                    r: b[0],
                    g: b[1],
                    b: b[2],
                    a: b[3],
                };
                let cx = c as u32 * cell_wp;
                let cy = r as u32 * cell_hp;
                for yy in 0..cell_hp {
                    for xx in 0..cell_wp {
                        sbuf[((cy + yy) * tw + (cx + xx)) as usize] = px;
                    }
                }
            }
        }
    }
    {
        let p = buffer.make_mut_bytes();
        for &r in &row_list {
            for c in 0..cols {
                let i = (r * cols + c) as usize;
                let cell: &TerminalPayloadCell = cells.get(i).unwrap_or(&FALLBACK_CELL);
                if cell.ch == '\0' || cell.ch == ' ' {
                    continue;
                }
                let fg = cell.fg_rgba;
                if let Some((m, coverage)) =
                    cached_raster_for_cell(font, glyph_fallback, cell.ch, size_draw)
                {
                    let w = m.width;
                    let h = m.height;
                    let b = m.bounds;
                    let cx = c as f32 * cell_wp as f32;
                    let cy = r as f32 * cell_hp as f32;
                    let baseline = cy + ascent;
                    let y_top = baseline - (b.ymin + b.height);
                    let x_left =
                        cx + (cell_wp as f32 - m.advance_width).max(0.0) * 0.5 + m.xmin as f32;
                    for y in 0..h {
                        for x in 0..w {
                            let a = *coverage.get(y * w + x).unwrap_or(&0) as f32 / 255.0;
                            if a <= 0.0 {
                                continue;
                            }
                            let px = (x_left + x as f32).round() as i32;
                            let py = (y_top + y as f32).round() as i32;
                            if px < 0 || py < 0 || (px as u32) >= tw || (py as u32) >= th {
                                continue;
                            }
                            let oi = (py as u32 * tw + px as u32) as usize * 4;
                            if oi + 3 < p.len() {
                                blend_over_rgba(p, oi, fg, a);
                            }
                        }
                    }
                } else {
                    draw_missing_glyphs_marker(p, tw, th, c as u32, r as u32, cell_wp, cell_hp, fg);
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_cursor(
    buffer: &mut SharedPixelBuffer<Rgba8Pixel>,
    cols: u16,
    rows: u16,
    cursor_col: u16,
    cursor_row: u16,
    cursor_visible: bool,
    cell_wp: u32,
    cell_hp: u32,
    tw: u32,
    th: u32,
    cursor_color: [u8; 4],
) {
    if !cursor_visible || (cursor_col as u32) >= cols as u32 || (cursor_row as u32) >= rows as u32 {
        return;
    }
    let cx = cursor_col as f32 * cell_wp as f32;
    let cy = cursor_row as f32 * cell_hp as f32;
    let a = 0.35f32;
    let p2 = buffer.make_mut_bytes();
    for yy in 0..cell_hp {
        for xx in 0..cell_wp {
            let px = cx as u32 + xx;
            let py = cy as u32 + yy;
            if px < tw && py < th {
                let oi = (py * tw + px) as usize * 4;
                if oi + 3 < p2.len() {
                    blend_straight_over(p2, oi, cursor_color, a);
                }
            }
        }
    }
}

/// Raster terminal cells, optionally patching into a retained buffer when only
/// a subset of rows changed (`full_redraw == false` and geometry matches).
#[allow(clippy::too_many_arguments)]
pub fn render_terminal_cells_retained(
    retained: &mut Option<RetainedRaster>,
    cols: u16,
    rows: u16,
    cells: &[TerminalPayloadCell],
    cursor_col: u16,
    cursor_row: u16,
    cursor_visible: bool,
    dirty_lines: &[u16],
    full_redraw: bool,
    font: &Font,
    glyph_fallback: Option<&Font>,
    size_px: f32,
    cell_w: u32,
    cell_h: u32,
    content_scale: f32,
    cursor_color: [u8; 4],
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
    let size_q = (size_draw * 4.0).round() as u32;
    let cell_wp = (cell_w as f32 * s).round().max(1.0) as u32;
    let cell_hp = (cell_h as f32 * s).round().max(1.0) as u32;
    let tw = cols as u32 * cell_wp;
    let th = rows as u32 * cell_hp;
    if tw == 0 || th == 0 {
        return None;
    }
    let line = font.horizontal_line_metrics(size_draw)?;
    let ascent = line.ascent;

    let geometry_ok = retained
        .as_ref()
        .is_some_and(|r| r.matches_geometry(cols, rows, cell_wp, cell_hp, size_q));
    let do_full = full_redraw || !geometry_ok;

    if do_full || retained.is_none() {
        let mut buffer = SharedPixelBuffer::new(tw, th);
        paint_rows(
            &mut buffer,
            cols,
            rows,
            cells,
            0..rows,
            font,
            glyph_fallback,
            size_draw,
            cell_wp,
            cell_hp,
            tw,
            th,
            ascent,
        );
        paint_cursor(
            &mut buffer,
            cols,
            rows,
            cursor_col,
            cursor_row,
            cursor_visible,
            cell_wp,
            cell_hp,
            tw,
            th,
            cursor_color,
        );
        let image = Image::from_rgba8(buffer.clone());
        *retained = Some(RetainedRaster {
            buffer,
            cols,
            rows,
            cell_wp,
            cell_hp,
            size_q,
            cursor_col,
            cursor_row,
            cursor_visible,
        });
        return Some(image);
    }

    let prev = retained.as_ref().expect("checked above");
    let mut dirty: Vec<u16> = dirty_lines.to_vec();
    // Re-paint previous + current cursor rows so the tint is cleared/redrawn.
    if prev.cursor_visible {
        dirty.push(prev.cursor_row);
    }
    if cursor_visible {
        dirty.push(cursor_row);
    }
    dirty.sort_unstable();
    dirty.dedup();

    let rast = retained.as_mut().expect("checked above");
    paint_rows(
        &mut rast.buffer,
        cols,
        rows,
        cells,
        dirty.into_iter(),
        font,
        glyph_fallback,
        size_draw,
        cell_wp,
        cell_hp,
        tw,
        th,
        ascent,
    );
    paint_cursor(
        &mut rast.buffer,
        cols,
        rows,
        cursor_col,
        cursor_row,
        cursor_visible,
        cell_wp,
        cell_hp,
        tw,
        th,
        cursor_color,
    );
    rast.cursor_col = cursor_col;
    rast.cursor_row = cursor_row;
    rast.cursor_visible = cursor_visible;
    Some(Image::from_rgba8(rast.buffer.clone()))
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
        &[],
        true,
        font,
        glyph_fallback,
        size_px,
        cell_w,
        cell_h,
        content_scale,
        cursor_color,
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
