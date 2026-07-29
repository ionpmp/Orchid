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
        }],
        ..Default::default()
    }])
}

fn para(text: &str) -> Paragraph {
    Paragraph {
        runs: vec![Run {
            text: text.into(),
            style: RunStyle::default(),
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
}
