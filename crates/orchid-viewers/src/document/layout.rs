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

use crate::document::cursor::{cursor_from_plain_offset, plain_offset_from_cursor, Cursor};
use crate::document::model::{Alignment, Block, Document, ListKind, Paragraph, Table};

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
/// Extra left inset per list indent level (`w:ilvl`).
pub const LIST_INDENT_PX: f32 = 24.0;
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
    #[must_use]
    pub fn render_document_with_selection(
        &mut self,
        doc: &Document,
        content_width: f32,
        selection: Option<(usize, usize)>,
    ) -> (Arc<Vec<u8>>, u32, u32) {
        let pad = PREVIEW_PADDING;
        let max_w = content_width.max(80.0);
        // Content-relative Y (padding applied at paint time) — must match hit-test.
        let mut layouts: Vec<LaidBlock> = Vec::new();
        let mut grids: Vec<TableGridGeom> = Vec::new();
        let mut total_h = 0.0;
        let para_gap = 10.0;
        let mut plain_offset = 0usize;
        let mut emitted_text = false;

        for block in &doc.blocks {
            match block {
                Block::Paragraph(p) => {
                    if emitted_text {
                        plain_offset += 1;
                    }
                    emitted_text = true;
                    let body_len = p.plain_text().len();
                    let prefix_len = list_prefix(p).len();
                    let indent = list_indent_px(p);
                    let layout = self.layout_paragraph(p, (max_w - indent).max(40.0));
                    let h = layout.height().max(16.0);
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
                    });
                    plain_offset += body_len;
                    total_h += h + para_gap;
                }
                Block::Table(t) => {
                    let grid = self.append_table_grid(
                        t,
                        max_w,
                        &mut total_h,
                        &mut plain_offset,
                        &mut emitted_text,
                        &mut layouts,
                    );
                    grids.push(grid);
                    total_h += para_gap;
                }
                Block::Image(img) => {
                    let (rgba, w, h_px) = prepare_preview_image(img, max_w);
                    let h = (h_px as f32).max(24.0);
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
                    });
                    total_h += h + para_gap;
                }
            }
            if total_h > MAX_PREVIEW_HEIGHT as f32 {
                break;
            }
        }

        let width = (max_w + pad * 2.0).ceil() as u32;
        let height = (total_h + pad * 2.0)
            .ceil()
            .clamp(64.0, MAX_PREVIEW_HEIGHT as f32) as u32;
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

        for grid in &grids {
            paint_table_grid(&mut pixels, width, height, pad, grid);
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
            if item.is_image || item.layout.len() == 0 {
                continue;
            }
            let origin_x = pad + item.x0 + item.indent_px;
            let origin_y = pad + item.y0;
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
                let y = (pad + item.y0) as u32;
                let x = (pad + item.x0) as u32;
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
            if item.layout.len() == 0 {
                continue;
            }
            render_layout_at(
                &mut self.scale_cx,
                &item.layout,
                &mut pixels,
                width,
                height,
                pad + item.x0 + item.indent_px,
                pad + item.y0,
            );
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
        let pad = PREVIEW_PADDING;
        let max_w = content_width.max(80.0);
        let local_x = x - pad;
        let local_y = y - pad;
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
                    let body_len = p.plain_text().len();
                    let prefix_len = list_prefix(p).len();
                    let indent = list_indent_px(p);
                    let layout = self.layout_paragraph(p, (max_w - indent).max(40.0));
                    let h = layout.height().max(16.0);
                    let y0 = total_h;
                    let y1 = total_h + h + para_gap;
                    if local_y >= y0 && local_y < y1 {
                        let ly = (local_y - y0).max(0.0);
                        let lx = (local_x - indent).clamp(0.0, (max_w - indent).max(40.0));
                        let body_idx = cluster_to_body_index(&layout, lx, ly, prefix_len, body_len);
                        return Some(cursor_from_plain_offset(doc, plain_offset + body_idx));
                    }
                    plain_offset += body_len;
                    total_h += h + para_gap;
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

    /// Lay out a table as an equal-width column grid; appends [`LaidBlock`]s and returns geometry.
    fn append_table_grid(
        &mut self,
        t: &Table,
        max_w: f32,
        total_h: &mut f32,
        plain_offset: &mut usize,
        emitted_text: &mut bool,
        layouts: &mut Vec<LaidBlock>,
    ) -> TableGridGeom {
        let ncols = table_column_count(t);
        let cell_w = max_w / ncols as f32;
        let table_y0 = *total_h;
        let mut row_heights = Vec::with_capacity(t.rows.len());

        for row in &t.rows {
            let row_y0 = *total_h;
            let mut cell_items: Vec<Vec<CellItemLayout>> = Vec::with_capacity(ncols);
            let mut max_content_h = 0.0f32;

            for ci in 0..ncols {
                let inner_w = (cell_w - TABLE_CELL_PAD * 2.0).max(16.0);
                let items = if let Some(cell) = row.cells.get(ci) {
                    self.layout_cell_items(cell, inner_w, plain_offset, emitted_text)
                } else {
                    Vec::new()
                };
                let mut content_h: f32 = items
                    .iter()
                    .map(|i| i.height() + TABLE_CELL_PARA_GAP)
                    .sum();
                if content_h > 0.0 {
                    content_h -= TABLE_CELL_PARA_GAP;
                }
                max_content_h = max_content_h.max(content_h);
                cell_items.push(items);
            }

            let row_h = (max_content_h + TABLE_CELL_PAD * 2.0).max(20.0);
            for (ci, items) in cell_items.into_iter().enumerate() {
                let x0 = ci as f32 * cell_w + TABLE_CELL_PAD;
                let mut y = row_y0 + TABLE_CELL_PAD;
                for item in items {
                    let h = item.height();
                    match item {
                        CellItemLayout::Para {
                            layout,
                            plain_start,
                            body_len,
                            prefix_len,
                            indent_px,
                            ..
                        } => {
                            layouts.push(LaidBlock {
                                layout,
                                y0: y,
                                x0,
                                indent_px,
                                plain_start,
                                body_len,
                                prefix_len,
                                is_image: false,
                                image_h: 0.0,
                                image_w: 0,
                                image_rgba: None,
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
                                x0,
                                indent_px: 0.0,
                                plain_start,
                                body_len: 0,
                                prefix_len: 0,
                                is_image: true,
                                image_h: h,
                                image_w,
                                image_rgba,
                            });
                        }
                    }
                    y += h + TABLE_CELL_PARA_GAP;
                }
            }
            *total_h += row_h;
            row_heights.push(row_h);
        }

        TableGridGeom {
            y0: table_y0,
            width: max_w,
            height: *total_h - table_y0,
            ncols,
            row_heights,
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
        let ncols = table_column_count(t);
        let cell_w = max_w / ncols as f32;

        for (row_idx, row) in t.rows.iter().enumerate() {
            let row_y0 = *total_h;
            let mut cell_items: Vec<Vec<CellItemLayout>> = Vec::with_capacity(ncols);
            let mut max_content_h = 0.0f32;

            for ci in 0..ncols {
                let inner_w = (cell_w - TABLE_CELL_PAD * 2.0).max(16.0);
                let items = if let Some(cell) = row.cells.get(ci) {
                    self.layout_cell_items(cell, inner_w, plain_offset, emitted_text)
                } else {
                    Vec::new()
                };
                let mut content_h: f32 = items
                    .iter()
                    .map(|i| i.height() + TABLE_CELL_PARA_GAP)
                    .sum();
                if content_h > 0.0 {
                    content_h -= TABLE_CELL_PARA_GAP;
                }
                max_content_h = max_content_h.max(content_h);
                cell_items.push(items);
            }

            let row_h = (max_content_h + TABLE_CELL_PAD * 2.0).max(20.0);
            let row_y1 = row_y0 + row_h;
            if local_y >= row_y0 && local_y < row_y1 {
                let col = if local_x < 0.0 {
                    0
                } else {
                    ((local_x / cell_w).floor() as usize).min(ncols.saturating_sub(1))
                };
                let x0 = col as f32 * cell_w + TABLE_CELL_PAD;
                let items = &cell_items[col];
                if items.is_empty() {
                    let offset = cell_items
                        .iter()
                        .take(col)
                        .flatten()
                        .last()
                        .map(|p| p.plain_start() + p.body_len())
                        .unwrap_or(*plain_offset);
                    return Some(cursor_from_plain_offset(doc, offset));
                }
                let mut y = row_y0 + TABLE_CELL_PAD;
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
                                col,
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
                                let lx = (local_x - x0 - indent_px)
                                    .clamp(0.0, (inner_w - indent_px).max(12.0));
                                let body_idx = cluster_to_body_index(
                                    layout,
                                    lx,
                                    ly,
                                    *prefix_len,
                                    *body_len,
                                );
                                Some(cursor_from_plain_offset(doc, plain_start + body_idx))
                            }
                        };
                    }
                    y = y1;
                }
            }
            *total_h = row_y1;
        }

        None
    }

    fn layout_cell_items(
        &mut self,
        cell: &crate::document::model::TableCell,
        inner_w: f32,
        plain_offset: &mut usize,
        emitted_text: &mut bool,
    ) -> Vec<CellItemLayout> {
        let mut items = Vec::new();
        if cell.paragraphs.is_empty() {
            for (image_idx, cell_img) in cell.images.iter().enumerate() {
                let (rgba, w, h_px) = prepare_preview_image(&cell_img.image, inner_w);
                let h = (h_px as f32).max(24.0);
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
            let indent = list_indent_px(p);
            let layout = self.layout_paragraph(p, (inner_w - indent).max(12.0));
            let h = layout.height().max(14.0);
            let plain_start = *plain_offset;
            items.push(CellItemLayout::Para {
                layout,
                plain_start,
                body_len,
                prefix_len,
                indent_px: indent,
                height: h,
            });
            *plain_offset += body_len;
            let after_text = *plain_offset;
            for (image_idx, cell_img) in cell.images.iter().enumerate() {
                if cell_img.after_paragraph != pi {
                    continue;
                }
                let (rgba, w, h_px) = prepare_preview_image(&cell_img.image, inner_w);
                let ih = (h_px as f32).max(24.0);
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
            let ih = (h_px as f32).max(24.0);
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

fn table_column_count(t: &Table) -> usize {
    t.rows.iter().map(|r| r.cells.len()).max().unwrap_or(1).max(1)
}

fn paint_table_grid(
    pixels: &mut [u8],
    buf_w: u32,
    buf_h: u32,
    pad: f32,
    grid: &TableGridGeom,
) {
    let x = (pad + 0.0).round() as u32;
    let y = (pad + grid.y0).round() as u32;
    let w = grid.width.ceil().max(1.0) as u32;
    let h = grid.height.ceil().max(1.0) as u32;
    let cell_w = grid.width / grid.ncols as f32;

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

    for c in 1..grid.ncols {
        let vx = (pad + c as f32 * cell_w).round() as u32;
        fill_rect(pixels, buf_w, buf_h, vx, y, 1, h, TABLE_GRID_COLOR);
    }

    let mut yy = y;
    for (i, rh) in grid.row_heights.iter().enumerate() {
        if i > 0 {
            fill_rect(pixels, buf_w, buf_h, x, yy, w, 1, TABLE_GRID_COLOR);
        }
        yy = yy.saturating_add(rh.ceil().max(1.0) as u32);
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
}

enum CellItemLayout {
    Para {
        layout: Layout<ColorBrush>,
        plain_start: usize,
        body_len: usize,
        prefix_len: usize,
        indent_px: f32,
        height: f32,
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
    row_heights: Vec<f32>,
}

const TABLE_CELL_PAD: f32 = 4.0;
const TABLE_CELL_PARA_GAP: f32 = 2.0;
const TABLE_GRID_COLOR: [u8; 4] = [180, 180, 188, 255];

const MAX_PREVIEW_IMAGE_H: u32 = 240;

fn preview_image_display_size(
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
    let resized = image::imageops::resize(
        &rgba,
        disp_w,
        disp_h,
        image::imageops::FilterType::Triangle,
    );
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

fn list_prefix(p: &Paragraph) -> String {
    match p.list {
        ListKind::None => String::new(),
        ListKind::Bullet => "• ".into(),
        ListKind::Numbered => "1. ".into(),
    }
}

fn list_indent_px(p: &Paragraph) -> f32 {
    if p.list == ListKind::None {
        0.0
    } else {
        f32::from(p.list_level) * LIST_INDENT_PX
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
        CellImage, ImageFormat, InlineImage, Run, RunStyle, Table, TableCell, TableRow,
    };

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
    fn table_grid_paints_vertical_border() {
        let mut dl = DocumentLayout::new();
        let doc = table_2x2_doc();
        let content_w = 400.0;
        let (bytes, w, h) = dl.render_document(&doc, content_w);
        assert!(w > 100 && h > 40);
        // Mid-column hairline at pad + content_w/2.
        let vx = (PREVIEW_PADDING + content_w / 2.0).round() as u32;
        let vy = (PREVIEW_PADDING + 8.0).round() as u32;
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
    fn table_hit_test_right_column() {
        let mut dl = DocumentLayout::new();
        let doc = table_2x2_doc();
        let plain = doc.plain_text();
        let bb = plain.find("BB").expect("BB");
        let content_w = 400.0;
        // Click near the left edge of the right column, top row.
        let x = PREVIEW_PADDING + content_w * 0.75;
        let y = PREVIEW_PADDING + TABLE_CELL_PAD + 4.0;
        let offset = dl
            .hit_test_plain_offset(&doc, content_w, x, y)
            .unwrap();
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
        let x = PREVIEW_PADDING + content_w * 0.25;
        let y = PREVIEW_PADDING + TABLE_CELL_PAD + 4.0;
        let offset = dl
            .hit_test_plain_offset(&doc, content_w, x, y)
            .unwrap();
        assert!(
            offset >= aa && offset <= aa + "AA".len(),
            "offset={offset} expected in AA at {aa}..{}",
            aa + "AA".len()
        );
    }

    #[test]
    fn table_cell_image_appears_in_preview() {
        let mut png = Vec::new();
        {
            let img = image::RgbaImage::from_pixel(8, 8, image::Rgba([220, 30, 30, 255]));
            img.write_to(
                &mut std::io::Cursor::new(&mut png),
                image::ImageFormat::Png,
            )
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
            bytes.chunks_exact(4).any(|px| px[0] > 180 && px[1] < 80 && px[2] < 80),
            "expected red cell-image pixels in {w}x{h} preview"
        );
    }

    #[test]
    fn table_cell_image_hit_test_selects_image_cursor() {
        let mut png = Vec::new();
        {
            let img = image::RgbaImage::from_pixel(8, 8, image::Rgba([220, 30, 30, 255]));
            img.write_to(
                &mut std::io::Cursor::new(&mut png),
                image::ImageFormat::Png,
            )
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
                    }],
                }],
                ..Default::default()
            })],
            ..Default::default()
        };
        let content_w = 400.0;
        // Click below the paragraph where the cell image is laid out.
        let x = PREVIEW_PADDING + content_w * 0.25;
        let y = PREVIEW_PADDING + TABLE_CELL_PAD + 28.0;
        let cursor = dl
            .hit_test_cursor(&doc, content_w, x, y)
            .expect("hit");
        assert_eq!(
            cursor.cell.and_then(|c| c.image_idx),
            Some(0),
            "expected cell image cursor"
        );
    }
}
