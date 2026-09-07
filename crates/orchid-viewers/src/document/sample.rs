//! Blank / sample DOCX helpers for the Document Editor launcher.

use std::path::Path;

use crate::document::model::{
    Alignment, Block, Document, ListKind, Paragraph, Run, RunStyle, Table, TableCell, TableRow,
};
use crate::document::ooxml::container::save_document;
use crate::error::Result;

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

fn para(text: &str) -> Paragraph {
    Paragraph {
        runs: vec![Run {
            text: text.into(),
            style: RunStyle::default(),
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// Small Tier-1 document with body text, a list, and a 2×2 table for manual testing.
#[must_use]
pub fn sample_document() -> Document {
    Document {
        blocks: vec![
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Sample document".into(),
                    style: RunStyle {
                        bold: true,
                        font_size_pt: Some(18.0),
                        ..Default::default()
                    },
                    ..Default::default()
                }],
                alignment: Alignment::Left,
                ..Default::default()
            }),
            Block::Paragraph(para(
                "Edit this DOCX in Orchid: type, format, and try the table below.",
            )),
            Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Bullet item — Tab / Shift+Tab changes indent.".into(),
                    style: RunStyle::default(),
                    ..Default::default()
                }],
                list: ListKind::Bullet,
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
            Block::Paragraph(para("Text after the table.")),
        ],
        ..Default::default()
    }
}

/// Write [`sample_document`] to `path` as a `.docx` package (creates parent dirs).
///
/// # Errors
///
/// Propagates IO / zip failures from the OOXML writer.
pub async fn create_sample_docx(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    save_document(&sample_document(), path).await
}

/// Write [`sample_document`] as a sealed `.orchid` (DOCX Raw + Clean-Text).
///
/// # Errors
///
/// Propagates sealed-write / OOXML failures.
pub async fn create_sample_orchid(path: &Path) -> Result<()> {
    create_sample_orchid_with_store(path, None).await
}

/// Like [`create_sample_orchid`], but writes a **linked** envelope when `store`
/// is provided (catalog / Untitled path with an app [`ChunkStore`]).
///
/// # Errors
///
/// Propagates linked/sealed write or OOXML failures.
pub async fn create_sample_orchid_with_store(
    path: &Path,
    store: Option<&orchid_crypto::ChunkStore>,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let doc = sample_document();
    match store {
        Some(store) => {
            crate::document::orchid_io::save_document_as_linked_orchid(
                &doc, path, store, None, 1, 0, None,
            )
            .await
        }
        None => crate::document::orchid_io::save_document_as_orchid(&doc, path, None).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sample_docx_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.docx");
        create_sample_docx(&path).await.unwrap();
        let doc = Document::from_docx(&path).await.unwrap();
        let plain = doc.plain_text();
        assert!(plain.contains("Sample document"));
        assert!(plain.contains("R1C1"));
        assert!(plain.contains("R2C2"));
        assert!(matches!(
            doc.blocks.iter().find(|b| matches!(b, Block::Table(_))),
            Some(Block::Table(t)) if t.rows.len() == 2 && t.rows[0].cells.len() == 2
        ));
    }

    #[tokio::test]
    async fn sample_orchid_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.orchid");
        create_sample_orchid(&path).await.unwrap();
        let doc = crate::document::orchid_io::open_document_from_orchid(&path, None)
            .await
            .unwrap();
        assert!(doc.plain_text().contains("Sample document"));
    }

    #[tokio::test]
    async fn sample_orchid_with_store_is_linked() {
        use std::sync::Arc;

        use orchid_crypto::ChunkStore;
        use orchid_format::{capability, SealedFile};
        use orchid_storage::StateStore;

        let td = tempfile::tempdir().unwrap();
        let storage = Arc::new(StateStore::open_in_memory("sample").unwrap());
        let store = ChunkStore::new(td.path().join("chunks"), storage).unwrap();
        let path = td.path().join("linked.orchid");
        create_sample_orchid_with_store(&path, Some(&store))
            .await
            .unwrap();
        let file = SealedFile::open(&path).unwrap();
        assert_ne!(file.header().capability_flags & capability::LINKED, 0);
        let doc = crate::document::orchid_io::open_document_from_orchid_with_store(
            &path,
            Some(&store),
            None,
        )
        .await
        .unwrap();
        assert!(doc.plain_text().contains("Sample document"));
    }
}
