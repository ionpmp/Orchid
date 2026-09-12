use super::*;
use crate::document::model::*;
use crate::document::ooxml::numbering::NumberingDefs;
use crate::document::ooxml::styles::StyleDefaults;

#[test]
fn parse_simple_paragraph() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:pPr><w:jc w:val="center"/></w:pPr>
              <w:r>
                <w:rPr><w:b/><w:color w:val="FF0000"/><w:sz w:val="24"/></w:rPr>
                <w:t>Hello</w:t>
              </w:r>
            </w:p>
            <w:sectPr>
              <w:pgSz w:w="12240" w:h="15840"/>
              <w:pgMar w:top="1440" w:bottom="1440" w:left="1440" w:right="1440"/>
            </w:sectPr>
          </w:body>
        </w:document>"#;
    let (blocks, setup, _, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(blocks.len(), 1);
    match &blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.alignment, Alignment::Center);
            assert_eq!(p.runs.len(), 1);
            assert_eq!(p.runs[0].text, "Hello");
            assert!(p.runs[0].style.bold);
            assert_eq!(p.runs[0].style.color, Some([0xFF, 0, 0]));
            assert_eq!(p.runs[0].style.font_size_pt, Some(12.0));
        }
        _ => panic!("expected paragraph"),
    }
    assert_eq!(setup.width_twips, 12240);
}

#[test]
fn parse_and_write_highlight() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:r>
                <w:rPr><w:highlight w:val="yellow"/></w:rPr>
                <w:t>Marked</w:t>
              </w:r>
            </w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => assert!(p.runs[0].style.highlight),
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:highlight") && text.contains("yellow"),
        "serialized XML missing highlight: {text}"
    );
}

#[test]
fn parse_and_write_strikethrough() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:r>
                <w:rPr><w:strike/></w:rPr>
                <w:t>Gone</w:t>
              </w:r>
            </w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => assert!(p.runs[0].style.strikethrough),
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:strike"),
        "serialized XML missing strike: {text}"
    );
    let (blocks2, _, _, _, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks2[0] {
        Block::Paragraph(p) => assert!(p.runs[0].style.strikethrough),
        _ => panic!("expected paragraph"),
    }
}

#[test]
fn parse_and_write_all_caps() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:r>
                <w:rPr><w:caps/></w:rPr>
                <w:t>title</w:t>
              </w:r>
            </w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => {
            assert!(p.runs[0].style.all_caps);
            assert_eq!(p.runs[0].text, "title");
        }
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:caps"),
        "serialized XML missing caps: {text}"
    );
    let (blocks2, _, _, _, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks2[0] {
        Block::Paragraph(p) => assert!(p.runs[0].style.all_caps),
        _ => panic!("expected paragraph"),
    }
}

#[test]
fn parse_and_write_small_caps() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:r>
                <w:rPr><w:smallCaps/></w:rPr>
                <w:t>Title</w:t>
              </w:r>
            </w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => {
            assert!(p.runs[0].style.small_caps);
            assert!(!p.runs[0].style.all_caps);
            assert_eq!(p.runs[0].text, "Title");
        }
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:smallCaps"),
        "serialized XML missing smallCaps: {text}"
    );
    let (blocks2, _, _, _, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks2[0] {
        Block::Paragraph(p) => assert!(p.runs[0].style.small_caps),
        _ => panic!("expected paragraph"),
    }
}

#[test]
fn parse_and_write_vanish() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:r>
                <w:rPr><w:vanish/></w:rPr>
                <w:t>Secret</w:t>
              </w:r>
            </w:p>
            <w:p>
              <w:r><w:t>Visible</w:t></w:r>
            </w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => {
            assert!(p.runs[0].style.vanish);
            assert_eq!(p.plain_text(), "Secret");
        }
        _ => panic!("expected paragraph"),
    }
    match &blocks[1] {
        Block::Paragraph(p) => assert!(!p.runs[0].style.vanish),
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:vanish"),
        "serialized XML missing vanish: {text}"
    );
    let (blocks2, _, _, _, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks2[0] {
        Block::Paragraph(p) => assert!(p.runs[0].style.vanish),
        _ => panic!("expected paragraph"),
    }
}

#[test]
fn parse_and_write_shadow() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:r>
                <w:rPr><w:shadow/></w:rPr>
                <w:t>Shadowed</w:t>
              </w:r>
            </w:p>
            <w:p>
              <w:r><w:t>Plain</w:t></w:r>
            </w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => {
            assert!(p.runs[0].style.shadow);
            assert!(!p.runs[0].style.vanish);
            assert_eq!(p.plain_text(), "Shadowed");
        }
        _ => panic!("expected paragraph"),
    }
    match &blocks[1] {
        Block::Paragraph(p) => assert!(!p.runs[0].style.shadow),
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:shadow"),
        "serialized XML missing shadow: {text}"
    );
    let (blocks2, _, _, _, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks2[0] {
        Block::Paragraph(p) => assert!(p.runs[0].style.shadow),
        _ => panic!("expected paragraph"),
    }
}

#[test]
fn parse_and_write_vert_align() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:r>
                <w:rPr><w:vertAlign w:val="superscript"/></w:rPr>
                <w:t>2</w:t>
              </w:r>
              <w:r>
                <w:rPr><w:vertAlign w:val="subscript"/></w:rPr>
                <w:t>n</w:t>
              </w:r>
            </w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => {
            assert!(p.runs[0].style.superscript);
            assert!(!p.runs[0].style.subscript);
            assert!(p.runs[1].style.subscript);
            assert!(!p.runs[1].style.superscript);
        }
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("superscript") && text.contains("subscript"),
        "serialized XML missing vertAlign: {text}"
    );
    let (blocks2, _, _, _, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks2[0] {
        Block::Paragraph(p) => {
            assert!(p.runs[0].style.superscript);
            assert!(p.runs[1].style.subscript);
        }
        _ => panic!("expected paragraph"),
    }
}

#[test]
fn parse_and_write_external_hyperlink() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
          <w:body>
            <w:p>
              <w:hyperlink r:id="rId5" w:history="1">
                <w:r>
                  <w:rPr><w:u w:val="single"/><w:color w:val="0563C1"/></w:rPr>
                  <w:t>Example</w:t>
                </w:r>
              </w:hyperlink>
            </w:p>
          </w:body>
        </w:document>"#;
    let mut rels = Relationships::new();
    rels.insert("rId5".into(), "https://example.com/".into());
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &rels,
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.plain_text(), "Example");
            let hl = p.runs[0].hyperlink.as_ref().expect("hyperlink");
            assert_eq!(hl.url, "https://example.com/");
            assert_eq!(hl.r_id.as_deref(), Some("rId5"));
            assert!(p.runs[0].style.underline);
        }
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:hyperlink") && text.contains("r:id=\"rId5\""),
        "missing hyperlink wrapper: {text}"
    );
    let (blocks2, _, _, _, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &rels,
        &HashMap::new(),
    )
    .unwrap();
    match &blocks2[0] {
        Block::Paragraph(p) => {
            let hl = p.runs[0].hyperlink.as_ref().expect("hyperlink");
            assert_eq!(hl.url, "https://example.com/");
        }
        _ => panic!("expected paragraph"),
    }
}

#[test]
fn parse_and_write_internal_hyperlink_and_bookmark() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:bookmarkStart w:id="0" w:name="intro"/>
              <w:bookmarkEnd w:id="0"/>
              <w:r><w:t>Intro</w:t></w:r>
            </w:p>
            <w:p>
              <w:hyperlink w:anchor="intro" w:history="1">
                <w:r><w:t>Go</w:t></w:r>
              </w:hyperlink>
            </w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, bookmarks, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(bookmarks.len(), 1);
    assert_eq!(bookmarks[0].name, "intro");
    assert_eq!(bookmarks[0].plain_offset, 0);
    match &blocks[1] {
        Block::Paragraph(p) => {
            let hl = p.runs[0].hyperlink.as_ref().expect("hyperlink");
            assert!(hl.is_internal());
            assert_eq!(hl.bookmark.as_deref(), Some("intro"));
            assert!(hl.url.is_empty());
        }
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        bookmarks,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:anchor=\"intro\"") && text.contains("w:bookmarkStart"),
        "missing internal link/bookmark: {text}"
    );
    let (blocks2, _, _, bookmarks2, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    assert!(bookmarks2.iter().any(|b| b.name == "intro"));
    match &blocks2[1] {
        Block::Paragraph(p) => {
            let hl = p.runs[0].hyperlink.as_ref().expect("hyperlink");
            assert_eq!(hl.bookmark.as_deref(), Some("intro"));
        }
        _ => panic!("expected paragraph"),
    }
}

#[test]
fn parse_and_write_grid_span_v_merge() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:tbl>
              <w:tr>
                <w:tc>
                  <w:tcPr>
                    <w:gridSpan w:val="2"/>
                    <w:vMerge w:val="restart"/>
                  </w:tcPr>
                  <w:p><w:r><w:t>Span</w:t></w:r></w:p>
                </w:tc>
              </w:tr>
              <w:tr>
                <w:tc>
                  <w:tcPr><w:vMerge/></w:tcPr>
                  <w:p><w:r><w:t></w:t></w:r></w:p>
                </w:tc>
              </w:tr>
            </w:tbl>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Table(t) => {
            assert_eq!(t.rows[0].cells[0].grid_span, Some(2));
            assert_eq!(t.rows[0].cells[0].v_merge, Some(VMerge::Restart));
            assert_eq!(t.rows[0].cells[0].paragraphs[0].plain_text(), "Span");
            assert_eq!(t.rows[1].cells[0].v_merge, Some(VMerge::Continue));
            assert!(t.unsupported.is_empty());
        }
        _ => panic!("expected table"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:gridSpan") && text.contains("w:val=\"2\""),
        "missing gridSpan: {text}"
    );
    assert!(
        text.contains("w:val=\"restart\""),
        "missing vMerge restart: {text}"
    );
    assert!(
        text.contains("<w:vMerge/>") || text.contains("<w:vMerge />"),
        "missing bare vMerge continue: {text}"
    );
    let (blocks2, _, _, _, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks2[0] {
        Block::Table(t) => {
            assert_eq!(t.rows[0].cells[0].grid_span, Some(2));
            assert_eq!(t.rows[0].cells[0].v_merge, Some(VMerge::Restart));
            assert_eq!(t.rows[1].cells[0].v_merge, Some(VMerge::Continue));
        }
        _ => panic!("expected table"),
    }
}

#[test]
fn parse_and_write_paragraph_spacing() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:pPr><w:spacing w:before="240" w:after="120"/></w:pPr>
              <w:r><w:t>Spaced</w:t></w:r>
            </w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.space_before_twips, 240);
            assert_eq!(p.space_after_twips, 120);
        }
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:before=\"240\"") && text.contains("w:after=\"120\""),
        "missing spacing attrs: {text}"
    );
}

#[test]
fn parse_and_write_line_spacing_auto() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:pPr><w:spacing w:line="360" w:lineRule="auto"/></w:pPr>
              <w:r><w:t>Tall</w:t></w:r>
            </w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.line_spacing, 360);
            assert_eq!(p.line_spacing_rule, LineSpacingRule::Auto);
        }
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:line=\"360\"") && text.contains("w:lineRule=\"auto\""),
        "serialized XML missing line spacing: {text}"
    );
}

#[test]
fn parse_and_write_line_spacing_exact() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:pPr><w:spacing w:line="480" w:lineRule="exact"/></w:pPr>
              <w:r><w:t>Exact</w:t></w:r>
            </w:p>
            <w:p>
              <w:pPr><w:spacing w:line="360" w:lineRule="atLeast"/></w:pPr>
              <w:r><w:t>AtLeast</w:t></w:r>
            </w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.line_spacing, 480);
            assert_eq!(p.line_spacing_rule, LineSpacingRule::Exact);
        }
        _ => panic!("expected paragraph"),
    }
    match &blocks[1] {
        Block::Paragraph(p) => {
            assert_eq!(p.line_spacing, 360);
            assert_eq!(p.line_spacing_rule, LineSpacingRule::AtLeast);
        }
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:line=\"480\"")
            && text.contains("w:lineRule=\"exact\"")
            && text.contains("w:lineRule=\"atLeast\""),
        "serialized XML missing exact/atLeast: {text}"
    );
}

#[test]
fn parse_and_write_paragraph_indent() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:pPr><w:ind w:left="720" w:firstLine="360"/></w:pPr>
              <w:r><w:t>First</w:t></w:r>
            </w:p>
            <w:p>
              <w:pPr><w:ind w:left="720" w:hanging="720"/></w:pPr>
              <w:r><w:t>Hang</w:t></w:r>
            </w:p>
            <w:p>
              <w:pPr><w:ind w:right="480"/></w:pPr>
              <w:r><w:t>Right</w:t></w:r>
            </w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.indent_left_twips, 720);
            assert_eq!(p.indent_first_line_twips, 360);
        }
        _ => panic!("expected paragraph"),
    }
    match &blocks[1] {
        Block::Paragraph(p) => {
            assert_eq!(p.indent_left_twips, 720);
            assert_eq!(p.indent_first_line_twips, -720);
        }
        _ => panic!("expected paragraph"),
    }
    match &blocks[2] {
        Block::Paragraph(p) => {
            assert_eq!(p.indent_right_twips, 480);
        }
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:left=\"720\"")
            && text.contains("w:firstLine=\"360\"")
            && text.contains("w:hanging=\"720\"")
            && text.contains("w:right=\"480\""),
        "serialized XML missing indent: {text}"
    );
}

#[test]
fn parse_and_write_cell_shading() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:tbl>
              <w:tr>
                <w:tc>
                  <w:tcPr><w:shd w:val="clear" w:fill="AABBCC"/></w:tcPr>
                  <w:p><w:r><w:t>A</w:t></w:r></w:p>
                </w:tc>
                <w:tc>
                  <w:tcPr><w:shd w:val="clear" w:fill="auto"/></w:tcPr>
                  <w:p><w:r><w:t>B</w:t></w:r></w:p>
                </w:tc>
              </w:tr>
            </w:tbl>
          </w:body>
        </w:document>"#;
    let (blocks, _, _, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    let Block::Table(t) = &blocks[0] else {
        panic!("expected table");
    };
    assert_eq!(t.rows[0].cells[0].shade_fill, Some([0xAA, 0xBB, 0xCC]));
    assert_eq!(t.rows[0].cells[1].shade_fill, None);
    let doc = Document {
        blocks,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:shd") && text.contains("AABBCC"),
        "missing cell shade: {text}"
    );
}

#[test]
fn parse_and_write_cell_borders() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:tbl>
              <w:tr>
                <w:tc>
                  <w:tcPr>
                    <w:tcBorders>
                      <w:top w:val="single" w:sz="4" w:space="0" w:color="auto"/>
                      <w:left w:val="single" w:sz="4" w:space="0" w:color="auto"/>
                      <w:bottom w:val="single" w:sz="4" w:space="0" w:color="auto"/>
                      <w:right w:val="single" w:sz="4" w:space="0" w:color="auto"/>
                    </w:tcBorders>
                  </w:tcPr>
                  <w:p><w:r><w:t>Box</w:t></w:r></w:p>
                </w:tc>
                <w:tc>
                  <w:p><w:r><w:t>Plain</w:t></w:r></w:p>
                </w:tc>
              </w:tr>
            </w:tbl>
          </w:body>
        </w:document>"#;
    let (blocks, _, _, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    let Block::Table(t) = &blocks[0] else {
        panic!("expected table");
    };
    assert_eq!(
        t.rows[0].cells[0].border_sides,
        CELL_BORDER_TOP | CELL_BORDER_LEFT | CELL_BORDER_BOTTOM | CELL_BORDER_RIGHT
    );
    assert_eq!(t.rows[0].cells[1].border_sides, 0);
    let doc = Document {
        blocks,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:tcBorders") && text.contains("w:top") && text.contains("w:bottom"),
        "missing cell borders: {text}"
    );
}

#[test]
fn parse_and_write_paragraph_shading() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:pPr><w:shd w:val="clear" w:color="auto" w:fill="FFCC00"/></w:pPr>
              <w:r><w:t>Shaded</w:t></w:r>
            </w:p>
            <w:p>
              <w:pPr><w:shd w:val="clear" w:fill="auto"/></w:pPr>
              <w:r><w:t>Clear</w:t></w:r>
            </w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.shade_fill, Some([0xFF, 0xCC, 0x00]));
        }
        _ => panic!("expected paragraph"),
    }
    match &blocks[1] {
        Block::Paragraph(p) => {
            assert_eq!(p.shade_fill, None);
        }
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:shd") && text.contains("FFCC00"),
        "serialized XML missing shading: {text}"
    );
    assert!(
        !text.contains("w:fill=\"auto\""),
        "auto fill should not be written: {text}"
    );
}

#[test]
fn parse_and_write_paragraph_borders() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:pPr>
                <w:pBdr>
                  <w:top w:val="single" w:sz="4" w:space="1" w:color="auto"/>
                  <w:left w:val="single" w:sz="4" w:space="1" w:color="auto"/>
                  <w:bottom w:val="single" w:sz="4" w:space="1" w:color="auto"/>
                  <w:right w:val="single" w:sz="4" w:space="1" w:color="auto"/>
                </w:pBdr>
              </w:pPr>
              <w:r><w:t>Bordered</w:t></w:r>
            </w:p>
            <w:p><w:r><w:t>Plain</w:t></w:r></w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(
                p.border_sides,
                CELL_BORDER_TOP | CELL_BORDER_LEFT | CELL_BORDER_BOTTOM | CELL_BORDER_RIGHT
            );
            assert_eq!(p.plain_text(), "Bordered");
        }
        _ => panic!("expected paragraph"),
    }
    match &blocks[1] {
        Block::Paragraph(p) => {
            assert_eq!(p.border_sides, 0);
        }
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:pBdr")
            && text.contains("w:top")
            && text.contains("w:left")
            && text.contains("w:bottom")
            && text.contains("w:right"),
        "serialized XML missing paragraph borders: {text}"
    );
}

#[test]
fn parse_and_write_keep_next() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:pPr><w:keepNext/></w:pPr>
              <w:r><w:t>Stay</w:t></w:r>
            </w:p>
            <w:p><w:r><w:t>Next</w:t></w:r></w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => {
            assert!(p.keep_next);
            assert_eq!(p.plain_text(), "Stay");
        }
        _ => panic!("expected paragraph"),
    }
    match &blocks[1] {
        Block::Paragraph(p) => assert!(!p.keep_next),
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:keepNext"),
        "serialized XML missing keepNext: {text}"
    );
    let (blocks2, _, _, _, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks2[0] {
        Block::Paragraph(p) => assert!(p.keep_next),
        _ => panic!("expected paragraph"),
    }
}

#[test]
fn parse_and_write_keep_lines() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:pPr><w:keepLines/></w:pPr>
              <w:r><w:t>Together</w:t></w:r>
            </w:p>
            <w:p><w:r><w:t>Loose</w:t></w:r></w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => {
            assert!(p.keep_lines);
            assert!(!p.keep_next);
            assert_eq!(p.plain_text(), "Together");
        }
        _ => panic!("expected paragraph"),
    }
    match &blocks[1] {
        Block::Paragraph(p) => assert!(!p.keep_lines),
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:keepLines"),
        "serialized XML missing keepLines: {text}"
    );
    let (blocks2, _, _, _, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks2[0] {
        Block::Paragraph(p) => assert!(p.keep_lines),
        _ => panic!("expected paragraph"),
    }
}

#[test]
fn parse_and_write_widow_control() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:pPr><w:widowControl/></w:pPr>
              <w:r><w:t>Protected</w:t></w:r>
            </w:p>
            <w:p><w:r><w:t>Default</w:t></w:r></w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => {
            assert!(p.widow_control);
            assert!(!p.keep_lines);
            assert_eq!(p.plain_text(), "Protected");
        }
        _ => panic!("expected paragraph"),
    }
    match &blocks[1] {
        Block::Paragraph(p) => assert!(!p.widow_control),
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:widowControl"),
        "serialized XML missing widowControl: {text}"
    );
    let (blocks2, _, _, _, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks2[0] {
        Block::Paragraph(p) => assert!(p.widow_control),
        _ => panic!("expected paragraph"),
    }
}

#[test]
fn parse_and_write_contextual_spacing() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:pPr><w:contextualSpacing/></w:pPr>
              <w:r><w:t>Same style</w:t></w:r>
            </w:p>
            <w:p><w:r><w:t>Default</w:t></w:r></w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => {
            assert!(p.contextual_spacing);
            assert!(!p.widow_control);
            assert_eq!(p.plain_text(), "Same style");
        }
        _ => panic!("expected paragraph"),
    }
    match &blocks[1] {
        Block::Paragraph(p) => assert!(!p.contextual_spacing),
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:contextualSpacing"),
        "serialized XML missing contextualSpacing: {text}"
    );
    let (blocks2, _, _, _, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks2[0] {
        Block::Paragraph(p) => assert!(p.contextual_spacing),
        _ => panic!("expected paragraph"),
    }
}

#[test]
fn parse_and_write_bidi() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:pPr><w:bidi/></w:pPr>
              <w:r><w:t>RTL</w:t></w:r>
            </w:p>
            <w:p><w:r><w:t>LTR</w:t></w:r></w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => {
            assert!(p.bidi);
            assert!(!p.contextual_spacing);
            assert_eq!(p.plain_text(), "RTL");
        }
        _ => panic!("expected paragraph"),
    }
    match &blocks[1] {
        Block::Paragraph(p) => assert!(!p.bidi),
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:bidi"),
        "serialized XML missing bidi: {text}"
    );
    let (blocks2, _, _, _, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks2[0] {
        Block::Paragraph(p) => assert!(p.bidi),
        _ => panic!("expected paragraph"),
    }
}

#[test]
fn parse_and_write_suppress_auto_hyphens() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:pPr><w:suppressAutoHyphens/></w:pPr>
              <w:r><w:t>NoHyphen</w:t></w:r>
            </w:p>
            <w:p><w:r><w:t>Default</w:t></w:r></w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => {
            assert!(p.suppress_auto_hyphens);
            assert!(!p.bidi);
            assert_eq!(p.plain_text(), "NoHyphen");
        }
        _ => panic!("expected paragraph"),
    }
    match &blocks[1] {
        Block::Paragraph(p) => assert!(!p.suppress_auto_hyphens),
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:suppressAutoHyphens"),
        "serialized XML missing suppressAutoHyphens: {text}"
    );
    let (blocks2, _, _, _, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks2[0] {
        Block::Paragraph(p) => assert!(p.suppress_auto_hyphens),
        _ => panic!("expected paragraph"),
    }
}

#[test]
fn parse_and_write_outline_level() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:pPr><w:outlineLvl w:val="0"/></w:pPr>
              <w:r><w:t>Heading</w:t></w:r>
            </w:p>
            <w:p><w:r><w:t>Body</w:t></w:r></w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.outline_level, Some(0));
            assert_eq!(p.plain_text(), "Heading");
        }
        _ => panic!("expected paragraph"),
    }
    match &blocks[1] {
        Block::Paragraph(p) => assert_eq!(p.outline_level, None),
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:outlineLvl"),
        "serialized XML missing outlineLvl: {text}"
    );
    let (blocks2, _, _, _, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks2[0] {
        Block::Paragraph(p) => assert_eq!(p.outline_level, Some(0)),
        _ => panic!("expected paragraph"),
    }
}

#[test]
fn parse_and_write_page_break_before() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p><w:r><w:t>One</w:t></w:r></w:p>
            <w:p>
              <w:pPr><w:pageBreakBefore/></w:pPr>
              <w:r><w:t>Two</w:t></w:r>
            </w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[1] {
        Block::Paragraph(p) => {
            assert!(p.page_break_before);
            assert_eq!(p.plain_text(), "Two");
        }
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:pageBreakBefore"),
        "missing pageBreakBefore: {text}"
    );
}

#[test]
fn parse_and_write_page_orientation_landscape() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p><w:r><w:t>Hi</w:t></w:r></w:p>
            <w:sectPr>
              <w:pgSz w:w="12240" w:h="15840" w:orient="landscape"/>
            </w:sectPr>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(page_setup.width_twips, 15840);
    assert_eq!(page_setup.height_twips, 12240);
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:orient=\"landscape\""),
        "missing orient: {text}"
    );
    assert!(
        text.contains("w:w=\"15840\"") && text.contains("w:h=\"12240\""),
        "missing landscape pgSz dims: {text}"
    );
}

#[test]
fn parse_and_write_header_footer_references() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
          <w:body>
            <w:p><w:r><w:t>Body</w:t></w:r></w:p>
            <w:sectPr>
              <w:pgSz w:w="12240" w:h="15840"/>
              <w:headerReference w:type="default" r:id="rId7"/>
              <w:footerReference w:type="default" r:id="rId8"/>
              <w:headerReference w:type="first" r:id="rId9"/>
            </w:sectPr>
          </w:body>
        </w:document>"#;
    let (_, page_setup, _, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(page_setup.header_r_id.as_deref(), Some("rId7"));
    assert_eq!(page_setup.footer_r_id.as_deref(), Some("rId8"));
    assert_eq!(page_setup.header_first_r_id.as_deref(), Some("rId9"));
    assert!(!page_setup.title_page);
    let mut page_setup = page_setup;
    page_setup.title_page = true;
    let doc = Document {
        page_setup,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:headerReference") && text.contains("r:id=\"rId7\""),
        "missing default headerReference: {text}"
    );
    assert!(
        text.contains("w:footerReference") && text.contains("r:id=\"rId8\""),
        "missing default footerReference: {text}"
    );
    assert!(
        text.contains("w:type=\"first\"") && text.contains("r:id=\"rId9\""),
        "missing first-page headerReference: {text}"
    );
    assert!(
        text.contains("<w:titlePg") || text.contains("<w:titlePg/>"),
        "missing titlePg: {text}"
    );
}

#[test]
fn parse_title_page_flag() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
          <w:body>
            <w:p><w:r><w:t>Body</w:t></w:r></w:p>
            <w:sectPr>
              <w:titlePg/>
              <w:headerReference w:type="first" r:id="rId1"/>
              <w:footerReference w:type="first" r:id="rId2"/>
            </w:sectPr>
          </w:body>
        </w:document>"#;
    let (_, page_setup, _, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    assert!(page_setup.title_page);
    assert_eq!(page_setup.header_first_r_id.as_deref(), Some("rId1"));
    assert_eq!(page_setup.footer_first_r_id.as_deref(), Some("rId2"));
}

#[test]
fn parse_and_write_even_and_odd_headers() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
          <w:body>
            <w:p><w:r><w:t>Body</w:t></w:r></w:p>
            <w:sectPr>
              <w:evenAndOddHeaders/>
              <w:headerReference w:type="default" r:id="rId1"/>
              <w:headerReference w:type="even" r:id="rId2"/>
              <w:footerReference w:type="default" r:id="rId3"/>
              <w:footerReference w:type="even" r:id="rId4"/>
            </w:sectPr>
          </w:body>
        </w:document>"#;
    let (_, page_setup, _, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    assert!(page_setup.even_and_odd_headers);
    assert_eq!(page_setup.header_r_id.as_deref(), Some("rId1"));
    assert_eq!(page_setup.header_even_r_id.as_deref(), Some("rId2"));
    assert_eq!(page_setup.footer_r_id.as_deref(), Some("rId3"));
    assert_eq!(page_setup.footer_even_r_id.as_deref(), Some("rId4"));
    let doc = Document {
        page_setup,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:evenAndOddHeaders"),
        "missing evenAndOddHeaders: {text}"
    );
    assert!(
        text.contains("w:type=\"even\"") && text.contains("r:id=\"rId2\""),
        "missing even headerReference: {text}"
    );
    assert!(
        text.contains("r:id=\"rId4\""),
        "missing even footerReference: {text}"
    );
}

#[test]
fn parse_and_write_pgmar_header_footer_distance() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p><w:r><w:t>Body</w:t></w:r></w:p>
            <w:sectPr>
              <w:pgMar w:top="1440" w:bottom="1440" w:left="1440" w:right="1440"
                       w:header="576" w:footer="864"/>
            </w:sectPr>
          </w:body>
        </w:document>"#;
    let (_, page_setup, _, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(page_setup.header_distance_twips, 576);
    assert_eq!(page_setup.footer_distance_twips, 864);
    let doc = Document {
        page_setup,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:header=\"576\""),
        "missing header distance: {text}"
    );
    assert!(
        text.contains("w:footer=\"864\""),
        "missing footer distance: {text}"
    );
}

#[test]
fn parse_and_write_fld_simple_page_numpages() {
    let xml = br#"<?xml version="1.0"?>
        <w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:p>
            <w:fldSimple w:instr=" PAGE ">
              <w:r><w:t>3</w:t></w:r>
            </w:fldSimple>
            <w:r><w:t> / </w:t></w:r>
            <w:fldSimple w:instr=" NUMPAGES ">
              <w:r><w:t>10</w:t></w:r>
            </w:fldSimple>
          </w:p>
        </w:hdr>"#;
    let paras = parse_story_xml(
        xml,
        "hdr",
        &StyleDefaults::default(),
        &NumberingDefs::default(),
    )
    .unwrap();
    assert_eq!(paras.len(), 1);
    assert_eq!(paras[0].runs.len(), 3);
    assert_eq!(paras[0].runs[0].field, Some(DocField::Page));
    assert_eq!(paras[0].runs[0].text, "3");
    assert!(paras[0].runs[1].field.is_none());
    assert_eq!(paras[0].runs[2].field, Some(DocField::NumPages));
    assert_eq!(paras[0].runs[2].text, "10");
    let out = write_story_xml("hdr", &paras).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:fldSimple") && text.contains("PAGE"),
        "{text}"
    );
    assert!(text.contains("NUMPAGES"), "{text}");
}

#[test]
fn parse_complex_page_field_collapses() {
    let xml = br#"<?xml version="1.0"?>
        <w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:p>
            <w:r><w:fldChar w:fldCharType="begin"/></w:r>
            <w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r>
            <w:r><w:fldChar w:fldCharType="separate"/></w:r>
            <w:r><w:t>7</w:t></w:r>
            <w:r><w:fldChar w:fldCharType="end"/></w:r>
          </w:p>
        </w:ftr>"#;
    let paras = parse_story_xml(
        xml,
        "ftr",
        &StyleDefaults::default(),
        &NumberingDefs::default(),
    )
    .unwrap();
    assert_eq!(paras[0].runs.len(), 1);
    assert_eq!(paras[0].runs[0].field, Some(DocField::Page));
    assert_eq!(paras[0].runs[0].text, "7");
}

#[test]
fn parse_and_write_header_story() {
    let xml = br#"<?xml version="1.0"?>
        <w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Acme Corp</w:t></w:r></w:p>
        </w:hdr>"#;
    let paras = parse_story_xml(
        xml,
        "hdr",
        &StyleDefaults::default(),
        &NumberingDefs::default(),
    )
    .unwrap();
    assert_eq!(paras.len(), 1);
    assert_eq!(paras[0].plain_text(), "Acme Corp");
    assert!(paras[0].runs[0].style.bold);
    let out = write_story_xml("hdr", &paras).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("<w:hdr"), "missing hdr root: {text}");
    assert!(text.contains("Acme Corp"), "missing text: {text}");
    assert!(text.contains("<w:b"), "missing bold: {text}");
    let again = parse_story_xml(
        &out,
        "hdr",
        &StyleDefaults::default(),
        &NumberingDefs::default(),
    )
    .unwrap();
    assert_eq!(again[0].plain_text(), "Acme Corp");
}

#[test]
fn parse_and_write_soft_break() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:r>
                <w:t>Line one</w:t>
                <w:br/>
                <w:t>Line two</w:t>
              </w:r>
            </w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Paragraph(p) => assert_eq!(p.plain_text(), "Line one\nLine two"),
        _ => panic!("expected paragraph"),
    }
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:br") && !text.contains(">Line one\nLine two<"),
        "soft break should serialize as w:br, not a newline in w:t: {text}"
    );
    let (blocks2, _, _, _, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks2[0] {
        Block::Paragraph(p) => assert_eq!(p.plain_text(), "Line one\nLine two"),
        _ => panic!("expected paragraph"),
    }
}

#[test]
fn write_round_trip_model() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:r><w:rPr><w:i/></w:rPr><w:t>Hi</w:t></w:r>
            </w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    let doc = Document {
        blocks,
        page_setup,
        unsupported,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let (blocks2, _, _, _, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match (&doc.blocks[0], &blocks2[0]) {
        (Block::Paragraph(a), Block::Paragraph(b)) => {
            assert_eq!(a.plain_text(), b.plain_text());
            assert!(b.runs[0].style.italic);
        }
        _ => panic!("expected paragraphs"),
    }
}

#[test]
fn parse_table_2x2() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:tbl>
              <w:tr>
                <w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc>
                <w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc>
              </w:tr>
              <w:tr>
                <w:tc><w:p><w:r><w:t>C</w:t></w:r></w:p></w:tc>
                <w:tc><w:p><w:r><w:t>D</w:t></w:r></w:p></w:tc>
              </w:tr>
            </w:tbl>
          </w:body>
        </w:document>"#;
    let (blocks, _, _, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Table(t) => {
            assert_eq!(t.rows.len(), 2);
            assert_eq!(t.rows[0].cells.len(), 2);
            assert_eq!(t.rows[0].cells[0].paragraphs[0].plain_text(), "A");
            assert_eq!(t.rows[1].cells[1].paragraphs[0].plain_text(), "D");
            assert!(t.column_widths_twips.is_empty());
        }
        _ => panic!("expected table"),
    }
}

#[test]
fn parse_table_uneven_tbl_grid() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:tbl>
              <w:tblGrid>
                <w:gridCol w:w="2000"/>
                <w:gridCol w:w="6000"/>
              </w:tblGrid>
              <w:tr>
                <w:tc><w:p><w:r><w:t>Narrow</w:t></w:r></w:p></w:tc>
                <w:tc><w:p><w:r><w:t>Wide</w:t></w:r></w:p></w:tc>
              </w:tr>
            </w:tbl>
          </w:body>
        </w:document>"#;
    let (blocks, _, _, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Table(t) => assert_eq!(t.column_widths_twips, vec![2000, 6000]),
        _ => panic!("expected table"),
    }
}

#[test]
fn parse_table_tcw_fallback_when_no_grid() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:tbl>
              <w:tr>
                <w:tc>
                  <w:tcPr><w:tcW w:w="1500" w:type="dxa"/></w:tcPr>
                  <w:p><w:r><w:t>A</w:t></w:r></w:p>
                </w:tc>
                <w:tc>
                  <w:tcPr><w:tcW w:w="4500" w:type="dxa"/></w:tcPr>
                  <w:p><w:r><w:t>B</w:t></w:r></w:p>
                </w:tc>
              </w:tr>
            </w:tbl>
          </w:body>
        </w:document>"#;
    let (blocks, _, _, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Table(t) => assert_eq!(t.column_widths_twips, vec![1500, 4500]),
        _ => panic!("expected table"),
    }
}

#[test]
fn write_table_preserves_column_widths() {
    let doc = Document {
        blocks: vec![Block::Table(Table {
            rows: vec![TableRow {
                cells: vec![
                    TableCell::from_paragraphs(vec![Paragraph {
                        runs: vec![Run {
                            text: "A".into(),
                            style: RunStyle::default(),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }]),
                    TableCell::from_paragraphs(vec![Paragraph {
                        runs: vec![Run {
                            text: "B".into(),
                            style: RunStyle::default(),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }]),
                ],
            }],
            column_widths_twips: vec![2000, 6000],
            ..Default::default()
        })],
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let (blocks, _, _, _, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    match &blocks[0] {
        Block::Table(t) => assert_eq!(t.column_widths_twips, vec![2000, 6000]),
        _ => panic!("expected table"),
    }
}

#[test]
fn parse_inline_drawing_to_image_block() {
    let mut png = Vec::new();
    {
        let img = image::RgbaImage::from_pixel(4, 2, image::Rgba([10, 20, 30, 255]));
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
    }
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
                    xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
                    xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
          <w:body>
            <w:p><w:r><w:t>Before</w:t></w:r></w:p>
            <w:p>
              <w:r>
                <w:drawing>
                  <wp:inline>
                    <wp:extent cx="914400" cy="457200"/>
                    <a:graphic>
                      <a:graphicData>
                        <pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">
                          <pic:blipFill>
                            <a:blip r:embed="rId7"/>
                          </pic:blipFill>
                        </pic:pic>
                      </a:graphicData>
                    </a:graphic>
                  </wp:inline>
                </w:drawing>
              </w:r>
            </w:p>
          </w:body>
        </w:document>"#;
    let mut rels = Relationships::new();
    rels.insert("rId7".into(), "media/dot.png".into());
    let mut media = HashMap::new();
    media.insert("word/media/dot.png".into(), png.clone());
    let (blocks, _, _, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &rels,
        &media,
    )
    .unwrap();
    assert_eq!(blocks.len(), 2);
    match &blocks[0] {
        Block::Paragraph(p) => assert_eq!(p.plain_text(), "Before"),
        _ => panic!("expected text paragraph"),
    }
    match &blocks[1] {
        Block::Image(img) => {
            assert_eq!(img.bytes, png);
            assert_eq!(img.width_px, 96); // 914400 EMU → 96 CSS px @ 96dpi
            assert_eq!(img.height_px, 48);
            assert_eq!(img.r_id.as_deref(), Some("rId7"));
            assert_eq!(img.part_path.as_deref(), Some("word/media/dot.png"));
        }
        _ => panic!("expected image block"),
    }
}

#[test]
fn parse_drawing_inside_table_cell() {
    let mut png = Vec::new();
    {
        let img = image::RgbaImage::from_pixel(4, 2, image::Rgba([200, 40, 40, 255]));
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
    }
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
                    xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
                    xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
          <w:body>
            <w:tbl>
              <w:tr>
                <w:tc>
                  <w:p><w:r><w:t>Caption</w:t></w:r></w:p>
                  <w:p>
                    <w:r>
                      <w:drawing>
                        <wp:inline>
                          <wp:extent cx="457200" cy="457200"/>
                          <a:graphic>
                            <a:graphicData>
                              <pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture">
                                <pic:blipFill>
                                  <a:blip r:embed="rId9"/>
                                </pic:blipFill>
                              </pic:pic>
                            </a:graphicData>
                          </a:graphic>
                        </wp:inline>
                      </w:drawing>
                    </w:r>
                  </w:p>
                </w:tc>
                <w:tc>
                  <w:p><w:r><w:t>Right</w:t></w:r></w:p>
                </w:tc>
              </w:tr>
            </w:tbl>
          </w:body>
        </w:document>"#;
    let mut rels = Relationships::new();
    rels.insert("rId9".into(), "media/cell.png".into());
    let mut media = HashMap::new();
    media.insert("word/media/cell.png".into(), png.clone());
    let (blocks, _, _, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &rels,
        &media,
    )
    .unwrap();
    match &blocks[0] {
        Block::Table(t) => {
            let cell = &t.rows[0].cells[0];
            assert_eq!(cell.paragraphs.len(), 2);
            assert_eq!(cell.paragraphs[0].plain_text(), "Caption");
            assert_eq!(cell.images.len(), 1);
            assert_eq!(cell.images[0].after_paragraph, 1);
            assert_eq!(cell.images[0].image.bytes, png);
            assert!(t.rows[0].cells[1].images.is_empty());
        }
        _ => panic!("expected table"),
    }
}

#[test]
fn mid_body_section_properties_round_trip() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
                    xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
          <w:body>
            <w:p>
              <w:pPr>
                <w:sectPr>
                  <w:pgSz w:w="12240" w:h="15840"/>
                  <w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="720"
                           w:header="720" w:footer="720"/>
                </w:sectPr>
              </w:pPr>
              <w:r><w:t>Section One</w:t></w:r>
            </w:p>
            <w:p><w:r><w:t>Section Two</w:t></w:r></w:p>
            <w:sectPr>
              <w:pgSz w:w="11906" w:h="16838"/>
              <w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"
                       w:header="720" w:footer="720"/>
            </w:sectPr>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    assert!(unsupported.is_empty());
    assert_eq!(page_setup.width_twips, 11906);
    let Block::Paragraph(p0) = &blocks[0] else {
        panic!("p0");
    };
    let sect = p0.section_properties.as_ref().expect("mid-body sectPr");
    assert_eq!(sect.margin_top_twips, 720);
    assert_eq!(sect.width_twips, 12240);
    let doc = Document {
        blocks: blocks.clone(),
        page_setup: page_setup.clone(),
        unsupported: Vec::new(),
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let (blocks2, page_setup2, _, _, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(page_setup2, page_setup);
    let Block::Paragraph(p0b) = &blocks2[0] else {
        panic!("p0b");
    };
    assert_eq!(p0b.section_properties, p0.section_properties);
}

#[test]
fn continuous_section_type_round_trip() {
    use crate::document::model::SectionBreakType;
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:pPr>
                <w:sectPr>
                  <w:type w:val="continuous"/>
                  <w:pgSz w:w="12240" w:h="15840"/>
                  <w:pgMar w:top="720" w:right="720" w:bottom="720" w:left="1440"
                           w:header="720" w:footer="720"/>
                </w:sectPr>
              </w:pPr>
              <w:r><w:t>A</w:t></w:r>
            </w:p>
            <w:p><w:r><w:t>B</w:t></w:r></w:p>
            <w:sectPr>
              <w:pgSz w:w="12240" w:h="15840"/>
              <w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"
                       w:header="720" w:footer="720"/>
            </w:sectPr>
          </w:body>
        </w:document>"#;
    let (blocks, _, _, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    let Block::Paragraph(p0) = &blocks[0] else {
        panic!("p0");
    };
    let sect = p0.section_properties.as_ref().expect("sectPr");
    assert_eq!(sect.section_break, SectionBreakType::Continuous);
    assert_eq!(sect.margin_left_twips, 1440);
    let out = write_document_xml(&Document {
        blocks: blocks.clone(),
        page_setup: PageSetup::default(),
        ..Default::default()
    })
    .unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:val=\"continuous\"") || text.contains("w:type w:val=\"continuous\""),
        "{text}"
    );
    let (blocks2, _, _, _, _) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    let Block::Paragraph(p0b) = &blocks2[0] else {
        panic!("p0b");
    };
    assert_eq!(
        p0b.section_properties.as_ref().unwrap().section_break,
        SectionBreakType::Continuous
    );
}

#[test]
fn pstyle_round_trip_and_outline_from_named_style() {
    use crate::document::model::{NamedParagraphStyle, RunStyle};

    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:pPr><w:pStyle w:val="Heading1"/></w:pPr>
              <w:r><w:t>Title</w:t></w:r>
            </w:p>
            <w:sectPr/>
          </w:body>
        </w:document>"#;
    let mut styles = StyleDefaults::default();
    styles.paragraph_styles.insert(
        "Heading1".into(),
        NamedParagraphStyle {
            style_id: "Heading1".into(),
            name: "heading 1".into(),
            outline_level: Some(0),
            run: RunStyle {
                bold: true,
                font_size_pt: Some(16.0),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &styles,
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    assert!(unsupported.is_empty());
    let Block::Paragraph(p) = &blocks[0] else {
        panic!("expected paragraph");
    };
    assert_eq!(p.style_id.as_deref(), Some("Heading1"));
    assert_eq!(p.outline_level, Some(0));
    let doc = Document {
        blocks: blocks.clone(),
        page_setup,
        paragraph_styles: styles.paragraph_styles.clone(),
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8(out.clone()).unwrap();
    assert!(
        text.contains("w:pStyle") && text.contains("Heading1"),
        "missing pStyle: {text}"
    );
    let (blocks2, _, _, _, _) = parse_document_xml(
        &out,
        &styles,
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    let Block::Paragraph(p2) = &blocks2[0] else {
        panic!("expected paragraph");
    };
    assert_eq!(p2.style_id.as_deref(), Some("Heading1"));
    assert_eq!(p2.outline_level, Some(0));
}

#[test]
fn rstyle_round_trip_and_preview_merge() {
    use crate::document::layout::DocumentLayout;
    use crate::document::model::{NamedCharacterStyle, RunStyle};

    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:r>
                <w:rPr><w:rStyle w:val="Strong"/></w:rPr>
                <w:t>Hi</w:t>
              </w:r>
            </w:p>
            <w:sectPr/>
          </w:body>
        </w:document>"#;
    let mut styles = StyleDefaults::default();
    styles.character_styles.insert(
        "Strong".into(),
        NamedCharacterStyle {
            style_id: "Strong".into(),
            name: "Strong".into(),
            run: RunStyle {
                bold: true,
                color: Some([0xC0, 0x00, 0x00]),
                ..Default::default()
            },
        },
    );
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &styles,
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    assert!(unsupported.is_empty());
    let Block::Paragraph(p) = &blocks[0] else {
        panic!("expected paragraph");
    };
    assert_eq!(p.runs[0].style_id.as_deref(), Some("Strong"));
    assert!(!p.runs[0].style.bold, "direct rPr should not bake style");
    let doc = Document {
        blocks: blocks.clone(),
        page_setup,
        character_styles: styles.character_styles.clone(),
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8(out.clone()).unwrap();
    assert!(
        text.contains("w:rStyle") && text.contains("Strong"),
        "missing rStyle: {text}"
    );
    let (blocks2, _, _, _, _) = parse_document_xml(
        &out,
        &styles,
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    let Block::Paragraph(p2) = &blocks2[0] else {
        panic!("expected paragraph");
    };
    assert_eq!(p2.runs[0].style_id.as_deref(), Some("Strong"));
    // Preview merge: named character style fills unset props.
    let mut layout = DocumentLayout::new();
    let (bytes, w, h) = layout.render_document(&doc, 400.0);
    assert!(w > 0 && h > 0 && !bytes.is_empty());
}

#[test]
fn comment_range_markers_round_trip() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:commentRangeStart w:id="0"/>
              <w:r><w:t>noted</w:t></w:r>
              <w:commentRangeEnd w:id="0"/>
              <w:r><w:commentReference w:id="0"/></w:r>
            </w:p>
            <w:sectPr/>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, _, _, ranges) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].id, 0);
    assert_eq!(ranges[0].start_plain, 0);
    assert_eq!(ranges[0].end_plain, 5);
    let doc = Document {
        blocks,
        page_setup,
        comment_ranges: ranges.clone(),
        comments: vec![crate::document::model::DocComment {
            id: 0,
            author: "Ada".into(),
            initials: "A".into(),
            date: String::new(),
            text: "note".into(),
        }],
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8(out.clone()).unwrap();
    assert!(text.contains("w:commentRangeStart"), "{text}");
    assert!(text.contains("w:commentRangeEnd"), "{text}");
    assert!(text.contains("w:commentReference"), "{text}");
    let (_, _, _, _, ranges2) = parse_document_xml(
        &out,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(ranges2, ranges);
}

#[test]
fn parse_date_and_filename_fields() {
    let xml = br#"<?xml version="1.0"?>
        <w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:p>
            <w:fldSimple w:instr=" DATE \@ &quot;yyyy-MM-dd&quot; ">
              <w:r><w:t>2020-01-01</w:t></w:r>
            </w:fldSimple>
            <w:r><w:t> </w:t></w:r>
            <w:fldSimple w:instr=" FILENAME ">
              <w:r><w:t>demo.docx</w:t></w:r>
            </w:fldSimple>
          </w:p>
        </w:hdr>"#;
    let paras = parse_story_xml(
        xml,
        "hdr",
        &StyleDefaults::default(),
        &NumberingDefs::default(),
    )
    .unwrap();
    assert_eq!(paras[0].runs[0].field, Some(DocField::Date));
    assert_eq!(paras[0].runs[2].field, Some(DocField::FileName));
    assert_eq!(DocField::Date.instr(), "DATE");
    assert_eq!(
        DocField::FileName.display(1, 1, Some("report.docx")),
        "report.docx"
    );
    let out = write_story_xml("hdr", &paras).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("DATE") && text.contains("FILENAME"), "{text}");
}

#[test]
fn parse_and_write_dstrike_emboss_imprint() {
    let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p><w:r>
              <w:rPr><w:dstrike/><w:emboss/></w:rPr>
              <w:t>A</w:t>
            </w:r></w:p>
            <w:p><w:r>
              <w:rPr><w:imprint/></w:rPr>
              <w:t>B</w:t>
            </w:r></w:p>
          </w:body>
        </w:document>"#;
    let (blocks, page_setup, unsupported, _, _) = parse_document_xml(
        xml,
        &StyleDefaults::default(),
        &NumberingDefs::default(),
        &Relationships::new(),
        &HashMap::new(),
    )
    .unwrap();
    assert!(unsupported.is_empty());
    let Block::Paragraph(p0) = &blocks[0] else {
        panic!("p0")
    };
    assert!(p0.runs[0].style.double_strikethrough);
    assert!(p0.runs[0].style.emboss);
    let Block::Paragraph(p1) = &blocks[1] else {
        panic!("p1")
    };
    assert!(p1.runs[0].style.imprint);
    let doc = Document {
        blocks,
        page_setup,
        ..Default::default()
    };
    let out = write_document_xml(&doc).unwrap();
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("w:dstrike") && text.contains("w:emboss") && text.contains("w:imprint"),
        "{text}"
    );
}
