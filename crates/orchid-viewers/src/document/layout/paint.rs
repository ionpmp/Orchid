//! Paragraph layout via `parley` + software rasterisation via `swash`.

#![allow(
    unused_imports,
    clippy::too_many_arguments,
    clippy::large_enum_variant,
    clippy::needless_range_loop
)]

use std::collections::HashMap;
use std::sync::Arc;

use parley::layout::{
    Alignment as ParleyAlignment, AlignmentOptions, Cluster, ClusterSide, GlyphRun, IndentOptions,
    PositionedLayoutItem,
};
use parley::style::{FontFamily, FontStyle, FontWeight, StyleProperty};
use parley::{FontContext, Layout, LayoutContext, LineHeight, RangedBuilder};
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Scaler, Source, StrikeWith};
use swash::zeno::{Format, Vector};
use swash::FontRef;

use crate::document::cursor::{cursor_from_plain_offset, plain_offset_from_cursor, Cursor};
use crate::document::model::{
    Alignment, Block, Document, LineSpacingRule, ListKind, NamedCharacterStyle,
    NamedParagraphStyle, PageSetup, Paragraph, RunStyle, SectionBreakType, Table, TableCell,
    TableRow, VMerge,
};

use super::tables::*;
use super::*;

pub(super) const MAX_PREVIEW_IMAGE_H: u32 = 240;

pub(super) const SELECTION_FILL: [u8; 4] = [147, 197, 253, 140]; // soft blue
pub(super) const CARET_FILL: [u8; 4] = [37, 99, 235, 220];
/// Soft amber wash for DOCX comment ranges (under/over glyphs in the base raster).
pub(super) const COMMENT_FILL: [u8; 4] = [253, 224, 71, 110];

pub(super) fn preview_image_display_size(
    img: &crate::document::model::InlineImage,
    max_w: f32,
) -> (u32, u32) {
    let mut disp_w = img.width_px;
    let mut disp_h = img.height_px;
    if disp_w == 0 || disp_h == 0 {
        if let Ok(decoded) = image::load_from_memory(&img.bytes) {
            if disp_w == 0 {
                disp_w = decoded.width();
            }
            if disp_h == 0 {
                disp_h = decoded.height();
            }
        } else {
            return (24, 24);
        }
    }
    disp_w = disp_w.max(1);
    disp_h = disp_h.max(1);
    if disp_w as f32 > max_w && max_w > 1.0 {
        let scale = max_w / disp_w as f32;
        disp_w = max_w as u32;
        disp_h = ((disp_h as f32) * scale).max(1.0) as u32;
    }
    if disp_h > MAX_PREVIEW_IMAGE_H {
        let scale = MAX_PREVIEW_IMAGE_H as f32 / disp_h as f32;
        disp_h = MAX_PREVIEW_IMAGE_H;
        disp_w = ((disp_w as f32) * scale).max(1.0) as u32;
    }
    (disp_w, disp_h)
}

/// Decode + scale an inline image for the preview canvas.
pub(super) fn prepare_preview_image(
    img: &crate::document::model::InlineImage,
    max_w: f32,
) -> (Option<Vec<u8>>, u32, u32) {
    let (disp_w, disp_h) = preview_image_display_size(img, max_w);
    let Ok(decoded) = image::load_from_memory(&img.bytes) else {
        return (None, disp_w, disp_h);
    };
    let rgba = decoded.into_rgba8();
    let (src_w, src_h) = rgba.dimensions();
    if disp_w == src_w && disp_h == src_h {
        return (Some(rgba.into_raw()), disp_w, disp_h);
    }
    let resized =
        image::imageops::resize(&rgba, disp_w, disp_h, image::imageops::FilterType::Triangle);
    (Some(resized.into_raw()), disp_w, disp_h)
}

pub(super) fn blit_rgba(
    pixels: &mut [u8],
    buf: (u32, u32),
    dest: (u32, u32),
    src_size: (u32, u32),
    rgba: &[u8],
) {
    let (buf_w, buf_h) = buf;
    let (x, y) = dest;
    let (src_w, src_h) = src_size;
    let copy_h = src_h.min(buf_h.saturating_sub(y));
    let copy_w = src_w.min(buf_w.saturating_sub(x));
    for row in 0..copy_h {
        for col in 0..copy_w {
            let si = ((row as usize) * (src_w as usize) + (col as usize)) * 4;
            if si + 3 >= rgba.len() {
                return;
            }
            blend_pixel(
                pixels,
                buf_w,
                x + col,
                y + row,
                rgba[si],
                rgba[si + 1],
                rgba[si + 2],
                rgba[si + 3],
            );
        }
    }
}

pub(super) fn overlay_selection_on_scene(
    scene: &RenderScene,
    selection: Option<(usize, usize)>,
) -> (Arc<Vec<u8>>, u32, u32) {
    if selection.is_none() {
        return (Arc::clone(&scene.base), scene.width, scene.height);
    }
    let mut pixels = scene.base.as_ref().clone();
    paint_selection_overlay(
        &scene.layouts,
        &mut pixels,
        scene.width,
        scene.height,
        scene.insets,
        scene.max_w,
        selection,
    );
    (Arc::new(pixels), scene.width, scene.height)
}

pub(super) fn paint_selection_overlay(
    layouts: &[LaidBlock],
    pixels: &mut [u8],
    width: u32,
    height: u32,
    insets: PreviewInsets,
    max_w: f32,
    selection: Option<(usize, usize)>,
) {
    let (sel_lo, sel_hi) = match selection {
        Some((a, b)) if a != b => (a.min(b), a.max(b)),
        _ => (0, 0),
    };
    let caret_at = match selection {
        Some((a, b)) if a == b => Some(a),
        _ => None,
    };
    for item in layouts {
        if item.is_image {
            if caret_at == Some(item.plain_start) {
                let y = (insets.top + item.y0) as u32;
                let x = (insets.left + item.x0) as u32;
                let box_h = item.image_h.max(24.0) as u32;
                let box_w = if item.image_w > 0 {
                    item.image_w
                } else {
                    (max_w as u32)
                        .saturating_sub(item.x0 as u32)
                        .saturating_sub(8)
                        .max(24)
                };
                fill_rect_blend(
                    pixels,
                    width,
                    height,
                    x.saturating_sub(1),
                    y.saturating_sub(1),
                    box_w.saturating_add(2),
                    box_h.saturating_add(2),
                    [180, 210, 255, 140],
                );
            }
            continue;
        }
        if item.layout.is_empty() {
            continue;
        }
        let origin_x = insets.left + item.x0 + item.indent_px;
        let origin_y = insets.top + item.y0;
        if sel_hi > sel_lo {
            let para_end = item.plain_start + item.body_len;
            let i0 = sel_lo.max(item.plain_start);
            let i1 = sel_hi.min(para_end);
            if i0 < i1 {
                let layout_lo = item.prefix_len + (i0 - item.plain_start);
                let layout_hi = item.prefix_len + (i1 - item.plain_start);
                paint_selection_range(
                    &item.layout,
                    pixels,
                    width,
                    height,
                    origin_x,
                    origin_y,
                    layout_lo,
                    layout_hi,
                    SELECTION_FILL,
                );
            }
        }
        if let Some(caret) = caret_at {
            let para_end = item.plain_start + item.body_len;
            if caret >= item.plain_start && caret <= para_end {
                let layout_idx = item.prefix_len + (caret - item.plain_start);
                paint_caret(
                    &item.layout,
                    pixels,
                    width,
                    height,
                    origin_x,
                    origin_y,
                    layout_idx,
                );
            }
        }
    }
}

pub(super) fn paint_selection_range(
    layout: &Layout<ColorBrush>,
    pixels: &mut [u8],
    buf_w: u32,
    buf_h: u32,
    origin_x: f32,
    origin_y: f32,
    layout_lo: usize,
    layout_hi: usize,
    fill: [u8; 4],
) {
    if layout_lo >= layout_hi {
        return;
    }
    for line in layout.lines() {
        let metrics = line.metrics();
        let line_top = origin_y + metrics.block_min_coord;
        let line_h = (metrics.block_max_coord - metrics.block_min_coord)
            .max(metrics.line_height)
            .max(14.0);
        for run in line.runs() {
            for cluster in run.visual_clusters() {
                let range = cluster.text_range();
                let overlap_lo = range.start.max(layout_lo);
                let overlap_hi = range.end.min(layout_hi);
                if overlap_lo >= overlap_hi {
                    continue;
                }
                let Some(x_off) = cluster.visual_offset() else {
                    continue;
                };
                let advance = cluster.advance().max(1.0);
                let frac_start = if range.end > range.start {
                    (overlap_lo - range.start) as f32 / (range.end - range.start) as f32
                } else {
                    0.0
                };
                let frac_end = if range.end > range.start {
                    (overlap_hi - range.start) as f32 / (range.end - range.start) as f32
                } else {
                    1.0
                };
                let x0 = origin_x + x_off + advance * frac_start;
                let x1 = origin_x + x_off + advance * frac_end;
                fill_rect_blend(
                    pixels,
                    buf_w,
                    buf_h,
                    x0.floor().max(0.0) as u32,
                    line_top.floor().max(0.0) as u32,
                    (x1 - x0).ceil().max(1.0) as u32,
                    line_h.ceil().max(1.0) as u32,
                    fill,
                );
            }
        }
    }
}

/// Paint comment ranges into the base preview raster (amber wash).
pub(super) fn paint_comment_highlights(
    layouts: &[LaidBlock],
    pixels: &mut [u8],
    width: u32,
    height: u32,
    insets: PreviewInsets,
    ranges: &[crate::document::model::CommentRange],
) {
    if ranges.is_empty() {
        return;
    }
    for c in ranges {
        let sel_lo = c.start_plain.min(c.end_plain);
        let sel_hi = c.start_plain.max(c.end_plain);
        if sel_lo >= sel_hi {
            // Zero-width: small amber caret-like tick at the offset.
            for item in layouts {
                if item.is_image || item.layout.is_empty() {
                    continue;
                }
                let para_end = item.plain_start + item.body_len;
                if sel_lo >= item.plain_start && sel_lo <= para_end {
                    let origin_x = insets.left + item.x0 + item.indent_px;
                    let origin_y = insets.top + item.y0;
                    let layout_idx = item.prefix_len + (sel_lo - item.plain_start);
                    paint_selection_range(
                        &item.layout,
                        pixels,
                        width,
                        height,
                        origin_x,
                        origin_y,
                        layout_idx,
                        layout_idx
                            .saturating_add(1)
                            .min(item.prefix_len + item.body_len),
                        COMMENT_FILL,
                    );
                }
            }
            continue;
        }
        for item in layouts {
            if item.is_image || item.layout.is_empty() {
                continue;
            }
            let para_end = item.plain_start + item.body_len;
            let i0 = sel_lo.max(item.plain_start);
            let i1 = sel_hi.min(para_end);
            if i0 < i1 {
                let origin_x = insets.left + item.x0 + item.indent_px;
                let origin_y = insets.top + item.y0;
                let layout_lo = item.prefix_len + (i0 - item.plain_start);
                let layout_hi = item.prefix_len + (i1 - item.plain_start);
                paint_selection_range(
                    &item.layout,
                    pixels,
                    width,
                    height,
                    origin_x,
                    origin_y,
                    layout_lo,
                    layout_hi,
                    COMMENT_FILL,
                );
            }
        }
    }
}

pub(super) fn paint_caret(
    layout: &Layout<ColorBrush>,
    pixels: &mut [u8],
    buf_w: u32,
    buf_h: u32,
    origin_x: f32,
    origin_y: f32,
    layout_idx: usize,
) {
    let (cluster, side) = if let Some(c) = Cluster::from_byte_index(layout, layout_idx) {
        (c, ClusterSide::Left)
    } else if layout_idx > 0 {
        if let Some(c) = Cluster::from_byte_index(layout, layout_idx - 1) {
            (c, ClusterSide::Right)
        } else {
            // Empty / unmapped — caret at line start.
            if let Some(line) = layout.lines().next() {
                let metrics = line.metrics();
                let x = origin_x + metrics.offset + metrics.inline_min_coord;
                let y = origin_y + metrics.block_min_coord;
                let h = (metrics.block_max_coord - metrics.block_min_coord)
                    .max(metrics.line_height)
                    .max(14.0);
                fill_rect_blend(
                    pixels,
                    buf_w,
                    buf_h,
                    x.floor().max(0.0) as u32,
                    y.floor().max(0.0) as u32,
                    2,
                    h.ceil().max(1.0) as u32,
                    CARET_FILL,
                );
            }
            return;
        }
    } else if let Some(line) = layout.lines().next() {
        let metrics = line.metrics();
        let x = origin_x + metrics.offset + metrics.inline_min_coord;
        let y = origin_y + metrics.block_min_coord;
        let h = (metrics.block_max_coord - metrics.block_min_coord)
            .max(metrics.line_height)
            .max(14.0);
        fill_rect_blend(
            pixels,
            buf_w,
            buf_h,
            x.floor().max(0.0) as u32,
            y.floor().max(0.0) as u32,
            2,
            h.ceil().max(1.0) as u32,
            CARET_FILL,
        );
        return;
    } else {
        return;
    };

    let Some(x_off) = cluster.visual_offset() else {
        return;
    };
    let line = cluster.line();
    let metrics = line.metrics();
    let x = origin_x
        + x_off
        + if side == ClusterSide::Right {
            cluster.advance()
        } else {
            0.0
        };
    let y = origin_y + metrics.block_min_coord;
    let h = (metrics.block_max_coord - metrics.block_min_coord)
        .max(metrics.line_height)
        .max(14.0);
    fill_rect_blend(
        pixels,
        buf_w,
        buf_h,
        x.floor().max(0.0) as u32,
        y.floor().max(0.0) as u32,
        2,
        h.ceil().max(1.0) as u32,
        CARET_FILL,
    );
}

pub(super) fn fill_rect_blend(
    pixels: &mut [u8],
    buf_w: u32,
    buf_h: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    rgba: [u8; 4],
) {
    let x1 = (x + w).min(buf_w);
    let y1 = (y + h).min(buf_h);
    for py in y..y1 {
        for px in x..x1 {
            blend_pixel(pixels, buf_w, px, py, rgba[0], rgba[1], rgba[2], rgba[3]);
        }
    }
}

pub(super) fn outline_level_font_pt(level: u8) -> f32 {
    match level {
        0 => 24.0,
        1 => 20.0,
        2 => 16.0,
        3 => 14.0,
        4 => 13.0,
        _ => 12.0,
    }
}

pub(super) fn list_prefix(p: &Paragraph) -> String {
    match p.list {
        ListKind::None => String::new(),
        ListKind::Bullet => "• ".into(),
        ListKind::Numbered => "1. ".into(),
    }
}

pub(super) fn list_indent_px(p: &Paragraph) -> f32 {
    let list = if p.list == ListKind::None {
        0.0
    } else {
        f32::from(p.list_level) * LIST_INDENT_PX
    };
    list + paragraph_left_indent_px(p)
}

pub(super) fn paragraph_right_indent_px(p: &Paragraph) -> f32 {
    twips_to_css_px(p.indent_right_twips)
}

/// Available wrap width after left (incl. list) and right paragraph indents.
#[allow(dead_code)]
pub(super) fn paragraph_wrap_width(max_w: f32, p: &Paragraph) -> f32 {
    (max_w - list_indent_px(p) - paragraph_right_indent_px(p)).max(40.0)
}

/// Left edge of the paragraph box after applying `w:ind` left/hanging.
pub(super) fn paragraph_left_indent_px(p: &Paragraph) -> f32 {
    let left = p.indent_left_twips;
    let fl = p.indent_first_line_twips;
    let base = if fl < 0 {
        left.saturating_sub(fl.unsigned_abs())
    } else {
        left
    };
    twips_to_css_px(base)
}

pub(super) fn apply_paragraph_text_indent(layout: &mut Layout<ColorBrush>, p: &Paragraph) {
    let fl = p.indent_first_line_twips;
    if fl == 0 {
        return;
    }
    let amount = twips_to_css_px(fl.unsigned_abs());
    layout.set_text_indent(
        amount,
        IndentOptions {
            hanging: fl < 0,
            ..IndentOptions::default()
        },
    );
}

pub(super) fn parley_alignment(a: Alignment) -> ParleyAlignment {
    match a {
        Alignment::Left => ParleyAlignment::Left,
        Alignment::Center => ParleyAlignment::Center,
        Alignment::Right => ParleyAlignment::Right,
        Alignment::Justify => ParleyAlignment::Justify,
    }
}

/// Alignment for preview layout. `w:bidi` paragraphs use an RTL base direction;
/// Word's left/right map to the paragraph start/end edges, so Left paints on the
/// right and Right on the left when bidi is set (parley has no public RTL base API).
pub(super) fn parley_alignment_for(p: &Paragraph) -> ParleyAlignment {
    if !p.bidi {
        return parley_alignment(p.alignment);
    }
    match p.alignment {
        Alignment::Left => ParleyAlignment::Right,
        Alignment::Right => ParleyAlignment::Left,
        Alignment::Center => ParleyAlignment::Center,
        Alignment::Justify => ParleyAlignment::Justify,
    }
}

/// Rasterise a single layout into an RGBA8 buffer (white background).
#[must_use]
pub fn render_to_rgba(layout: &Layout<ColorBrush>, width: u32, height: u32) -> Vec<u8> {
    let mut pixels = vec![255u8; (width as usize) * (height as usize) * 4];
    if width == 0 || height == 0 {
        return pixels;
    }
    let mut scale_cx = ScaleContext::new();
    render_layout_at(&mut scale_cx, layout, &mut pixels, width, height, 0.0, 0.0);
    pixels
}

pub(super) fn render_layout_at(
    scale_cx: &mut ScaleContext,
    layout: &Layout<ColorBrush>,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    origin_x: f32,
    origin_y: f32,
) {
    for line in layout.lines() {
        for item in line.items() {
            match item {
                PositionedLayoutItem::GlyphRun(glyph_run) => {
                    render_glyph_run(
                        scale_cx, &glyph_run, pixels, width, height, origin_x, origin_y,
                    );
                }
                PositionedLayoutItem::InlineBox(_) => {}
            }
        }
    }
}

pub(super) fn render_glyph_run(
    context: &mut ScaleContext,
    glyph_run: &GlyphRun<'_, ColorBrush>,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    origin_x: f32,
    origin_y: f32,
) {
    let mut run_x = glyph_run.offset();
    let style = glyph_run.style();
    let brush = style.brush;
    let run_y = glyph_run.baseline() + brush.baseline_shift;
    let run = glyph_run.run();
    let font = run.font();
    let font_size = run.font_size();
    let normalized_coords = run.normalized_coords();

    if brush.highlight {
        let metrics = run.metrics();
        let x = (origin_x + glyph_run.offset()).floor().max(0.0) as u32;
        let y = (origin_y + run_y - metrics.ascent).floor().max(0.0) as u32;
        let w = glyph_run.advance().ceil().max(1.0) as u32;
        let h = (metrics.ascent + metrics.descent).ceil().max(1.0) as u32;
        fill_rect(pixels, width, height, x, y, w, h, HIGHLIGHT_YELLOW);
    }

    let Some(font_ref) = FontRef::from_index(font.data.as_ref(), font.index as usize) else {
        return;
    };
    let mut scaler = context
        .builder(font_ref)
        .size(font_size)
        .hint(true)
        .normalized_coords(normalized_coords)
        .build();

    for glyph in glyph_run.glyphs() {
        let glyph_x = origin_x + run_x + glyph.x;
        let glyph_y = origin_y + run_y + glyph.y;
        run_x += glyph.advance;
        if brush.shadow {
            let mut shadow_brush = brush;
            shadow_brush.r = 0;
            shadow_brush.g = 0;
            shadow_brush.b = 0;
            shadow_brush.a = shadow_brush.a.min(96);
            render_glyph(
                pixels,
                width,
                height,
                &mut scaler,
                shadow_brush,
                glyph.id as u16,
                glyph_x + 1.5,
                glyph_y + 1.5,
            );
        }
        if brush.emboss {
            let mut hi = brush;
            hi.r = 255;
            hi.g = 255;
            hi.b = 255;
            hi.a = hi.a.min(140);
            render_glyph(
                pixels,
                width,
                height,
                &mut scaler,
                hi,
                glyph.id as u16,
                glyph_x - 1.0,
                glyph_y - 1.0,
            );
            let mut lo = brush;
            lo.r = 0;
            lo.g = 0;
            lo.b = 0;
            lo.a = lo.a.min(100);
            render_glyph(
                pixels,
                width,
                height,
                &mut scaler,
                lo,
                glyph.id as u16,
                glyph_x + 1.0,
                glyph_y + 1.0,
            );
        } else if brush.imprint {
            let mut lo = brush;
            lo.r = 0;
            lo.g = 0;
            lo.b = 0;
            lo.a = lo.a.min(100);
            render_glyph(
                pixels,
                width,
                height,
                &mut scaler,
                lo,
                glyph.id as u16,
                glyph_x - 1.0,
                glyph_y - 1.0,
            );
            let mut hi = brush;
            hi.r = 255;
            hi.g = 255;
            hi.b = 255;
            hi.a = hi.a.min(120);
            render_glyph(
                pixels,
                width,
                height,
                &mut scaler,
                hi,
                glyph.id as u16,
                glyph_x + 1.0,
                glyph_y + 1.0,
            );
        }
        render_glyph(
            pixels,
            width,
            height,
            &mut scaler,
            brush,
            glyph.id as u16,
            glyph_x,
            glyph_y,
        );
    }

    let run_metrics = run.metrics();
    if let Some(decoration) = &style.underline {
        let offset = decoration.offset.unwrap_or(run_metrics.underline_offset);
        let size = decoration.size.unwrap_or(run_metrics.underline_size);
        render_decoration(
            pixels,
            width,
            height,
            glyph_run,
            decoration.brush,
            offset,
            size,
            origin_x,
            origin_y,
        );
    }
    if let Some(decoration) = &style.strikethrough {
        let offset = decoration
            .offset
            .unwrap_or(run_metrics.strikethrough_offset);
        let size = decoration.size.unwrap_or(run_metrics.strikethrough_size);
        render_decoration(
            pixels,
            width,
            height,
            glyph_run,
            decoration.brush,
            offset,
            size,
            origin_x,
            origin_y,
        );
        if decoration.brush.double_strike {
            render_decoration(
                pixels,
                width,
                height,
                glyph_run,
                decoration.brush,
                offset - size * 2.5,
                size,
                origin_x,
                origin_y,
            );
        }
    }
}

pub(super) fn render_decoration(
    pixels: &mut [u8],
    buf_w: u32,
    buf_h: u32,
    glyph_run: &GlyphRun<'_, ColorBrush>,
    brush: ColorBrush,
    offset: f32,
    line_w: f32,
    origin_x: f32,
    origin_y: f32,
) {
    let y = origin_y + glyph_run.baseline() - offset;
    let x0 = origin_x + glyph_run.offset();
    let x1 = x0 + glyph_run.advance();
    let y0 = y.floor().max(0.0) as u32;
    let y1 = (y + line_w.max(1.0)).ceil() as u32;
    let xa = x0.floor().max(0.0) as u32;
    let xb = x1.ceil() as u32;
    for py in y0..y1.min(buf_h) {
        for px in xa..xb.min(buf_w) {
            blend_pixel(pixels, buf_w, px, py, brush.r, brush.g, brush.b, brush.a);
        }
    }
}

pub(super) fn render_glyph(
    pixels: &mut [u8],
    buf_w: u32,
    buf_h: u32,
    scaler: &mut Scaler<'_>,
    brush: ColorBrush,
    glyph_id: u16,
    glyph_x: f32,
    glyph_y: f32,
) {
    let offset = Vector::new(glyph_x.fract(), glyph_y.fract());
    let Some(rendered) = Render::new(&[
        Source::ColorOutline(0),
        Source::ColorBitmap(StrikeWith::BestFit),
        Source::Outline,
    ])
    .format(Format::Alpha)
    .offset(offset)
    .render(scaler, glyph_id) else {
        return;
    };

    let glyph_width = rendered.placement.width;
    let glyph_height = rendered.placement.height;
    let base_x = glyph_x.floor() as i32 + rendered.placement.left;
    let base_y = glyph_y.floor() as i32 - rendered.placement.top;

    match rendered.content {
        Content::Mask => {
            let mut i = 0usize;
            for row in 0..glyph_height {
                for col in 0..glyph_width {
                    let x = base_x + col as i32;
                    let y = base_y + row as i32;
                    if x >= 0 && y >= 0 {
                        let alpha = rendered.data[i];
                        if alpha > 0 {
                            blend_pixel(
                                pixels, buf_w, x as u32, y as u32, brush.r, brush.g, brush.b, alpha,
                            );
                        }
                    }
                    i += 1;
                    let _ = buf_h;
                }
            }
        }
        Content::Color => {
            let row_size = glyph_width as usize * 4;
            for (row, row_bytes) in rendered.data.chunks_exact(row_size).enumerate() {
                for (col, px) in row_bytes.chunks_exact(4).enumerate() {
                    let x = base_x + col as i32;
                    let y = base_y + row as i32;
                    if x >= 0 && y >= 0 && (y as u32) < buf_h && (x as u32) < buf_w {
                        blend_pixel(
                            pixels, buf_w, x as u32, y as u32, px[0], px[1], px[2], px[3],
                        );
                    }
                }
            }
        }
        Content::SubpixelMask => {}
    }
}

pub(super) fn blend_pixel(
    pixels: &mut [u8],
    buf_w: u32,
    x: u32,
    y: u32,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
) {
    if a == 0 {
        return;
    }
    let idx = ((y as usize) * (buf_w as usize) + (x as usize)) * 4;
    if idx + 3 >= pixels.len() {
        return;
    }
    if a == 255 {
        pixels[idx] = r;
        pixels[idx + 1] = g;
        pixels[idx + 2] = b;
        pixels[idx + 3] = 255;
        return;
    }
    let src_a = f32::from(a) / 255.0;
    let dst_a = 1.0 - src_a;
    pixels[idx] = (f32::from(r) * src_a + f32::from(pixels[idx]) * dst_a) as u8;
    pixels[idx + 1] = (f32::from(g) * src_a + f32::from(pixels[idx + 1]) * dst_a) as u8;
    pixels[idx + 2] = (f32::from(b) * src_a + f32::from(pixels[idx + 2]) * dst_a) as u8;
    pixels[idx + 3] = 255;
}

pub(super) fn twips_to_css_px(twips: u32) -> f32 {
    // 1440 twips = 1 inch = 96 CSS px.
    (twips as f32) * 96.0 / 1440.0
}

pub(super) fn paragraph_line_height(p: &Paragraph) -> LineHeight {
    match p.line_spacing_rule {
        LineSpacingRule::Auto => {
            let rel = if p.line_spacing == 0 {
                1.35
            } else {
                (p.line_spacing as f32 / 240.0).clamp(0.5, 4.0)
            };
            LineHeight::FontSizeRelative(rel)
        }
        LineSpacingRule::Exact => LineHeight::Absolute(twips_to_css_px(p.line_spacing).max(1.0)),
        // Word "at least": floor line box to the given height. Approximate with
        // Absolute max(value, typical default line box).
        LineSpacingRule::AtLeast => {
            let min_h = twips_to_css_px(p.line_spacing).max(1.0);
            LineHeight::Absolute(min_h.max(14.0 * 1.2))
        }
    }
}

pub(super) fn paint_dashed_h_line(
    pixels: &mut [u8],
    buf_w: u32,
    buf_h: u32,
    x0: i32,
    x1: i32,
    y: i32,
    rgba: [u8; 4],
) {
    if y < 0 || (y as u32) >= buf_h {
        return;
    }
    let lo = x0.max(0) as u32;
    let hi = x1.max(0) as u32;
    let hi = hi.min(buf_w);
    let mut on = true;
    let mut run = 0u32;
    for px in lo..hi {
        if on {
            let i = ((y as usize) * (buf_w as usize) + (px as usize)) * 4;
            if i + 3 < pixels.len() {
                pixels[i] = rgba[0];
                pixels[i + 1] = rgba[1];
                pixels[i + 2] = rgba[2];
                pixels[i + 3] = rgba[3];
            }
        }
        run += 1;
        if run >= 4 {
            run = 0;
            on = !on;
        }
    }
}

pub(super) fn paint_solid_h_line(
    pixels: &mut [u8],
    buf_w: u32,
    buf_h: u32,
    x0: i32,
    x1: i32,
    y: i32,
    rgba: [u8; 4],
) {
    if y < 0 || (y as u32) >= buf_h {
        return;
    }
    let lo = x0.max(0) as u32;
    let hi = x1.max(0) as u32;
    let hi = hi.min(buf_w);
    for px in lo..hi {
        let i = ((y as usize) * (buf_w as usize) + (px as usize)) * 4;
        if i + 3 < pixels.len() {
            pixels[i] = rgba[0];
            pixels[i + 1] = rgba[1];
            pixels[i + 2] = rgba[2];
            pixels[i + 3] = rgba[3];
        }
    }
}

pub(super) fn paint_solid_v_line(
    pixels: &mut [u8],
    buf_w: u32,
    buf_h: u32,
    x: i32,
    y0: i32,
    y1: i32,
    rgba: [u8; 4],
) {
    if x < 0 || (x as u32) >= buf_w {
        return;
    }
    let lo = y0.max(0) as u32;
    let hi = y1.max(0) as u32;
    let hi = hi.min(buf_h);
    for py in lo..hi {
        let i = ((py as usize) * (buf_w as usize) + (x as usize)) * 4;
        if i + 3 < pixels.len() {
            pixels[i] = rgba[0];
            pixels[i + 1] = rgba[1];
            pixels[i + 2] = rgba[2];
            pixels[i + 3] = rgba[3];
        }
    }
}

pub(super) fn fill_rect(
    pixels: &mut [u8],
    buf_w: u32,
    buf_h: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    rgba: [u8; 4],
) {
    let x1 = (x + w).min(buf_w);
    let y1 = (y + h).min(buf_h);
    for py in y..y1 {
        for px in x..x1 {
            let i = ((py as usize) * (buf_w as usize) + (px as usize)) * 4;
            pixels[i] = rgba[0];
            pixels[i + 1] = rgba[1];
            pixels[i + 2] = rgba[2];
            pixels[i + 3] = rgba[3];
        }
    }
}
