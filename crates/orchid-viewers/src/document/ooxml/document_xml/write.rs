//! Serialise `word/document.xml`.

#![allow(unused_imports)]

use std::collections::HashMap;
use std::io::Cursor;

use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;

use crate::document::model::{
    Alignment, Block, Bookmark, CellImage, CommentRange, DocField, Document, Hyperlink,
    ImageFormat, InlineImage, LineSpacingRule, ListKind, OpaqueXmlNode, PageSetup, Paragraph, Run,
    RunStyle, SectionBreakType, Table, TableCell, TableRow, VMerge, CELL_BORDER_BOTTOM,
    CELL_BORDER_LEFT, CELL_BORDER_RIGHT, CELL_BORDER_TOP,
};
use crate::document::ooxml::numbering::NumberingDefs;
use crate::document::ooxml::styles::StyleDefaults;
use crate::error::{Result, ViewerError};

use super::helpers::*;
use super::Relationships;

/// Serialise a header or footer story (`w:hdr` / `w:ftr`).
///
/// # Errors
///
/// [`ViewerError::DocumentSave`] on writer failures.
pub fn write_story_xml(root_local: &str, paragraphs: &[Paragraph]) -> Result<Vec<u8>> {
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    writer
        .write_event(Event::Decl(quick_xml::events::BytesDecl::new(
            "1.0",
            Some("UTF-8"),
            Some("yes"),
        )))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;

    let root_tag = format!("w:{root_local}");
    let mut root = BytesStart::new(root_tag.as_str());
    root.push_attribute((
        "xmlns:w",
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    ));
    root.push_attribute((
        "xmlns:r",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    ));
    writer
        .write_event(Event::Start(root))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;

    let mut bookmark_id = 0u32;
    if paragraphs.is_empty() {
        write_paragraph(
            &mut writer,
            &Paragraph::default(),
            &[],
            &[],
            0,
            &mut bookmark_id,
        )?;
    } else {
        for p in paragraphs {
            write_paragraph(&mut writer, p, &[], &[], 0, &mut bookmark_id)?;
        }
    }

    writer
        .write_event(Event::End(BytesEnd::new(root_tag.as_str())))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(writer.into_inner().into_inner())
}

pub(super) fn write_fld_simple(writer: &mut Writer<Cursor<Vec<u8>>>, run: &Run) -> Result<()> {
    let Some(field) = run.field else {
        return write_run(writer, run);
    };
    let mut start = BytesStart::new("w:fldSimple");
    start.push_attribute(("w:instr", format!(" {} ", field.instr()).as_str()));
    writer
        .write_event(Event::Start(start))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    write_run(
        writer,
        &Run {
            text: if run.text.is_empty() {
                field.display(1, 1, None)
            } else {
                run.text.clone()
            },
            style: run.style.clone(),
            style_id: run.style_id.clone(),
            hyperlink: None,
            field: None,
        },
    )?;
    writer
        .write_event(Event::End(BytesEnd::new("w:fldSimple")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(())
}

/// Serialise a [`Document`] to `word/document.xml` bytes.
///
/// # Errors
///
/// [`ViewerError::DocumentSave`] on writer failures.
pub fn write_document_xml(doc: &Document) -> Result<Vec<u8>> {
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    writer
        .write_event(Event::Decl(quick_xml::events::BytesDecl::new(
            "1.0",
            Some("UTF-8"),
            Some("yes"),
        )))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;

    let mut doc_start = BytesStart::new("w:document");
    doc_start.push_attribute((
        "xmlns:w",
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    ));
    doc_start.push_attribute((
        "xmlns:r",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    ));
    doc_start.push_attribute((
        "xmlns:wp",
        "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
    ));
    doc_start.push_attribute((
        "xmlns:a",
        "http://schemas.openxmlformats.org/drawingml/2006/main",
    ));
    doc_start.push_attribute((
        "xmlns:pic",
        "http://schemas.openxmlformats.org/drawingml/2006/picture",
    ));
    writer
        .write_event(Event::Start(doc_start))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    writer
        .write_event(Event::Start(BytesStart::new("w:body")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;

    let mut drawing_id = 1u32;
    let mut plain_len = 0usize;
    let mut bookmark_id = 0u32;
    for block in &doc.blocks {
        match block {
            Block::Paragraph(p) => {
                let para_start = if plain_len > 0 { plain_len + 1 } else { 0 };
                write_paragraph(
                    &mut writer,
                    p,
                    &doc.bookmarks,
                    &doc.comment_ranges,
                    para_start,
                    &mut bookmark_id,
                )?;
                plain_len = para_start + p.plain_text().len();
            }
            Block::Table(t) => {
                write_table(&mut writer, t, &mut drawing_id)?;
                // Match Document::plain_text: each cell paragraph is separated by `\n`.
                for row in &t.rows {
                    for cell in &row.cells {
                        for p in &cell.paragraphs {
                            let start = if plain_len > 0 { plain_len + 1 } else { 0 };
                            plain_len = start + p.plain_text().len();
                        }
                    }
                }
            }
            Block::Image(img) => {
                write_image_paragraph(&mut writer, img, &mut drawing_id)?;
            }
        }
    }

    for node in &doc.unsupported {
        // Best-effort: write raw XML bytes as-is.
        let cursor = writer.get_mut();
        cursor.get_mut().extend_from_slice(&node.raw_xml);
        let end = cursor.get_ref().len() as u64;
        cursor.set_position(end);
    }

    write_sect_pr(&mut writer, &doc.page_setup)?;

    writer
        .write_event(Event::End(BytesEnd::new("w:body")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    writer
        .write_event(Event::End(BytesEnd::new("w:document")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;

    Ok(writer.into_inner().into_inner())
}

pub(super) fn write_comment_range_start(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    id: u32,
) -> Result<()> {
    let mut start = BytesStart::new("w:commentRangeStart");
    start.push_attribute(("w:id", id.to_string().as_str()));
    writer
        .write_event(Event::Empty(start))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))
}

pub(super) fn write_comment_range_end_and_ref(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    id: u32,
) -> Result<()> {
    let id_s = id.to_string();
    let mut end = BytesStart::new("w:commentRangeEnd");
    end.push_attribute(("w:id", id_s.as_str()));
    writer
        .write_event(Event::Empty(end))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    writer
        .write_event(Event::Start(BytesStart::new("w:r")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    let mut cref = BytesStart::new("w:commentReference");
    cref.push_attribute(("w:id", id_s.as_str()));
    writer
        .write_event(Event::Empty(cref))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    writer
        .write_event(Event::End(BytesEnd::new("w:r")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))
}

pub(super) fn write_collapsed_bookmark(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    id: u32,
    name: &str,
) -> Result<()> {
    let id_s = id.to_string();
    let mut start = BytesStart::new("w:bookmarkStart");
    start.push_attribute(("w:id", id_s.as_str()));
    start.push_attribute(("w:name", name));
    writer
        .write_event(Event::Empty(start))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    let mut end = BytesStart::new("w:bookmarkEnd");
    end.push_attribute(("w:id", id_s.as_str()));
    writer
        .write_event(Event::Empty(end))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(())
}

pub(super) fn write_paragraph(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    p: &Paragraph,
    bookmarks: &[Bookmark],
    comment_ranges: &[CommentRange],
    para_start: usize,
    bookmark_id: &mut u32,
) -> Result<()> {
    writer
        .write_event(Event::Start(BytesStart::new("w:p")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    writer
        .write_event(Event::Start(BytesStart::new("w:pPr")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    if p.alignment != Alignment::Left {
        let mut jc = BytesStart::new("w:jc");
        jc.push_attribute(("w:val", alignment_val(p.alignment)));
        writer
            .write_event(Event::Empty(jc))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if p.page_break_before {
        writer
            .write_event(Event::Empty(BytesStart::new("w:pageBreakBefore")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if p.keep_next {
        writer
            .write_event(Event::Empty(BytesStart::new("w:keepNext")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if p.keep_lines {
        writer
            .write_event(Event::Empty(BytesStart::new("w:keepLines")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if p.widow_control {
        writer
            .write_event(Event::Empty(BytesStart::new("w:widowControl")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if p.contextual_spacing {
        writer
            .write_event(Event::Empty(BytesStart::new("w:contextualSpacing")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if p.bidi {
        writer
            .write_event(Event::Empty(BytesStart::new("w:bidi")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if p.suppress_auto_hyphens {
        writer
            .write_event(Event::Empty(BytesStart::new("w:suppressAutoHyphens")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if let Some(ref sid) = p.style_id {
        let mut ps = BytesStart::new("w:pStyle");
        ps.push_attribute(("w:val", sid.as_str()));
        writer
            .write_event(Event::Empty(ps))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if let Some(lvl) = p.outline_level {
        let mut ol = BytesStart::new("w:outlineLvl");
        ol.push_attribute(("w:val", lvl.to_string().as_str()));
        writer
            .write_event(Event::Empty(ol))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if p.space_before_twips > 0
        || p.space_after_twips > 0
        || p.line_spacing > 0
        || p.line_spacing_rule != LineSpacingRule::Auto
    {
        let mut spacing = BytesStart::new("w:spacing");
        if p.space_before_twips > 0 {
            spacing.push_attribute(("w:before", p.space_before_twips.to_string().as_str()));
        }
        if p.space_after_twips > 0 {
            spacing.push_attribute(("w:after", p.space_after_twips.to_string().as_str()));
        }
        if p.line_spacing > 0 || p.line_spacing_rule != LineSpacingRule::Auto {
            let line_val = if p.line_spacing > 0 {
                p.line_spacing
            } else {
                // Exact/AtLeast with 0 is invalid; emit a minimal 1 twip to keep the rule.
                1
            };
            spacing.push_attribute(("w:line", line_val.to_string().as_str()));
            let rule = match p.line_spacing_rule {
                LineSpacingRule::Auto => "auto",
                LineSpacingRule::Exact => "exact",
                LineSpacingRule::AtLeast => "atLeast",
            };
            spacing.push_attribute(("w:lineRule", rule));
        }
        writer
            .write_event(Event::Empty(spacing))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if p.indent_left_twips > 0 || p.indent_first_line_twips != 0 || p.indent_right_twips > 0 {
        let mut ind = BytesStart::new("w:ind");
        if p.indent_left_twips > 0 {
            ind.push_attribute(("w:left", p.indent_left_twips.to_string().as_str()));
        }
        if p.indent_right_twips > 0 {
            ind.push_attribute(("w:right", p.indent_right_twips.to_string().as_str()));
        }
        if p.indent_first_line_twips > 0 {
            ind.push_attribute((
                "w:firstLine",
                p.indent_first_line_twips.to_string().as_str(),
            ));
        } else if p.indent_first_line_twips < 0 {
            ind.push_attribute((
                "w:hanging",
                (-p.indent_first_line_twips).to_string().as_str(),
            ));
        }
        writer
            .write_event(Event::Empty(ind))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if let Some([r, g, b]) = p.shade_fill {
        let mut shd = BytesStart::new("w:shd");
        shd.push_attribute(("w:val", "clear"));
        shd.push_attribute(("w:color", "auto"));
        let fill = format!("{r:02X}{g:02X}{b:02X}");
        shd.push_attribute(("w:fill", fill.as_str()));
        writer
            .write_event(Event::Empty(shd))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if p.border_sides != 0 {
        write_paragraph_borders(writer, p.border_sides)?;
    }
    if p.list != ListKind::None {
        writer
            .write_event(Event::Start(BytesStart::new("w:numPr")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        let mut ilvl = BytesStart::new("w:ilvl");
        ilvl.push_attribute(("w:val", p.list_level.to_string().as_str()));
        writer
            .write_event(Event::Empty(ilvl))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        let id = p
            .num_id
            .or_else(|| crate::document::ooxml::numbering::num_id_for_kind(p.list));
        if let Some(id) = id {
            let mut num = BytesStart::new("w:numId");
            num.push_attribute(("w:val", id.to_string().as_str()));
            writer
                .write_event(Event::Empty(num))
                .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        }
        writer
            .write_event(Event::End(BytesEnd::new("w:numPr")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if let Some(ref setup) = p.section_properties {
        write_sect_pr(writer, setup)?;
    }
    writer
        .write_event(Event::End(BytesEnd::new("w:pPr")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;

    // Bookmarks at paragraph start (including empty paragraphs).
    for b in bookmarks {
        if b.plain_offset == para_start {
            write_collapsed_bookmark(writer, *bookmark_id, &b.name)?;
            *bookmark_id += 1;
        }
    }
    for c in comment_ranges {
        if c.start_plain == para_start {
            write_comment_range_start(writer, c.id)?;
        }
    }
    for c in comment_ranges {
        if c.end_plain == para_start && c.start_plain == para_start {
            write_comment_range_end_and_ref(writer, c.id)?;
        }
    }

    let mut run_rel = 0usize;
    let mut i = 0;
    while i < p.runs.len() {
        if run_rel > 0 {
            for b in bookmarks {
                if b.plain_offset == para_start + run_rel {
                    write_collapsed_bookmark(writer, *bookmark_id, &b.name)?;
                    *bookmark_id += 1;
                }
            }
            for c in comment_ranges {
                if c.start_plain == para_start + run_rel {
                    write_comment_range_start(writer, c.id)?;
                }
            }
            for c in comment_ranges {
                if c.end_plain == para_start + run_rel && c.start_plain != c.end_plain {
                    write_comment_range_end_and_ref(writer, c.id)?;
                }
            }
        }
        if let Some(ref hl) = p.runs[i].hyperlink {
            let target = hl.display_target();
            let mut j = i + 1;
            while j < p.runs.len()
                && p.runs[j]
                    .hyperlink
                    .as_ref()
                    .is_some_and(|h| h.display_target() == target)
            {
                j += 1;
            }
            let mut start = BytesStart::new("w:hyperlink");
            if hl.is_internal() {
                if let Some(name) = hl.bookmark.as_deref() {
                    start.push_attribute(("w:anchor", name));
                }
            } else {
                let rid = hl.r_id.as_deref().unwrap_or("rId0");
                start.push_attribute(("r:id", rid));
            }
            start.push_attribute(("w:history", "1"));
            writer
                .write_event(Event::Start(start))
                .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
            for run in &p.runs[i..j] {
                write_run(writer, run)?;
                run_rel += run.text.len();
            }
            writer
                .write_event(Event::End(BytesEnd::new("w:hyperlink")))
                .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
            i = j;
        } else if p.runs[i].field.is_some() {
            write_fld_simple(writer, &p.runs[i])?;
            run_rel += p.runs[i].text.len();
            i += 1;
        } else {
            write_run(writer, &p.runs[i])?;
            run_rel += p.runs[i].text.len();
            i += 1;
        }
    }
    for c in comment_ranges {
        if c.end_plain == para_start + run_rel && c.start_plain != c.end_plain {
            write_comment_range_end_and_ref(writer, c.id)?;
        }
    }
    for node in &p.unsupported {
        writer.get_mut().get_mut().extend_from_slice(&node.raw_xml);
    }
    writer
        .write_event(Event::End(BytesEnd::new("w:p")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(())
}

pub(super) fn write_paragraph_border_side(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    tag: &str,
) -> Result<()> {
    let mut el = BytesStart::new(format!("w:{tag}"));
    el.push_attribute(("w:val", "single"));
    el.push_attribute(("w:sz", "4"));
    el.push_attribute(("w:space", "1"));
    el.push_attribute(("w:color", "auto"));
    writer
        .write_event(Event::Empty(el))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(())
}

pub(super) fn write_paragraph_borders(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    sides: u8,
) -> Result<()> {
    writer
        .write_event(Event::Start(BytesStart::new("w:pBdr")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    if sides & CELL_BORDER_TOP != 0 {
        write_paragraph_border_side(writer, "top")?;
    }
    if sides & CELL_BORDER_LEFT != 0 {
        write_paragraph_border_side(writer, "left")?;
    }
    if sides & CELL_BORDER_BOTTOM != 0 {
        write_paragraph_border_side(writer, "bottom")?;
    }
    if sides & CELL_BORDER_RIGHT != 0 {
        write_paragraph_border_side(writer, "right")?;
    }
    writer
        .write_event(Event::End(BytesEnd::new("w:pBdr")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(())
}

pub(super) fn write_run(writer: &mut Writer<Cursor<Vec<u8>>>, run: &Run) -> Result<()> {
    writer
        .write_event(Event::Start(BytesStart::new("w:r")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    writer
        .write_event(Event::Start(BytesStart::new("w:rPr")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    if let Some(ref sid) = run.style_id {
        if !sid.is_empty() {
            let mut rs = BytesStart::new("w:rStyle");
            rs.push_attribute(("w:val", sid.as_str()));
            writer
                .write_event(Event::Empty(rs))
                .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        }
    }
    if run.style.bold {
        writer
            .write_event(Event::Empty(BytesStart::new("w:b")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if run.style.italic {
        writer
            .write_event(Event::Empty(BytesStart::new("w:i")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if run.style.underline {
        let mut u = BytesStart::new("w:u");
        u.push_attribute(("w:val", "single"));
        writer
            .write_event(Event::Empty(u))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if run.style.strikethrough {
        writer
            .write_event(Event::Empty(BytesStart::new("w:strike")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if run.style.double_strikethrough {
        writer
            .write_event(Event::Empty(BytesStart::new("w:dstrike")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if run.style.highlight {
        let mut hl = BytesStart::new("w:highlight");
        hl.push_attribute(("w:val", "yellow"));
        writer
            .write_event(Event::Empty(hl))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if run.style.superscript {
        let mut va = BytesStart::new("w:vertAlign");
        va.push_attribute(("w:val", "superscript"));
        writer
            .write_event(Event::Empty(va))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    } else if run.style.subscript {
        let mut va = BytesStart::new("w:vertAlign");
        va.push_attribute(("w:val", "subscript"));
        writer
            .write_event(Event::Empty(va))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if run.style.all_caps {
        writer
            .write_event(Event::Empty(BytesStart::new("w:caps")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if run.style.small_caps {
        writer
            .write_event(Event::Empty(BytesStart::new("w:smallCaps")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if run.style.vanish {
        writer
            .write_event(Event::Empty(BytesStart::new("w:vanish")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if run.style.shadow {
        writer
            .write_event(Event::Empty(BytesStart::new("w:shadow")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if run.style.emboss {
        writer
            .write_event(Event::Empty(BytesStart::new("w:emboss")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if run.style.imprint {
        writer
            .write_event(Event::Empty(BytesStart::new("w:imprint")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if let Some([r, g, b]) = run.style.color {
        let mut c = BytesStart::new("w:color");
        c.push_attribute(("w:val", format!("{r:02X}{g:02X}{b:02X}").as_str()));
        writer
            .write_event(Event::Empty(c))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if let Some(ref family) = run.style.font_family {
        let mut f = BytesStart::new("w:rFonts");
        f.push_attribute(("w:ascii", family.as_str()));
        f.push_attribute(("w:hAnsi", family.as_str()));
        writer
            .write_event(Event::Empty(f))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if let Some(pt) = run.style.font_size_pt {
        let mut sz = BytesStart::new("w:sz");
        sz.push_attribute(("w:val", ((pt * 2.0) as u32).to_string().as_str()));
        writer
            .write_event(Event::Empty(sz))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    writer
        .write_event(Event::End(BytesEnd::new("w:rPr")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;

    // Soft line breaks are `\n` in the model (from `w:br` on read). Emit them as
    // empty `<w:br/>` elements rather than embedding newlines inside `<w:t>`.
    let normalized = run.text.replace("\r\n", "\n").replace('\r', "\n");
    let parts: Vec<&str> = normalized.split('\n').collect();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            writer
                .write_event(Event::Empty(BytesStart::new("w:br")))
                .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        }
        if part.is_empty() {
            if parts.len() == 1 {
                write_text_element(writer, "")?;
            }
            continue;
        }
        write_text_element(writer, part)?;
    }
    writer
        .write_event(Event::End(BytesEnd::new("w:r")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(())
}

pub(super) fn write_text_element(writer: &mut Writer<Cursor<Vec<u8>>>, text: &str) -> Result<()> {
    let mut t = BytesStart::new("w:t");
    if text.starts_with(' ') || text.ends_with(' ') || text.contains('\t') {
        t.push_attribute(("xml:space", "preserve"));
    }
    writer
        .write_event(Event::Start(t))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    writer
        .write_event(Event::Text(BytesText::new(text)))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    writer
        .write_event(Event::End(BytesEnd::new("w:t")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(())
}

pub(super) fn write_tc_border_side(writer: &mut Writer<Cursor<Vec<u8>>>, tag: &str) -> Result<()> {
    let mut el = BytesStart::new(format!("w:{tag}"));
    el.push_attribute(("w:val", "single"));
    el.push_attribute(("w:sz", "4"));
    el.push_attribute(("w:space", "0"));
    el.push_attribute(("w:color", "auto"));
    writer
        .write_event(Event::Empty(el))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(())
}

pub(super) fn write_tc_borders(writer: &mut Writer<Cursor<Vec<u8>>>, sides: u8) -> Result<()> {
    writer
        .write_event(Event::Start(BytesStart::new("w:tcBorders")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    if sides & CELL_BORDER_TOP != 0 {
        write_tc_border_side(writer, "top")?;
    }
    if sides & CELL_BORDER_LEFT != 0 {
        write_tc_border_side(writer, "left")?;
    }
    if sides & CELL_BORDER_BOTTOM != 0 {
        write_tc_border_side(writer, "bottom")?;
    }
    if sides & CELL_BORDER_RIGHT != 0 {
        write_tc_border_side(writer, "right")?;
    }
    writer
        .write_event(Event::End(BytesEnd::new("w:tcBorders")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(())
}

pub(super) fn write_table(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    t: &Table,
    drawing_id: &mut u32,
) -> Result<()> {
    writer
        .write_event(Event::Start(BytesStart::new("w:tbl")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    if !t.column_widths_twips.is_empty() {
        writer
            .write_event(Event::Start(BytesStart::new("w:tblGrid")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        for &w in &t.column_widths_twips {
            let mut grid_col = BytesStart::new("w:gridCol");
            grid_col.push_attribute(("w:w", w.to_string().as_str()));
            writer
                .write_event(Event::Empty(grid_col))
                .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        }
        writer
            .write_event(Event::End(BytesEnd::new("w:tblGrid")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    for row in &t.rows {
        writer
            .write_event(Event::Start(BytesStart::new("w:tr")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        for (ci, cell) in row.cells.iter().enumerate() {
            writer
                .write_event(Event::Start(BytesStart::new("w:tc")))
                .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
            let width = t.column_widths_twips.get(ci).copied();
            let need_tc_pr = width.is_some()
                || cell.grid_span.is_some()
                || cell.v_merge.is_some()
                || cell.shade_fill.is_some()
                || cell.border_sides != 0;
            if need_tc_pr {
                writer
                    .write_event(Event::Start(BytesStart::new("w:tcPr")))
                    .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
                if let Some(w) = width {
                    let mut tc_w = BytesStart::new("w:tcW");
                    tc_w.push_attribute(("w:w", w.to_string().as_str()));
                    tc_w.push_attribute(("w:type", "dxa"));
                    writer
                        .write_event(Event::Empty(tc_w))
                        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
                }
                if let Some(span) = cell.grid_span.filter(|&s| s > 1) {
                    let mut gs = BytesStart::new("w:gridSpan");
                    gs.push_attribute(("w:val", span.to_string().as_str()));
                    writer
                        .write_event(Event::Empty(gs))
                        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
                }
                if let Some(vm) = cell.v_merge {
                    let mut vm_el = BytesStart::new("w:vMerge");
                    match vm {
                        VMerge::Restart => {
                            vm_el.push_attribute(("w:val", "restart"));
                        }
                        VMerge::Continue => {}
                    }
                    writer
                        .write_event(Event::Empty(vm_el))
                        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
                }
                if cell.border_sides != 0 {
                    write_tc_borders(writer, cell.border_sides)?;
                }
                if let Some([r, g, b]) = cell.shade_fill {
                    let mut shd = BytesStart::new("w:shd");
                    shd.push_attribute(("w:val", "clear"));
                    shd.push_attribute(("w:color", "auto"));
                    let fill = format!("{r:02X}{g:02X}{b:02X}");
                    shd.push_attribute(("w:fill", fill.as_str()));
                    writer
                        .write_event(Event::Empty(shd))
                        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
                }
                writer
                    .write_event(Event::End(BytesEnd::new("w:tcPr")))
                    .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
            }
            if cell.paragraphs.is_empty() {
                let mut bm_id = 0u32;
                write_paragraph(writer, &Paragraph::default(), &[], &[], 0, &mut bm_id)?;
                for ci in cell.images.iter().filter(|c| c.after_paragraph == 0) {
                    write_image_paragraph(writer, &ci.image, drawing_id)?;
                }
            } else {
                for (i, p) in cell.paragraphs.iter().enumerate() {
                    let mut bm_id = 0u32;
                    write_paragraph(writer, p, &[], &[], 0, &mut bm_id)?;
                    for ci in cell.images.iter().filter(|c| c.after_paragraph == i) {
                        write_image_paragraph(writer, &ci.image, drawing_id)?;
                    }
                }
            }
            writer
                .write_event(Event::End(BytesEnd::new("w:tc")))
                .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        }
        writer
            .write_event(Event::End(BytesEnd::new("w:tr")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    writer
        .write_event(Event::End(BytesEnd::new("w:tbl")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(())
}

pub(super) fn write_image_paragraph(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    img: &InlineImage,
    drawing_id: &mut u32,
) -> Result<()> {
    let Some(rid) = img.r_id.as_deref() else {
        return Err(ViewerError::DocumentSave(
            "image missing relationship id before save".into(),
        ));
    };
    let cx = css_px_to_emu(img.width_px);
    let cy = css_px_to_emu(img.height_px);
    let id = *drawing_id;
    *drawing_id = drawing_id.saturating_add(1);
    let name = img
        .part_path
        .as_deref()
        .and_then(|p| p.rsplit('/').next())
        .unwrap_or("image");
    // Minimal wp:inline drawing Word/LibreOffice accept.
    let xml = format!(
        r#"<w:p><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="{id}" name="{name}"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="0" name="{name}"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#
    );
    // Append via the underlying `Vec` then seek: `Cursor` position is not
    // advanced by `Vec::extend`, and later `Writer` events would overwrite us.
    let cursor = writer.get_mut();
    cursor.get_mut().extend_from_slice(xml.as_bytes());
    let end = cursor.get_ref().len() as u64;
    cursor.set_position(end);
    Ok(())
}

pub(super) fn write_sect_pr(writer: &mut Writer<Cursor<Vec<u8>>>, setup: &PageSetup) -> Result<()> {
    writer
        .write_event(Event::Start(BytesStart::new("w:sectPr")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    if setup.section_break == SectionBreakType::Continuous {
        let mut ty = BytesStart::new("w:type");
        ty.push_attribute(("w:val", "continuous"));
        writer
            .write_event(Event::Empty(ty))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    let landscape = setup.width_twips > setup.height_twips;
    let mut sz = BytesStart::new("w:pgSz");
    sz.push_attribute(("w:w", setup.width_twips.to_string().as_str()));
    sz.push_attribute(("w:h", setup.height_twips.to_string().as_str()));
    if landscape {
        sz.push_attribute(("w:orient", "landscape"));
    }
    writer
        .write_event(Event::Empty(sz))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    let mut mar = BytesStart::new("w:pgMar");
    mar.push_attribute(("w:top", setup.margin_top_twips.to_string().as_str()));
    mar.push_attribute(("w:bottom", setup.margin_bottom_twips.to_string().as_str()));
    mar.push_attribute(("w:left", setup.margin_left_twips.to_string().as_str()));
    mar.push_attribute(("w:right", setup.margin_right_twips.to_string().as_str()));
    mar.push_attribute(("w:header", setup.header_distance_twips.to_string().as_str()));
    mar.push_attribute(("w:footer", setup.footer_distance_twips.to_string().as_str()));
    writer
        .write_event(Event::Empty(mar))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    if let Some(ref id) = setup.header_r_id {
        let mut href = BytesStart::new("w:headerReference");
        href.push_attribute(("w:type", "default"));
        href.push_attribute(("r:id", id.as_str()));
        writer
            .write_event(Event::Empty(href))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if let Some(ref id) = setup.footer_r_id {
        let mut fref = BytesStart::new("w:footerReference");
        fref.push_attribute(("w:type", "default"));
        fref.push_attribute(("r:id", id.as_str()));
        writer
            .write_event(Event::Empty(fref))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if let Some(ref id) = setup.header_first_r_id {
        let mut href = BytesStart::new("w:headerReference");
        href.push_attribute(("w:type", "first"));
        href.push_attribute(("r:id", id.as_str()));
        writer
            .write_event(Event::Empty(href))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if let Some(ref id) = setup.footer_first_r_id {
        let mut fref = BytesStart::new("w:footerReference");
        fref.push_attribute(("w:type", "first"));
        fref.push_attribute(("r:id", id.as_str()));
        writer
            .write_event(Event::Empty(fref))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if let Some(ref id) = setup.header_even_r_id {
        let mut href = BytesStart::new("w:headerReference");
        href.push_attribute(("w:type", "even"));
        href.push_attribute(("r:id", id.as_str()));
        writer
            .write_event(Event::Empty(href))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if let Some(ref id) = setup.footer_even_r_id {
        let mut fref = BytesStart::new("w:footerReference");
        fref.push_attribute(("w:type", "even"));
        fref.push_attribute(("r:id", id.as_str()));
        writer
            .write_event(Event::Empty(fref))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if setup.title_page {
        writer
            .write_event(Event::Empty(BytesStart::new("w:titlePg")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    if setup.even_and_odd_headers {
        writer
            .write_event(Event::Empty(BytesStart::new("w:evenAndOddHeaders")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    writer
        .write_event(Event::End(BytesEnd::new("w:sectPr")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(())
}
