//! Find next/previous in the DOCX plain-text stream.

use orchid_viewers::document::model::{
    Block, Document as Doc, Paragraph, Run, RunStyle, Table, TableCell, TableRow,
};
use orchid_viewers::document::DocumentViewer;

fn sample_doc() -> Doc {
    fn cell(text: &str) -> TableCell {
        TableCell::from_paragraphs(vec![Paragraph {
            runs: vec![Run {
                text: text.into(),
                style: RunStyle::default(),
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

    assert!(viewer.preview_find("hello", true));
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    let first = plain.to_lowercase().find("hello").unwrap();
    assert_eq!(viewer.selection_plain_offsets(), (first, first + 5));
    assert_eq!(viewer.find_match_status(), (1, 3));

    assert!(viewer.preview_find("hello", true));
    let second = plain.to_lowercase()[first + 5..]
        .find("hello")
        .map(|rel| first + 5 + rel)
        .unwrap();
    assert_eq!(viewer.selection_plain_offsets(), (second, second + 5));
    assert_eq!(viewer.find_match_status(), (2, 3));

    assert!(viewer.preview_find("hello", true));
    let third = plain.to_lowercase()[second + 5..]
        .find("hello")
        .map(|rel| second + 5 + rel)
        .unwrap();
    assert_eq!(viewer.selection_plain_offsets(), (third, third + 5));
    assert_eq!(viewer.find_match_status(), (3, 3));

    // Wrap to first.
    assert!(viewer.preview_find("hello", true));
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

    assert!(viewer.preview_find("HELLO", false));
    let last = plain.to_lowercase().rfind("hello").unwrap();
    assert_eq!(viewer.selection_plain_offsets(), (last, last + 5));

    assert!(!viewer.preview_find("   ", true));
    assert!(!viewer.preview_find("zzz-missing", true));
    assert_eq!(viewer.find_match_status(), (0, 0));
}
