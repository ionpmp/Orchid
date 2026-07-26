//! Paragraph layout via `parley` + software rasterisation via `swash`.

use std::collections::HashMap;
use std::sync::Arc;

use parley::layout::{
    Alignment as ParleyAlignment, AlignmentOptions, Cluster, ClusterSide, GlyphRun,
    PositionedLayoutItem,
};
use parley::style::{FontFamily, FontStyle, FontWeight, StyleProperty};
use parley::{FontContext, Layout, LayoutContext, LineHeight, RangedBuilder};
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Scaler, Source, StrikeWith};
use swash::zeno::{Format, Vector};
use swash::FontRef;

use crate::document::model::{Alignment, Block, Document, ListKind, Paragraph};

/// Brush colour for styled runs (RGBA).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorBrush {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha.
    pub a: u8,
}

impl Default for ColorBrush {
    fn default() -> Self {
        Self {
            r: 32,
            g: 32,
            b: 32,
            a: 255,
        }
    }
}

/// Default content width for the document preview (CSS pixels).
pub const DEFAULT_PREVIEW_WIDTH: f32 = 720.0;
/// Padding around the page content.
pub const PREVIEW_PADDING: f32 = 28.0;
/// Cap rendered page height so huge docs stay interactive.
pub const MAX_PREVIEW_HEIGHT: u32 = 4096;

/// Owns parley + swash contexts for laying out and rasterising paragraphs.
pub struct DocumentLayout {
    font_cx: FontContext,
    layout_cx: LayoutContext<ColorBrush>,
    scale_cx: ScaleContext,
}

impl std::fmt::Debug for DocumentLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocumentLayout").finish_non_exhaustive()
    }
}

impl Default for DocumentLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentLayout {
    /// Create layout contexts.
    #[must_use]
    pub fn new() -> Self {
        Self {
            font_cx: FontContext::new(),
            layout_cx: LayoutContext::new(),
            scale_cx: ScaleContext::new(),
        }
    }

    /// Lay out a single paragraph at `max_width` CSS pixels.
    pub fn layout_paragraph(&mut self, p: &Paragraph, max_width: f32) -> Layout<ColorBrush> {
        let prefix = list_prefix(p);
        let body = p.plain_text();
        let text = if prefix.is_empty() {
            body
        } else {
            format!("{prefix}{body}")
        };
        let mut builder: RangedBuilder<'_, ColorBrush> =
            self.layout_cx
                .ranged_builder(&mut self.font_cx, &text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(14.0));
        builder.push_default(LineHeight::FontSizeRelative(1.35));
        builder.push_default(StyleProperty::Brush(ColorBrush::default()));
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Segoe UI")));

        let mut offset = prefix.len();
        for run in &p.runs {
            let len = run.text.len();
            if len == 0 {
                continue;
            }
            let end = offset + len;
            if run.style.bold {
                builder.push(StyleProperty::FontWeight(FontWeight::BOLD), offset..end);
            }
            if run.style.italic {
                builder.push(StyleProperty::FontStyle(FontStyle::Italic), offset..end);
            }
            if run.style.underline {
                builder.push(StyleProperty::Underline(true), offset..end);
            }
            if let Some(pt) = run.style.font_size_pt {
                builder.push(StyleProperty::FontSize(pt), offset..end);
            }
            if let Some(ref family) = run.style.font_family {
                builder.push(
                    StyleProperty::FontFamily(FontFamily::named(family.as_str())),
                    offset..end,
                );
            }
            if let Some([r, g, b]) = run.style.color {
                builder.push(
                    StyleProperty::Brush(ColorBrush { r, g, b, a: 255 }),
                    offset..end,
                );
            }
            offset = end;
        }

        let mut layout = builder.build(&text);
        layout.break_all_lines(Some(max_width.max(1.0)));
        layout.align(parley_alignment(p.alignment), AlignmentOptions::default());
        layout
    }

    /// Rasterise an entire document into an RGBA8 page image.
    #[must_use]
    pub fn render_document(
        &mut self,
        doc: &Document,
        content_width: f32,
    ) -> (Arc<Vec<u8>>, u32, u32) {
        let pad = PREVIEW_PADDING;
        let max_w = content_width.max(80.0);
        let mut layouts: Vec<(Layout<ColorBrush>, f32)> = Vec::new();
        let mut total_h = pad;
        let para_gap = 10.0;

        for block in &doc.blocks {
            match block {
                Block::Paragraph(p) => {
                    let layout = self.layout_paragraph(p, max_w);
                    let h = layout.height().max(16.0);
                    layouts.push((layout, total_h));
                    total_h += h + para_gap;
                }
                Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            for p in &cell.paragraphs {
                                let layout = self.layout_paragraph(p, max_w);
                                let h = layout.height().max(14.0);
                                layouts.push((layout, total_h));
                                total_h += h + 4.0;
                            }
                        }
                    }
                    total_h += para_gap;
                }
                Block::Image(img) => {
                    // Placeholder bar for inline images (full decode is Tier-2).
                    let h = (img.height_px.min(120) as f32).max(24.0);
                    layouts.push((Layout::default(), total_h));
                    let _ = h;
                    total_h += h + para_gap;
                }
            }
            if total_h > MAX_PREVIEW_HEIGHT as f32 {
                break;
            }
        }

        let width = (max_w + pad * 2.0).ceil() as u32;
        let height = (total_h + pad)
            .ceil()
            .clamp(64.0, MAX_PREVIEW_HEIGHT as f32) as u32;
        let mut pixels = vec![255u8; (width as usize) * (height as usize) * 4];

        // Soft page border / margin cue.
        fill_rect(
            &mut pixels,
            width,
            height,
            0,
            0,
            width,
            height,
            [250, 250, 252, 255],
        );

        for (layout, y0) in &layouts {
            if layout.len() == 0 {
                // Image placeholder strip.
                let y = (y0 + pad) as u32;
                fill_rect(
                    &mut pixels,
                    width,
                    height,
                    pad as u32,
                    y,
                    (max_w as u32).saturating_sub(8),
                    24,
                    [220, 220, 228, 255],
                );
                continue;
            }
            render_layout_at(
                &mut self.scale_cx,
                layout,
                &mut pixels,
                width,
                height,
                pad,
                pad + y0,
            );
        }

        (Arc::new(pixels), width, height)
    }

    /// Map a point in preview-image CSS pixels to a UTF-8 offset in [`Document::plain_text`].
    ///
    /// Coordinates are relative to the top-left of the rendered page (including padding).
    #[must_use]
    pub fn hit_test_plain_offset(
        &mut self,
        doc: &Document,
        content_width: f32,
        x: f32,
        y: f32,
    ) -> Option<usize> {
        let pad = PREVIEW_PADDING;
        let max_w = content_width.max(80.0);
        let local_x = x - pad;
        let local_y = y - pad;
        let para_gap = 10.0;

        let mut total_h = 0.0;
        let mut plain_offset = 0usize;
        let mut emitted_text = false;

        if local_y < 0.0 {
            return Some(0);
        }

        for block in &doc.blocks {
            match block {
                Block::Paragraph(p) => {
                    if emitted_text {
                        plain_offset += 1;
                    }
                    emitted_text = true;
                    let body_len = p.plain_text().len();
                    let prefix_len = list_prefix(p).len();
                    let layout = self.layout_paragraph(p, max_w);
                    let h = layout.height().max(16.0);
                    let y0 = total_h;
                    let y1 = total_h + h + para_gap;
                    if local_y >= y0 && local_y < y1 {
                        let ly = (local_y - y0).max(0.0);
                        let lx = local_x.clamp(0.0, max_w);
                        let body_idx = cluster_to_body_index(&layout, lx, ly, prefix_len, body_len);
                        return Some(plain_offset + body_idx);
                    }
                    plain_offset += body_len;
                    total_h += h + para_gap;
                }
                Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            for p in &cell.paragraphs {
                                if emitted_text {
                                    plain_offset += 1;
                                }
                                emitted_text = true;
                                let body_len = p.plain_text().len();
                                let prefix_len = list_prefix(p).len();
                                let layout = self.layout_paragraph(p, max_w);
                                let h = layout.height().max(14.0);
                                let y0 = total_h;
                                let y1 = total_h + h + 4.0;
                                if local_y >= y0 && local_y < y1 {
                                    let ly = (local_y - y0).max(0.0);
                                    let lx = local_x.clamp(0.0, max_w);
                                    let body_idx =
                                        cluster_to_body_index(&layout, lx, ly, prefix_len, body_len);
                                    return Some(plain_offset + body_idx);
                                }
                                plain_offset += body_len;
                                total_h += h + 4.0;
                            }
                        }
                    }
                    total_h += para_gap;
                }
                Block::Image(img) => {
                    let h = (img.height_px.min(120) as f32).max(24.0);
                    let y0 = total_h;
                    let y1 = total_h + h + para_gap;
                    if local_y >= y0 && local_y < y1 {
                        return Some(plain_offset);
                    }
                    total_h += h + para_gap;
                }
            }
            if total_h > MAX_PREVIEW_HEIGHT as f32 {
                break;
            }
        }
        Some(plain_offset)
    }
}

fn cluster_to_body_index(
    layout: &Layout<ColorBrush>,
    x: f32,
    y: f32,
    prefix_len: usize,
    body_len: usize,
) -> usize {
    let Some((cluster, side)) = Cluster::from_point(layout, x, y) else {
        return if y <= 0.0 { 0 } else { body_len };
    };
    let range = cluster.text_range();
    let layout_idx = match side {
        ClusterSide::Left => range.start,
        ClusterSide::Right => range.end,
    };
    layout_idx.saturating_sub(prefix_len).min(body_len)
}

fn list_prefix(p: &Paragraph) -> String {
    match p.list {
        ListKind::None => String::new(),
        ListKind::Bullet => "• ".into(),
        ListKind::Numbered => "1. ".into(),
    }
}

fn parley_alignment(a: Alignment) -> ParleyAlignment {
    match a {
        Alignment::Left => ParleyAlignment::Left,
        Alignment::Center => ParleyAlignment::Center,
        Alignment::Right => ParleyAlignment::Right,
        Alignment::Justify => ParleyAlignment::Justify,
    }
}

/// Cache of laid-out paragraphs keyed by block index.
#[derive(Debug, Default)]
pub struct LayoutCache {
    cache: HashMap<usize, CachedLayout>,
    /// Width the cache was built for.
    width: f32,
}

struct CachedLayout {
    layout: Layout<ColorBrush>,
}

impl std::fmt::Debug for CachedLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedLayout").finish_non_exhaustive()
    }
}

impl LayoutCache {
    /// Empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Get or compute layout for paragraph `idx`.
    pub fn get_or_layout(
        &mut self,
        idx: usize,
        p: &Paragraph,
        dl: &mut DocumentLayout,
        width: f32,
    ) -> &Layout<ColorBrush> {
        if (self.width - width).abs() > 0.5 {
            self.cache.clear();
            self.width = width;
        }
        self.cache.entry(idx).or_insert_with(|| CachedLayout {
            layout: dl.layout_paragraph(p, width),
        });
        &self.cache.get(&idx).expect("just inserted").layout
    }

    /// Invalidate one paragraph.
    pub fn invalidate(&mut self, idx: usize) {
        self.cache.remove(&idx);
    }

    /// Drop the entire cache.
    pub fn invalidate_all(&mut self) {
        self.cache.clear();
    }

    /// Whether `idx` is cached (tests).
    #[must_use]
    pub fn contains(&self, idx: usize) -> bool {
        self.cache.contains_key(&idx)
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

fn render_layout_at(
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
                    render_glyph_run(scale_cx, &glyph_run, pixels, width, height, origin_x, origin_y);
                }
                PositionedLayoutItem::InlineBox(_) => {}
            }
        }
    }
}

fn render_glyph_run(
    context: &mut ScaleContext,
    glyph_run: &GlyphRun<'_, ColorBrush>,
    pixels: &mut [u8],
    width: u32,
    height: u32,
    origin_x: f32,
    origin_y: f32,
) {
    let mut run_x = glyph_run.offset();
    let run_y = glyph_run.baseline();
    let style = glyph_run.style();
    let brush = style.brush;
    let run = glyph_run.run();
    let font = run.font();
    let font_size = run.font_size();
    let normalized_coords = run.normalized_coords();

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
}

fn render_decoration(
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

fn render_glyph(
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
    .render(scaler, glyph_id)
    else {
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
                                pixels,
                                buf_w,
                                x as u32,
                                y as u32,
                                brush.r,
                                brush.g,
                                brush.b,
                                alpha,
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
                            pixels,
                            buf_w,
                            x as u32,
                            y as u32,
                            px[0],
                            px[1],
                            px[2],
                            px[3],
                        );
                    }
                }
            }
        }
        Content::SubpixelMask => {}
    }
}

fn blend_pixel(
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

fn fill_rect(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::model::{Run, RunStyle};

    fn sample_paragraph() -> Paragraph {
        Paragraph {
            runs: vec![Run {
                text: "Hello layout".into(),
                style: RunStyle {
                    bold: true,
                    ..Default::default()
                },
            }],
            ..Default::default()
        }
    }

    #[test]
    fn layout_paragraph_has_lines() {
        let mut dl = DocumentLayout::new();
        let layout = dl.layout_paragraph(&sample_paragraph(), 400.0);
        assert!(layout.len() > 0);
    }

    #[test]
    fn cache_hits_second_call() {
        let mut dl = DocumentLayout::new();
        let mut cache = LayoutCache::new();
        let p = sample_paragraph();
        let _ = cache.get_or_layout(0, &p, &mut dl, 400.0);
        assert!(cache.contains(0));
        cache.invalidate(0);
        assert!(!cache.contains(0));
    }

    #[test]
    fn render_produces_non_white_ink() {
        let mut dl = DocumentLayout::new();
        let layout = dl.layout_paragraph(&sample_paragraph(), 200.0);
        let buf = render_to_rgba(&layout, 256, 64);
        assert!(
            buf.chunks_exact(4)
                .any(|px| px[0] < 240 || px[1] < 240 || px[2] < 240),
            "expected glyph ink in the buffer"
        );
    }

    #[test]
    fn render_document_has_size() {
        let mut dl = DocumentLayout::new();
        let doc = Document {
            blocks: vec![Block::Paragraph(sample_paragraph())],
            ..Default::default()
        };
        let (bytes, w, h) = dl.render_document(&doc, 400.0);
        assert!(w > 100);
        assert!(h > 40);
        assert_eq!(bytes.len(), (w * h * 4) as usize);
    }

    #[test]
    fn hit_test_finds_second_paragraph() {
        let mut dl = DocumentLayout::new();
        let doc = Document {
            blocks: vec![
                Block::Paragraph(Paragraph {
                    runs: vec![Run {
                        text: "First".into(),
                        style: RunStyle::default(),
                    }],
                    ..Default::default()
                }),
                Block::Paragraph(Paragraph {
                    runs: vec![Run {
                        text: "Second line here".into(),
                        style: RunStyle::default(),
                    }],
                    ..Default::default()
                }),
            ],
            ..Default::default()
        };
        // Click near the top of the second paragraph (padding + first para height + gap).
        let offset = dl
            .hit_test_plain_offset(&doc, 400.0, PREVIEW_PADDING + 8.0, PREVIEW_PADDING + 40.0)
            .unwrap();
        // "First\n" = 6 bytes; caret should land in the second paragraph.
        assert!(offset >= 6, "offset={offset}");
        assert!(offset <= doc.plain_text().len());
    }

    #[test]
    fn hit_test_top_left_is_start() {
        let mut dl = DocumentLayout::new();
        let doc = Document {
            blocks: vec![Block::Paragraph(sample_paragraph())],
            ..Default::default()
        };
        let offset = dl
            .hit_test_plain_offset(&doc, 400.0, PREVIEW_PADDING + 2.0, PREVIEW_PADDING + 2.0)
            .unwrap();
        assert_eq!(offset, 0);
    }
}
