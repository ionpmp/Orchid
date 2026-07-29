//! Table cell cursor / typing for the DOCX preview.

use orchid_viewers::document::model::{
    Alignment, Block, Document as Doc, ListKind, Paragraph, Run, RunStyle, Table, TableCell,
    TableRow,
};
use orchid_viewers::document::{cursor_from_plain_offset, DocumentViewer};

fn table_2x2() -> Doc {
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

fn cell_paras(doc: &Doc, row: usize, col: usize) -> Vec<String> {
    match &doc.blocks[1] {
        Block::Table(t) => t.rows[row].cells[col]
            .paragraphs
            .iter()
            .map(|p| p.plain_text())
            .collect(),
        _ => panic!("expected table"),
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
    assert_eq!(cell_paras(doc, 0, 0), vec!["R1C1!".to_string()]);
    assert_eq!(cell_paras(doc, 0, 1), vec!["R1C2".to_string()]);
    assert_eq!(cell_paras(doc, 1, 0), vec!["R2C1".to_string()]);
    assert_eq!(cell_paras(doc, 1, 1), vec!["R2C2".to_string()]);
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
fn enter_splits_table_cell_paragraph() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(table_2x2());
    viewer.set_source_mode(false);
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    // Split after "R2" inside "R2C2".
    let off = plain.find("R2C2").expect("R2C2") + 2;
    viewer.set_selection_plain_offsets(off, off);
    viewer.preview_insert_paragraph_break().unwrap();

    let guard = viewer.document();
    let doc = guard.as_ref().unwrap();
    assert_eq!(
        cell_paras(doc, 1, 1),
        vec!["R2".to_string(), "C2".to_string()]
    );
    assert_eq!(cell_paras(doc, 0, 0), vec!["R1C1".to_string()]);
    let caret = viewer.selection().head;
    assert_eq!(caret.cell.map(|c| c.para_idx), Some(1));
    assert_eq!(caret.byte_offset, 0);
}

#[test]
fn backspace_joins_table_cell_paragraphs() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(table_2x2());
    viewer.set_source_mode(false);
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    let off = plain.find("R1C1").expect("R1C1") + 2;
    viewer.set_selection_plain_offsets(off, off);
    viewer.preview_insert_paragraph_break().unwrap();
    {
        let guard = viewer.document();
        assert_eq!(
            cell_paras(guard.as_ref().unwrap(), 0, 0),
            vec!["R1".to_string(), "C1".to_string()]
        );
    }
    // Caret is at start of second para — Backspace joins.
    viewer.preview_delete_backward().unwrap();
    let guard = viewer.document();
    assert_eq!(
        cell_paras(guard.as_ref().unwrap(), 0, 0),
        vec!["R1C1".to_string()]
    );
}

#[test]
fn paste_multiline_into_table_cell() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(table_2x2());
    viewer.set_source_mode(false);
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    let off = plain.find("R1C2").expect("R1C2") + "R1C2".len();
    viewer.set_selection_plain_offsets(off, off);
    viewer.preview_paste_plain("\nline2\nline3").unwrap();
    let guard = viewer.document();
    assert_eq!(
        cell_paras(guard.as_ref().unwrap(), 0, 1),
        vec![
            "R1C2".to_string(),
            "line2".to_string(),
            "line3".to_string()
        ]
    );
    assert_eq!(
        cell_paras(guard.as_ref().unwrap(), 0, 0),
        vec!["R1C1".to_string()]
    );
}

#[test]
fn backspace_at_cell_start_moves_without_merging_neighbor() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(table_2x2());
    viewer.set_source_mode(false);
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    let off = plain.find("R1C2").expect("R1C2");
    viewer.set_selection_plain_offsets(off, off);
    viewer.preview_delete_backward().unwrap();
    let guard = viewer.document();
    let doc = guard.as_ref().unwrap();
    assert_eq!(cell_paras(doc, 0, 0), vec!["R1C1".to_string()]);
    assert_eq!(cell_paras(doc, 0, 1), vec!["R1C2".to_string()]);
    // Caret moved to end of previous cell.
    let caret = viewer.selection().head;
    assert_eq!(caret.cell.map(|c| (c.row, c.col)), Some((0, 0)));
    assert_eq!(caret.byte_offset, 4);
}

#[test]
fn align_and_list_in_table_cell() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(table_2x2());
    viewer.set_source_mode(false);
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    let off = plain.find("R1C1").expect("R1C1");
    viewer.set_selection_plain_offsets(off, off);
    viewer.set_alignment_all(Alignment::Center).unwrap();
    viewer.toggle_list_all(ListKind::Bullet).unwrap();
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        match &doc.blocks[1] {
            Block::Table(t) => {
                let p = &t.rows[0].cells[0].paragraphs[0];
                assert_eq!(p.alignment, Alignment::Center);
                assert_eq!(p.list, ListKind::Bullet);
                assert_eq!(t.rows[0].cells[1].paragraphs[0].alignment, Alignment::Left);
                assert_eq!(t.rows[0].cells[1].paragraphs[0].list, ListKind::None);
            }
            _ => panic!("table"),
        }
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(p.alignment, Alignment::Left);
                assert_eq!(p.list, ListKind::None);
            }
            _ => panic!("p0"),
        }
    }
    viewer.bump_list_level_selection(1).unwrap();
    {
        let guard = viewer.document();
        match &guard.as_ref().unwrap().blocks[1] {
            Block::Table(t) => {
                assert_eq!(t.rows[0].cells[0].paragraphs[0].list_level, 1);
            }
            _ => panic!("table"),
        }
    }
    viewer.bump_list_level_selection(-1).unwrap();
    viewer.bump_list_level_selection(-1).unwrap(); // clear list
    let guard = viewer.document();
    match &guard.as_ref().unwrap().blocks[1] {
        Block::Table(t) => {
            let p = &t.rows[0].cells[0].paragraphs[0];
            assert_eq!(p.list, ListKind::None);
            assert_eq!(p.list_level, 0);
            assert_eq!(p.alignment, Alignment::Center);
        }
        _ => panic!("table"),
    }
}

#[test]
fn align_both_paragraphs_in_same_cell() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(table_2x2());
    viewer.set_source_mode(false);
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    let off = plain.find("R2C2").expect("R2C2") + 2;
    viewer.set_selection_plain_offsets(off, off);
    viewer.preview_insert_paragraph_break().unwrap();
    // Select both paragraphs of the split cell ("R2\nC2" — last occurrence).
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    let start = plain.rfind("R2\nC2").expect("R2\\nC2");
    let end = start + "R2\nC2".len();
    viewer.set_selection_plain_offsets(start, end);
    viewer.set_alignment_all(Alignment::Right).unwrap();
    let guard = viewer.document();
    match &guard.as_ref().unwrap().blocks[1] {
        Block::Table(t) => {
            let cell = &t.rows[1].cells[1];
            assert_eq!(cell.paragraphs.len(), 2);
            assert_eq!(cell.paragraphs[0].alignment, Alignment::Right);
            assert_eq!(cell.paragraphs[1].alignment, Alignment::Right);
            assert_eq!(t.rows[1].cells[0].paragraphs[0].alignment, Alignment::Left);
        }
        _ => panic!("table"),
    }
}

#[test]
fn tab_navigates_table_cells() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(table_2x2());
    viewer.set_source_mode(false);
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    let off = plain.find("R1C1").expect("R1C1");
    viewer.set_selection_plain_offsets(off, off);
    assert!(viewer.preview_move_table_cell(true));
    assert_eq!(
        viewer.selection().head.cell.map(|c| (c.row, c.col)),
        Some((0, 1))
    );
    assert!(viewer.preview_move_table_cell(true));
    assert_eq!(
        viewer.selection().head.cell.map(|c| (c.row, c.col)),
        Some((1, 0))
    );
    assert!(viewer.preview_move_table_cell(true));
    assert_eq!(
        viewer.selection().head.cell.map(|c| (c.row, c.col)),
        Some((1, 1))
    );
    // Last cell — no further move.
    assert!(!viewer.preview_move_table_cell(true));
    assert_eq!(
        viewer.selection().head.cell.map(|c| (c.row, c.col)),
        Some((1, 1))
    );
    assert!(viewer.preview_move_table_cell(false));
    assert_eq!(
        viewer.selection().head.cell.map(|c| (c.row, c.col)),
        Some((1, 0))
    );
    // Neighbors unchanged.
    let guard = viewer.document();
    assert_eq!(
        cell_paras(guard.as_ref().unwrap(), 0, 0),
        vec!["R1C1".to_string()]
    );
}

#[test]
fn insert_row_below_caret() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(table_2x2());
    viewer.set_source_mode(false);
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    let off = plain.find("R1C1").expect("R1C1");
    viewer.set_selection_plain_offsets(off, off);
    viewer.preview_insert_table_row().unwrap();
    let guard = viewer.document();
    match &guard.as_ref().unwrap().blocks[1] {
        Block::Table(t) => {
            assert_eq!(t.rows.len(), 3);
            assert_eq!(t.rows[1].cells[0].paragraphs[0].plain_text(), "");
            assert_eq!(t.rows[2].cells[0].paragraphs[0].plain_text(), "R2C1");
        }
        _ => panic!("table"),
    }
    assert_eq!(
        viewer.selection().head.cell.map(|c| (c.row, c.col)),
        Some((1, 0))
    );
}

#[test]
fn insert_column_right_of_caret() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(table_2x2());
    viewer.set_source_mode(false);
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    let off = plain.find("R1C1").expect("R1C1");
    viewer.set_selection_plain_offsets(off, off);
    viewer.preview_insert_table_column().unwrap();
    let guard = viewer.document();
    match &guard.as_ref().unwrap().blocks[1] {
        Block::Table(t) => {
            assert_eq!(t.rows[0].cells.len(), 3);
            assert_eq!(t.rows[0].cells[1].paragraphs[0].plain_text(), "");
            assert_eq!(t.rows[0].cells[2].paragraphs[0].plain_text(), "R1C2");
        }
        _ => panic!("table"),
    }
    assert_eq!(
        viewer.selection().head.cell.map(|c| (c.row, c.col)),
        Some((0, 1))
    );
}

#[test]
fn delete_row_and_column_refuse_last() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(table_2x2());
    viewer.set_source_mode(false);
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    let off = plain.find("R2C2").expect("R2C2");
    viewer.set_selection_plain_offsets(off, off);
    viewer.preview_delete_table_row().unwrap();
    {
        let guard = viewer.document();
        match &guard.as_ref().unwrap().blocks[1] {
            Block::Table(t) => assert_eq!(t.rows.len(), 1),
            _ => panic!("table"),
        }
    }
    assert!(viewer.preview_delete_table_row().is_err());

    viewer.preview_delete_table_column().unwrap();
    {
        let guard = viewer.document();
        match &guard.as_ref().unwrap().blocks[1] {
            Block::Table(t) => assert_eq!(t.rows[0].cells.len(), 1),
            _ => panic!("table"),
        }
    }
    assert!(viewer.preview_delete_table_column().is_err());
}

#[test]
fn insert_table_after_caret_block() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(table_2x2());
    viewer.set_source_mode(false);
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    let off = plain.find("Before").expect("Before") + "Before".len();
    viewer.set_selection_plain_offsets(off, off);
    viewer.preview_insert_table(2, 3).unwrap();
    let guard = viewer.document();
    let doc = guard.as_ref().unwrap();
    assert_eq!(doc.blocks.len(), 4);
    match &doc.blocks[1] {
        Block::Table(t) => {
            assert_eq!(t.rows.len(), 2);
            assert_eq!(t.rows[0].cells.len(), 3);
            assert_eq!(t.rows[0].cells[0].paragraphs[0].plain_text(), "");
        }
        _ => panic!("expected new table at block 1"),
    }
    match &doc.blocks[2] {
        Block::Table(_) => {}
        _ => panic!("original table shifted to block 2"),
    }
    assert_eq!(
        viewer.selection().head.cell.map(|c| (c.row, c.col)),
        Some((0, 0))
    );
    assert_eq!(viewer.selection().head.block_idx, 1);
}

#[test]
fn tab_outside_table_does_not_move_cells() {
    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(table_2x2());
    viewer.set_source_mode(false);
    let plain = {
        let guard = viewer.document();
        guard.as_ref().unwrap().plain_text()
    };
    let off = plain.find("Before").expect("Before");
    viewer.set_selection_plain_offsets(off, off);
    assert!(!viewer.preview_move_table_cell(true));
    assert!(viewer.selection().head.cell.is_none());
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
