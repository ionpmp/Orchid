//! Find next/previous in the DOCX plain-text stream.

use orchid_viewers::document::model::{
    Block, Document as Doc, PageSetup, Paragraph, Run, RunStyle, Table, TableCell, TableRow,
};
use orchid_viewers::document::DocumentViewer;
use orchid_viewers::{Viewer, ViewerSnapshot};

fn sample_doc() -> Doc {
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
    Doc {
        blocks: vec![
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Alpha Hello world".into(),
                    style: RunStyle::default(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            Block::Table(Table {
                rows: vec![TableRow {
                    cells: vec![cell("Hello cell"), cell("Other")],
                }],
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Trailing HELLO".into(),
                    style: RunStyle::default(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
        ],
        ..Default::default()
    }
}

#[test]
fn find_forward_wraps_case_insensitive() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(sample_doc());
    viewer.set_source_mode(false);
    viewer.set_selection_plain_offsets(0, 0);

    assert!(viewer.preview_find("hello", true, false));
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    let first = plain.to_lowercase().find("hello").unwrap();
    assert_eq!(viewer.selection_plain_offsets(), (first, first + 5));
    assert_eq!(viewer.find_match_status(), (1, 3));

    assert!(viewer.preview_find("hello", true, false));
    let second = plain.to_lowercase()[first + 5..]
        .find("hello")
        .map(|rel| first + 5 + rel)
        .unwrap();
    assert_eq!(viewer.selection_plain_offsets(), (second, second + 5));
    assert_eq!(viewer.find_match_status(), (2, 3));

    assert!(viewer.preview_find("hello", true, false));
    let third = plain.to_lowercase()[second + 5..]
        .find("hello")
        .map(|rel| second + 5 + rel)
        .unwrap();
    assert_eq!(viewer.selection_plain_offsets(), (third, third + 5));
    assert_eq!(viewer.find_match_status(), (3, 3));

    // Wrap to first.
    assert!(viewer.preview_find("hello", true, false));
    assert_eq!(viewer.selection_plain_offsets(), (first, first + 5));
    assert_eq!(viewer.find_match_status(), (1, 3));
}

#[test]
fn find_backward_and_empty_query() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(sample_doc());
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    let end = plain.len();
    viewer.set_selection_plain_offsets(end, end);

    assert!(viewer.preview_find("HELLO", false, false));
    let last = plain.to_lowercase().rfind("hello").unwrap();
    assert_eq!(viewer.selection_plain_offsets(), (last, last + 5));

    assert!(!viewer.preview_find("   ", true, false));
    assert!(!viewer.preview_find("zzz-missing", true, false));
    assert_eq!(viewer.find_match_status(), (0, 0));
}

#[test]
fn replace_current_and_all() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(sample_doc());
    viewer.set_source_mode(false);
    viewer.set_selection_plain_offsets(0, 0);

    assert!(viewer.preview_replace_current("hello", "Hi", false).unwrap());
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    assert!(plain.contains("Hi"));
    assert_eq!(plain.to_lowercase().matches("hello").count(), 2);

    assert_eq!(viewer.preview_replace_all("hello", "X", false).unwrap(), 2);
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    assert!(!plain.to_lowercase().contains("hello"));
    assert!(plain.contains('X'));
    assert!(viewer.can_undo());
    viewer.undo().unwrap();
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    assert_eq!(plain.to_lowercase().matches("hello").count(), 2);
}

#[test]
fn find_match_case_sensitive() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(sample_doc());
    viewer.set_selection_plain_offsets(0, 0);

    assert!(viewer.preview_find("HELLO", true, true));
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    let only = plain.find("HELLO").unwrap();
    assert_eq!(viewer.selection_plain_offsets(), (only, only + 5));
    assert_eq!(viewer.find_match_status(), (1, 1));

    // Lowercase needle must not match mixed-case / uppercase runs.
    assert!(!viewer.preview_find("hello", true, true));
    assert_eq!(viewer.find_match_status(), (0, 0));
}

#[test]
fn cycle_page_size_letter_and_a4() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(sample_doc());
    {
        let guard = viewer.document();
        let ps = &guard.as_ref().unwrap().page_setup;
        assert_eq!(ps.width_twips, 12240);
        assert_eq!(ps.height_twips, 15840);
    }
    {
        let ViewerSnapshot::Document(snap) = viewer.snapshot() else {
            panic!("expected document snapshot");
        };
        assert!(!snap.page_is_a4);
    }
    viewer.cycle_page_size().unwrap();
    {
        let guard = viewer.document();
        let ps = &guard.as_ref().unwrap().page_setup;
        assert_eq!(ps.width_twips, 11906);
        assert_eq!(ps.height_twips, 16838);
    }
    {
        let ViewerSnapshot::Document(snap) = viewer.snapshot() else {
            panic!("expected document snapshot");
        };
        assert!(snap.page_is_a4);
    }
    viewer.cycle_page_size().unwrap();
    {
        let guard = viewer.document();
        let ps = &guard.as_ref().unwrap().page_setup;
        assert_eq!(ps.width_twips, 12240);
        assert_eq!(ps.height_twips, 15840);
    }
    {
        let ViewerSnapshot::Document(snap) = viewer.snapshot() else {
            panic!("expected document snapshot");
        };
        assert!(!snap.page_is_a4);
    }
    viewer.undo().unwrap();
    {
        let guard = viewer.document();
        let ps = &guard.as_ref().unwrap().page_setup;
        assert_eq!(ps.width_twips, 11906);
    }
}

#[test]
fn toggle_page_orientation_swaps_dimensions() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(sample_doc());
    {
        let guard = viewer.document();
        let ps = &guard.as_ref().unwrap().page_setup;
        assert_eq!(ps.width_twips, 12240);
        assert_eq!(ps.height_twips, 15840);
    }
    viewer.toggle_page_orientation().unwrap();
    {
        let guard = viewer.document();
        let ps = &guard.as_ref().unwrap().page_setup;
        assert_eq!(ps.width_twips, 15840);
        assert_eq!(ps.height_twips, 12240);
    }
    {
        let ViewerSnapshot::Document(snap) = viewer.snapshot() else {
            panic!("expected document snapshot");
        };
        assert!(snap.page_landscape);
        assert!(!snap.page_is_a4);
    }
    viewer.cycle_page_size().unwrap();
    {
        let guard = viewer.document();
        let ps = &guard.as_ref().unwrap().page_setup;
        assert_eq!(ps.width_twips, 16838);
        assert_eq!(ps.height_twips, 11906);
    }
    {
        let ViewerSnapshot::Document(snap) = viewer.snapshot() else {
            panic!("expected document snapshot");
        };
        assert!(snap.page_landscape);
        assert!(snap.page_is_a4);
    }
}

#[test]
fn bump_indent_left_selection_steps_and_clamps() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(sample_doc());
    viewer.set_selection_plain_offsets(0, 0);

    viewer.bump_indent_left_selection(720).unwrap();
    {
        let guard = viewer.document();
        let p = match &guard.as_ref().unwrap().blocks[0] {
            Block::Paragraph(p) => p,
            _ => panic!("expected paragraph"),
        };
        assert_eq!(p.indent_left_twips, 720);
        assert_eq!(p.indent_first_line_twips, 0);
    }

    viewer.bump_indent_left_selection(720).unwrap();
    {
        let guard = viewer.document();
        let p = match &guard.as_ref().unwrap().blocks[0] {
            Block::Paragraph(p) => p,
            _ => panic!("expected paragraph"),
        };
        assert_eq!(p.indent_left_twips, 1440);
    }

    // Clamp at 2880 (2″).
    for _ in 0..10 {
        viewer.bump_indent_left_selection(720).unwrap();
    }
    {
        let guard = viewer.document();
        let p = match &guard.as_ref().unwrap().blocks[0] {
            Block::Paragraph(p) => p,
            _ => panic!("expected paragraph"),
        };
        assert_eq!(p.indent_left_twips, 2880);
    }

    viewer.bump_indent_left_selection(-720).unwrap();
    {
        let guard = viewer.document();
        let p = match &guard.as_ref().unwrap().blocks[0] {
            Block::Paragraph(p) => p,
            _ => panic!("expected paragraph"),
        };
        assert_eq!(p.indent_left_twips, 2160);
    }

    viewer.undo().unwrap();
    {
        let guard = viewer.document();
        let p = match &guard.as_ref().unwrap().blocks[0] {
            Block::Paragraph(p) => p,
            _ => panic!("expected paragraph"),
        };
        assert_eq!(p.indent_left_twips, 2880);
    }

    // Floor at 0.
    for _ in 0..20 {
        viewer.bump_indent_left_selection(-720).unwrap();
    }
    {
        let guard = viewer.document();
        let p = match &guard.as_ref().unwrap().blocks[0] {
            Block::Paragraph(p) => p,
            _ => panic!("expected paragraph"),
        };
        assert_eq!(p.indent_left_twips, 0);
    }
}

#[test]
fn bump_indent_right_selection_steps_and_clamps() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(sample_doc());
    viewer.set_selection_plain_offsets(0, 0);

    viewer.bump_indent_right_selection(720).unwrap();
    {
        let guard = viewer.document();
        let p = match &guard.as_ref().unwrap().blocks[0] {
            Block::Paragraph(p) => p,
            _ => panic!("expected paragraph"),
        };
        assert_eq!(p.indent_right_twips, 720);
        assert_eq!(p.indent_left_twips, 0);
    }

    for _ in 0..10 {
        viewer.bump_indent_right_selection(720).unwrap();
    }
    {
        let guard = viewer.document();
        let p = match &guard.as_ref().unwrap().blocks[0] {
            Block::Paragraph(p) => p,
            _ => panic!("expected paragraph"),
        };
        assert_eq!(p.indent_right_twips, 2880);
    }

    viewer.undo().unwrap();
    {
        let guard = viewer.document();
        let p = match &guard.as_ref().unwrap().blocks[0] {
            Block::Paragraph(p) => p,
            _ => panic!("expected paragraph"),
        };
        assert!(p.indent_right_twips < 2880);
    }

    for _ in 0..20 {
        viewer.bump_indent_right_selection(-720).unwrap();
    }
    {
        let guard = viewer.document();
        let p = match &guard.as_ref().unwrap().blocks[0] {
            Block::Paragraph(p) => p,
            _ => panic!("expected paragraph"),
        };
        assert_eq!(p.indent_right_twips, 0);
    }
}

#[test]
fn bump_indent_first_line_selection_steps_and_clamps() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(sample_doc());
    viewer.set_selection_plain_offsets(0, 0);

    viewer.bump_indent_first_line_selection(360).unwrap();
    {
        let guard = viewer.document();
        let p = match &guard.as_ref().unwrap().blocks[0] {
            Block::Paragraph(p) => p,
            _ => panic!("expected paragraph"),
        };
        assert_eq!(p.indent_first_line_twips, 360);
        assert_eq!(p.indent_left_twips, 0);
    }

    viewer.bump_indent_first_line_selection(-720).unwrap();
    {
        let guard = viewer.document();
        let p = match &guard.as_ref().unwrap().blocks[0] {
            Block::Paragraph(p) => p,
            _ => panic!("expected paragraph"),
        };
        assert_eq!(p.indent_first_line_twips, -360);
    }

    for _ in 0..20 {
        viewer.bump_indent_first_line_selection(-360).unwrap();
    }
    {
        let guard = viewer.document();
        let p = match &guard.as_ref().unwrap().blocks[0] {
            Block::Paragraph(p) => p,
            _ => panic!("expected paragraph"),
        };
        assert_eq!(p.indent_first_line_twips, -1440);
    }

    for _ in 0..40 {
        viewer.bump_indent_first_line_selection(360).unwrap();
    }
    {
        let guard = viewer.document();
        let p = match &guard.as_ref().unwrap().blocks[0] {
            Block::Paragraph(p) => p,
            _ => panic!("expected paragraph"),
        };
        assert_eq!(p.indent_first_line_twips, 1440);
    }

    viewer.undo().unwrap();
    {
        let guard = viewer.document();
        let p = match &guard.as_ref().unwrap().blocks[0] {
            Block::Paragraph(p) => p,
            _ => panic!("expected paragraph"),
        };
        assert!(p.indent_first_line_twips < 1440);
    }
}

#[test]
fn find_sets_scroll_y_for_later_match() {
    let viewer = DocumentViewer::new();
    let mut blocks = Vec::new();
    for i in 0..40 {
        blocks.push(Block::Paragraph(Paragraph {
            runs: vec![Run {
                text: format!("Line {i} filler text for scrolling."),
                style: RunStyle::default(),
                ..Default::default()
            }],
            ..Default::default()
        }));
    }
    blocks.push(Block::Paragraph(Paragraph {
        runs: vec![Run {
            text: "UNIQUE_FIND_TARGET here".into(),
            style: RunStyle::default(),
            ..Default::default()
        }],
        ..Default::default()
    }));
    *viewer.document_mut() = Some(Doc {
        blocks,
        ..Default::default()
    });
    viewer.set_preview_width(400.0);
    assert!(viewer.preview_find("UNIQUE_FIND_TARGET", true, true));
    let ViewerSnapshot::Document(snap) = viewer.snapshot() else {
        panic!("expected document snapshot");
    };
    assert!(
        snap.find_scroll_y_px > 100,
        "late match should scroll down: y={}",
        snap.find_scroll_y_px
    );
}

#[test]
fn bump_paragraph_spacing_before_and_after() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(sample_doc());
    viewer.set_selection_plain_offsets(0, 0);

    viewer.bump_paragraph_spacing_selection(120, 0).unwrap();
    {
        let guard = viewer.document();
        let p = match &guard.as_ref().unwrap().blocks[0] {
            Block::Paragraph(p) => p,
            _ => panic!("expected paragraph"),
        };
        assert_eq!(p.space_before_twips, 120);
        assert_eq!(p.space_after_twips, 0);
    }

    viewer.bump_paragraph_spacing_selection(0, 120).unwrap();
    {
        let guard = viewer.document();
        let p = match &guard.as_ref().unwrap().blocks[0] {
            Block::Paragraph(p) => p,
            _ => panic!("expected paragraph"),
        };
        assert_eq!(p.space_before_twips, 120);
        assert_eq!(p.space_after_twips, 120);
    }

    viewer.undo().unwrap();
    {
        let guard = viewer.document();
        let p = match &guard.as_ref().unwrap().blocks[0] {
            Block::Paragraph(p) => p,
            _ => panic!("expected paragraph"),
        };
        assert_eq!(p.space_after_twips, 0);
        assert_eq!(p.space_before_twips, 120);
    }
}

#[test]
fn insert_comment_on_selection_and_undo() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(sample_doc());
    viewer.set_selection_plain_offsets(0, 5);
    let id = viewer.insert_comment_at_selection().unwrap();
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        assert_eq!(doc.comments.len(), 1);
        assert_eq!(doc.comments[0].id, id);
        assert_eq!(doc.comments[0].author, "Orchid");
        assert!(!doc.comments[0].text.is_empty());
        assert_eq!(doc.comment_ranges.len(), 1);
        assert_eq!(doc.comment_ranges[0].id, id);
        assert_eq!(doc.comment_ranges[0].start_plain, 0);
        assert_eq!(doc.comment_ranges[0].end_plain, 5);
    }
    viewer.undo().unwrap();
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        assert!(doc.comments.is_empty());
        assert!(doc.comment_ranges.is_empty());
    }
}

#[test]
fn delete_comment_at_caret_and_undo() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(sample_doc());
    viewer.set_selection_plain_offsets(0, 5);
    let id = viewer.insert_comment_at_selection().unwrap();
    viewer.set_selection_plain_offsets(2, 2);
    assert_eq!(viewer.delete_comment_at_selection().unwrap(), Some(id));
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        assert!(doc.comments.is_empty());
        assert!(doc.comment_ranges.is_empty());
    }
    viewer.undo().unwrap();
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        assert_eq!(doc.comments.len(), 1);
        assert_eq!(doc.comment_ranges.len(), 1);
    }
    viewer.set_selection_plain_offsets(1000, 1000);
    assert_eq!(viewer.delete_comment_at_selection().unwrap(), None);
}

#[test]
fn comment_at_caret_in_snapshot() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(sample_doc());
    viewer.set_selection_plain_offsets(0, 5);
    viewer.insert_comment_at_selection().unwrap();
    viewer.set_selection_plain_offsets(2, 2);
    let ViewerSnapshot::Document(snap) = viewer.snapshot() else {
        panic!("expected document");
    };
    assert!(
        snap.comment_at_caret.contains("Orchid"),
        "got {:?}",
        snap.comment_at_caret
    );
    viewer.set_selection_plain_offsets(1000, 1000);
    let ViewerSnapshot::Document(snap) = viewer.snapshot() else {
        panic!("expected document");
    };
    assert!(snap.comment_at_caret.is_empty());
}

#[test]
fn set_comment_text_at_caret_and_undo() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(sample_doc());
    viewer.set_selection_plain_offsets(0, 5);
    let id = viewer.insert_comment_at_selection().unwrap();
    viewer.set_selection_plain_offsets(2, 2);
    assert_eq!(
        viewer.set_comment_text_at_selection("Edited note").unwrap(),
        Some(id)
    );
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        assert_eq!(doc.comments[0].text, "Edited note");
    }
    let ViewerSnapshot::Document(snap) = viewer.snapshot() else {
        panic!("expected document");
    };
    assert_eq!(snap.comment_edit_text, "Edited note");
    assert!(snap.comment_at_caret.contains("Edited note"));
    viewer.undo().unwrap();
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        assert_ne!(doc.comments[0].text, "Edited note");
    }
    assert_eq!(viewer.set_comment_text_at_selection("   ").unwrap(), None);
    viewer.set_selection_plain_offsets(1000, 1000);
    assert_eq!(
        viewer.set_comment_text_at_selection("orphan").unwrap(),
        None
    );
}

#[test]
fn goto_comment_next_prev_wraps() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(sample_doc());
    viewer.set_selection_plain_offsets(0, 5);
    let id0 = viewer.insert_comment_at_selection().unwrap();
    viewer.set_selection_plain_offsets(6, 11);
    let id1 = viewer.insert_comment_at_selection().unwrap();
    assert_ne!(id0, id1);
    viewer.set_selection_plain_offsets(0, 0);
    assert!(viewer.goto_comment(true).unwrap());
    assert_eq!(viewer.selection_plain_offsets(), (0, 5));
    assert!(viewer.goto_comment(true).unwrap());
    assert_eq!(viewer.selection_plain_offsets(), (6, 11));
    assert!(viewer.goto_comment(true).unwrap());
    assert_eq!(viewer.selection_plain_offsets(), (0, 5));
    assert!(viewer.goto_comment(false).unwrap());
    assert_eq!(viewer.selection_plain_offsets(), (6, 11));
}

#[test]
fn page_chrome_edits_caret_section_not_only_trailing() {
    let mut sect0 = PageSetup::default(); // Letter
    sect0.width_twips = 12240;
    sect0.height_twips = 15840;
    let mut trailing = PageSetup::default();
    trailing.width_twips = 11906; // A4
    trailing.height_twips = 16838;
    let doc = Doc {
        blocks: vec![
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "SectionZero".into(),
                    style: RunStyle::default(),
                    ..Default::default()
                }],
                section_properties: Some(sect0.clone()),
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "TrailingSect".into(),
                    style: RunStyle::default(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
        ],
        page_setup: trailing.clone(),
        ..Default::default()
    };
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(doc);

    // Caret in section 0 → Pg cycles Letter → A4 on mid-body only.
    viewer.set_selection_plain_offsets(0, 0);
    {
        let ViewerSnapshot::Document(snap) = viewer.snapshot() else {
            panic!("expected document");
        };
        assert!(!snap.page_is_a4, "caret in Letter section");
    }
    viewer.cycle_page_size().unwrap();
    {
        let guard = viewer.document();
        let d = guard.as_ref().unwrap();
        let Block::Paragraph(p0) = &d.blocks[0] else {
            panic!("p0");
        };
        let ps = p0.section_properties.as_ref().unwrap();
        assert_eq!(ps.width_twips, 11906);
        assert_eq!(ps.height_twips, 16838);
        assert_eq!(d.page_setup.width_twips, 11906, "trailing untouched");
        assert_eq!(d.page_setup.height_twips, 16838);
    }
    {
        let ViewerSnapshot::Document(snap) = viewer.snapshot() else {
            panic!("expected document");
        };
        assert!(snap.page_is_a4);
    }
    viewer.undo().unwrap();
    {
        let guard = viewer.document();
        let d = guard.as_ref().unwrap();
        let Block::Paragraph(p0) = &d.blocks[0] else {
            panic!("p0");
        };
        assert_eq!(
            p0.section_properties.as_ref().unwrap().width_twips,
            12240
        );
    }

    // Caret in trailing section → Or flips trailing only.
    viewer.set_selection_plain_offsets(20, 20); // into "TrailingSect"
    viewer.toggle_page_orientation().unwrap();
    {
        let guard = viewer.document();
        let d = guard.as_ref().unwrap();
        assert_eq!(d.page_setup.width_twips, 16838);
        assert_eq!(d.page_setup.height_twips, 11906);
        let Block::Paragraph(p0) = &d.blocks[0] else {
            panic!("p0");
        };
        assert_eq!(
            p0.section_properties.as_ref().unwrap().width_twips,
            12240,
            "mid-body untouched"
        );
    }
    {
        let ViewerSnapshot::Document(snap) = viewer.snapshot() else {
            panic!("expected document");
        };
        assert!(snap.page_landscape);
        assert!(snap.page_is_a4);
    }
}

#[test]
fn section_break_stamps_caret_section_page_setup() {
    let mut sect0 = PageSetup::default();
    sect0.margin_left_twips = 720;
    sect0.width_twips = 12240;
    let mut trailing = PageSetup::default();
    trailing.margin_left_twips = 2160;
    trailing.width_twips = 11906;
    let doc = Doc {
        blocks: vec![
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "AAAA".into(),
                    ..Default::default()
                }],
                section_properties: Some(sect0.clone()),
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "BBBB".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
        ],
        page_setup: trailing,
        ..Default::default()
    };
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(doc);
    viewer.set_source_mode(false);
    viewer.set_selection_plain_offsets(2, 2); // in section 0
    viewer.preview_insert_section_break().unwrap();
    {
        let guard = viewer.document();
        let d = guard.as_ref().unwrap();
        // Ending para of first section should carry section-0 geometry.
        let Block::Paragraph(p0) = &d.blocks[0] else {
            panic!("p0");
        };
        let ps = p0.section_properties.as_ref().expect("sectPr on split left");
        assert_eq!(ps.margin_left_twips, 720);
        assert_eq!(ps.width_twips, 12240);
    }
}
