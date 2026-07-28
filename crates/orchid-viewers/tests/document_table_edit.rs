//! Table cell cursor / typing for the DOCX preview.

use orchid_viewers::document::model::{
    Block, Document as Doc, Paragraph, Run, RunStyle, Table, TableCell, TableRow,
};
use orchid_viewers::document::{cursor_from_plain_offset, DocumentViewer};
use orchid_viewers::Viewer;

fn table_2x2() -> Doc {
    fn cell(text: &str) -> TableCell {
        TableCell {
            paragraphs: vec![Paragraph {
                runs: vec![Run {
                    text: text.into(),
                    style: RunStyle::default(),
                }],
                ..Default::default()
            }],
        }
    }
    Doc {
        blocks: vec![
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Before".into(),
                    style: RunStyle::default(),
                }],
                ..Default::default()
            }),
            Block::Table(Table {
                rows: vec![
                    TableRow {
                        cells: vec![cell("R1C1"), cell("R1C2")],
                    },
                    TableRow {
                        cells: vec![cell("R2C1"), cell("R2C2")],
                    },
                ],
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "After".into(),
                    style: RunStyle::default(),
                }],
                ..Default::default()
            }),
        ],
        ..Default::default()
    }
}

#[test]
fn type_into_table_cell_keeps_neighbors() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(table_2x2());
    viewer.set_source_mode(false);

    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    let off = plain.find("R1C1").expect("R1C1") + "R1C1".len();
    viewer.set_selection_plain_offsets(off, off);
    assert!(viewer.selection().head.cell.is_some());
    viewer.preview_insert_text("!").unwrap();

    let guard = viewer.document();
    let doc = guard.as_ref().unwrap();
    match &doc.blocks[1] {
        Block::Table(t) => {
            assert_eq!(t.rows[0].cells[0].paragraphs[0].plain_text(), "R1C1!");
            assert_eq!(t.rows[0].cells[1].paragraphs[0].plain_text(), "R1C2");
            assert_eq!(t.rows[1].cells[0].paragraphs[0].plain_text(), "R2C1");
            assert_eq!(t.rows[1].cells[1].paragraphs[0].plain_text(), "R2C2");
        }
        _ => panic!("expected table"),
    }
    match &doc.blocks[0] {
        Block::Paragraph(p) => assert_eq!(p.plain_text(), "Before"),
        _ => panic!("p0"),
    }
    match &doc.blocks[2] {
        Block::Paragraph(p) => assert_eq!(p.plain_text(), "After"),
        _ => panic!("p2"),
    }
}

#[test]
fn enter_in_table_cell_is_rejected() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(table_2x2());
    viewer.set_source_mode(false);
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    let off = plain.find("R2C2").expect("R2C2");
    viewer.set_selection_plain_offsets(off, off);
    assert!(viewer.preview_insert_paragraph_break().is_err());
}

#[test]
fn body_after_table_still_editable() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(table_2x2());
    viewer.set_source_mode(false);
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    let off = plain.find("After").expect("After") + "After".len();
    let c = {
        let guard = viewer.document();
        cursor_from_plain_offset(guard.as_ref().unwrap(), off)
    };
    assert!(c.cell.is_none());
    assert_eq!(c.block_idx, 2);
    viewer.set_selection_plain_offsets(off, off);
    viewer.preview_insert_text("!").unwrap();
    let guard = viewer.document();
    match &guard.as_ref().unwrap().blocks[2] {
        Block::Paragraph(p) => assert_eq!(p.plain_text(), "After!"),
        _ => panic!("p2"),
    }
}
