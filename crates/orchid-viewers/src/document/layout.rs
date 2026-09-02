//! Paragraph layout via `parley` + software rasterisation via `swash`.

#![allow(
    clippy::too_many_arguments,
    clippy::large_enum_variant,
    clippy::needless_range_loop
)]

use std::collections::HashMap;
use std::sync::Arc;

use parley::layout::{
    Alignment as ParleyAlignment, AlignmentOptions, Cluster, ClusterSide, GlyphRun,
    IndentOptions, PositionedLayoutItem,
};
use parley::style::{FontFamily, FontStyle, FontWeight, StyleProperty};
use parley::{FontContext, Layout, LayoutContext, LineHeight, RangedBuilder};
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Scaler, Source, StrikeWith};
use swash::zeno::{Format, Vector};
use swash::FontRef;

use crate::document::cursor::{cursor_from_plain_offset, plain_offset_from_cursor, Cursor};
use crate::document::model::{
    Alignment, Block, Document, LineSpacingRule, ListKind, PageSetup, Paragraph, Table, TableCell,
    TableRow, VMerge,
};

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
    /// When true, paint a yellow highlight behind the glyph run.
    pub highlight: bool,
    /// Extra baseline shift in CSS px (negative raises for superscript).
    pub baseline_shift: f32,
    /// Soft drop-shadow for `w:shadow` runs.
    pub shadow: bool,
}

impl Default for ColorBrush {
    fn default() -> Self {
        Self {
            r: 32,
            g: 32,
            b: 32,
            a: 255,
            highlight: false,
            baseline_shift: 0.0,
            shadow: false,
        }
    }
}

/// Word-style yellow highlight fill.
const HIGHLIGHT_YELLOW: [u8; 4] = [255, 255, 0, 255];

/// Default content width for the document preview (CSS pixels).
pub const DEFAULT_PREVIEW_WIDTH: f32 = 720.0;
/// Extra left inset per list indent level (`w:ilvl`).
pub const LIST_INDENT_PX: f32 = 24.0;
/// Cap rendered page height so huge docs stay interactive.
pub const MAX_PREVIEW_HEIGHT: u32 = 4096;
/// Device-pixel ratio for soft-rendered preview (sharper on HiDPI).
pub const PREVIEW_RENDER_SCALE: f32 = 2.0;

/// CSS-pixel page margins for the preview canvas (from `w:pgMar`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewInsets {
    /// Left margin.
    pub left: f32,
    /// Right margin.
    pub right: f32,
    /// Top margin.
    pub top: f32,
    /// Bottom margin.
    pub bottom: f32,
}

impl PreviewInsets {
    /// Convert [`PageSetup`] twip margins to CSS pixels (96 dpi).
    #[must_use]
    pub fn from_page_setup(ps: &PageSetup) -> Self {
        Self {
            left: twips_to_css_px(ps.margin_left_twips),
            right: twips_to_css_px(ps.margin_right_twips),
            top: twips_to_css_px(ps.margin_top_twips),
            bottom: twips_to_css_px(ps.margin_bottom_twips),
        }
    }

    /// Insets for the default US-Letter 1″ margins.
    #[must_use]
    pub fn default_letter() -> Self {
        Self::from_page_setup(&PageSetup::default())
    }
}

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
    ///
    /// `scale` is the Parley display scale (1.0 for hit-testing, [`PREVIEW_RENDER_SCALE`] when rasterising).
    pub fn layout_paragraph(
        &mut self,
        p: &Paragraph,
        max_width: f32,
        scale: f32,
    ) -> Layout<ColorBrush> {
        let scale = scale.max(0.5);
        let prefix = list_prefix(p);
        let body = p
            .runs
            .iter()
            .map(|r| {
                if r.style.all_caps || r.style.small_caps {
                    r.text.to_uppercase()
                } else {
                    r.text.clone()
                }
            })
            .collect::<String>();
        let text = if prefix.is_empty() {
            body
        } else {
            format!("{prefix}{body}")
        };
        let mut builder: RangedBuilder<'_, ColorBrush> =
            self.layout_cx
                .ranged_builder(&mut self.font_cx, &text, scale, true);
        let default_pt = p
            .outline_level
            .map(outline_level_font_pt)
            .unwrap_or(14.0);
        builder.push_default(StyleProperty::FontSize(default_pt));
        if p.outline_level.is_some_and(|lvl| lvl <= 2) {
            builder.push_default(StyleProperty::FontWeight(FontWeight::BOLD));
        }
        builder.push_default(paragraph_line_height(p));
        builder.push_default(StyleProperty::Brush(ColorBrush::default()));
        builder.push_default(StyleProperty::FontFamily(FontFamily::named("Segoe UI")));

        let mut offset = prefix.len();
        for run in &p.runs {
            let display = if run.style.all_caps || run.style.small_caps {
                run.text.to_uppercase()
            } else {
                run.text.clone()
            };
            let len = display.len();
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
            let is_link = run.hyperlink.is_some();
            if run.style.underline || is_link {
                builder.push(StyleProperty::Underline(true), offset..end);
            }
            if run.style.strikethrough {
                builder.push(StyleProperty::Strikethrough(true), offset..end);
            }
            let base_pt = run
                .style
                .font_size_pt
                .unwrap_or(default_pt);
            let (mut effective_pt, baseline_shift) = if run.style.superscript {
                (base_pt * 0.65, -base_pt * 0.4)
            } else if run.style.subscript {
                (base_pt * 0.65, base_pt * 0.2)
            } else {
                (base_pt, 0.0)
            };
            // Approximate small-caps: uppercase glyphs at ~80% size when caps is off.
            if run.style.small_caps && !run.style.all_caps {
                effective_pt *= 0.8;
            }
            if run.style.font_size_pt.is_some()
                || run.style.superscript
                || run.style.subscript
                || (run.style.small_caps && !run.style.all_caps)
            {
                builder.push(StyleProperty::FontSize(effective_pt), offset..end);
            }
            if let Some(ref family) = run.style.font_family {
                builder.push(
                    StyleProperty::FontFamily(FontFamily::named(family.as_str())),
                    offset..end,
                );
            }
            // Word-default link blue when the run has no explicit colour.
            const LINK_BLUE: [u8; 3] = [0x05, 0x63, 0xC1];
            let link_color = is_link.then_some(LINK_BLUE);
            let paint_color = run.style.color.or(link_color);
            if run.style.highlight
                || paint_color.is_some()
                || baseline_shift != 0.0
                || run.style.vanish
                || run.style.shadow
            {
                let mut brush = ColorBrush::default();
                if let Some([r, g, b]) = paint_color {
                    brush.r = r;
                    brush.g = g;
                    brush.b = b;
                }
                brush.highlight = run.style.highlight;
                brush.baseline_shift = baseline_shift * scale;
                brush.shadow = run.style.shadow;
                if run.style.vanish {
                    // Keep hidden text editable/visible in Preview as a faint ghost.
                    brush.a = 72;
                }
                builder.push(StyleProperty::Brush(brush), offset..end);
            }
            offset = end;
        }

        let mut layout = builder.build(&text);
        apply_paragraph_text_indent(&mut layout, p);
        layout.break_all_lines(Some(max_width.max(1.0)));
        layout.align(parley_alignment(p.alignment), AlignmentOptions::default());
        layout
    }

    /// Rasterise an entire document into an RGBA8 page image.
    ///
    /// `selection` is a plain-text UTF-8 byte range (`start == end` draws a caret).
    #[must_use]
    pub fn render_document(
        &mut self,
        doc: &Document,
        content_width: f32,
    ) -> (Arc<Vec<u8>>, u32, u32) {
        self.render_document_with_selection(doc, content_width, None)
    }

    /// Like [`Self::render_document`], with optional selection / caret overlay.
    ///
    /// Rasterises at [`PREVIEW_RENDER_SCALE`] device pixels per CSS pixel so text
    /// stays sharp when Slint displays the image at logical size on HiDPI screens.
    #[must_use]
    pub fn render_document_with_selection(
        &mut self,
        doc: &Document,
        content_width: f32,
        selection: Option<(usize, usize)>,
    ) -> (Arc<Vec<u8>>, u32, u32) {
        let scale = PREVIEW_RENDER_SCALE;
        let base_insets = PreviewInsets::from_page_setup(&doc.page_setup);
        let insets = PreviewInsets {
            left: base_insets.left * scale,
            right: base_insets.right * scale,
            top: base_insets.top * scale,
            bottom: base_insets.bottom * scale,
        };
        let max_w = content_width.max(80.0) * scale;
        // Content-relative Y (padding applied at paint time) — hit-test stays at scale 1.
        let mut layouts: Vec<LaidBlock> = Vec::new();
        let mut grids: Vec<TableGridGeom> = Vec::new();
        let mut total_h = 0.0;
        let para_gap = 10.0 * scale;
        let mut plain_offset = 0usize;
        let mut emitted_text = false;
        let max_preview_h = MAX_PREVIEW_HEIGHT as f32 * scale;

        for block in &doc.blocks {
            match block {
                Block::Paragraph(p) => {
                    if emitted_text {
                        plain_offset += 1;
                    }
                    emitted_text = true;
                    let mut rule_y = None;
                    if p.page_break_before {
                        let page_break_gap = 28.0 * scale;
                        rule_y = Some(total_h + 10.0 * scale);
                        total_h += page_break_gap;
                    }
                    total_h += twips_to_css_px(p.space_before_twips) * scale;
                    let body_len = p.plain_text().len();
                    let prefix_len = list_prefix(p).len();
                    let indent = list_indent_px(p) * scale;
                    let layout = self.layout_paragraph(
                        p,
                        (max_w - indent - paragraph_right_indent_px(p) * scale).max(12.0 * scale),
                        scale,
                    );
                    let h = layout.height().max(16.0 * scale);
                    layouts.push(LaidBlock {
                        layout,
                        y0: total_h,
                        x0: 0.0,
                        indent_px: indent,
                        plain_start: plain_offset,
                        body_len,
                        prefix_len,
                        is_image: false,
                        image_h: 0.0,
                        image_w: 0,
                        image_rgba: None,
                        page_break_rule_y: rule_y,
                        shade_fill: p.shade_fill,
                        shade_w: max_w,
                        border_sides: p.border_sides,
                    });
                    plain_offset += body_len;
                    let after = (twips_to_css_px(p.space_after_twips) * scale).max(para_gap);
                    total_h += h + after;
                }
                Block::Table(t) => {
                    let grid = self.append_table_grid(
                        t,
                        max_w,
                        &mut total_h,
                        &mut plain_offset,
                        &mut emitted_text,
                        &mut layouts,
                        scale,
                    );
                    grids.push(grid);
                    total_h += para_gap;
                }
                Block::Image(img) => {
                    let (rgba, w, h_px) = prepare_preview_image(img, max_w);
                    let h = (h_px as f32).max(24.0 * scale);
                    layouts.push(LaidBlock {
                        layout: Layout::default(),
                        y0: total_h,
                        x0: 0.0,
                        indent_px: 0.0,
                        plain_start: plain_offset,
                        body_len: 0,
                        prefix_len: 0,
                        is_image: true,
                        image_h: h,
                        image_w: w,
                        image_rgba: rgba,
                        page_break_rule_y: None,
                        shade_fill: None,
                        shade_w: 0.0,
                        border_sides: 0,
                    });
                    total_h += h + para_gap;
                }
            }
            if total_h > max_preview_h {
                break;
            }
        }

        let width = (max_w + insets.left + insets.right).ceil() as u32;
        let height = (total_h + insets.top + insets.bottom)
            .ceil()
            .clamp(64.0 * scale, max_preview_h) as u32;
        let mut pixels = vec![255u8; (width as usize) * (height as usize) * 4];

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

        for item in &layouts {
            if let Some(ry) = item.page_break_rule_y {
                let y = (insets.top + ry).round() as i32;
                if y >= 0 && (y as u32) < height {
                    let x0 = insets.left.round() as i32;
                    let x1 = (insets.left + max_w).round() as i32;
                    paint_dashed_h_line(
                        &mut pixels,
                        width,
                        height,
                        x0,
                        x1,
                        y,
                        [180, 180, 190, 255],
                    );
                }
            }
        }

        for grid in &grids {
            paint_table_grid(&mut pixels, width, height, insets.left, insets.top, grid);
        }

        // Paragraph shading under selection / glyphs.
        for item in &layouts {
            if item.is_image {
                continue;
            }
            let Some([r, g, b]) = item.shade_fill else {
                continue;
            };
            let h = if item.layout.is_empty() {
                16.0
            } else {
                item.layout.height().max(16.0)
            };
            let w = item.shade_w.max(1.0);
            fill_rect(
                &mut pixels,
                width,
                height,
                (insets.left + item.x0).round() as u32,
                (insets.top + item.y0).round() as u32,
                w.round() as u32,
                h.round() as u32,
                [r, g, b, 255],
            );
        }

        let (sel_lo, sel_hi) = match selection {
            Some((a, b)) if a != b => (a.min(b), a.max(b)),
            _ => (0, 0),
        };
        let caret_at = match selection {
            Some((a, b)) if a == b => Some(a),
            _ => None,
        };

        // Selection / caret under the glyphs.
        for item in &layouts {
            if item.is_image || item.layout.is_empty() {
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
                        &mut pixels,
                        width,
                        height,
                        origin_x,
                        origin_y,
                        layout_lo,
                        layout_hi,
                    );
                }
            }
            if let Some(caret) = caret_at {
                let para_end = item.plain_start + item.body_len;
                if caret >= item.plain_start && caret <= para_end {
                    let layout_idx = item.prefix_len + (caret - item.plain_start);
                    paint_caret(
                        &item.layout,
                        &mut pixels,
                        width,
                        height,
                        origin_x,
                        origin_y,
                        layout_idx,
                    );
                }
            }
        }

        for item in &layouts {
            if item.is_image {
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
                if caret_at == Some(item.plain_start) {
                    fill_rect(
                        &mut pixels,
                        width,
                        height,
                        x.saturating_sub(1),
                        y.saturating_sub(1),
                        box_w.saturating_add(2),
                        box_h.saturating_add(2),
                        [180, 210, 255, 255],
                    );
                }
                if let (Some(rgba), true) = (&item.image_rgba, item.image_w > 0) {
                    blit_rgba(
                        &mut pixels,
                        (width, height),
                        (x, y),
                        (item.image_w, box_h.max(1)),
                        rgba,
                    );
                } else {
                    fill_rect(
                        &mut pixels,
                        width,
                        height,
                        x,
                        y,
                        (max_w as u32)
                            .saturating_sub(item.x0 as u32)
                            .saturating_sub(8)
                            .max(24),
                        box_h,
                        [220, 220, 228, 255],
                    );
                }
                continue;
            }
            if item.layout.is_empty() {
                continue;
            }
            render_layout_at(
                &mut self.scale_cx,
                &item.layout,
                &mut pixels,
                width,
                height,
                insets.left + item.x0 + item.indent_px,
                insets.top + item.y0,
            );
        }

        for item in &layouts {
            if item.is_image || item.border_sides == 0 {
                continue;
            }
            let body_h = if item.layout.is_empty() {
                16.0 * scale
            } else {
                item.layout.height().max(16.0 * scale)
            };
            let x0 = (insets.left + item.x0).round() as i32;
            let x1 = (insets.left + item.x0 + item.shade_w.max(1.0)).round() as i32;
            let y0 = (insets.top + item.y0).round() as i32;
            let y1 = (insets.top + item.y0 + body_h).round() as i32;
            if item.border_sides & crate::document::model::CELL_BORDER_TOP != 0 {
                paint_solid_h_line(&mut pixels, width, height, x0, x1, y0, PARA_BORDER_COLOR);
            }
            if item.border_sides & crate::document::model::CELL_BORDER_BOTTOM != 0 {
                paint_solid_h_line(&mut pixels, width, height, x0, x1, y1, PARA_BORDER_COLOR);
            }
            if item.border_sides & crate::document::model::CELL_BORDER_LEFT != 0 {
                paint_solid_v_line(&mut pixels, width, height, x0, y0, y1, PARA_BORDER_COLOR);
            }
            if item.border_sides & crate::document::model::CELL_BORDER_RIGHT != 0 {
                paint_solid_v_line(&mut pixels, width, height, x1, y0, y1, PARA_BORDER_COLOR);
            }
        }

        (Arc::new(pixels), width, height)
    }

    /// Map a point in preview-image CSS pixels to a document [`Cursor`].
    ///
    /// Coordinates are relative to the top-left of the rendered page (including padding).
    #[must_use]
    pub fn hit_test_cursor(
        &mut self,
        doc: &Document,
        content_width: f32,
        x: f32,
        y: f32,
    ) -> Option<Cursor> {
        let insets = PreviewInsets::from_page_setup(&doc.page_setup);
        let max_w = content_width.max(80.0);
        let local_x = x - insets.left;
        let local_y = y - insets.top;
        let para_gap = 10.0;

        let mut total_h = 0.0;
        let mut plain_offset = 0usize;
        let mut emitted_text = false;

        if local_y < 0.0 {
            return Some(Cursor::default());
        }

        for (bi, block) in doc.blocks.iter().enumerate() {
            match block {
                Block::Paragraph(p) => {
                    if emitted_text {
                        plain_offset += 1;
                    }
                    emitted_text = true;
                    if p.page_break_before {
                        total_h += 28.0;
                    }
                    total_h += twips_to_css_px(p.space_before_twips);
                    let body_len = p.plain_text().len();
                    let prefix_len = list_prefix(p).len();
                    let indent = list_indent_px(p);
                    let wrap_w = paragraph_wrap_width(max_w, p);
                    let layout = self.layout_paragraph(p, wrap_w, 1.0);
                    let h = layout.height().max(16.0);
                    let y0 = total_h;
                    let after = twips_to_css_px(p.space_after_twips).max(para_gap);
                    let y1 = total_h + h + after;
                    if local_y >= y0 && local_y < y1 {
                        let ly = (local_y - y0).max(0.0);
                        let lx = (local_x - indent).clamp(0.0, wrap_w);
                        let body_idx = cluster_to_body_index(&layout, lx, ly, prefix_len, body_len);
                        return Some(cursor_from_plain_offset(doc, plain_offset + body_idx));
                    }
                    plain_offset += body_len;
                    total_h += h + after;
                }
                Block::Table(t) => {
                    if let Some(cursor) = self.hit_test_table_cursor(
                        doc,
                        bi,
                        t,
                        max_w,
                        local_x,
                        local_y,
                        &mut total_h,
                        &mut plain_offset,
                        &mut emitted_text,
                    ) {
                        return Some(cursor);
                    }
                    total_h += para_gap;
                }
                Block::Image(img) => {
                    let (_, h_px) = preview_image_display_size(img, max_w);
                    let h = (h_px as f32).max(24.0);
                    let y0 = total_h;
                    let y1 = total_h + h + para_gap;
                    if local_y >= y0 && local_y < y1 {
                        return Some(Cursor::at(bi, 0, 0));
                    }
                    total_h += h + para_gap;
                }
            }
            if total_h > MAX_PREVIEW_HEIGHT as f32 {
                break;
            }
        }
        Some(cursor_from_plain_offset(doc, plain_offset))
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
        self.hit_test_cursor(doc, content_width, x, y)
            .map(|cursor| plain_offset_from_cursor(doc, cursor))
    }

    /// Image-space Y (CSS px, including page top inset) for a plain-text byte offset.
    ///
    /// Used to scroll the Preview `Flickable` to a Find match. Returns the top of the
    /// containing paragraph / cell item (line-precise Y is not required for MVP).
    #[must_use]
    pub fn y_for_plain_offset(
        &mut self,
        doc: &Document,
        content_width: f32,
        target: usize,
    ) -> f32 {
        let insets = PreviewInsets::from_page_setup(&doc.page_setup);
        let max_w = content_width.max(80.0);
        let para_gap = 10.0;
        let mut total_h = 0.0;
        let mut plain_offset = 0usize;
        let mut emitted_text = false;

        for block in &doc.blocks {
            match block {
                Block::Paragraph(p) => {
                    if emitted_text {
                        plain_offset += 1;
                    }
                    emitted_text = true;
                    if p.page_break_before {
                        total_h += 28.0;
                    }
                    total_h += twips_to_css_px(p.space_before_twips);
                    let body_len = p.plain_text().len();
                    let y0 = total_h;
                    let wrap_w = paragraph_wrap_width(max_w, p);
                    let layout = self.layout_paragraph(p, wrap_w, 1.0);
                    let h = layout.height().max(16.0);
                    if target >= plain_offset && target <= plain_offset + body_len {
                        return insets.top + y0;
                    }
                    plain_offset += body_len;
                    total_h += h + twips_to_css_px(p.space_after_twips).max(para_gap);
                }
                Block::Table(t) => {
                    let range_start = plain_offset;
                    let measured =
                        self.measure_table(t, max_w, &mut plain_offset, &mut emitted_text, 1.0);
                    let table_y0 = total_h;
                    let row_y0s = row_origins(table_y0, &measured.row_heights);
                    if target >= range_start && target <= plain_offset {
                        for (ri, mrow) in measured.rows.iter().enumerate() {
                            for &(ci, col0, colspan) in &mrow.placements {
                                let Some(cell) = t.rows.get(ri).and_then(|r| r.cells.get(ci))
                                else {
                                    continue;
                                };
                                if is_vmerge_continue(cell) {
                                    continue;
                                }
                                let rowspan = vmerge_rowspan(t, ri, col0).max(1);
                                let (_x0, _w, y0, _h) = cell_rect(
                                    &measured.col_widths,
                                    &row_y0s,
                                    &measured.row_heights,
                                    col0,
                                    colspan,
                                    ri,
                                    rowspan,
                                );
                                let mut y = y0 + TABLE_CELL_PAD;
                                if let Some(items) = mrow.items.get(ci) {
                                    for item in items {
                                        let item_h = item.height();
                                        let in_item = match item {
                                            CellItemLayout::Para {
                                                plain_start,
                                                body_len,
                                                ..
                                            } => {
                                                target >= *plain_start
                                                    && target <= plain_start + body_len
                                            }
                                            CellItemLayout::Image { plain_start, .. } => {
                                                target == *plain_start
                                            }
                                        };
                                        if in_item {
                                            return insets.top + y;
                                        }
                                        y += item_h + TABLE_CELL_PARA_GAP;
                                    }
                                }
                            }
                        }
                        return insets.top + table_y0;
                    }
                    total_h += measured.row_heights.iter().sum::<f32>() + para_gap;
                }
                Block::Image(img) => {
                    let (_, h_px) = preview_image_display_size(img, max_w);
                    let h = (h_px as f32).max(24.0);
                    total_h += h + para_gap;
                }
            }
        }
        insets.top + total_h
    }

    /// Lay out a table on the `tblGrid` / `gridSpan` / `vMerge` geometry.
    fn append_table_grid(
        &mut self,
        t: &Table,
        max_w: f32,
        total_h: &mut f32,
        plain_offset: &mut usize,
        emitted_text: &mut bool,
        layouts: &mut Vec<LaidBlock>,
        scale: f32,
    ) -> TableGridGeom {
        let mut measured = self.measure_table(t, max_w, plain_offset, emitted_text, scale);
        let table_y0 = *total_h;
        let row_y0s = row_origins(table_y0, &measured.row_heights);
        let mut cell_rects = Vec::new();
        let pad = TABLE_CELL_PAD * scale;
        let gap = TABLE_CELL_PARA_GAP * scale;

        for (ri, mrow) in measured.rows.iter_mut().enumerate() {
            let placements = mrow.placements.clone();
            for (ci, col0, colspan) in placements {
                let Some(cell) = t.rows.get(ri).and_then(|r| r.cells.get(ci)) else {
                    continue;
                };
                if is_vmerge_continue(cell) {
                    continue;
                }
                let rowspan = vmerge_rowspan(t, ri, col0).max(1);
                let (x0, w, y0, h) = cell_rect(
                    &measured.col_widths,
                    &row_y0s,
                    &measured.row_heights,
                    col0,
                    colspan,
                    ri,
                    rowspan,
                );
                cell_rects.push(TableCellRect {
                    x0,
                    y0,
                    w,
                    h,
                    shade_fill: cell.shade_fill,
                    border_sides: cell.border_sides,
                });
                let x_pad = x0 + pad;
                let mut y = y0 + pad;
                let items = mrow
                    .items
                    .get_mut(ci)
                    .map(std::mem::take)
                    .unwrap_or_default();
                for item in items {
                    let item_h = item.height();
                    match item {
                        CellItemLayout::Para {
                            layout,
                            plain_start,
                            body_len,
                            prefix_len,
                            indent_px,
                            shade_fill,
                            shade_w,
                            border_sides,
                            ..
                        } => {
                            layouts.push(LaidBlock {
                                layout,
                                y0: y,
                                x0: x_pad,
                                indent_px,
                                plain_start,
                                body_len,
                                prefix_len,
                                is_image: false,
                                image_h: 0.0,
                                image_w: 0,
                                image_rgba: None,
                                page_break_rule_y: None,
                                shade_fill,
                                shade_w,
                                border_sides,
                            });
                        }
                        CellItemLayout::Image {
                            plain_start,
                            image_w,
                            image_rgba,
                            ..
                        } => {
                            layouts.push(LaidBlock {
                                layout: Layout::default(),
                                y0: y,
                                x0: x_pad,
                                indent_px: 0.0,
                                plain_start,
                                body_len: 0,
                                prefix_len: 0,
                                is_image: true,
                                image_h: item_h,
                                image_w,
                                image_rgba,
                                page_break_rule_y: None,
                                shade_fill: None,
                                shade_w: 0.0,
                                border_sides: 0,
                            });
                        }
                    }
                    y += item_h + gap;
                }
            }
        }

        *total_h = table_y0 + measured.row_heights.iter().sum::<f32>();
        TableGridGeom {
            y0: table_y0,
            width: max_w,
            height: *total_h - table_y0,
            ncols: measured.ncols,
            col_widths: measured.col_widths,
            row_heights: measured.row_heights,
            cell_rects,
        }
    }

    /// Hit-test inside a table grid; advances `total_h` / plain offsets like layout.
    fn hit_test_table_cursor(
        &mut self,
        doc: &Document,
        block_idx: usize,
        t: &Table,
        max_w: f32,
        local_x: f32,
        local_y: f32,
        total_h: &mut f32,
        plain_offset: &mut usize,
        emitted_text: &mut bool,
    ) -> Option<Cursor> {
        let measured = self.measure_table(t, max_w, plain_offset, emitted_text, 1.0);
        let table_y0 = *total_h;
        let row_y0s = row_origins(table_y0, &measured.row_heights);
        *total_h = table_y0 + measured.row_heights.iter().sum::<f32>();

        let mut hit: Option<(usize, usize, usize, usize, f32, f32, f32)> = None;
        // (row, cell_idx, col0, colspan, x0, y0, w)
        for (ri, mrow) in measured.rows.iter().enumerate() {
            for &(ci, col0, colspan) in &mrow.placements {
                let Some(cell) = t.rows.get(ri).and_then(|r| r.cells.get(ci)) else {
                    continue;
                };
                if is_vmerge_continue(cell) {
                    continue;
                }
                let rowspan = vmerge_rowspan(t, ri, col0).max(1);
                let (x0, w, y0, h) = cell_rect(
                    &measured.col_widths,
                    &row_y0s,
                    &measured.row_heights,
                    col0,
                    colspan,
                    ri,
                    rowspan,
                );
                if local_x >= x0 && local_x < x0 + w && local_y >= y0 && local_y < y0 + h {
                    hit = Some((ri, ci, col0, colspan, x0, y0, w));
                    break;
                }
            }
            if hit.is_some() {
                break;
            }
        }

        let (row_idx, cell_idx, _col0, _colspan, x0, y0, cell_w) = hit?;
        let items = measured
            .rows
            .get(row_idx)
            .and_then(|r| r.items.get(cell_idx))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if items.is_empty() {
            let offset = measured
                .rows
                .iter()
                .take(row_idx + 1)
                .flat_map(|r| r.items.iter().flatten())
                .last()
                .map(|p| p.plain_start() + p.body_len())
                .unwrap_or(*plain_offset);
            return Some(cursor_from_plain_offset(doc, offset));
        }
        let x_pad = x0 + TABLE_CELL_PAD;
        let mut y = y0 + TABLE_CELL_PAD;
        for (i, item) in items.iter().enumerate() {
            let y1 = y + item.height() + TABLE_CELL_PARA_GAP;
            let last = i + 1 == items.len();
            if local_y < y1 || last {
                return match item {
                    CellItemLayout::Image {
                        image_idx,
                        after_paragraph,
                        ..
                    } => Some(Cursor::on_cell_image(
                        block_idx,
                        row_idx,
                        cell_idx,
                        *after_paragraph,
                        *image_idx,
                    )),
                    CellItemLayout::Para {
                        layout,
                        plain_start,
                        body_len,
                        prefix_len,
                        indent_px,
                        ..
                    } => {
                        let ly = (local_y - y).max(0.0);
                        let inner_w = (cell_w - TABLE_CELL_PAD * 2.0).max(16.0);
                        let lx = (local_x - x_pad - indent_px)
                            .clamp(0.0, (inner_w - indent_px).max(12.0));
                        let body_idx =
                            cluster_to_body_index(layout, lx, ly, *prefix_len, *body_len);
                        Some(cursor_from_plain_offset(doc, plain_start + body_idx))
                    }
                };
            }
            y = y1;
        }
        None
    }

    fn measure_table(
        &mut self,
        t: &Table,
        max_w: f32,
        plain_offset: &mut usize,
        emitted_text: &mut bool,
        scale: f32,
    ) -> MeasuredTable {
        let scale = scale.max(0.5);
        let pad = TABLE_CELL_PAD * scale;
        let gap = TABLE_CELL_PARA_GAP * scale;
        let ncols = table_grid_column_count(t);
        let col_widths = table_column_widths_px(t, max_w, ncols);
        let mut rows = Vec::with_capacity(t.rows.len());
        let mut content_hs: Vec<Vec<f32>> = Vec::with_capacity(t.rows.len());

        for row in &t.rows {
            let placements = assign_row_columns(row, ncols);
            let mut items = Vec::with_capacity(row.cells.len());
            let mut heights = Vec::with_capacity(row.cells.len());
            for (ci, cell) in row.cells.iter().enumerate() {
                let (col0, span) = placements
                    .iter()
                    .find(|p| p.0 == ci)
                    .map(|p| (p.1, p.2))
                    .unwrap_or((0, 1));
                let cell_w = col_widths.iter().skip(col0).take(span).sum::<f32>();
                let inner_w = (cell_w - pad * 2.0).max(16.0 * scale);
                let cell_items =
                    self.layout_cell_items(cell, inner_w, plain_offset, emitted_text, scale);
                let mut content_h: f32 = cell_items.iter().map(|i| i.height() + gap).sum();
                if content_h > 0.0 {
                    content_h -= gap;
                }
                if is_vmerge_continue(cell) {
                    content_h = 0.0;
                }
                heights.push(content_h);
                items.push(cell_items);
            }
            content_hs.push(heights);
            rows.push(MeasuredRow { placements, items });
        }

        let mut row_heights: Vec<f32> = content_hs
            .iter()
            .map(|hs| (hs.iter().copied().fold(0.0f32, f32::max) + pad * 2.0).max(20.0 * scale))
            .collect();
        if row_heights.is_empty() {
            row_heights.push(20.0 * scale);
        }

        for (ri, row) in t.rows.iter().enumerate() {
            let Some(mrow) = rows.get(ri) else {
                continue;
            };
            for &(ci, col0, _) in &mrow.placements {
                let Some(cell) = row.cells.get(ci) else {
                    continue;
                };
                if is_vmerge_continue(cell) {
                    continue;
                }
                let rowspan = vmerge_rowspan(t, ri, col0).max(1);
                let needed = content_hs
                    .get(ri)
                    .and_then(|h| h.get(ci))
                    .copied()
                    .unwrap_or(0.0)
                    + pad * 2.0;
                let end = (ri + rowspan).min(row_heights.len());
                let have: f32 = row_heights[ri..end].iter().sum();
                if needed > have && end > ri {
                    row_heights[end - 1] += needed - have;
                }
            }
        }

        MeasuredTable {
            ncols,
            col_widths,
            row_heights,
            rows,
        }
    }

    fn layout_cell_items(
        &mut self,
        cell: &crate::document::model::TableCell,
        inner_w: f32,
        plain_offset: &mut usize,
        emitted_text: &mut bool,
        scale: f32,
    ) -> Vec<CellItemLayout> {
        let scale = scale.max(0.5);
        let mut items = Vec::new();
        if cell.paragraphs.is_empty() {
            for (image_idx, cell_img) in cell.images.iter().enumerate() {
                let (rgba, w, h_px) = prepare_preview_image(&cell_img.image, inner_w);
                let h = (h_px as f32).max(24.0 * scale);
                items.push(CellItemLayout::Image {
                    plain_start: *plain_offset,
                    image_idx,
                    after_paragraph: cell_img.after_paragraph,
                    height: h,
                    image_w: w,
                    image_rgba: rgba,
                });
            }
            return items;
        }

        for (pi, p) in cell.paragraphs.iter().enumerate() {
            if *emitted_text {
                *plain_offset += 1;
            }
            *emitted_text = true;
            let body_len = p.plain_text().len();
            let prefix_len = list_prefix(p).len();
            let indent = list_indent_px(p) * scale;
            let wrap_w =
                (inner_w - indent - paragraph_right_indent_px(p) * scale).max(12.0 * scale);
            let layout = self.layout_paragraph(p, wrap_w, scale);
            let h = layout.height().max(14.0 * scale);
            let plain_start = *plain_offset;
            items.push(CellItemLayout::Para {
                layout,
                plain_start,
                body_len,
                prefix_len,
                indent_px: indent,
                height: h,
                shade_fill: p.shade_fill,
                shade_w: inner_w,
                border_sides: p.border_sides,
            });
            *plain_offset += body_len;
            let after_text = *plain_offset;
            for (image_idx, cell_img) in cell.images.iter().enumerate() {
                if cell_img.after_paragraph != pi {
                    continue;
                }
                let (rgba, w, h_px) = prepare_preview_image(&cell_img.image, inner_w);
                let ih = (h_px as f32).max(24.0 * scale);
                items.push(CellItemLayout::Image {
                    plain_start: after_text,
                    image_idx,
                    after_paragraph: pi,
                    height: ih,
                    image_w: w,
                    image_rgba: rgba,
                });
            }
        }
        // Orphan images (bad indices) go at the end.
        let last = cell.paragraphs.len().saturating_sub(1);
        for (image_idx, cell_img) in cell.images.iter().enumerate() {
            if cell_img.after_paragraph <= last {
                continue;
            }
            let (rgba, w, h_px) = prepare_preview_image(&cell_img.image, inner_w);
            let ih = (h_px as f32).max(24.0 * scale);
            items.push(CellItemLayout::Image {
                plain_start: *plain_offset,
                image_idx,
                after_paragraph: cell_img.after_paragraph,
                height: ih,
                image_w: w,
                image_rgba: rgba,
            });
        }
        items
    }
}

fn cell_colspan(cell: &TableCell) -> usize {
    cell.grid_span.unwrap_or(1).max(1) as usize
}

fn is_vmerge_continue(cell: &TableCell) -> bool {
    matches!(cell.v_merge, Some(VMerge::Continue))
}

fn table_grid_column_count(t: &Table) -> usize {
    let from_grid = t.column_widths_twips.len();
    let from_rows = t
        .rows
        .iter()
        .map(|r| r.cells.iter().map(cell_colspan).sum::<usize>())
        .max()
        .unwrap_or(1);
    from_grid.max(from_rows).max(1)
}

fn assign_row_columns(row: &TableRow, ncols: usize) -> Vec<(usize, usize, usize)> {
    let mut col = 0usize;
    let mut out = Vec::new();
    for (i, cell) in row.cells.iter().enumerate() {
        if col >= ncols {
            break;
        }
        let span = cell_colspan(cell).min(ncols - col);
        out.push((i, col, span));
        col += span;
    }
    out
}

fn cell_at_grid_col(row: &TableRow, col: usize) -> Option<&TableCell> {
    let mut c = 0usize;
    for cell in &row.cells {
        let span = cell_colspan(cell);
        if col >= c && col < c + span {
            return Some(cell);
        }
        c += span;
    }
    None
}

fn vmerge_rowspan(t: &Table, row_idx: usize, col0: usize) -> usize {
    let Some(row) = t.rows.get(row_idx) else {
        return 1;
    };
    let Some(cell) = cell_at_grid_col(row, col0) else {
        return 1;
    };
    if is_vmerge_continue(cell) {
        return 0;
    }
    if !matches!(cell.v_merge, Some(VMerge::Restart)) {
        return 1;
    }
    let mut n = 1;
    for r in (row_idx + 1)..t.rows.len() {
        match cell_at_grid_col(&t.rows[r], col0) {
            Some(c) if is_vmerge_continue(c) => n += 1,
            _ => break,
        }
    }
    n
}

fn row_origins(table_y0: f32, row_heights: &[f32]) -> Vec<f32> {
    let mut y = table_y0;
    let mut out = Vec::with_capacity(row_heights.len());
    for &h in row_heights {
        out.push(y);
        y += h;
    }
    out
}

fn cell_rect(
    col_widths: &[f32],
    row_y0s: &[f32],
    row_heights: &[f32],
    col0: usize,
    colspan: usize,
    row0: usize,
    rowspan: usize,
) -> (f32, f32, f32, f32) {
    let x0 = col_widths.iter().take(col0).sum::<f32>();
    let w = col_widths
        .iter()
        .skip(col0)
        .take(colspan.max(1))
        .sum::<f32>();
    let y0 = row_y0s.get(row0).copied().unwrap_or(0.0);
    let h = row_heights
        .iter()
        .skip(row0)
        .take(rowspan.max(1))
        .sum::<f32>();
    (x0, w, y0, h)
}

/// Pixel widths for each column. Falls back to equal split when `column_widths_twips`
/// is empty, all-zero, or length-mismatched vs `ncols`.
fn table_column_widths_px(t: &Table, max_w: f32, ncols: usize) -> Vec<f32> {
    let equal = || (0..ncols).map(|_| max_w / ncols as f32).collect();
    if ncols == 0 {
        return Vec::new();
    }
    if t.column_widths_twips.is_empty() {
        return equal();
    }
    let mut twips: Vec<u32> = t.column_widths_twips.iter().copied().take(ncols).collect();
    while twips.len() < ncols {
        twips.push(twips.last().copied().unwrap_or(1).max(1));
    }
    let sum: u64 = twips.iter().map(|&w| u64::from(w.max(1))).sum();
    if sum == 0 {
        return equal();
    }
    twips
        .iter()
        .map(|&w| max_w * (w.max(1) as f32) / sum as f32)
        .collect()
}

fn paint_table_grid(
    pixels: &mut [u8],
    buf_w: u32,
    buf_h: u32,
    pad_x: f32,
    pad_y: f32,
    grid: &TableGridGeom,
) {
    let x = pad_x.round() as u32;
    let y = (pad_y + grid.y0).round() as u32;
    let w = grid.width.ceil().max(1.0) as u32;
    let h = grid.height.ceil().max(1.0) as u32;

    // Outer box.
    fill_rect(pixels, buf_w, buf_h, x, y, w, 1, TABLE_GRID_COLOR);
    fill_rect(
        pixels,
        buf_w,
        buf_h,
        x,
        y.saturating_add(h.saturating_sub(1)),
        w,
        1,
        TABLE_GRID_COLOR,
    );
    fill_rect(pixels, buf_w, buf_h, x, y, 1, h, TABLE_GRID_COLOR);
    fill_rect(
        pixels,
        buf_w,
        buf_h,
        x.saturating_add(w.saturating_sub(1)),
        y,
        1,
        h,
        TABLE_GRID_COLOR,
    );

    if grid.cell_rects.is_empty() {
        let mut x_cursor = 0.0f32;
        for c in 0..grid.ncols.saturating_sub(1) {
            x_cursor += grid.col_widths.get(c).copied().unwrap_or(0.0);
            let vx = (pad_x + x_cursor).round() as u32;
            fill_rect(pixels, buf_w, buf_h, vx, y, 1, h, TABLE_GRID_COLOR);
        }
        let mut yy = y;
        for (i, rh) in grid.row_heights.iter().enumerate() {
            if i > 0 {
                fill_rect(pixels, buf_w, buf_h, x, yy, w, 1, TABLE_GRID_COLOR);
            }
            yy = yy.saturating_add(rh.ceil().max(1.0) as u32);
        }
        return;
    }

    for rect in &grid.cell_rects {
        let cx = (pad_x + rect.x0).round() as u32;
        let cy = (pad_y + rect.y0).round() as u32;
        let cw = rect.w.ceil().max(1.0) as u32;
        let ch = rect.h.ceil().max(1.0) as u32;
        if let Some([r, g, b]) = rect.shade_fill {
            fill_rect(pixels, buf_w, buf_h, cx, cy, cw, ch, [r, g, b, 255]);
        }
        let top = if rect.border_sides & crate::document::model::CELL_BORDER_TOP != 0 {
            PARA_BORDER_COLOR
        } else {
            TABLE_GRID_COLOR
        };
        let bottom = if rect.border_sides & crate::document::model::CELL_BORDER_BOTTOM != 0 {
            PARA_BORDER_COLOR
        } else {
            TABLE_GRID_COLOR
        };
        let left = if rect.border_sides & crate::document::model::CELL_BORDER_LEFT != 0 {
            PARA_BORDER_COLOR
        } else {
            TABLE_GRID_COLOR
        };
        let right = if rect.border_sides & crate::document::model::CELL_BORDER_RIGHT != 0 {
            PARA_BORDER_COLOR
        } else {
            TABLE_GRID_COLOR
        };
        fill_rect(pixels, buf_w, buf_h, cx, cy, cw, 1, top);
        fill_rect(
            pixels,
            buf_w,
            buf_h,
            cx,
            cy.saturating_add(ch.saturating_sub(1)),
            cw,
            1,
            bottom,
        );
        fill_rect(pixels, buf_w, buf_h, cx, cy, 1, ch, left);
        fill_rect(
            pixels,
            buf_w,
            buf_h,
            cx.saturating_add(cw.saturating_sub(1)),
            cy,
            1,
            ch,
            right,
        );
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

struct LaidBlock {
    layout: Layout<ColorBrush>,
    /// Content-relative top (excluding page padding).
    y0: f32,
    /// Content-relative left (table cells offset into their column).
    x0: f32,
    /// Extra left inset for list indent level.
    indent_px: f32,
    plain_start: usize,
    body_len: usize,
    prefix_len: usize,
    is_image: bool,
    image_h: f32,
    image_w: u32,
    image_rgba: Option<Vec<u8>>,
    /// Content-relative Y of a page-break hairline, if this block starts a new page.
    page_break_rule_y: Option<f32>,
    /// Paragraph `w:shd` fill (RGB); painted under selection/glyphs.
    shade_fill: Option<[u8; 3]>,
    /// Content-relative width available for paragraph shade (body column or cell).
    shade_w: f32,
    /// Paragraph borders (`w:pBdr`); same bit layout as cell borders.
    border_sides: u8,
}

enum CellItemLayout {
    Para {
        layout: Layout<ColorBrush>,
        plain_start: usize,
        body_len: usize,
        prefix_len: usize,
        indent_px: f32,
        height: f32,
        shade_fill: Option<[u8; 3]>,
        shade_w: f32,
        border_sides: u8,
    },
    Image {
        /// Caret offset when the image is clicked (end of preceding text).
        plain_start: usize,
        /// Index into [`TableCell::images`](crate::document::model::TableCell::images).
        image_idx: usize,
        /// Paragraph after which this image is anchored.
        after_paragraph: usize,
        height: f32,
        image_w: u32,
        image_rgba: Option<Vec<u8>>,
    },
}

impl CellItemLayout {
    fn height(&self) -> f32 {
        match self {
            Self::Para { height, .. } | Self::Image { height, .. } => *height,
        }
    }

    fn plain_start(&self) -> usize {
        match self {
            Self::Para { plain_start, .. } | Self::Image { plain_start, .. } => *plain_start,
        }
    }

    fn body_len(&self) -> usize {
        match self {
            Self::Para { body_len, .. } => *body_len,
            Self::Image { .. } => 0,
        }
    }
}

struct TableGridGeom {
    y0: f32,
    width: f32,
    height: f32,
    ncols: usize,
    col_widths: Vec<f32>,
    row_heights: Vec<f32>,
    /// Visible (non-`vMerge` continue) cell boxes, content-relative.
    cell_rects: Vec<TableCellRect>,
}

struct TableCellRect {
    x0: f32,
    y0: f32,
    w: f32,
    h: f32,
    shade_fill: Option<[u8; 3]>,
    border_sides: u8,
}

struct MeasuredTable {
    ncols: usize,
    col_widths: Vec<f32>,
    row_heights: Vec<f32>,
    rows: Vec<MeasuredRow>,
}

struct MeasuredRow {
    /// `(cell_idx, grid_col0, colspan)`
    placements: Vec<(usize, usize, usize)>,
    items: Vec<Vec<CellItemLayout>>,
}

const TABLE_CELL_PAD: f32 = 4.0;
const TABLE_CELL_PARA_GAP: f32 = 2.0;
const TABLE_GRID_COLOR: [u8; 4] = [180, 180, 188, 255];
const PARA_BORDER_COLOR: [u8; 4] = [120, 120, 128, 255];

const MAX_PREVIEW_IMAGE_H: u32 = 240;

fn preview_image_display_size(img: &crate::document::model::InlineImage, max_w: f32) -> (u32, u32) {
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
fn prepare_preview_image(
    img: &crate::document::model::InlineImage,
    max_w: f32,
) -> (Option<Vec<u8>>, u32, u32) {
    let (disp_w, disp_h) = preview_image_display_size(img, max_w);
    let Ok(decoded) = image::load_from_memory(&img.bytes) else {
        return (None, disp_w, disp_h);
    };
    let rgba = decoded.to_rgba8();
    let (src_w, src_h) = rgba.dimensions();
    if disp_w == src_w && disp_h == src_h {
        return (Some(rgba.into_raw()), disp_w, disp_h);
    }
    let resized =
        image::imageops::resize(&rgba, disp_w, disp_h, image::imageops::FilterType::Triangle);
    (Some(resized.into_raw()), disp_w, disp_h)
}

fn blit_rgba(
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

const SELECTION_FILL: [u8; 4] = [147, 197, 253, 140]; // soft blue
const CARET_FILL: [u8; 4] = [37, 99, 235, 220];

fn paint_selection_range(
    layout: &Layout<ColorBrush>,
    pixels: &mut [u8],
    buf_w: u32,
    buf_h: u32,
    origin_x: f32,
    origin_y: f32,
    layout_lo: usize,
    layout_hi: usize,
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
                    SELECTION_FILL,
                );
            }
        }
    }
}

fn paint_caret(
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

fn fill_rect_blend(
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


fn outline_level_font_pt(level: u8) -> f32 {
    match level {
        0 => 24.0,
        1 => 20.0,
        2 => 16.0,
        3 => 14.0,
        4 => 13.0,
        _ => 12.0,
    }
}

fn list_prefix(p: &Paragraph) -> String {
    match p.list {
        ListKind::None => String::new(),
        ListKind::Bullet => "• ".into(),
        ListKind::Numbered => "1. ".into(),
    }
}

fn list_indent_px(p: &Paragraph) -> f32 {
    let list = if p.list == ListKind::None {
        0.0
    } else {
        f32::from(p.list_level) * LIST_INDENT_PX
    };
    list + paragraph_left_indent_px(p)
}

fn paragraph_right_indent_px(p: &Paragraph) -> f32 {
    twips_to_css_px(p.indent_right_twips)
}

/// Available wrap width after left (incl. list) and right paragraph indents.
fn paragraph_wrap_width(max_w: f32, p: &Paragraph) -> f32 {
    (max_w - list_indent_px(p) - paragraph_right_indent_px(p)).max(40.0)
}

/// Left edge of the paragraph box after applying `w:ind` left/hanging.
fn paragraph_left_indent_px(p: &Paragraph) -> f32 {
    let left = p.indent_left_twips;
    let fl = p.indent_first_line_twips;
    let base = if fl < 0 {
        left.saturating_sub(fl.unsigned_abs())
    } else {
        left
    };
    twips_to_css_px(base)
}

fn apply_paragraph_text_indent(layout: &mut Layout<ColorBrush>, p: &Paragraph) {
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
            layout: dl.layout_paragraph(p, width, 1.0),
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
                    render_glyph_run(
                        scale_cx, &glyph_run, pixels, width, height, origin_x, origin_y,
                    );
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

fn blend_pixel(pixels: &mut [u8], buf_w: u32, x: u32, y: u32, r: u8, g: u8, b: u8, a: u8) {
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

fn twips_to_css_px(twips: u32) -> f32 {
    // 1440 twips = 1 inch = 96 CSS px.
    (twips as f32) * 96.0 / 1440.0
}

fn paragraph_line_height(p: &Paragraph) -> LineHeight {
    match p.line_spacing_rule {
        LineSpacingRule::Auto => {
            let rel = if p.line_spacing == 0 {
                1.35
            } else {
                (p.line_spacing as f32 / 240.0).clamp(0.5, 4.0)
            };
            LineHeight::FontSizeRelative(rel)
        }
        LineSpacingRule::Exact => {
            LineHeight::Absolute(twips_to_css_px(p.line_spacing).max(1.0))
        }
        // Word "at least": floor line box to the given height. Approximate with
        // Absolute max(value, typical default line box).
        LineSpacingRule::AtLeast => {
            let min_h = twips_to_css_px(p.line_spacing).max(1.0);
            LineHeight::Absolute(min_h.max(14.0 * 1.2))
        }
    }
}

fn paint_dashed_h_line(
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

fn paint_solid_h_line(
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


fn paint_solid_v_line(
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
    use crate::document::model::{
        CellImage, ImageFormat, InlineImage, LineSpacingRule, Run, RunStyle, Table, TableCell,
        TableRow, VMerge, CELL_BORDER_ALL,
    };

    fn sample_paragraph() -> Paragraph {
        Paragraph {
            runs: vec![Run {
                text: "Hello layout".into(),
                style: RunStyle {
                    bold: true,
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn layout_paragraph_has_lines() {
        let mut dl = DocumentLayout::new();
        let layout = dl.layout_paragraph(&sample_paragraph(), 400.0, 1.0);
        assert!(!layout.is_empty());
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
        let layout = dl.layout_paragraph(&sample_paragraph(), 200.0, 1.0);
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
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                Block::Paragraph(Paragraph {
                    runs: vec![Run {
                        text: "Second line here".into(),
                        style: RunStyle::default(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
            ],
            ..Default::default()
        };
        // Click near the top of the second paragraph (padding + first para height + gap).
        let offset = dl
            .hit_test_plain_offset(&doc, 400.0, PreviewInsets::default_letter().left + 8.0, PreviewInsets::default_letter().left + 40.0)
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
            .hit_test_plain_offset(&doc, 400.0, PreviewInsets::default_letter().left + 2.0, PreviewInsets::default_letter().left + 2.0)
            .unwrap();
        assert_eq!(offset, 0);
    }

    #[test]
    fn selection_paint_tints_pixels() {
        let mut dl = DocumentLayout::new();
        let doc = Document {
            blocks: vec![Block::Paragraph(sample_paragraph())],
            ..Default::default()
        };
        let (plain, _, _) = dl.render_document(&doc, 400.0);
        let (selected, w, h) = dl.render_document_with_selection(&doc, 400.0, Some((0, 5)));
        assert_eq!(plain.len(), selected.len());
        assert!(
            plain.as_slice() != selected.as_slice(),
            "selection should change the preview pixels ({w}x{h})"
        );
        // Selection blue channel should appear somewhere.
        assert!(
            selected
                .chunks_exact(4)
                .any(|px| px[2] > px[0] && px[2] > 180),
            "expected bluish selection tint"
        );
    }

    fn cell(text: &str) -> TableCell {
        TableCell::from_paragraphs(vec![Paragraph {
            runs: vec![Run {
                text: text.into(),
                style: RunStyle::default(),
                ..Default::default()
            }],
            ..Default::default()
        }])
    }

    fn table_2x2_doc() -> Document {
        Document {
            blocks: vec![Block::Table(Table {
                rows: vec![
                    TableRow {
                        cells: vec![cell("AA"), cell("BB")],
                    },
                    TableRow {
                        cells: vec![cell("CC"), cell("DD")],
                    },
                ],
                ..Default::default()
            })],
            ..Default::default()
        }
    }

    #[test]
    fn paragraph_border_paints_preview_line() {
        let mut dl = DocumentLayout::new();
        let mut doc = Document::default();
        doc.blocks.push(Block::Paragraph(Paragraph {
            runs: vec![Run {
                text: "Line".into(),
                ..Default::default()
            }],
            border_sides: CELL_BORDER_ALL,
            ..Default::default()
        }));
        let content_w = 400.0;
        let (bytes, w, h) = dl.render_document(&doc, content_w);
        assert!(w > 100 && h > 40);
        let s = PREVIEW_RENDER_SCALE;
        let insets = PreviewInsets::default_letter();
        let x_lo = ((insets.left + 8.0) * s).round() as u32;
        let x_hi = ((insets.left + content_w - 8.0) * s).round() as u32;
        let y_lo = ((insets.top + 8.0) * s).round() as u32;
        let y_hi = ((insets.top + 80.0) * s).round() as u32;
        let mut found = false;
        'scan: for vy in y_lo..y_hi.min(h) {
            for vx in x_lo..x_hi.min(w) {
                let i = ((vy as usize) * (w as usize) + (vx as usize)) * 4;
                if bytes[i] == PARA_BORDER_COLOR[0]
                    && bytes[i + 1] == PARA_BORDER_COLOR[1]
                    && bytes[i + 2] == PARA_BORDER_COLOR[2]
                {
                    found = true;
                    break 'scan;
                }
            }
        }
        assert!(found, "expected paragraph border pixels in preview band");
    }

    #[test]
    fn table_cell_shade_fills_preview_pixels() {
        let mut dl = DocumentLayout::new();
        let mut doc = table_2x2_doc();
        if let Block::Table(t) = &mut doc.blocks[0] {
            t.rows[0].cells[0].shade_fill = Some([0xFF, 0x00, 0x00]);
        }
        let content_w = 400.0;
        let (bytes, w, h) = dl.render_document(&doc, content_w);
        assert!(w > 100 && h > 40);
        let s = PREVIEW_RENDER_SCALE;
        let insets = PreviewInsets::default_letter();
        // Sample inside the top-left cell (past the hairline border).
        let vx = ((insets.left + 8.0) * s).round() as u32;
        let vy = ((insets.top + 8.0) * s).round() as u32;
        let i = ((vy as usize) * (w as usize) + (vx as usize)) * 4;
        assert!(
            bytes[i] > 200 && bytes[i + 1] < 40 && bytes[i + 2] < 40,
            "expected red cell shade at ({vx},{vy}) got rgba({},{},{},{})",
            bytes[i],
            bytes[i + 1],
            bytes[i + 2],
            bytes[i + 3]
        );
    }

    #[test]
    fn table_cell_border_paints_stronger_edges() {
        use crate::document::model::CELL_BORDER_ALL;

        let mut dl = DocumentLayout::new();
        let mut doc = table_2x2_doc();
        if let Block::Table(t) = &mut doc.blocks[0] {
            t.rows[0].cells[0].border_sides = CELL_BORDER_ALL;
        }
        let content_w = 400.0;
        let (bytes, w, h) = dl.render_document(&doc, content_w);
        assert!(w > 100 && h > 40);
        let s = PREVIEW_RENDER_SCALE;
        let insets = PreviewInsets::default_letter();
        // Top edge of bordered cell (first row; y0 already includes table offset).
        let vx = ((insets.left + 20.0) * s).round() as u32;
        let vy = (insets.top * s).round() as u32;
        let i = ((vy as usize) * (w as usize) + (vx as usize)) * 4;
        assert!(
            bytes[i] == PARA_BORDER_COLOR[0]
                && bytes[i + 1] == PARA_BORDER_COLOR[1]
                && bytes[i + 2] == PARA_BORDER_COLOR[2],
            "expected strong cell border at ({vx},{vy}) got rgba({},{},{},{})",
            bytes[i],
            bytes[i + 1],
            bytes[i + 2],
            bytes[i + 3]
        );
    }

    #[test]
    fn table_grid_paints_vertical_border() {
        let mut dl = DocumentLayout::new();
        let doc = table_2x2_doc();
        let content_w = 400.0;
        let (bytes, w, h) = dl.render_document(&doc, content_w);
        assert!(w > 100 && h > 40);
        let s = PREVIEW_RENDER_SCALE;
        // Mid-column hairline at pad + content_w/2 (device pixels).
        let vx = ((PreviewInsets::default_letter().left + content_w / 2.0) * s).round() as u32;
        let vy = ((PreviewInsets::default_letter().left + 8.0) * s).round() as u32;
        let i = ((vy as usize) * (w as usize) + (vx as usize)) * 4;
        assert!(
            bytes[i] == TABLE_GRID_COLOR[0]
                && bytes[i + 1] == TABLE_GRID_COLOR[1]
                && bytes[i + 2] == TABLE_GRID_COLOR[2],
            "expected grid border at ({vx},{vy}) got rgba({},{},{},{})",
            bytes[i],
            bytes[i + 1],
            bytes[i + 2],
            bytes[i + 3]
        );
    }

    #[test]
    fn uneven_column_widths_shift_vertical_border() {
        let mut dl = DocumentLayout::new();
        let mut doc = table_2x2_doc();
        if let Block::Table(t) = &mut doc.blocks[0] {
            t.column_widths_twips = vec![2000, 6000];
        }
        let content_w = 400.0;
        let (bytes, w, h) = dl.render_document(&doc, content_w);
        assert!(w > 100 && h > 40);
        let s = PREVIEW_RENDER_SCALE;
        // 1:3 split → border at 25% of content width, not 50%.
        let vx = ((PreviewInsets::default_letter().left + content_w * 0.25) * s).round() as u32;
        let vy = ((PreviewInsets::default_letter().left + 8.0) * s).round() as u32;
        let i = ((vy as usize) * (w as usize) + (vx as usize)) * 4;
        assert!(
            bytes[i] == TABLE_GRID_COLOR[0]
                && bytes[i + 1] == TABLE_GRID_COLOR[1]
                && bytes[i + 2] == TABLE_GRID_COLOR[2],
            "expected uneven grid border at ({vx},{vy}) got rgba({},{},{},{})",
            bytes[i],
            bytes[i + 1],
            bytes[i + 2],
            bytes[i + 3]
        );
        // Equal-split midpoint should not be a vertical border.
        let mid = (PreviewInsets::default_letter().left + content_w / 2.0).round() as u32;
        let mi = ((vy as usize) * (w as usize) + (mid as usize)) * 4;
        assert!(
            !(bytes[mi] == TABLE_GRID_COLOR[0]
                && bytes[mi + 1] == TABLE_GRID_COLOR[1]
                && bytes[mi + 2] == TABLE_GRID_COLOR[2]),
            "midpoint should not be a column border for 1:3 widths"
        );
    }

    #[test]
    fn uneven_column_hit_test_uses_widths() {
        let mut dl = DocumentLayout::new();
        let mut doc = table_2x2_doc();
        if let Block::Table(t) = &mut doc.blocks[0] {
            t.column_widths_twips = vec![2000, 6000];
        }
        let plain = doc.plain_text();
        let aa = plain.find("AA").expect("AA");
        let bb = plain.find("BB").expect("BB");
        let content_w = 400.0;
        let y = PreviewInsets::default_letter().left + TABLE_CELL_PAD + 4.0;
        // 20% into table → narrow left column.
        let left = dl
            .hit_test_plain_offset(&doc, content_w, PreviewInsets::default_letter().left + content_w * 0.2, y)
            .unwrap();
        assert!(
            left >= aa && left <= aa + "AA".len(),
            "left hit offset={left} expected AA"
        );
        // 70% into table → wide right column.
        let right = dl
            .hit_test_plain_offset(&doc, content_w, PreviewInsets::default_letter().left + content_w * 0.7, y)
            .unwrap();
        assert!(
            right >= bb && right <= bb + "BB".len(),
            "right hit offset={right} expected BB"
        );
    }

    #[test]
    fn table_hit_test_right_column() {
        let mut dl = DocumentLayout::new();
        let doc = table_2x2_doc();
        let plain = doc.plain_text();
        let bb = plain.find("BB").expect("BB");
        let content_w = 400.0;
        // Click near the left edge of the right column, top row.
        let x = PreviewInsets::default_letter().left + content_w * 0.75;
        let y = PreviewInsets::default_letter().left + TABLE_CELL_PAD + 4.0;
        let offset = dl.hit_test_plain_offset(&doc, content_w, x, y).unwrap();
        assert!(
            offset >= bb && offset <= bb + "BB".len(),
            "offset={offset} expected in BB at {bb}..{}; plain={plain:?}",
            bb + "BB".len()
        );
    }

    #[test]
    fn table_hit_test_left_column() {
        let mut dl = DocumentLayout::new();
        let doc = table_2x2_doc();
        let plain = doc.plain_text();
        let aa = plain.find("AA").expect("AA");
        let content_w = 400.0;
        let x = PreviewInsets::default_letter().left + content_w * 0.25;
        let y = PreviewInsets::default_letter().left + TABLE_CELL_PAD + 4.0;
        let offset = dl.hit_test_plain_offset(&doc, content_w, x, y).unwrap();
        assert!(
            offset >= aa && offset <= aa + "AA".len(),
            "offset={offset} expected in AA at {aa}..{}",
            aa + "AA".len()
        );
    }

    fn is_grid_px(bytes: &[u8], w: u32, x: u32, y: u32) -> bool {
        let i = ((y as usize) * (w as usize) + (x as usize)) * 4;
        bytes[i] == TABLE_GRID_COLOR[0]
            && bytes[i + 1] == TABLE_GRID_COLOR[1]
            && bytes[i + 2] == TABLE_GRID_COLOR[2]
    }

    #[test]
    fn grid_span_covers_two_columns() {
        let mut dl = DocumentLayout::new();
        let doc = Document {
            blocks: vec![Block::Table(Table {
                rows: vec![
                    TableRow {
                        cells: vec![{
                            let mut c = cell("WIDE");
                            c.grid_span = Some(2);
                            c
                        }],
                    },
                    TableRow {
                        cells: vec![cell("L"), cell("R")],
                    },
                ],
                ..Default::default()
            })],
            ..Default::default()
        };
        let content_w = 400.0;
        let (bytes, w, h) = dl.render_document(&doc, content_w);
        assert!(w > 100 && h > 40);
        let vx = (PreviewInsets::default_letter().left + content_w / 2.0).round() as u32;
        let vy = (PreviewInsets::default_letter().left + TABLE_CELL_PAD + 4.0).round() as u32;
        assert!(
            !is_grid_px(&bytes, w, vx, vy),
            "spanned first row should not have a mid-column border"
        );
        let plain = doc.plain_text();
        let wide = plain.find("WIDE").expect("WIDE");
        let offset = dl
            .hit_test_plain_offset(
                &doc,
                content_w,
                PreviewInsets::default_letter().left + content_w * 0.75,
                PreviewInsets::default_letter().left + TABLE_CELL_PAD + 4.0,
            )
            .unwrap();
        assert!(
            offset >= wide && offset <= wide + "WIDE".len(),
            "right half of spanned cell should still hit WIDE, offset={offset}"
        );
    }

    #[test]
    fn vmerge_skips_internal_horizontal_border() {
        let mut dl = DocumentLayout::new();
        let doc = Document {
            blocks: vec![Block::Table(Table {
                rows: vec![
                    TableRow {
                        cells: vec![
                            {
                                let mut c = cell("TOP");
                                c.v_merge = Some(VMerge::Restart);
                                c
                            },
                            cell("R1"),
                        ],
                    },
                    TableRow {
                        cells: vec![
                            TableCell {
                                paragraphs: vec![Paragraph::default()],
                                v_merge: Some(VMerge::Continue),
                                ..Default::default()
                            },
                            cell("R2"),
                        ],
                    },
                ],
                ..Default::default()
            })],
            ..Default::default()
        };
        let content_w = 400.0;
        let (bytes, w, h) = dl.render_document(&doc, content_w);
        assert!(w > 100 && h > 50);
        // Interior of the merged left cell, around the old row split.
        let x = (PreviewInsets::default_letter().left + content_w * 0.25).round() as u32;
        let mut grid_rows = 0usize;
        for y in 0..h {
            if is_grid_px(&bytes, w, x, y) {
                grid_rows += 1;
            }
        }
        // Outer top + bottom only — not a third rule through the merge.
        assert!(
            grid_rows <= 4,
            "merged column should not paint a mid-row rule (grid rows={grid_rows})"
        );
        let plain = doc.plain_text();
        let top = plain.find("TOP").expect("TOP");
        let lower = dl
            .hit_test_plain_offset(
                &doc,
                content_w,
                PreviewInsets::default_letter().left + content_w * 0.2,
                PreviewInsets::default_letter().left + 36.0,
            )
            .unwrap();
        assert!(
            lower >= top && lower <= top + "TOP".len(),
            "click in continue slot should hit the restart cell, offset={lower}"
        );
    }

    #[test]
    fn table_cell_image_appears_in_preview() {
        let mut png = Vec::new();
        {
            let img = image::RgbaImage::from_pixel(8, 8, image::Rgba([220, 30, 30, 255]));
            img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .unwrap();
        }
        let mut dl = DocumentLayout::new();
        let doc = Document {
            blocks: vec![Block::Table(Table {
                rows: vec![TableRow {
                    cells: vec![
                        TableCell {
                            paragraphs: vec![Paragraph {
                                runs: vec![Run {
                                    text: "Pic".into(),
                                    style: RunStyle::default(),
                                    ..Default::default()
                                }],
                                ..Default::default()
                            }],
                            images: vec![CellImage {
                                after_paragraph: 0,
                                image: InlineImage {
                                    bytes: png,
                                    format: ImageFormat::Png,
                                    width_px: 8,
                                    height_px: 8,
                                    r_id: None,
                                    part_path: None,
                                },
                            }],
                            ..Default::default()
                        },
                        cell("Right"),
                    ],
                }],
                ..Default::default()
            })],
            ..Default::default()
        };
        let (bytes, w, h) = dl.render_document(&doc, 400.0);
        assert!(w > 100 && h > 40);
        assert!(
            bytes
                .chunks_exact(4)
                .any(|px| px[0] > 180 && px[1] < 80 && px[2] < 80),
            "expected red cell-image pixels in {w}x{h} preview"
        );
    }

    #[test]
    fn table_cell_image_hit_test_selects_image_cursor() {
        let mut png = Vec::new();
        {
            let img = image::RgbaImage::from_pixel(8, 8, image::Rgba([220, 30, 30, 255]));
            img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .unwrap();
        }
        let mut dl = DocumentLayout::new();
        let doc = Document {
            blocks: vec![Block::Table(Table {
                rows: vec![TableRow {
                    cells: vec![TableCell {
                        paragraphs: vec![Paragraph {
                            runs: vec![Run {
                                text: "Pic".into(),
                                style: RunStyle::default(),
                                ..Default::default()
                            }],
                            ..Default::default()
                        }],
                        images: vec![CellImage {
                            after_paragraph: 0,
                            image: InlineImage {
                                bytes: png,
                                format: ImageFormat::Png,
                                width_px: 8,
                                height_px: 8,
                                r_id: None,
                                part_path: None,
                            },
                        }],
                        ..Default::default()
                    }],
                }],
                ..Default::default()
            })],
            ..Default::default()
        };
        let content_w = 400.0;
        // Click below the paragraph where the cell image is laid out.
        let x = PreviewInsets::default_letter().left + content_w * 0.25;
        let y = PreviewInsets::default_letter().left + TABLE_CELL_PAD + 28.0;
        let cursor = dl.hit_test_cursor(&doc, content_w, x, y).expect("hit");
        assert_eq!(
            cursor.cell.and_then(|c| c.image_idx),
            Some(0),
            "expected cell image cursor"
        );
    }

    #[test]
    fn preview_honors_asymmetric_page_margins() {
        let mut dl = DocumentLayout::new();
        let mut page_setup = PageSetup::default();
        page_setup.margin_left_twips = 720; // 0.5″
        page_setup.margin_right_twips = 2160; // 1.5″
        page_setup.margin_top_twips = 480; // 1/3″
        page_setup.margin_bottom_twips = 960; // 2/3″
        let doc = Document {
            blocks: vec![Block::Paragraph(sample_paragraph())],
            page_setup,
            ..Default::default()
        };
        let content_w = 400.0;
        let insets = PreviewInsets::from_page_setup(&doc.page_setup);
        assert!((insets.left - 48.0).abs() < 0.1);
        assert!((insets.right - 144.0).abs() < 0.1);
        assert!((insets.top - 32.0).abs() < 0.1);
        assert!((insets.bottom - 64.0).abs() < 0.1);

        let (_, w, h) = dl.render_document(&doc, content_w);
        let s = PREVIEW_RENDER_SCALE;
        assert_eq!(
            w,
            ((content_w + insets.left + insets.right) * s).ceil() as u32
        );
        assert!(h as f32 >= (insets.top + insets.bottom + 16.0) * s);

        let offset = dl
            .hit_test_plain_offset(
                &doc,
                content_w,
                insets.left + 2.0,
                insets.top + 2.0,
            )
            .unwrap();
        assert_eq!(offset, 0);
    }

    #[test]
    fn line_spacing_auto_increases_layout_height() {
        let mut dl = DocumentLayout::new();
        let text = "Line one wraps here when narrow.\nLine two also wraps.";
        let single = Paragraph {
            runs: vec![Run {
                text: text.into(),
                style: RunStyle::default(),
                ..Default::default()
            }],
            line_spacing: 240,
            ..Default::default()
        };
        let double = Paragraph {
            line_spacing: 480,
            ..single.clone()
        };
        let h1 = dl.layout_paragraph(&single, 120.0, 1.0).height();
        let h2 = dl.layout_paragraph(&double, 120.0, 1.0).height();
        assert!(
            h2 > h1 * 1.3,
            "double spacing should be clearly taller: single={h1} double={h2}"
        );
    }

    #[test]
    fn line_spacing_exact_sets_absolute_height() {
        let mut dl = DocumentLayout::new();
        let text = "One line only.";
        let auto = Paragraph {
            runs: vec![Run {
                text: text.into(),
                style: RunStyle::default(),
                ..Default::default()
            }],
            line_spacing: 240,
            line_spacing_rule: LineSpacingRule::Auto,
            ..Default::default()
        };
        let exact = Paragraph {
            line_spacing: 720, // 0.5″ ≈ 48 CSS px
            line_spacing_rule: LineSpacingRule::Exact,
            ..auto.clone()
        };
        let h_auto = dl.layout_paragraph(&auto, 400.0, 1.0).height();
        let h_exact = dl.layout_paragraph(&exact, 400.0, 1.0).height();
        assert!(
            h_exact > h_auto + 10.0,
            "exact 720 twips should be taller than single auto: auto={h_auto} exact={h_exact}"
        );
    }

    #[test]
    fn paragraph_left_indent_narrows_layout_width() {
        let mut dl = DocumentLayout::new();
        let long = "Word ".repeat(40);
        let flush = Paragraph {
            runs: vec![Run {
                text: long.clone(),
                style: RunStyle::default(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let indented = Paragraph {
            indent_left_twips: 2880, // 2″ → ~192 CSS px less width
            ..flush.clone()
        };
        let h_flush = dl.layout_paragraph(&flush, 400.0, 1.0).height();
        // Same content_width budget as document layout: max_w - indent.
        let indent = list_indent_px(&indented);
        let h_ind = dl
            .layout_paragraph(&indented, (400.0 - indent).max(40.0), 1.0)
            .height();
        assert!(
            h_ind > h_flush,
            "2″ left indent should wrap to more lines: flush={h_flush} indented={h_ind}"
        );
    }
}
