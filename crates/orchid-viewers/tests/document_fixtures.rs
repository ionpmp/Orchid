//! Integration tests against real Word/LibreOffice `.docx` fixtures.
//!
//! Fixtures live in `tests/fixtures/docx/` and must be created manually
//! (see that directory's README).

use std::path::{Path, PathBuf};

use orchid_viewers::document::model::{Alignment, Block, ListKind};
use orchid_viewers::document::ooxml::container::{open_document, save_document};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docx")
}

fn fixture(name: &str) -> PathBuf {
    let path = fixtures_dir().join(name);
    assert!(
        path.is_file(),
        "missing fixture {name}; create it with Word/LibreOffice and place it in {:?}",
        fixtures_dir()
    );
    path
}

fn paragraphs(doc: &orchid_viewers::document::model::Document) -> Vec<&orchid_viewers::document::model::Paragraph> {
    let mut out = Vec::new();
    for block in &doc.blocks {
        match block {
            Block::Paragraph(p) => out.push(p),
            Block::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        for p in &cell.paragraphs {
                            out.push(p);
                        }
                    }
                }
            }
            Block::Image(_) => {}
        }
    }
    out
}

#[test]
fn all_fixtures_open() {
    for name in [
        "basic_formatting.docx",
        "alignment.docx",
        "lists.docx",
        "table_simple.docx",
        "inline_image.docx",
        "page_setup.docx",
    ] {
        let doc = open_document(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(
            !doc.plain_text().is_empty(),
            "{name} produced empty plain text"
        );
    }
}

#[test]
fn basic_formatting_styles() {
    let doc = open_document(&fixture("basic_formatting.docx")).unwrap();
    let plain = doc.plain_text();
    assert!(plain.contains("bold run"), "{plain}");
    assert!(plain.contains("italic run"), "{plain}");
    assert!(plain.contains("underlined run"), "{plain}");
    assert!(plain.contains("colored run"), "{plain}");
    assert!(plain.contains("Courier New") || plain.contains("custom font"), "{plain}");

    let runs: Vec<_> = paragraphs(&doc).iter().flat_map(|p| p.runs.iter()).collect();
    assert!(runs.iter().any(|r| r.style.bold), "expected a bold run");
    assert!(runs.iter().any(|r| r.style.italic), "expected an italic run");
    assert!(
        runs.iter().any(|r| r.style.underline),
        "expected an underlined run"
    );
    assert!(
        runs.iter().any(|r| r.style.color.is_some()),
        "expected a coloured run"
    );
    assert!(
        runs
            .iter()
            .any(|r| r.style.font_family.as_deref() == Some("Courier New")),
        "expected Courier New font family"
    );
}

#[test]
fn alignment_paragraphs() {
    let doc = open_document(&fixture("alignment.docx")).unwrap();
    let paras = paragraphs(&doc);
    let alignments: Vec<_> = paras.iter().map(|p| p.alignment).collect();
    assert!(
        alignments.contains(&Alignment::Left),
        "missing left: {alignments:?}"
    );
    assert!(
        alignments.contains(&Alignment::Center),
        "missing center: {alignments:?}"
    );
    assert!(
        alignments.contains(&Alignment::Right),
        "missing right: {alignments:?}"
    );
    assert!(
        alignments.contains(&Alignment::Justify),
        "missing justify: {alignments:?}"
    );
}

#[test]
fn lists_bullet_and_numbered() {
    let doc = open_document(&fixture("lists.docx")).unwrap();
    let paras = paragraphs(&doc);
    let bullets = paras
        .iter()
        .filter(|p| p.list == ListKind::Bullet)
        .count();
    let numbered = paras
        .iter()
        .filter(|p| p.list == ListKind::Numbered)
        .count();
    assert!(bullets >= 3, "expected ≥3 bullets, got {bullets}");
    assert!(numbered >= 3, "expected ≥3 numbered, got {numbered}");
    let plain = doc.plain_text();
    assert!(plain.contains("First bullet item"), "{plain}");
    assert!(plain.contains("First numbered item"), "{plain}");
}

#[test]
fn table_simple_3x3() {
    let doc = open_document(&fixture("table_simple.docx")).unwrap();
    let table = doc
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Table(t) => Some(t),
            _ => None,
        })
        .expect("expected a table block");
    assert_eq!(table.rows.len(), 3, "rows");
    for (i, row) in table.rows.iter().enumerate() {
        assert_eq!(row.cells.len(), 3, "row {i} cells");
    }
    let plain = doc.plain_text();
    assert!(plain.contains("R1C1") && plain.contains("R3C3"), "{plain}");
}

#[test]
fn page_setup_non_default() {
    let doc = open_document(&fixture("page_setup.docx")).unwrap();
    assert_eq!(doc.page_setup.width_twips, 7000);
    assert_eq!(doc.page_setup.height_twips, 10000);
    assert_eq!(doc.page_setup.margin_top_twips, 2000);
    assert_eq!(doc.page_setup.margin_bottom_twips, 2000);
    assert_eq!(doc.page_setup.margin_left_twips, 1500);
    assert_eq!(doc.page_setup.margin_right_twips, 1500);
}

#[test]
fn inline_image_opens_and_keeps_text() {
    let doc = open_document(&fixture("inline_image.docx")).unwrap();
    let plain = doc.plain_text();
    assert!(plain.contains("Inline Image Fixture"), "{plain}");
    assert!(plain.contains("End of document"), "{plain}");
    // Tier-1 may still skip drawings into Block::Image; prefer image block when present,
    // otherwise require the media part to be retained for round-trip.
    let has_image_block = doc.blocks.iter().any(|b| matches!(b, Block::Image(_)));
    let has_media = doc
        .retained_parts
        .iter()
        .any(|(path, bytes)| path.contains("media/") && !bytes.is_empty());
    assert!(
        has_image_block || has_media,
        "expected an image block or retained media part"
    );
}

#[tokio::test]
async fn fixtures_roundtrip_plain_text() {
    let dir = tempfile::tempdir().unwrap();
    for name in [
        "basic_formatting.docx",
        "alignment.docx",
        "lists.docx",
        "table_simple.docx",
        "page_setup.docx",
        "inline_image.docx",
    ] {
        let src = fixture(name);
        let loaded = open_document(&src).unwrap_or_else(|e| panic!("open {name}: {e}"));
        let before = loaded.plain_text();
        let out = dir.path().join(name);
        save_document(&loaded, &out)
            .await
            .unwrap_or_else(|e| panic!("save {name}: {e}"));
        let again = open_document(&out).unwrap_or_else(|e| panic!("reopen {name}: {e}"));
        assert_eq!(
            again.plain_text(),
            before,
            "plain text changed after round-trip of {name}"
        );
        if name == "page_setup.docx" {
            assert_eq!(again.page_setup, loaded.page_setup);
        }
        if name == "table_simple.docx" {
            let tables = again
                .blocks
                .iter()
                .filter(|b| matches!(b, Block::Table(_)))
                .count();
            assert!(tables >= 1, "table lost on round-trip");
        }
    }
}

#[test]
fn fixture_paths_are_under_crate() {
    // Sanity: keep the helper honest for IDEs that run tests from repo root.
    assert!(fixtures_dir().ends_with(Path::new("tests/fixtures/docx")));
}
