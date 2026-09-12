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
fn bidi_left_align_places_line_on_the_right() {
    let mut dl = DocumentLayout::new();
    let mut ltr = sample_paragraph();
    ltr.alignment = Alignment::Left;
    let mut rtl = sample_paragraph();
    rtl.alignment = Alignment::Left;
    rtl.bidi = true;
    let width = 400.0;
    let ltr_layout = dl.layout_paragraph(&ltr, width, 1.0);
    let rtl_layout = dl.layout_paragraph(&rtl, width, 1.0);
    let ltr_off = ltr_layout.get(0).expect("ltr line").metrics().offset;
    let rtl_off = rtl_layout.get(0).expect("rtl line").metrics().offset;
    assert!(
            rtl_off > ltr_off + 20.0,
            "bidi left-align should push the line toward the right edge (ltr_off={ltr_off}, rtl_off={rtl_off})"
        );
}

#[test]
fn preview_paints_header_in_top_margin() {
    let mut dl = DocumentLayout::new();
    let mut page_setup = PageSetup::default();
    page_setup.margin_top_twips = 1440; // 1″
    let doc = Document {
        blocks: vec![Block::Paragraph(Paragraph {
            runs: vec![Run {
                text: "Body".into(),
                style: RunStyle::default(),
                ..Default::default()
            }],
            ..Default::default()
        })],
        page_setup,
        header: vec![Paragraph {
            runs: vec![Run {
                text: "HEADERINK".into(),
                style: RunStyle {
                    bold: true,
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let (bytes, w, h) = dl.render_document(&doc, 400.0);
    let s = PREVIEW_RENDER_SCALE;
    let insets = PreviewInsets::from_page_setup(&doc.page_setup);
    let margin_bottom = (insets.top * s).round() as u32;
    assert!(margin_bottom > 8);
    assert!(h > margin_bottom);
    let mut ink_in_margin = false;
    for y in 0..margin_bottom.min(h) {
        for x in 0..w {
            let i = ((y as usize) * (w as usize) + (x as usize)) * 4;
            if bytes[i] < 240 || bytes[i + 1] < 240 || bytes[i + 2] < 240 {
                ink_in_margin = true;
                break;
            }
        }
        if ink_in_margin {
            break;
        }
    }
    assert!(
        ink_in_margin,
        "expected header glyph ink inside the top margin band"
    );
}

#[test]
fn preview_paints_first_header_when_title_page() {
    let mut dl = DocumentLayout::new();
    let mut page_setup = PageSetup::default();
    page_setup.margin_top_twips = 1440;
    page_setup.title_page = true;
    let doc = Document {
        blocks: vec![Block::Paragraph(Paragraph {
            runs: vec![Run {
                text: "Body".into(),
                style: RunStyle::default(),
                ..Default::default()
            }],
            ..Default::default()
        })],
        page_setup,
        header: vec![Paragraph {
            runs: vec![Run {
                text: "DEFAULTONLY".into(),
                style: RunStyle {
                    bold: true,
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        }],
        header_first: vec![Paragraph {
            runs: vec![Run {
                text: "FIRSTPAGEHDR".into(),
                style: RunStyle {
                    bold: true,
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let (bytes, w, h) = dl.render_document(&doc, 400.0);
    let s = PREVIEW_RENDER_SCALE;
    let insets = PreviewInsets::from_page_setup(&doc.page_setup);
    let margin_bottom = (insets.top * s).round() as u32;
    let mut ink_in_margin = false;
    for y in 0..margin_bottom.min(h) {
        for x in 0..w {
            let i = ((y as usize) * (w as usize) + (x as usize)) * 4;
            if bytes[i] < 240 || bytes[i + 1] < 240 || bytes[i + 2] < 240 {
                ink_in_margin = true;
                break;
            }
        }
        if ink_in_margin {
            break;
        }
    }
    assert!(
        ink_in_margin,
        "expected first-page header glyph ink inside the top margin"
    );
}

#[test]
fn preview_skips_even_header_on_page_one() {
    // With even/odd enabled, page 1 is odd → default story (empty here).
    let mut dl = DocumentLayout::new();
    let mut page_setup = PageSetup::default();
    page_setup.margin_top_twips = 1440;
    page_setup.even_and_odd_headers = true;
    let doc = Document {
        blocks: vec![Block::Paragraph(Paragraph {
            runs: vec![Run {
                text: "Body".into(),
                style: RunStyle::default(),
                ..Default::default()
            }],
            ..Default::default()
        })],
        page_setup,
        header_even: vec![Paragraph {
            runs: vec![Run {
                text: "EVENONLYHDR".into(),
                style: RunStyle {
                    bold: true,
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let (bytes, w, h) = dl.render_document(&doc, 400.0);
    let s = PREVIEW_RENDER_SCALE;
    let insets = PreviewInsets::from_page_setup(&doc.page_setup);
    let margin_bottom = (insets.top * s).round() as u32;
    let mut ink_in_margin = false;
    for y in 0..margin_bottom.min(h) {
        for x in 0..w {
            let i = ((y as usize) * (w as usize) + (x as usize)) * 4;
            if bytes[i] < 240 || bytes[i + 1] < 240 || bytes[i + 2] < 240 {
                ink_in_margin = true;
                break;
            }
        }
        if ink_in_margin {
            break;
        }
    }
    assert!(
        !ink_in_margin,
        "page 1 must not paint the even-page header in the top margin"
    );
}

#[test]
fn preview_paints_even_header_after_page_break() {
    let mut page_setup = PageSetup::default();
    page_setup.margin_top_twips = 1440;
    page_setup.even_and_odd_headers = true;
    let blocks = vec![
        Block::Paragraph(Paragraph {
            runs: vec![Run {
                text: "PageOne".into(),
                style: RunStyle::default(),
                ..Default::default()
            }],
            ..Default::default()
        }),
        Block::Paragraph(Paragraph {
            page_break_before: true,
            runs: vec![Run {
                text: "PageTwo".into(),
                style: RunStyle::default(),
                ..Default::default()
            }],
            ..Default::default()
        }),
    ];
    let with_even = Document {
        blocks: blocks.clone(),
        page_setup: page_setup.clone(),
        header_even: vec![Paragraph {
            runs: vec![Run {
                text: "EVENPAGEHDR".into(),
                style: RunStyle {
                    bold: true,
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let without_even = Document {
        blocks,
        page_setup,
        ..Default::default()
    };
    // Separate layout engines — `DocumentLayout` caches the last scene by width.
    let (bytes_with, w, h) = DocumentLayout::new().render_document(&with_even, 400.0);
    let (bytes_without, w2, h2) = DocumentLayout::new().render_document(&without_even, 400.0);
    assert_eq!((w, h), (w2, h2));
    let s = PREVIEW_RENDER_SCALE;
    let insets = PreviewInsets::from_page_setup(&with_even.page_setup);
    let margin_bottom = (insets.top * s).round() as u32;

    let count_dark = |bytes: &[u8], y0: u32| {
        let mut n = 0usize;
        for y in y0..h {
            for x in 0..w {
                let i = ((y as usize) * (w as usize) + (x as usize)) * 4;
                // Glyphs are near-black; dashed page-break rules stay ~180 gray.
                if bytes[i] < 100 || bytes[i + 1] < 100 || bytes[i + 2] < 100 {
                    n += 1;
                }
            }
        }
        n
    };

    let top_dark = {
        let mut n = 0usize;
        for y in 0..margin_bottom.min(h) {
            for x in 0..w {
                let i = ((y as usize) * (w as usize) + (x as usize)) * 4;
                if bytes_with[i] < 100 || bytes_with[i + 1] < 100 || bytes_with[i + 2] < 100 {
                    n += 1;
                }
            }
        }
        n
    };
    assert_eq!(top_dark, 0, "even header must not appear on page 1");

    let dark_with = count_dark(&bytes_with, margin_bottom);
    let dark_without = count_dark(&bytes_without, margin_bottom);
    assert!(
            dark_with > dark_without + 10,
            "expected even-page header ink after the page break (with={dark_with}, without={dark_without})"
        );
}

#[test]
fn margin_stories_for_page_picks_first_even_default() {
    let doc = Document {
        page_setup: PageSetup {
            title_page: true,
            even_and_odd_headers: true,
            ..Default::default()
        },
        header: vec![Paragraph {
            runs: vec![Run {
                text: "DEF".into(),
                ..Default::default()
            }],
            ..Default::default()
        }],
        header_first: vec![Paragraph {
            runs: vec![Run {
                text: "FIRST".into(),
                ..Default::default()
            }],
            ..Default::default()
        }],
        header_even: vec![Paragraph {
            runs: vec![Run {
                text: "EVEN".into(),
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    assert_eq!(
        margin_stories_for_page(&doc, 1, &doc.page_setup).0[0].plain_text(),
        "FIRST"
    );
    assert_eq!(
        margin_stories_for_page(&doc, 2, &doc.page_setup).0[0].plain_text(),
        "EVEN"
    );
    assert_eq!(
        margin_stories_for_page(&doc, 3, &doc.page_setup).0[0].plain_text(),
        "DEF"
    );
}

#[test]
fn cache_hits_second_call() {
    let mut dl = DocumentLayout::new();
    let mut cache = LayoutCache::new();
    let p = sample_paragraph();
    let _ = cache.get_or_layout(0, &p, &mut dl, 400.0, 1.0);
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
        .hit_test_plain_offset(
            &doc,
            400.0,
            PreviewInsets::default_letter().left + 8.0,
            PreviewInsets::default_letter().left + 40.0,
        )
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
        .hit_test_plain_offset(
            &doc,
            400.0,
            PreviewInsets::default_letter().left + 2.0,
            PreviewInsets::default_letter().left + 2.0,
        )
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

#[test]
fn comment_range_paints_amber_wash() {
    use crate::document::model::CommentRange;
    let mut dl = DocumentLayout::new();
    let plain_doc = Document {
        blocks: vec![Block::Paragraph(sample_paragraph())],
        ..Default::default()
    };
    let commented = Document {
        blocks: vec![Block::Paragraph(sample_paragraph())],
        comment_ranges: vec![CommentRange {
            id: 0,
            start_plain: 0,
            end_plain: 5,
        }],
        ..Default::default()
    };
    let (plain, _, _) = dl.render_document(&plain_doc, 400.0);
    let (with_c, _, _) = DocumentLayout::new().render_document(&commented, 400.0);
    assert_eq!(plain.len(), with_c.len());
    assert_ne!(
        plain.as_slice(),
        with_c.as_slice(),
        "comment highlight should change preview pixels"
    );
    assert!(
        with_c
            .chunks_exact(4)
            .any(|px| px[0] > 200 && px[1] > 180 && px[2] < 150),
        "expected amber comment wash"
    );
}

#[test]
fn selection_overlay_reuses_scene() {
    let mut dl = DocumentLayout::new();
    let doc = Document {
        blocks: vec![Block::Paragraph(sample_paragraph())],
        ..Default::default()
    };
    let (first, w, h) = dl.render_document_with_selection(&doc, 400.0, Some((0, 0)));
    assert!(dl.scene.is_some());
    let (second, w2, h2) = dl.render_document_with_selection(&doc, 400.0, Some((0, 5)));
    assert_eq!((w, h), (w2, h2));
    assert_eq!(first.len(), second.len());
    assert_ne!(
        first.as_slice(),
        second.as_slice(),
        "selection overlay should tint the cached base raster"
    );
    assert!(dl.layout_cache.contains(0));
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
        .hit_test_plain_offset(
            &doc,
            content_w,
            PreviewInsets::default_letter().left + content_w * 0.2,
            y,
        )
        .unwrap();
    assert!(
        left >= aa && left <= aa + "AA".len(),
        "left hit offset={left} expected AA"
    );
    // 70% into table → wide right column.
    let right = dl
        .hit_test_plain_offset(
            &doc,
            content_w,
            PreviewInsets::default_letter().left + content_w * 0.7,
            y,
        )
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
fn preview_substitutes_page_fields_in_footer() {
    use crate::document::model::DocField;
    let mut page_setup = PageSetup::default();
    page_setup.margin_bottom_twips = 1440;
    page_setup.footer_distance_twips = 720;
    let with_field = Document {
        blocks: vec![
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "One".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                page_break_before: true,
                runs: vec![Run {
                    text: "Two".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
        ],
        page_setup: page_setup.clone(),
        footer: vec![Paragraph {
            runs: vec![Run {
                text: "0".into(),
                field: Some(DocField::Page),
                style: RunStyle {
                    bold: true,
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let without = Document {
        blocks: with_field.blocks.clone(),
        page_setup,
        ..Default::default()
    };
    let (bytes_with, w, h) = DocumentLayout::new().render_document(&with_field, 400.0);
    let (bytes_without, w2, h2) = DocumentLayout::new().render_document(&without, 400.0);
    assert_eq!((w, h), (w2, h2));
    let dark = |bytes: &[u8]| bytes.chunks_exact(4).filter(|px| px[0] < 100).count();
    assert!(
        dark(&bytes_with) > dark(&bytes_without) + 5,
        "PAGE field footer should add glyph ink"
    );
}

#[test]
fn preview_honors_header_distance_from_page_edge() {
    // Larger header distance should push header ink lower in the top margin.
    let mut near = PageSetup::default();
    near.margin_top_twips = 1440;
    near.header_distance_twips = 240; // 1/6″
    let mut far = near.clone();
    far.header_distance_twips = 960; // 2/3″
    let mk = |page_setup: PageSetup| Document {
        blocks: vec![Block::Paragraph(Paragraph {
            runs: vec![Run {
                text: "Body".into(),
                style: RunStyle::default(),
                ..Default::default()
            }],
            ..Default::default()
        })],
        page_setup,
        header: vec![Paragraph {
            runs: vec![Run {
                text: "HDRDIST".into(),
                style: RunStyle {
                    bold: true,
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let (bytes_near, w, h) = DocumentLayout::new().render_document(&mk(near), 400.0);
    let (bytes_far, w2, h2) = DocumentLayout::new().render_document(&mk(far), 400.0);
    assert_eq!((w, h), (w2, h2));
    let first_dark_y = |bytes: &[u8]| -> Option<u32> {
        for y in 0..h {
            for x in 0..w {
                let i = ((y as usize) * (w as usize) + (x as usize)) * 4;
                if bytes[i] < 100 || bytes[i + 1] < 100 || bytes[i + 2] < 100 {
                    return Some(y);
                }
            }
        }
        None
    };
    let y_near = first_dark_y(&bytes_near).expect("near header ink");
    let y_far = first_dark_y(&bytes_far).expect("far header ink");
    assert!(
        y_far > y_near + 4,
        "larger w:header distance should paint lower (near={y_near}, far={y_far})"
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
        .hit_test_plain_offset(&doc, content_w, insets.left + 2.0, insets.top + 2.0)
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

#[test]
fn preview_section_break_starts_new_page_band() {
    let doc = Document {
        blocks: vec![
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "One".into(),
                    style: RunStyle::default(),
                    ..Default::default()
                }],
                section_properties: Some(PageSetup::default()),
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Two".into(),
                    style: RunStyle::default(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
        ],
        ..Default::default()
    };
    let (_, _, h) = DocumentLayout::new().render_document(&doc, 400.0);
    let without = Document {
        blocks: vec![
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "One".into(),
                    style: RunStyle::default(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Two".into(),
                    style: RunStyle::default(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
        ],
        ..Default::default()
    };
    let (_, _, h2) = DocumentLayout::new().render_document(&without, 400.0);
    assert!(
        h > h2 + 10,
        "section break should add a page-band gap (with={h}, without={h2})"
    );
}

#[test]
fn preview_uses_section_header_distance_per_page_band() {
    let mut sect1 = PageSetup::default();
    sect1.margin_top_twips = 1440;
    sect1.header_distance_twips = 240;
    let mut sect2 = PageSetup::default();
    sect2.margin_top_twips = 1440;
    sect2.header_distance_twips = 960;
    let doc = Document {
        blocks: vec![
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "One".into(),
                    style: RunStyle::default(),
                    ..Default::default()
                }],
                section_properties: Some(sect1),
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Two".into(),
                    style: RunStyle::default(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
        ],
        page_setup: sect2,
        header: vec![Paragraph {
            runs: vec![Run {
                text: "HDR".into(),
                style: RunStyle {
                    bold: true,
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let (bytes, w, h) = DocumentLayout::new().render_document(&doc, 400.0);
    assert!(w > 0 && h > 0);
    // Ink should exist (header painted); geometry differs from single-setup docs.
    let dark = bytes.chunks_exact(4).filter(|px| px[0] < 100).count();
    assert!(dark > 10, "expected header ink across section bands");
    let setups = collect_section_page_setups(&doc);
    assert_eq!(setups.len(), 2);
    assert_eq!(setups[0].header_distance_twips, 240);
    assert_eq!(setups[1].header_distance_twips, 960);
}

#[test]
fn preview_section_body_margin_shifts_x0() {
    let mut narrow = PageSetup::default();
    narrow.margin_left_twips = 720; // 0.5″
    narrow.margin_right_twips = 720;
    let mut wide = PageSetup::default();
    wide.margin_left_twips = 2160; // 1.5″
    wide.margin_right_twips = 2160;
    let doc = Document {
        blocks: vec![
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Narrow".into(),
                    style: RunStyle {
                        bold: true,
                        ..Default::default()
                    },
                    ..Default::default()
                }],
                section_properties: Some(narrow.clone()),
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "WideMargin".into(),
                    style: RunStyle {
                        bold: true,
                        ..Default::default()
                    },
                    ..Default::default()
                }],
                ..Default::default()
            }),
        ],
        page_setup: wide.clone(),
        ..Default::default()
    };
    let union = union_page_setup_margins(&collect_section_page_setups(&doc));
    assert_eq!(union.margin_left_twips, 2160);
    let (x0_n, wrap_n) = section_body_origin_and_width(&narrow, &union, 400.0, 1.0);
    let (x0_w, wrap_w) = section_body_origin_and_width(&wide, &union, 400.0, 1.0);
    assert!(x0_n < -10.0, "narrow section should shift left (x0={x0_n})");
    assert!(
        (x0_w).abs() < 0.1,
        "wide section x0 should be ~0 (x0={x0_w})"
    );
    assert!(wrap_n > wrap_w + 10.0, "narrow section gets wider wrap");
    let (_, _, h) = DocumentLayout::new().render_document(&doc, 400.0);
    assert!(h > 40);
}

#[test]
fn preview_section_page_width_shrinks_wrap() {
    // Mid-body Letter, trailing A4 — same 1″ margins; A4 content column is narrower.
    let mut letter = PageSetup::default(); // 12240 × 15840
    letter.margin_left_twips = 1440;
    letter.margin_right_twips = 1440;
    let mut a4 = PageSetup::default();
    a4.width_twips = 11906;
    a4.height_twips = 16838;
    a4.margin_left_twips = 1440;
    a4.margin_right_twips = 1440;
    let doc = Document {
        blocks: vec![
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "LetterSect".into(),
                    style: RunStyle::default(),
                    ..Default::default()
                }],
                section_properties: Some(letter.clone()),
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "A4Sect".into(),
                    style: RunStyle::default(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
        ],
        page_setup: a4.clone(),
        ..Default::default()
    };
    let union = union_page_setup_margins(&collect_section_page_setups(&doc));
    assert_eq!(union.width_twips, 12240);
    let (x0_l, wrap_l) = section_body_origin_and_width(&letter, &union, 400.0, 1.0);
    let (x0_a, wrap_a) = section_body_origin_and_width(&a4, &union, 400.0, 1.0);
    assert!(x0_l.abs() < 0.1, "same left margin → x0≈0 (got {x0_l})");
    assert!(x0_a.abs() < 0.1, "same left margin → x0≈0 (got {x0_a})");
    assert!(
        (wrap_l - 400.0).abs() < 0.5,
        "Letter (union width) wrap ≈ max_w (got {wrap_l})"
    );
    assert!(
        wrap_a < wrap_l - 5.0,
        "A4 section wrap should be narrower than Letter (a4={wrap_a}, letter={wrap_l})"
    );
    let (_, _, h) = DocumentLayout::new().render_document(&doc, 400.0);
    assert!(h > 40);
}

#[test]
fn preview_merges_paragraph_style_ppr_spacing_and_shade() {
    use crate::document::model::{NamedParagraphStyle, ParagraphStyleProps};
    use std::collections::HashMap;

    let mut paragraph_styles = HashMap::new();
    paragraph_styles.insert(
        "Title".into(),
        NamedParagraphStyle {
            style_id: "Title".into(),
            name: "Title".into(),
            paragraph: ParagraphStyleProps {
                alignment: Some(Alignment::Center),
                space_before_twips: Some(480),
                space_after_twips: Some(240),
                shade_fill: Some([0xFF, 0xF2, 0xCC]),
                ..Default::default()
            },
            run: RunStyle {
                bold: true,
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let p = Paragraph {
        runs: vec![Run {
            text: "Hello".into(),
            ..Default::default()
        }],
        style_id: Some("Title".into()),
        ..Default::default()
    };
    let styled = apply_named_paragraph_style(&paragraph_styles, &HashMap::new(), &p);
    assert_eq!(styled.alignment, Alignment::Center);
    assert_eq!(styled.space_before_twips, 480);
    assert_eq!(styled.space_after_twips, 240);
    assert_eq!(styled.shade_fill, Some([0xFF, 0xF2, 0xCC]));
    assert!(styled.runs[0].style.bold);
    let override_p = Paragraph {
        style_id: Some("Title".into()),
        space_before_twips: 100,
        runs: vec![Run {
            text: "X".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let merged = apply_named_paragraph_style(&paragraph_styles, &HashMap::new(), &override_p);
    assert_eq!(merged.space_before_twips, 100);
    assert_eq!(merged.space_after_twips, 240);
}

#[test]
fn continuous_section_skips_preview_page_band() {
    use crate::document::model::SectionBreakType;
    let mut next = PageSetup::default();
    next.section_break = SectionBreakType::NextPage;
    let mut cont = PageSetup::default();
    cont.section_break = SectionBreakType::Continuous;
    let mk = |sect: PageSetup| Document {
        blocks: vec![
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "One".into(),
                    style: RunStyle::default(),
                    ..Default::default()
                }],
                section_properties: Some(sect),
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Two".into(),
                    style: RunStyle::default(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
        ],
        ..Default::default()
    };
    let (_, _, h_next) = DocumentLayout::new().render_document(&mk(next), 400.0);
    let (_, _, h_cont) = DocumentLayout::new().render_document(&mk(cont), 400.0);
    assert!(
        h_next > h_cont + 10,
        "continuous should skip page-band gap (next={h_next}, cont={h_cont})"
    );
}
