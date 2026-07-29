//! In-memory document model for Tier-1 rich text (DOCX-compatible).

use std::path::Path;

use crate::error::{Result, ViewerError};

/// Opaque XML fragment preserved for round-trip fidelity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpaqueXmlNode {
    /// Diagnostic path hint, e.g. `"w:body/w:p[3]/w:pPr/w:extra"`.
    pub position_hint: String,
    /// Raw XML bytes of the element (including start/end tags).
    pub raw_xml: Vec<u8>,
}

/// Character-level style for a text run.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunStyle {
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Single underline.
    pub underline: bool,
    /// RGB colour (`None` = theme/default).
    pub color: Option<[u8; 3]>,
    /// Font family name.
    pub font_family: Option<String>,
    /// Font size in points.
    pub font_size_pt: Option<f32>,
}

/// A contiguous run of text with uniform style.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Run {
    /// Plain text content (may be empty).
    pub text: String,
    /// Character style.
    pub style: RunStyle,
}

/// Paragraph alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Alignment {
    /// Left-aligned (default).
    #[default]
    Left,
    /// Centered.
    Center,
    /// Right-aligned.
    Right,
    /// Justified.
    Justify,
}

/// List marker kind for a paragraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListKind {
    /// Not a list item.
    #[default]
    None,
    /// Bulleted list.
    Bullet,
    /// Numbered list.
    Numbered,
}

/// A paragraph: sequence of runs plus paragraph-level properties.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Paragraph {
    /// Text runs.
    pub runs: Vec<Run>,
    /// Alignment.
    pub alignment: Alignment,
    /// List kind.
    pub list: ListKind,
    /// Nesting level (0 = top).
    pub list_level: u8,
    /// OOXML numbering id (`numId`), if any.
    pub num_id: Option<u32>,
    /// Unsupported child elements preserved for round-trip.
    pub unsupported: Vec<OpaqueXmlNode>,
}

impl Paragraph {
    /// Concatenate all run text.
    #[must_use]
    pub fn plain_text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }
}

/// An image inside a table cell, anchored after a paragraph.
#[derive(Debug, Clone, PartialEq)]
pub struct CellImage {
    /// Index into [`TableCell::paragraphs`] after which this image is drawn.
    pub after_paragraph: usize,
    /// Image payload.
    pub image: InlineImage,
}

/// One cell in a table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableCell {
    /// Paragraphs inside the cell.
    pub paragraphs: Vec<Paragraph>,
    /// Images parsed from `w:drawing` in cell paragraphs (preview-only; not cursor-addressable).
    pub images: Vec<CellImage>,
}

impl TableCell {
    /// Cell with paragraphs and no images.
    #[must_use]
    pub fn from_paragraphs(paragraphs: Vec<Paragraph>) -> Self {
        Self {
            paragraphs,
            images: Vec::new(),
        }
    }
}

/// One row in a table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableRow {
    /// Cells left-to-right.
    pub cells: Vec<TableCell>,
}

/// A simple table (no cell merges in Tier 1).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    /// Rows top-to-bottom.
    pub rows: Vec<TableRow>,
    /// Unsupported elements (e.g. merge markers) preserved for round-trip.
    pub unsupported: Vec<OpaqueXmlNode>,
}

/// Image codec for inline images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    /// PNG.
    Png,
    /// JPEG.
    Jpeg,
    /// GIF.
    Gif,
    /// BMP.
    Bmp,
    /// Other / unknown.
    Other,
}

impl ImageFormat {
    /// Guess from a file extension or MIME-ish suffix.
    #[must_use]
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "png" => Self::Png,
            "jpg" | "jpeg" => Self::Jpeg,
            "gif" => Self::Gif,
            "bmp" => Self::Bmp,
            _ => Self::Other,
        }
    }
}

/// An inline (floating-as-inline) image.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineImage {
    /// Encoded image bytes.
    pub bytes: Vec<u8>,
    /// Codec.
    pub format: ImageFormat,
    /// Pixel width (decoded or from OOXML extents).
    pub width_px: u32,
    /// Pixel height.
    pub height_px: u32,
    /// Relationship id from the package (e.g. `"rId5"`), if known.
    pub r_id: Option<String>,
    /// Part path inside the package (e.g. `"word/media/image1.png"`).
    pub part_path: Option<String>,
}

/// Top-level block in the document body.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// Paragraph.
    Paragraph(Paragraph),
    /// Table.
    Table(Table),
    /// Inline image treated as a block (drawing outside a paragraph run).
    Image(InlineImage),
}

/// Page size and margins in twentieths of a point (twips).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageSetup {
    /// Page width in twips (US Letter default = 12240).
    pub width_twips: u32,
    /// Page height in twips (US Letter default = 15840).
    pub height_twips: u32,
    /// Top margin.
    pub margin_top_twips: u32,
    /// Bottom margin.
    pub margin_bottom_twips: u32,
    /// Left margin.
    pub margin_left_twips: u32,
    /// Right margin.
    pub margin_right_twips: u32,
}

impl Default for PageSetup {
    fn default() -> Self {
        // US Letter, 1-inch margins.
        Self {
            width_twips: 12240,
            height_twips: 15840,
            margin_top_twips: 1440,
            margin_bottom_twips: 1440,
            margin_left_twips: 1440,
            margin_right_twips: 1440,
        }
    }
}

/// Full in-memory document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Document {
    /// Body blocks in order.
    pub blocks: Vec<Block>,
    /// Page geometry.
    pub page_setup: PageSetup,
    /// Unsupported body-level elements preserved for round-trip.
    pub unsupported: Vec<OpaqueXmlNode>,
    /// Original package parts we did not rewrite (styles, numbering, …).
    /// Populated by the reader; used by the writer for fidelity.
    pub retained_parts: Vec<(String, Vec<u8>)>,
    /// Content types XML (original bytes), if present.
    pub content_types: Option<Vec<u8>>,
    /// Package relationships (`_rels/.rels`).
    pub package_rels: Option<Vec<u8>>,
    /// Document relationships (`word/_rels/document.xml.rels`).
    pub document_rels: Option<Vec<u8>>,
}

impl Document {
    /// Open a `.docx` from disk and build a [`Document`].
    ///
    /// # Errors
    ///
    /// Propagates IO / zip / XML parse failures as [`ViewerError`].
    pub async fn from_docx(path: &Path) -> Result<Self> {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || crate::document::ooxml::container::open_document(&path))
            .await
            .map_err(|e| ViewerError::DocumentParse(format!("join: {e}")))?
    }

    /// Concatenate all paragraph text with blank lines between blocks.
    #[must_use]
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for block in &self.blocks {
            match block {
                Block::Paragraph(p) => {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(&p.plain_text());
                }
                Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            for p in &cell.paragraphs {
                                if !out.is_empty() {
                                    out.push('\n');
                                }
                                out.push_str(&p.plain_text());
                            }
                        }
                    }
                }
                Block::Image(_) => {}
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_do_not_panic() {
        let _ = Document::default();
        let _ = Paragraph::default();
        let _ = RunStyle::default();
        let _ = PageSetup::default();
    }

    #[test]
    fn paragraph_equality() {
        let a = Paragraph {
            runs: vec![Run {
                text: "hi".into(),
                style: RunStyle {
                    bold: true,
                    ..Default::default()
                },
            }],
            ..Default::default()
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn plain_text_joins_runs() {
        let p = Paragraph {
            runs: vec![
                Run {
                    text: "Hello ".into(),
                    style: RunStyle::default(),
                },
                Run {
                    text: "world".into(),
                    style: RunStyle::default(),
                },
            ],
            ..Default::default()
        };
        assert_eq!(p.plain_text(), "Hello world");
    }
}
