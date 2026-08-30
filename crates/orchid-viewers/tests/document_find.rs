//! Find next/previous in the DOCX plain-text stream.

use orchid_viewers::document::model::{
    Block, Document as Doc, Paragraph, Run, RunStyle, Table, TableCell, TableRow,
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
