//! Synthetic DOCX round-trips (no Microsoft Word binaries required).

use orchid_viewers::document::model::{
    Alignment, Block, Document, ListKind, Paragraph, Run, RunStyle,
};
use orchid_viewers::document::ooxml::container::{open_document, save_document};
use orchid_viewers::{DocumentViewer, Viewer, ViewerSnapshot};

#[tokio::test]
async fn roundtrip_styles_align_lists() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("styled.docx");

    let doc = Document {
        blocks: vec![
            Block::Paragraph(Paragraph {
                runs: vec![
                    Run {
                        text: "Bold".into(),
                        style: RunStyle {
                            bold: true,
                            ..Default::default()
                        },
                    },
                    Run {
                        text: " and ".into(),
                        style: RunStyle::default(),
                    },
                    Run {
                        text: "italic".into(),
                        style: RunStyle {
                            italic: true,
                            ..Default::default()
                        },
                    },
                ],
                alignment: Alignment::Left,
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Centered".into(),
                    style: RunStyle {
                        underline: true,
                        color: Some([0xCC, 0x00, 0x00]),
                        ..Default::default()
                    },
                }],
                alignment: Alignment::Center,
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Right aligned".into(),
                    style: RunStyle::default(),
                }],
                alignment: Alignment::Right,
                ..Default::default()
            }),
        ],
        ..Default::default()
    };

    save_document(&doc, &path).await.unwrap();
    let loaded = open_document(&path).unwrap();

    assert_eq!(loaded.blocks.len(), 3);
    match &loaded.blocks[0] {
        Block::Paragraph(p) => {
            assert!(p
                .runs
                .iter()
                .any(|r| r.style.bold && r.text.contains("Bold")));
            assert!(p
                .runs
                .iter()
                .any(|r| r.style.italic && r.text.contains("italic")));
        }
        _ => panic!("p0"),
    }
    match &loaded.blocks[1] {
        Block::Paragraph(p) => {
            assert_eq!(p.alignment, Alignment::Center);
            assert!(p.runs[0].style.underline);
            assert_eq!(p.runs[0].style.color, Some([0xCC, 0x00, 0x00]));
        }
        _ => panic!("p1"),
    }
    match &loaded.blocks[2] {
        Block::Paragraph(p) => assert_eq!(p.alignment, Alignment::Right),
        _ => panic!("p2"),
    }
}

#[tokio::test]
async fn roundtrip_bullet_and_numbered_lists() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lists.docx");

    let doc = Document {
        blocks: vec![
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Bullet one".into(),
                    style: RunStyle::default(),
                }],
                list: ListKind::Bullet,
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Bullet two".into(),
                    style: RunStyle::default(),
                }],
                list: ListKind::Bullet,
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Number one".into(),
                    style: RunStyle::default(),
                }],
                list: ListKind::Numbered,
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Plain".into(),
                    style: RunStyle::default(),
                }],
                ..Default::default()
            }),
        ],
        ..Default::default()
    };

    save_document(&doc, &path).await.unwrap();
    let loaded = open_document(&path).unwrap();
    assert_eq!(loaded.blocks.len(), 4);
    match &loaded.blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.list, ListKind::Bullet);
            assert_eq!(p.num_id, Some(1));
            assert_eq!(p.plain_text(), "Bullet one");
        }
        _ => panic!("p0"),
    }
    match &loaded.blocks[1] {
        Block::Paragraph(p) => assert_eq!(p.list, ListKind::Bullet),
        _ => panic!("p1"),
    }
    match &loaded.blocks[2] {
        Block::Paragraph(p) => {
            assert_eq!(p.list, ListKind::Numbered);
            assert_eq!(p.num_id, Some(2));
        }
        _ => panic!("p2"),
    }
    match &loaded.blocks[3] {
        Block::Paragraph(p) => {
            assert_eq!(p.list, ListKind::None);
            assert!(p.num_id.is_none());
        }
        _ => panic!("p3"),
    }
}

#[test]
fn list_toggle_in_memory() {
    use orchid_viewers::document::model::Document as Doc;
    let viewer = DocumentViewer::new();
    // Inject a document without going through OOXML numbering parts.
    *viewer.document_mut() = Some(Doc {
        blocks: vec![
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Item".into(),
                    style: RunStyle::default(),
                }],
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Other".into(),
                    style: RunStyle::default(),
                }],
                ..Default::default()
            }),
        ],
        ..Default::default()
    });
    viewer.set_source_mode(true);
    viewer.set_selection_plain_offsets(0, 4); // "Item"
    viewer.toggle_list_all(ListKind::Bullet).unwrap();
    let guard = viewer.document();
    let doc = guard.as_ref().unwrap();
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.list, ListKind::Bullet);
            assert_eq!(p.num_id, Some(1));
        }
        _ => panic!("p0"),
    }
    match &doc.blocks[1] {
        Block::Paragraph(p) => assert_eq!(p.list, ListKind::None),
        _ => panic!("p1"),
    }
}

#[tokio::test]
async fn selection_format_only_selected_span() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sel.docx");
    write_minimal_docx(
        &path,
        br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p><w:r><w:t>Hello world</w:t></w:r></w:p>
            <w:p><w:r><w:t>Second</w:t></w:r></w:p>
          </w:body>
        </w:document>"#,
    );

    let mut viewer = DocumentViewer::new();
    let registry = std::sync::Arc::new(orchid_fs::FsProviderRegistry::new());
    registry
        .register(std::sync::Arc::new(orchid_fs::LocalProvider::new()))
        .unwrap();
    viewer
        .open(orchid_fs::FsPath::from_local(&path).unwrap(), registry)
        .await
        .unwrap();

    viewer.set_source_mode(true);
    // Select "world" in "Hello world" (bytes 6..11).
    viewer.set_selection_plain_offsets(6, 11);
    viewer.toggle_style_selection('b').unwrap();

    let guard = viewer.document();
    let doc = guard.as_ref().unwrap();
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            let bold: String = p
                .runs
                .iter()
                .filter(|r| r.style.bold)
                .map(|r| r.text.as_str())
                .collect();
            let plain: String = p
                .runs
                .iter()
                .filter(|r| !r.style.bold)
                .map(|r| r.text.as_str())
                .collect();
            assert!(bold.contains("world"), "bold={bold:?}");
            assert!(plain.contains("Hello"), "plain={plain:?}");
        }
        _ => panic!("paragraph"),
    }
    // Second paragraph untouched.
    match &doc.blocks[1] {
        Block::Paragraph(p) => assert!(!p.runs[0].style.bold),
        _ => panic!("p1"),
    }
    drop(guard);

    let ViewerSnapshot::Document(snap) = viewer.snapshot() else {
        panic!("snapshot");
    };
    assert!(snap.preview_width_px > 0);
}

#[test]
fn preview_paste_cut_and_vertical_move() {
    use orchid_viewers::document::model::Document as Doc;

    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(Doc {
        blocks: vec![
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Alpha".into(),
                    style: RunStyle::default(),
                }],
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Beta".into(),
                    style: RunStyle::default(),
                }],
                ..Default::default()
            }),
        ],
        ..Default::default()
    });
    viewer.set_source_mode(false);
    viewer.set_selection_plain_offsets(0, 5); // "Alpha"
    assert_eq!(viewer.selected_plain_text(), "Alpha");
    let cut = viewer.preview_cut_selection().unwrap();
    assert_eq!(cut, "Alpha");
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        // First paragraph emptied; plain text starts with newline then Beta.
        assert!(doc.plain_text().contains("Beta"));
        assert!(!doc.plain_text().contains("Alpha"));
    }
    viewer.set_selection_plain_offsets(0, 0);
    viewer.preview_paste_plain("Hi\nThere").unwrap();
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        assert!(doc.plain_text().starts_with("Hi\nThere"));
    }
    // Caret after "There"; move up should land on "Hi" line.
    viewer.preview_move_vertical(-1, false);
    let sel = viewer.selection();
    assert_eq!(sel.head.block_idx, 0);
}

#[test]
fn preview_typing_insert_backspace_and_return() {
    use orchid_viewers::document::model::Document as Doc;

    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(Doc {
        blocks: vec![Block::Paragraph(Paragraph {
            runs: vec![Run {
                text: "Hi".into(),
                style: RunStyle::default(),
            }],
            ..Default::default()
        })],
        ..Default::default()
    });
    viewer.set_source_mode(false);
    viewer.set_selection_plain_offsets(2, 2); // after "Hi"
    viewer.preview_insert_text("!").unwrap();
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        assert_eq!(doc.plain_text(), "Hi!");
    }
    viewer.preview_delete_backward().unwrap();
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        assert_eq!(doc.plain_text(), "Hi");
    }
    viewer.preview_insert_paragraph_break().unwrap();
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        assert_eq!(doc.blocks.len(), 2);
        assert_eq!(doc.plain_text(), "Hi\n");
    }
    viewer.preview_insert_text("Yo").unwrap();
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        assert_eq!(doc.plain_text(), "Hi\nYo");
    }
}

#[test]
fn preview_select_all_home_end_and_font_color() {
    use orchid_viewers::document::model::Document as Doc;
    use orchid_viewers::ViewerSnapshot;

    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(Doc {
        blocks: vec![
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Hello".into(),
                    style: RunStyle::default(),
                }],
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "World".into(),
                    style: RunStyle::default(),
                }],
                ..Default::default()
            }),
        ],
        ..Default::default()
    });
    viewer.set_source_mode(false);
    viewer.set_selection_plain_offsets(2, 2);
    viewer.preview_select_all();
    assert_eq!(viewer.selected_plain_text(), "Hello\nWorld");

    viewer.set_selection_plain_offsets(8, 8); // in "World"
    viewer.preview_move_line_boundary(false, false);
    assert_eq!(viewer.selection().head.block_idx, 1);
    assert_eq!(viewer.selection().head.byte_offset, 0);
    viewer.preview_move_line_boundary(true, false);
    assert_eq!(viewer.selection().head.byte_offset, 5);

    viewer.preview_move_document_boundary(false, false);
    assert_eq!(viewer.selection().head.block_idx, 0);
    assert_eq!(viewer.selection().head.byte_offset, 0);
    viewer.preview_move_document_boundary(true, false);
    assert_eq!(viewer.selection().head.block_idx, 1);
    assert_eq!(viewer.selection().head.byte_offset, 5);

    viewer.set_selection_plain_offsets(0, 5); // "Hello"
    viewer.bump_font_size_selection(1).unwrap();
    viewer.set_color_selection([0xdc, 0x26, 0x26]).unwrap();
    {
        let guard = viewer.document();
        let doc = guard.as_ref().unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(p.runs[0].style.font_size_pt, Some(16.0));
                assert_eq!(p.runs[0].style.color, Some([0xdc, 0x26, 0x26]));
            }
            _ => panic!("p0"),
        }
        match &doc.blocks[1] {
            Block::Paragraph(p) => {
                assert!(p.runs[0].style.font_size_pt.is_none());
                assert!(p.runs[0].style.color.is_none());
            }
            _ => panic!("p1"),
        }
    }
    viewer.set_selection_plain_offsets(0, 0);
    let ViewerSnapshot::Document(snap) = viewer.snapshot() else {
        panic!("snapshot");
    };
    assert_eq!(snap.font_size_pt, 16.0);
    assert_eq!(snap.color_rgb, 0x00dc_2626);
}

#[test]
fn preview_pointer_selects_second_paragraph_for_bold() {
    use orchid_viewers::document::layout::PREVIEW_PADDING;
    use orchid_viewers::document::model::Document as Doc;

    let viewer = DocumentViewer::new();
    *viewer.document_mut() = Some(Doc {
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
                    text: "Second".into(),
                    style: RunStyle::default(),
                }],
                ..Default::default()
            }),
        ],
        ..Default::default()
    });
    viewer.set_source_mode(false);
    // Click into the second paragraph in preview coordinates.
    viewer.preview_pointer(0, PREVIEW_PADDING + 8.0, PREVIEW_PADDING + 40.0);
    viewer.toggle_style_all('b').unwrap();

    let guard = viewer.document();
    let doc = guard.as_ref().unwrap();
    match &doc.blocks[0] {
        Block::Paragraph(p) => assert!(!p.runs[0].style.bold, "first should stay plain"),
        _ => panic!("p0"),
    }
    match &doc.blocks[1] {
        Block::Paragraph(p) => assert!(p.runs[0].style.bold, "second should be bold"),
        _ => panic!("p1"),
    }
}

fn write_minimal_docx(path: &std::path::Path, document_xml: &[u8]) {
    use std::io::Write;
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default();
    zip.start_file("[Content_Types].xml", opts).unwrap();
    zip.write_all(
        br#"<?xml version="1.0"?>
        <Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
          <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
          <Default Extension="xml" ContentType="application/xml"/>
          <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
        </Types>"#,
    )
    .unwrap();
    zip.start_file("_rels/.rels", opts).unwrap();
    zip.write_all(
        br#"<?xml version="1.0"?>
        <Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
          <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
        </Relationships>"#,
    )
    .unwrap();
    zip.start_file("word/document.xml", opts).unwrap();
    zip.write_all(document_xml).unwrap();
    zip.finish().unwrap();
}
