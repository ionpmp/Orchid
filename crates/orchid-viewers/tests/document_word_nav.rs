//! Word navigation / deletion for the DOCX preview caret.

use orchid_viewers::document::model::{Block, Document as Doc, Paragraph, Run, RunStyle};
use orchid_viewers::document::DocumentViewer;
use orchid_viewers::Viewer;

#[test]
fn preview_word_move_and_word_delete() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(Doc {
        blocks: vec![Block::Paragraph(Paragraph {
            runs: vec![Run {
                text: "Hello brave world".into(),
                style: RunStyle::default(),
            ..Default::default()
        }],
            ..Default::default()
        })],
        ..Default::default()
    });
    viewer.set_source_mode(false);
    // Caret at start of "brave" (byte 6).
    viewer.set_selection_plain_offsets(6, 6);
    viewer.preview_move_by_words(1, false);
    assert_eq!(viewer.selection().head.byte_offset, 12); // start of "world"
    viewer.preview_move_by_words(1, false);
    assert_eq!(viewer.selection().head.byte_offset, 17); // end
    viewer.preview_move_by_words(-1, false);
    assert_eq!(viewer.selection().head.byte_offset, 12); // start of "world"
    viewer.preview_move_by_words(-1, true);
    assert_eq!(viewer.selected_plain_text(), "brave ");

    viewer.set_selection_plain_offsets(17, 17);
    viewer.preview_delete_word_backward().unwrap();
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        assert_eq!(doc.plain_text(), "Hello brave ");
    }
    viewer.set_selection_plain_offsets(6, 6);
    viewer.preview_delete_word_forward().unwrap();
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        assert_eq!(doc.plain_text(), "Hello ");
    }
}

#[test]
fn preview_select_word_at_offset() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(Doc {
        blocks: vec![Block::Paragraph(Paragraph {
            runs: vec![Run {
                text: "Hello brave world".into(),
                style: RunStyle::default(),
            ..Default::default()
        }],
            ..Default::default()
        })],
        ..Default::default()
    });
    viewer.set_source_mode(false);
    // Mid-"brave" → select whole word.
    viewer.preview_select_word_at_offset(8);
    assert_eq!(viewer.selected_plain_text(), "brave");
    // On the space after "Hello" → snap into preceding word.
    viewer.preview_select_word_at_offset(5);
    assert_eq!(viewer.selected_plain_text(), "Hello");
    // Punctuation run.
    *viewer.document_mut() = Some(Doc {
        blocks: vec![Block::Paragraph(Paragraph {
            runs: vec![Run {
                text: "Hi!!! there".into(),
                style: RunStyle::default(),
            ..Default::default()
        }],
            ..Default::default()
        })],
        ..Default::default()
    });
    viewer.preview_select_word_at_offset(3);
    assert_eq!(viewer.selected_plain_text(), "!!!");
}

#[test]
fn preview_triple_click_selects_paragraph() {
    use orchid_viewers::document::layout::PREVIEW_PADDING;

    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(Doc {
        blocks: vec![
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "First paragraph here".into(),
                    style: RunStyle::default(),
            ..Default::default()
        }],
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Second line".into(),
                    style: RunStyle::default(),
            ..Default::default()
        }],
                ..Default::default()
            }),
        ],
        ..Default::default()
    });
    viewer.set_source_mode(false);
    let x = PREVIEW_PADDING + 8.0;
    let y = PREVIEW_PADDING + 8.0;

    // Explicit phase 4 (triple-click).
    viewer.preview_pointer(4, x, y);
    assert_eq!(viewer.selected_plain_text(), "First paragraph here");

    // Multi-click downs: 1 caret → 2 word → 3 paragraph.
    viewer.preview_pointer(0, x, y);
    viewer.preview_pointer(0, x, y);
    assert_eq!(viewer.selected_plain_text(), "First");
    viewer.preview_pointer(0, x, y);
    assert_eq!(viewer.selected_plain_text(), "First paragraph here");
}

#[test]
fn preview_font_family_cycle_and_set() {
    use orchid_viewers::ViewerSnapshot;

    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(Doc {
        blocks: vec![Block::Paragraph(Paragraph {
            runs: vec![Run {
                text: "Hello".into(),
                style: RunStyle::default(),
            ..Default::default()
        }],
            ..Default::default()
        })],
        ..Default::default()
    });
    viewer.set_source_mode(false);
    viewer.set_selection_plain_offsets(0, 5);
    viewer.set_font_family_selection("Calibri").unwrap();
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(p.runs[0].style.font_family.as_deref(), Some("Calibri"));
            }
            _ => panic!("p0"),
        }
    }
    viewer.bump_font_family_selection(1).unwrap();
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(p.runs[0].style.font_family.as_deref(), Some("Arial"));
            }
            _ => panic!("p0"),
        }
    }
    let ViewerSnapshot::Document(snap) = viewer.snapshot() else {
        panic!("snapshot");
    };
    assert_eq!(snap.font_family, "Arial");
}

#[test]
fn preview_list_indent_outdent() {
    use orchid_viewers::document::model::ListKind;

    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(Doc {
        blocks: vec![Block::Paragraph(Paragraph {
            runs: vec![Run {
                text: "Item".into(),
                style: RunStyle::default(),
            ..Default::default()
        }],
            list: ListKind::Bullet,
            list_level: 0,
            ..Default::default()
        })],
        ..Default::default()
    });
    viewer.set_source_mode(false);
    viewer.set_selection_plain_offsets(0, 0);
    viewer.bump_list_level_selection(1).unwrap();
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(p.list, ListKind::Bullet);
                assert_eq!(p.list_level, 1);
            }
            _ => panic!("p0"),
        }
    }
    viewer.bump_list_level_selection(1).unwrap();
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => assert_eq!(p.list_level, 2),
            _ => panic!("p0"),
        }
    }
    viewer.bump_list_level_selection(-1).unwrap();
    viewer.bump_list_level_selection(-1).unwrap();
    viewer.bump_list_level_selection(-1).unwrap(); // past 0 → clear list
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(p.list, ListKind::None);
                assert_eq!(p.list_level, 0);
            }
            _ => panic!("p0"),
        }
    }
}
