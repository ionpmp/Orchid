//! Rasterize a [`orchid_widgets::TerminalPayload`] with the same `fontdue::Font` used for
//! [`orchid_terminal::FontMetrics`], then return a Slint `Image` for a single `Image` view
//! (one draw path, no per-cell `Text` / Skia mismatch).

use fontdue::Font;
use orchid_widgets::TerminalPayload;
use orchid_widgets::TerminalPayloadCell;
use slint::Image;
use slint::SharedPixelBuffer;
use slint::Rgba8Pixel;

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

/// Raster the terminal to an RGBA image of size `cols * cell_w` by `rows * cell_h` pixels.
/// `size_px` is `typography.size_md` for the loaded face. `cursor_color` is straight `[r, g, b, a]`
/// like `TerminalPayloadCell` colours.
pub fn render_terminal(
    t: &TerminalPayload,
    font: &Font,
    size_px: f32,
    cell_w: u32,
    cell_h: u32,
    cursor_color: [u8; 4],
) -> Option<Image> {
    if t.cols == 0 || t.rows == 0 {
        return Some(Image::default());
    }
    let tw = t.cols as u32 * cell_w;
    let th = t.rows as u32 * cell_h;
    if tw == 0 || th == 0 {
        return None;
    }
    let line = font.horizontal_line_metrics(size_px)?;

    let mut buffer: SharedPixelBuffer<Rgba8Pixel> = SharedPixelBuffer::new(tw, th);
    {
        let s = buffer.make_mut_slice();
        for r in 0..t.rows {
            for c in 0..t.cols {
                let i = (r * t.cols + c) as usize;
                let cell: &TerminalPayloadCell = t.cells.get(i).unwrap_or(&FALLBACK_CELL);
                let b = cell.bg_rgba;
                let px: Rgba8Pixel = Rgba8Pixel {
                    r: b[0],
                    g: b[1],
                    b: b[2],
                    a: b[3],
                };
                let cx = c as u32 * cell_w;
                let cy = r as u32 * cell_h;
                for yy in 0..cell_h {
                    for xx in 0..cell_w {
                        s[((cy + yy) * tw + (cx + xx)) as usize] = px;
                    }
                }
            }
        }
    }

    {
        let p = buffer.make_mut_bytes();
        for r in 0..t.rows {
            for c in 0..t.cols {
                let i = (r * t.cols + c) as usize;
                let cell: &TerminalPayloadCell = t.cells.get(i).unwrap_or(&FALLBACK_CELL);
                if cell.ch == '\0' || cell.ch == ' ' {
                    continue;
                }
                let (m, coverage) = font.rasterize(cell.ch, size_px);
                if coverage.is_empty() || m.width == 0 || m.height == 0 {
                    continue;
                }
                let w = m.width;
                let h = m.height;
                let b = m.bounds;
                let cx = c as f32 * cell_w as f32;
                let cy = r as f32 * cell_h as f32;
                // Baseline in y-down: line box top of row + ascent.
                let baseline = cy + line.ascent;
                // y-up: bottom = ymin, top of outline = ymin + height.
                let y_top = baseline - (b.ymin + b.height);
                let x_left = cx
                    + (cell_w as f32 - m.advance_width).max(0.0) * 0.5
                    + m.xmin as f32;
                let fg = cell.fg_rgba;
                for y in 0..h {
                    for x in 0..w {
                        let a = *coverage.get(y * w + x).unwrap_or(&0) as f32 / 255.0;
                        if a <= 0.0 {
                            continue;
                        }
                        let px = (x_left + x as f32).round() as i32;
                        let py = (y_top + y as f32).round() as i32;
                        if px < 0
                            || py < 0
                            || (px as u32) >= tw
                            || (py as u32) >= th
                        {
                            continue;
                        }
                        let oi = (py as u32 * tw + px as u32) as usize * 4;
                        if oi + 3 < p.len() {
                            blend_over_rgba(p, oi, fg, a);
                        }
                    }
                }
            }
        }
    }
    if t.cursor_visible
        && (t.cursor_col as u32) < t.cols as u32
        && (t.cursor_row as u32) < t.rows as u32
    {
        let cx = t.cursor_col as f32 * cell_w as f32;
        let cy = t.cursor_row as f32 * cell_h as f32;
        let a = 0.35f32;
        let p2 = buffer.make_mut_bytes();
        for yy in 0..cell_h {
            for xx in 0..cell_w {
                let px = (cx as u32 + xx) as u32;
                let py = (cy as u32 + yy) as u32;
                if px < tw && py < th {
                    let oi = (py * tw + px) as usize * 4;
                    if oi + 3 < p2.len() {
                        blend_straight_over(p2, oi, cursor_color, a);
                    }
                }
            }
        }
    }
    Some(Image::from_rgba8(buffer))
}

const FALLBACK_CELL: TerminalPayloadCell = TerminalPayloadCell {
    ch: ' ',
    fg_rgba: [0xE6, 0xEB, 0xF0, 0xFF],
    bg_rgba: [0x12, 0x14, 0x18, 0xFF],
    bold: false,
    italic: false,
    underline: false,
};
