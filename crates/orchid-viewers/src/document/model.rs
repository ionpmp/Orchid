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
    /// Single strikethrough.
    pub strikethrough: bool,
    /// Yellow highlight background (Word `w:highlight`).
    pub highlight: bool,
    /// Superscript (`w:vertAlign` = `superscript`). Mutually exclusive with [`Self::subscript`].
    pub superscript: bool,
    /// Subscript (`w:vertAlign` = `subscript`). Mutually exclusive with [`Self::superscript`].
    pub subscript: bool,
    /// RGB colour (`None` = theme/default).
    pub color: Option<[u8; 3]>,
    /// Font family name.
    pub font_family: Option<String>,
    /// Font size in points.
    pub font_size_pt: Option<f32>,
}

/// Hyperlink resolved from `w:hyperlink` (external relationship and/or `w:anchor`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hyperlink {
    /// Target URL (`Relationship/@Target`, typically `TargetMode="External"`).
    /// Empty when this is an internal bookmark link only.
    pub url: String,
    /// Package relationship id (`rIdN`) when known; allocated on save if missing.
    pub r_id: Option<String>,
    /// Internal bookmark name (`w:anchor`). When set, Ctrl+click jumps in-document.
    pub bookmark: Option<String>,
}

impl Hyperlink {
    /// Whether this link targets an in-document bookmark.
    #[must_use]
    pub fn is_internal(&self) -> bool {
        self.bookmark.as_ref().is_some_and(|b| !b.is_empty())
    }

    /// UI / compare key: external URL, or `#bookmark` for internal links.
    #[must_use]
    pub fn display_target(&self) -> String {
        if let Some(name) = self.bookmark.as_ref().filter(|b| !b.is_empty()) {
            format!("#{name}")
        } else {
            self.url.clone()
        }
    }
}

/// Named destination from `w:bookmarkStart` (plain-text offset into [`Document::plain_text`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bookmark {
    /// Bookmark name (`w:name`).
    pub name: String,
    /// Byte offset into [`Document::plain_text`] at the bookmark start.
    pub plain_offset: usize,
}

/// A contiguous run of text with uniform style.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Run {
    /// Plain text content (may be empty).
    pub text: String,
    /// Character style.
    pub style: RunStyle,
    /// External hyperlink covering this run (`None` = plain text).
    pub hyperlink: Option<Hyperlink>,
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
    /// Force a page break before this paragraph (`w:pageBreakBefore` / Ctrl+Enter).
    pub page_break_before: bool,
    /// Space before paragraph in twips (`w:spacing/@w:before`). `0` = none.
    pub space_before_twips: u32,
    /// Space after paragraph in twips (`w:spacing/@w:after`). `0` = none.
    pub space_after_twips: u32,
    /// Raw `w:spacing/@w:line` value: 240ths of a line when
    /// [`LineSpacingRule::Auto`], otherwise twips. `0` + Auto = engine default.
    pub line_spacing: u32,
    /// How [`Self::line_spacing`] is interpreted (`w:lineRule`).
    pub line_spacing_rule: LineSpacingRule,
    /// Left indent in twips (`w:ind/@w:left` or `w:start`). Combined with list indent in preview.
    pub indent_left_twips: u32,
    /// First-line indent in twips: positive = `w:firstLine`, negative = −`w:hanging`.
    pub indent_first_line_twips: i32,
    /// Right indent in twips (`w:ind/@w:right` or `w:end`).
    pub indent_right_twips: u32,
    /// Unsupported child elements preserved for round-trip.
    pub unsupported: Vec<OpaqueXmlNode>,
}

/// How `w:spacing/@w:line` is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineSpacingRule {
    /// Multiples of a line (240 = single). Default when omitted.
    #[default]
    Auto,
    /// Exact line height in twips.
    Exact,
    /// Minimum line height in twips.
    AtLeast,
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

/// Vertical merge state for a table cell (`w:vMerge`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VMerge {
    /// First cell of a vertical merge (`w:val="restart"`).
    Restart,
    /// Continuation of a vertical merge (bare `<w:vMerge/>` or `continue`).
    #[default]
    Continue,
}

/// One cell in a table.
///
/// Paragraphs hold editable text; [`Self::images`] are anchored after a paragraph
/// index and are selectable/deletable via the preview cell-image cursor.
/// [`Self::grid_span`] / [`Self::v_merge`] are preserved for OOXML round-trip
/// and drive preview cell boxes (continue slots are covered by the restart).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableCell {
    /// Paragraphs inside the cell.
    pub paragraphs: Vec<Paragraph>,
    /// Images parsed from `w:drawing` in cell paragraphs (click/delete/arrow navigable).
    pub images: Vec<CellImage>,
    /// Horizontal span in grid columns (`w:gridSpan/@w:val`). `None` means 1.
    pub grid_span: Option<u32>,
    /// Vertical merge (`w:vMerge`). `None` means not merged.
    pub v_merge: Option<VMerge>,
}

impl TableCell {
    /// Cell with paragraphs and no images.
    #[must_use]
    pub fn from_paragraphs(paragraphs: Vec<Paragraph>) -> Self {
        Self {
            paragraphs,
            images: Vec::new(),
            grid_span: None,
            v_merge: None,
        }
    }
}

/// One row in a table.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableRow {
    /// Cells left-to-right.
    pub cells: Vec<TableCell>,
}

/// A table of cells.
///
/// Cell `gridSpan` / `vMerge` attrs are round-tripped on [`TableCell`] and
/// rendered as spanning boxes in the preview.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Table {
    /// Rows top-to-bottom.
    pub rows: Vec<TableRow>,
    /// Per-column preferred widths in twips (`w:tblGrid` / `w:gridCol/@w:w`).
    ///
    /// Empty means equal-width columns in the preview (inserted blank tables).
    pub column_widths_twips: Vec<u32>,
    /// Unsupported elements preserved for round-trip (non-merge leftovers).
    pub unsupported: Vec<OpaqueXmlNode>,
}

impl Table {
    /// Empty `rows`×`cols` grid of blank paragraphs (each dimension clamped to 1..=20).
    #[must_use]
    pub fn empty(rows: usize, cols: usize) -> Self {
        let rows = rows.clamp(1, 20);
        let cols = cols.clamp(1, 20);
        Self {
            rows: (0..rows)
                .map(|_| TableRow {
                    cells: (0..cols)
                        .map(|_| TableCell::from_paragraphs(vec![Paragraph::default()]))
                        .collect(),
                })
                .collect(),
            column_widths_twips: Vec::new(),
            unsupported: Vec::new(),
        }
    }
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
    /// Named destinations (`w:bookmarkStart`); first occurrence of a name wins on jump.
    pub bookmarks: Vec<Bookmark>,
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
    /// Plain-text offset of the first bookmark named `name`, if any.
    #[must_use]
    pub fn bookmark_offset(&self, name: &str) -> Option<usize> {
        self.bookmarks
            .iter()
            .find(|b| b.name == name)
            .map(|b| b.plain_offset)
    }

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

/// Word and character counts for the document status strip.
///
/// Words are whitespace-separated; characters exclude `\n` / `\r` (paragraph
/// joiners in [`Document::plain_text`]) but include spaces and punctuation.
#[must_use]
pub fn text_stats(plain: &str) -> (u32, u32) {
    let words = plain.split_whitespace().count() as u32;
    let chars = plain
        .chars()
        .filter(|c| *c != '\n' && *c != '\r')
        .count() as u32;
    (words, chars)
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
                ..Default::default()
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
                    ..Default::default()
                },
                Run {
                    text: "world".into(),
                    style: RunStyle::default(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(p.plain_text(), "Hello world");
    }

    #[test]
    fn text_stats_counts_words_and_chars() {
        let (w, c) = text_stats("Hello world\nnext");
        assert_eq!(w, 3);
        assert_eq!(c, 15); // "Hello world" (11) + "next" (4)
        assert_eq!(text_stats(""), (0, 0));
        assert_eq!(text_stats("  a  b  "), (2, 8));
    }
}
