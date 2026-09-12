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

use super::flow::*;
use super::paint::*;
use super::*;

impl DocumentLayout {
    /// Layout and paint header/footer paragraphs into the page margin band.
    pub(super) fn paint_margin_story(
        &mut self,
        paragraph_styles: &HashMap<String, NamedParagraphStyle>,
        character_styles: &HashMap<String, NamedCharacterStyle>,
        paragraphs: &[crate::document::model::Paragraph],
        max_w: f32,
        origin_x: f32,
        origin_y: f32,
        scale: f32,
        pixels: &mut [u8],
        width: u32,
        height: u32,
    ) {
        let mut y = origin_y;
        let gap = 4.0 * scale;
        for p in paragraphs {
            let styled = apply_named_paragraph_style(paragraph_styles, character_styles, p);
            let indent = list_indent_px(&styled) * scale;
            let layout = self.layout_paragraph(
                &styled,
                (max_w - indent - paragraph_right_indent_px(&styled) * scale).max(12.0 * scale),
                scale,
            );
            if !layout.is_empty() {
                render_layout_at(
                    &mut self.scale_cx,
                    &layout,
                    pixels,
                    width,
                    height,
                    origin_x + indent,
                    y,
                );
            }
            y += layout.height().max(12.0 * scale) + gap;
            // Keep margin stories from flooding the body canvas.
            if y > origin_y + 200.0 * scale {
                break;
            }
        }
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
        let section_setups = collect_section_page_setups(doc);
        let union = union_page_setup_margins(&section_setups);
        let insets = PreviewInsets::from_page_setup(&union);
        let max_w = content_width.max(80.0);
        let local_x = x - insets.left;
        let local_y = y - insets.top;
        let para_gap = 10.0;
        let mut section_idx = 0usize;

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
                    let styled = apply_named_paragraph_style(
                        &doc.paragraph_styles,
                        &doc.character_styles,
                        p,
                    );
                    total_h += twips_to_css_px(styled.space_before_twips);
                    let body_len = p.plain_text().len();
                    let prefix_len = list_prefix(&styled).len();
                    let indent = list_indent_px(&styled);
                    let sect = section_setups.get(section_idx).unwrap_or(&doc.page_setup);
                    let (sect_x0, sect_wrap) =
                        section_body_origin_and_width(sect, &union, max_w, 1.0);
                    let wrap_w =
                        (sect_wrap - indent - paragraph_right_indent_px(&styled)).max(12.0);
                    let layout = self.layout_paragraph(&styled, wrap_w, 1.0);
                    let h = layout.height().max(16.0);
                    let y0 = total_h;
                    let after = twips_to_css_px(styled.space_after_twips).max(para_gap);
                    let y1 = total_h + h + after;
                    if local_y >= y0 && local_y < y1 {
                        let ly = (local_y - y0).max(0.0);
                        let lx = (local_x - sect_x0 - indent).clamp(0.0, wrap_w);
                        let body_idx = cluster_to_body_index(&layout, lx, ly, prefix_len, body_len);
                        return Some(cursor_from_plain_offset(doc, plain_offset + body_idx));
                    }
                    plain_offset += body_len;
                    total_h += h + after;
                    if let Some(ref ps) = p.section_properties {
                        section_idx = (section_idx + 1).min(section_setups.len().saturating_sub(1));
                        if section_forces_page_band(ps) {
                            total_h += 28.0;
                        }
                    }
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
    pub fn y_for_plain_offset(&mut self, doc: &Document, content_width: f32, target: usize) -> f32 {
        let section_setups = collect_section_page_setups(doc);
        let union = union_page_setup_margins(&section_setups);
        let insets = PreviewInsets::from_page_setup(&union);
        let max_w = content_width.max(80.0);
        let para_gap = 10.0;
        let mut section_idx = 0usize;
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
                    let styled = apply_named_paragraph_style(
                        &doc.paragraph_styles,
                        &doc.character_styles,
                        p,
                    );
                    total_h += twips_to_css_px(styled.space_before_twips);
                    let body_len = p.plain_text().len();
                    let y0 = total_h;
                    let indent = list_indent_px(&styled);
                    let sect = section_setups.get(section_idx).unwrap_or(&doc.page_setup);
                    let (_sect_x0, sect_wrap) =
                        section_body_origin_and_width(sect, &union, max_w, 1.0);
                    let wrap_w =
                        (sect_wrap - indent - paragraph_right_indent_px(&styled)).max(12.0);
                    let layout = self.layout_paragraph(&styled, wrap_w, 1.0);
                    let h = layout.height().max(16.0);
                    if target >= plain_offset && target <= plain_offset + body_len {
                        return insets.top + y0;
                    }
                    plain_offset += body_len;
                    total_h += h + twips_to_css_px(styled.space_after_twips).max(para_gap);
                    if let Some(ref ps) = p.section_properties {
                        section_idx = (section_idx + 1).min(section_setups.len().saturating_sub(1));
                        if section_forces_page_band(ps) {
                            total_h += 28.0;
                        }
                    }
                }
                Block::Table(t) => {
                    let range_start = plain_offset;
                    let measured = self.measure_table(
                        &doc.paragraph_styles,
                        &doc.character_styles,
                        t,
                        max_w,
                        &mut plain_offset,
                        &mut emitted_text,
                        1.0,
                    );
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
    pub(super) fn append_table_grid(
        &mut self,
        paragraph_styles: &HashMap<String, NamedParagraphStyle>,
        character_styles: &HashMap<String, NamedCharacterStyle>,
        t: &Table,
        max_w: f32,
        total_h: &mut f32,
        plain_offset: &mut usize,
        emitted_text: &mut bool,
        layouts: &mut Vec<LaidBlock>,
        scale: f32,
    ) -> TableGridGeom {
        let mut measured = self.measure_table(
            paragraph_styles,
            character_styles,
            t,
            max_w,
            plain_offset,
            emitted_text,
            scale,
        );
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
    pub(super) fn hit_test_table_cursor(
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
        let measured = self.measure_table(
            &doc.paragraph_styles,
            &doc.character_styles,
            t,
            max_w,
            plain_offset,
            emitted_text,
            1.0,
        );
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

    pub(super) fn measure_table(
        &mut self,
        paragraph_styles: &HashMap<String, NamedParagraphStyle>,
        character_styles: &HashMap<String, NamedCharacterStyle>,
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
                let cell_items = self.layout_cell_items(
                    paragraph_styles,
                    character_styles,
                    cell,
                    inner_w,
                    plain_offset,
                    emitted_text,
                    scale,
                );
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

    pub(super) fn layout_cell_items(
        &mut self,
        paragraph_styles: &HashMap<String, NamedParagraphStyle>,
        character_styles: &HashMap<String, NamedCharacterStyle>,
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
            let styled = apply_named_paragraph_style(paragraph_styles, character_styles, p);
            let body_len = p.plain_text().len();
            let prefix_len = list_prefix(&styled).len();
            let indent = list_indent_px(&styled) * scale;
            let wrap_w =
                (inner_w - indent - paragraph_right_indent_px(&styled) * scale).max(12.0 * scale);
            let layout = self.layout_paragraph(&styled, wrap_w, scale);
            let h = layout.height().max(14.0 * scale);
            let plain_start = *plain_offset;
            items.push(CellItemLayout::Para {
                layout,
                plain_start,
                body_len,
                prefix_len,
                indent_px: indent,
                height: h,
                shade_fill: styled.shade_fill,
                shade_w: inner_w,
                border_sides: styled.border_sides,
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

pub(super) fn cell_colspan(cell: &TableCell) -> usize {
    cell.grid_span.unwrap_or(1).max(1) as usize
}

pub(super) fn is_vmerge_continue(cell: &TableCell) -> bool {
    matches!(cell.v_merge, Some(VMerge::Continue))
}

pub(super) fn table_grid_column_count(t: &Table) -> usize {
    let from_grid = t.column_widths_twips.len();
    let from_rows = t
        .rows
        .iter()
        .map(|r| r.cells.iter().map(cell_colspan).sum::<usize>())
        .max()
        .unwrap_or(1);
    from_grid.max(from_rows).max(1)
}

pub(super) fn assign_row_columns(row: &TableRow, ncols: usize) -> Vec<(usize, usize, usize)> {
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

pub(super) fn cell_at_grid_col(row: &TableRow, col: usize) -> Option<&TableCell> {
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

pub(super) fn vmerge_rowspan(t: &Table, row_idx: usize, col0: usize) -> usize {
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

pub(super) fn row_origins(table_y0: f32, row_heights: &[f32]) -> Vec<f32> {
    let mut y = table_y0;
    let mut out = Vec::with_capacity(row_heights.len());
    for &h in row_heights {
        out.push(y);
        y += h;
    }
    out
}

pub(super) fn cell_rect(
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
pub(super) fn table_column_widths_px(t: &Table, max_w: f32, ncols: usize) -> Vec<f32> {
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

pub(super) fn paint_table_grid(
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

pub(super) fn cluster_to_body_index(
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

pub(super) struct LaidBlock {
    pub(super) layout: Layout<ColorBrush>,
    /// Content-relative top (excluding page padding).
    pub(super) y0: f32,
    /// Content-relative left (table cells offset into their column).
    pub(super) x0: f32,
    /// Extra left inset for list indent level.
    pub(super) indent_px: f32,
    pub(super) plain_start: usize,
    pub(super) body_len: usize,
    pub(super) prefix_len: usize,
    pub(super) is_image: bool,
    pub(super) image_h: f32,
    pub(super) image_w: u32,
    pub(super) image_rgba: Option<Vec<u8>>,
    /// Content-relative Y of a page-break hairline, if this block starts a new page.
    pub(super) page_break_rule_y: Option<f32>,
    /// Paragraph `w:shd` fill (RGB); painted under selection/glyphs.
    pub(super) shade_fill: Option<[u8; 3]>,
    /// Content-relative width available for paragraph shade (body column or cell).
    pub(super) shade_w: f32,
    /// Paragraph borders (`w:pBdr`); same bit layout as cell borders.
    pub(super) border_sides: u8,
}

pub(super) enum CellItemLayout {
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

pub(super) struct TableGridGeom {
    y0: f32,
    width: f32,
    height: f32,
    ncols: usize,
    col_widths: Vec<f32>,
    row_heights: Vec<f32>,
    /// Visible (non-`vMerge` continue) cell boxes, content-relative.
    cell_rects: Vec<TableCellRect>,
}

pub(super) struct TableCellRect {
    x0: f32,
    y0: f32,
    w: f32,
    h: f32,
    shade_fill: Option<[u8; 3]>,
    border_sides: u8,
}

pub(super) struct MeasuredTable {
    ncols: usize,
    col_widths: Vec<f32>,
    row_heights: Vec<f32>,
    rows: Vec<MeasuredRow>,
}

pub(super) struct MeasuredRow {
    /// `(cell_idx, grid_col0, colspan)`
    placements: Vec<(usize, usize, usize)>,
    items: Vec<Vec<CellItemLayout>>,
}

pub(super) const TABLE_CELL_PAD: f32 = 4.0;

pub(super) const TABLE_CELL_PARA_GAP: f32 = 2.0;

pub(super) const TABLE_GRID_COLOR: [u8; 4] = [180, 180, 188, 255];
pub(super) const PARA_BORDER_COLOR: [u8; 4] = [120, 120, 128, 255];

impl CellItemLayout {
    pub(super) fn height(&self) -> f32 {
        match self {
            Self::Para { height, .. } | Self::Image { height, .. } => *height,
        }
    }

    pub(super) fn plain_start(&self) -> usize {
        match self {
            Self::Para { plain_start, .. } | Self::Image { plain_start, .. } => *plain_start,
        }
    }

    pub(super) fn body_len(&self) -> usize {
        match self {
            Self::Para { body_len, .. } => *body_len,
            Self::Image { .. } => 0,
        }
    }
}
