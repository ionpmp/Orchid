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

use super::paint::*;
use super::tables::*;
use super::*;

impl DocumentLayout {
    /// Create layout contexts.
    #[must_use]
    pub fn new() -> Self {
        Self {
            font_cx: FontContext::new(),
            layout_cx: LayoutContext::new(),
            scale_cx: ScaleContext::new(),
            layout_cache: LayoutCache::new(),
            scene: None,
            field_file_name: None,
        }
    }

    /// Set the open file name used when resolving `FILENAME` fields in Preview.
    pub fn set_field_file_name(&mut self, name: Option<String>) {
        if self.field_file_name != name {
            self.field_file_name = name;
            self.drop_render_scene();
        }
    }

    /// Drop cached paragraph layouts and the last preview raster.
    ///
    /// Call when document content or preview width changes. Selection-only
    /// updates should leave the scene in place.
    pub fn drop_render_scene(&mut self) {
        self.scene = None;
        self.layout_cache.invalidate_all();
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
        let default_pt = p.outline_level.map(outline_level_font_pt).unwrap_or(14.0);
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
            if run.style.strikethrough || run.style.double_strikethrough {
                builder.push(StyleProperty::Strikethrough(true), offset..end);
            }
            let base_pt = run.style.font_size_pt.unwrap_or(default_pt);
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
                || run.style.double_strikethrough
                || run.style.emboss
                || run.style.imprint
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
                brush.double_strike = run.style.double_strikethrough;
                brush.emboss = run.style.emboss;
                brush.imprint = run.style.imprint;
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
        layout.align(parley_alignment_for(p), AlignmentOptions::default());
        layout
    }

    pub(super) fn cached_paragraph_layout(
        &mut self,
        idx: usize,
        p: &Paragraph,
        width: f32,
        scale: f32,
    ) -> Layout<ColorBrush> {
        self.layout_cache.prepare(width, scale);
        if let Some(hit) = self.layout_cache.get_clone(idx) {
            return hit;
        }
        let layout = self.layout_paragraph(p, width, scale);
        self.layout_cache.insert(idx, layout.clone());
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
        if let Some(scene) = self.scene.as_ref() {
            if (scene.content_width - content_width).abs() <= 0.5 {
                return overlay_selection_on_scene(scene, selection);
            }
        }
        let scale = PREVIEW_RENDER_SCALE;
        let section_setups = collect_section_page_setups(doc);
        let union = union_page_setup_margins(&section_setups);
        let base_insets = PreviewInsets::from_page_setup(&union);
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
        // Content-relative Y of each 1-based page band (page breaks via `w:pageBreakBefore`).
        let mut page_starts: Vec<f32> = vec![0.0];
        // Parallel to `page_starts`: which body section owns that preview page band.
        let mut section_idx = 0usize;
        let mut page_section: Vec<usize> = vec![0];
        let para_gap = 10.0 * scale;
        let mut plain_offset = 0usize;
        let mut emitted_text = false;
        let max_preview_h = MAX_PREVIEW_HEIGHT as f32 * scale;

        for (block_idx, block) in doc.blocks.iter().enumerate() {
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
                        page_starts.push(total_h);
                        page_section.push(section_idx);
                    }
                    // Body fields: page=1 until per-band PAGE resolution; DATE/FILENAME ok.
                    let styled = apply_named_paragraph_style(
                        &doc.paragraph_styles,
                        &doc.character_styles,
                        p,
                    );
                    total_h += twips_to_css_px(styled.space_before_twips) * scale;
                    let body_len = p.plain_text().len();
                    let prefix_len = list_prefix(&styled).len();
                    let indent = list_indent_px(&styled) * scale;
                    let sect = section_setups.get(section_idx).unwrap_or(&doc.page_setup);
                    let (sect_x0, sect_wrap) =
                        section_body_origin_and_width(sect, &union, max_w, scale);
                    let wrap_w = (sect_wrap - indent - paragraph_right_indent_px(&styled) * scale)
                        .max(12.0 * scale);
                    let resolved = resolve_paragraph_fields(
                        &styled,
                        1,
                        page_starts.len().max(1) as u32,
                        self.field_file_name.as_deref(),
                    );
                    let layout = self.cached_paragraph_layout(block_idx, &resolved, wrap_w, scale);
                    let h = layout.height().max(16.0 * scale);
                    layouts.push(LaidBlock {
                        layout,
                        y0: total_h,
                        x0: sect_x0,
                        indent_px: indent,
                        plain_start: plain_offset,
                        body_len,
                        prefix_len,
                        is_image: false,
                        image_h: 0.0,
                        image_w: 0,
                        image_rgba: None,
                        page_break_rule_y: rule_y,
                        shade_fill: styled.shade_fill,
                        shade_w: sect_wrap.max(1.0),
                        border_sides: styled.border_sides,
                    });
                    plain_offset += body_len;
                    let after = (twips_to_css_px(styled.space_after_twips) * scale).max(para_gap);
                    total_h += h + after;
                    if let Some(ref ps) = p.section_properties {
                        section_idx = (section_idx + 1).min(section_setups.len().saturating_sub(1));
                        if section_forces_page_band(ps) {
                            let page_break_gap = 28.0 * scale;
                            total_h += page_break_gap;
                            page_starts.push(total_h);
                            page_section.push(section_idx);
                        }
                    }
                }
                Block::Table(t) => {
                    let grid = self.append_table_grid(
                        &doc.paragraph_styles,
                        &doc.character_styles,
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
                        box_w,
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

        // Header / footer stories in page margins. Preview paints the correct
        // first / even / default story at each page band, using that section's
        // `w:pgMar` `@w:header` / `@w:footer` distances (and titlePg / evenOdd).
        let page_count = (page_starts.len() as u32).max(1);
        for (page_i, &start_y) in page_starts.iter().enumerate() {
            let page = (page_i + 1) as u32;
            let end_y = page_starts.get(page_i + 1).copied().unwrap_or(total_h);
            let setup = section_setups
                .get(page_section.get(page_i).copied().unwrap_or(0))
                .unwrap_or(&doc.page_setup);
            let header_off = twips_to_css_px(setup.header_distance_twips) * scale;
            let footer_off = twips_to_css_px(setup.footer_distance_twips) * scale;
            let (header_story, footer_story) = margin_stories_for_page(doc, page, setup);
            let header_resolved = resolve_story_fields(
                header_story,
                page,
                page_count,
                self.field_file_name.as_deref(),
            );
            let footer_resolved = resolve_story_fields(
                footer_story,
                page,
                page_count,
                self.field_file_name.as_deref(),
            );
            // `start_y` is content-relative; header sits `header_off` below the page top.
            let header_y = (start_y + header_off).max(0.0);
            let footer_y = if page_i + 1 == page_starts.len() {
                (height as f32 - footer_off - 14.0 * scale).max(0.0)
            } else {
                (insets.top + end_y - footer_off.min(insets.bottom).max(12.0 * scale)).max(0.0)
            };
            if !header_resolved.is_empty() {
                self.paint_margin_story(
                    &doc.paragraph_styles,
                    &doc.character_styles,
                    &header_resolved,
                    max_w,
                    insets.left,
                    header_y,
                    scale,
                    &mut pixels,
                    width,
                    height,
                );
            }
            if !footer_resolved.is_empty() {
                self.paint_margin_story(
                    &doc.paragraph_styles,
                    &doc.character_styles,
                    &footer_resolved,
                    max_w,
                    insets.left,
                    footer_y,
                    scale,
                    &mut pixels,
                    width,
                    height,
                );
            }
        }

        paint_comment_highlights(
            &layouts,
            &mut pixels,
            width,
            height,
            insets,
            &doc.comment_ranges,
        );

        let base = Arc::new(pixels);
        self.scene = Some(RenderScene {
            content_width,
            layouts,
            insets,
            max_w,
            width,
            height,
            base: Arc::clone(&base),
        });
        overlay_selection_on_scene(self.scene.as_ref().expect("scene just stored"), selection)
    }
}

/// Fill unset run props from a named paragraph style (direct formatting wins).
pub(super) fn merge_run_under_named(dst: &mut RunStyle, base: &RunStyle) {
    if !dst.bold {
        dst.bold = base.bold;
    }
    if !dst.italic {
        dst.italic = base.italic;
    }
    if !dst.underline {
        dst.underline = base.underline;
    }
    if !dst.strikethrough {
        dst.strikethrough = base.strikethrough;
    }
    if !dst.double_strikethrough {
        dst.double_strikethrough = base.double_strikethrough;
    }
    if !dst.highlight {
        dst.highlight = base.highlight;
    }
    if !dst.superscript && !dst.subscript {
        dst.superscript = base.superscript;
        dst.subscript = base.subscript;
    }
    if !dst.all_caps {
        dst.all_caps = base.all_caps;
    }
    if !dst.small_caps {
        dst.small_caps = base.small_caps;
    }
    if !dst.vanish {
        dst.vanish = base.vanish;
    }
    if !dst.shadow {
        dst.shadow = base.shadow;
    }
    if !dst.emboss {
        dst.emboss = base.emboss;
    }
    if !dst.imprint {
        dst.imprint = base.imprint;
    }
    if dst.color.is_none() {
        dst.color = base.color;
    }
    if dst.font_family.is_none() {
        dst.font_family = base.font_family.clone();
    }
    if dst.font_size_pt.is_none() {
        dst.font_size_pt = base.font_size_pt;
    }
}

/// Apply `w:rStyle` then `w:pStyle` character/paragraph defaults for Preview layout.
///
/// Precedence: direct formatting > character style > paragraph style.
pub(super) fn apply_named_paragraph_style(
    paragraph_styles: &HashMap<String, NamedParagraphStyle>,
    character_styles: &HashMap<String, NamedCharacterStyle>,
    p: &Paragraph,
) -> Paragraph {
    let mut out = p.clone();
    for run in &mut out.runs {
        if let Some(id) = run.style_id.as_deref() {
            if let Some(cs) = character_styles.get(id) {
                merge_run_under_named(&mut run.style, &cs.run);
            }
        }
    }
    let Some(id) = p.style_id.as_deref() else {
        return out;
    };
    let Some(ns) = paragraph_styles.get(id) else {
        return out;
    };
    if out.outline_level.is_none() {
        out.outline_level = ns.outline_level;
    }
    merge_paragraph_under_named(&mut out, &ns.paragraph);
    for run in &mut out.runs {
        merge_run_under_named(&mut run.style, &ns.run);
    }
    out
}

/// Fill unset paragraph props from a named style `w:pPr` (direct formatting wins).
pub(super) fn merge_paragraph_under_named(
    dst: &mut Paragraph,
    base: &crate::document::model::ParagraphStyleProps,
) {
    if let Some(a) = base.alignment {
        if dst.alignment == Alignment::Left {
            dst.alignment = a;
        }
    }
    if dst.space_before_twips == 0 {
        if let Some(v) = base.space_before_twips {
            dst.space_before_twips = v;
        }
    }
    if dst.space_after_twips == 0 {
        if let Some(v) = base.space_after_twips {
            dst.space_after_twips = v;
        }
    }
    if dst.line_spacing == 0 && dst.line_spacing_rule == LineSpacingRule::Auto {
        if let Some(v) = base.line_spacing {
            dst.line_spacing = v;
        }
        if let Some(r) = base.line_spacing_rule {
            dst.line_spacing_rule = r;
        }
    }
    if dst.indent_left_twips == 0 {
        if let Some(v) = base.indent_left_twips {
            dst.indent_left_twips = v;
        }
    }
    if dst.indent_right_twips == 0 {
        if let Some(v) = base.indent_right_twips {
            dst.indent_right_twips = v;
        }
    }
    if dst.indent_first_line_twips == 0 {
        if let Some(v) = base.indent_first_line_twips {
            dst.indent_first_line_twips = v;
        }
    }
    if dst.shade_fill.is_none() {
        dst.shade_fill = base.shade_fill;
    }
    if dst.border_sides == 0 {
        if let Some(sides) = base.border_sides {
            dst.border_sides = sides;
        }
    }
    if !dst.keep_next {
        dst.keep_next = base.keep_next;
    }
    if !dst.keep_lines {
        dst.keep_lines = base.keep_lines;
    }
    if !dst.widow_control {
        dst.widow_control = base.widow_control;
    }
    if !dst.contextual_spacing {
        dst.contextual_spacing = base.contextual_spacing;
    }
    if !dst.bidi {
        dst.bidi = base.bidi;
    }
    if !dst.suppress_auto_hyphens {
        dst.suppress_auto_hyphens = base.suppress_auto_hyphens;
    }
}

/// Substitute field display text (`PAGE`, `DATE`, `FILENAME`, …) for preview paint.
pub(super) fn resolve_story_fields(
    paragraphs: &[crate::document::model::Paragraph],
    page: u32,
    page_count: u32,
    file_name: Option<&str>,
) -> Vec<crate::document::model::Paragraph> {
    paragraphs
        .iter()
        .map(|p| resolve_paragraph_fields(p, page, page_count, file_name))
        .collect()
}

pub(super) fn resolve_paragraph_fields(
    p: &crate::document::model::Paragraph,
    page: u32,
    page_count: u32,
    file_name: Option<&str>,
) -> crate::document::model::Paragraph {
    let mut out = p.clone();
    for run in &mut out.runs {
        if let Some(field) = run.field {
            run.text = field.display(page, page_count, file_name);
        }
    }
    out
}

/// Mid-body section ends: next-page forces a Preview page band; continuous does not.
pub(super) fn section_forces_page_band(ps: &PageSetup) -> bool {
    ps.section_break != SectionBreakType::Continuous
}

/// Mid-body `w:pPr/w:sectPr` setups followed by the trailing body `w:sectPr`.
pub(super) fn collect_section_page_setups(doc: &Document) -> Vec<PageSetup> {
    let mut setups = Vec::new();
    for block in &doc.blocks {
        if let Block::Paragraph(p) = block {
            if let Some(ref ps) = p.section_properties {
                setups.push(ps.clone());
            }
        }
    }
    setups.push(doc.page_setup.clone());
    setups
}

/// Widest page margins across sections (canvas padding); narrower sections shift via `x0`.
pub(super) fn union_page_setup_margins(setups: &[PageSetup]) -> PageSetup {
    let mut u = setups.first().cloned().unwrap_or_default();
    for s in setups.iter().skip(1) {
        u.margin_left_twips = u.margin_left_twips.max(s.margin_left_twips);
        u.margin_right_twips = u.margin_right_twips.max(s.margin_right_twips);
        u.margin_top_twips = u.margin_top_twips.max(s.margin_top_twips);
        u.margin_bottom_twips = u.margin_bottom_twips.max(s.margin_bottom_twips);
        u.width_twips = u.width_twips.max(s.width_twips);
        u.height_twips = u.height_twips.max(s.height_twips);
    }
    u
}

/// Content column width in twips (`pgSz` width minus left/right `pgMar`).
pub(super) fn page_content_width_twips(ps: &PageSetup) -> u32 {
    ps.width_twips
        .saturating_sub(ps.margin_left_twips)
        .saturating_sub(ps.margin_right_twips)
        .max(1)
}

/// Content-column origin and wrap width for a section inside the union canvas.
///
/// `max_w` maps to the union section’s content column (`pgSz` − L/R margins). A
/// narrower page or wider margins shrinks wrap; a left-margin delta shifts `x0`
/// (≤ 0 when this section’s left margin is narrower than the union).
pub(super) fn section_body_origin_and_width(
    sect: &PageSetup,
    union: &PageSetup,
    max_w: f32,
    scale: f32,
) -> (f32, f32) {
    let left_delta =
        twips_to_css_px(sect.margin_left_twips) - twips_to_css_px(union.margin_left_twips);
    let x0 = left_delta * scale;
    let union_content = page_content_width_twips(union) as f32;
    let sect_content = page_content_width_twips(sect) as f32;
    let wrap = (max_w * (sect_content / union_content)).max(12.0 * scale);
    (x0, wrap)
}

/// Pick header/footer stories for a 1-based preview page index.
pub(super) fn margin_stories_for_page<'a>(
    doc: &'a Document,
    page: u32,
    setup: &PageSetup,
) -> (
    &'a [crate::document::model::Paragraph],
    &'a [crate::document::model::Paragraph],
) {
    let use_first = setup.title_page && page == 1;
    let use_even = setup.even_and_odd_headers && !use_first && page % 2 == 0;
    let header = if use_first && !doc.header_first.is_empty() {
        doc.header_first.as_slice()
    } else if use_even && !doc.header_even.is_empty() {
        doc.header_even.as_slice()
    } else {
        doc.header.as_slice()
    };
    let footer = if use_first && !doc.footer_first.is_empty() {
        doc.footer_first.as_slice()
    } else if use_even && !doc.footer_even.is_empty() {
        doc.footer_even.as_slice()
    } else {
        doc.footer.as_slice()
    };
    (header, footer)
}
