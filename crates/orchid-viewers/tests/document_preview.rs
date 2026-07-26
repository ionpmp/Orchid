//! Document viewer preview + synthetic DOCX round-trip.

use orchid_viewers::document::layout::DocumentLayout;
use orchid_viewers::document::model::{
    Alignment, Block, Document, ListKind, Paragraph, Run, RunStyle,
};
use orchid_viewers::{DocumentViewer, Viewer, ViewerSnapshot};

#[tokio::test]
async fn preview_render_after_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preview.docx");
    write_minimal_docx(
        &path,
        br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:r><w:rPr><w:b/></w:rPr><w:t>Bold preview</w:t></w:r>
            </w:p>
            <w:p>
              <w:pPr><w:jc w:val="center"/></w:pPr>
              <w:r><w:rPr><w:i/></w:rPr><w:t>Centered italic</w:t></w:r>
            </w:p>
          </w:body>
        </w:document>"#,
    );

    let mut viewer = DocumentViewer::new();
    let registry = std::sync::Arc::new(orchid_fs::FsProviderRegistry::new());
    registry
        .register(std::sync::Arc::new(orchid_fs::LocalProvider::new()))
        .unwrap();
    let fs_path = orchid_fs::FsPath::from_local(&path).unwrap();
    viewer.open(fs_path, registry).await.unwrap();

    let ViewerSnapshot::Document(snap) = viewer.snapshot() else {
        panic!("expected document snapshot");
    };
    assert!(snap.preview_width_px > 100);
    assert!(snap.preview_height_px > 40);
    assert_eq!(
        snap.preview_rgba.len(),
        (snap.preview_width_px * snap.preview_height_px * 4) as usize
    );
    assert!(
        snap.preview_rgba
            .chunks_exact(4)
            .any(|px| px[0] < 240 || px[1] < 240 || px[2] < 240),
        "preview should contain ink"
    );
    assert!(snap.bold);
}

#[test]
fn layout_respects_alignment_and_lists() {
    let mut dl = DocumentLayout::new();
    let doc = Document {
        blocks: vec![
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Centered".into(),
                    style: RunStyle {
                        bold: true,
                        ..Default::default()
                    },
                }],
                alignment: Alignment::Center,
                ..Default::default()
            }),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Bullet item".into(),
                    style: RunStyle::default(),
                }],
                list: ListKind::Bullet,
                ..Default::default()
            }),
        ],
        ..Default::default()
    };
    let (bytes, w, h) = dl.render_document(&doc, 480.0);
    assert!(w > 200 && h > 60);
    assert_eq!(bytes.len(), (w * h * 4) as usize);
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
