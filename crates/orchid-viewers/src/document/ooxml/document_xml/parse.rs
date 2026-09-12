//! Parse `word/document.xml`.

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

/// Complex `w:fldChar` state while walking a paragraph.
pub(super) enum ComplexFieldParse {
    Off,
    Instr {
        instr: String,
        style: RunStyle,
        link: Option<Hyperlink>,
    },
    Result {
        field: Option<DocField>,
        text: String,
        style: RunStyle,
        link: Option<Hyperlink>,
    },
}

/// Parse document relationships XML.
///
/// # Errors
///
/// [`ViewerError::DocumentParse`] on malformed XML.
pub fn parse_relationships(bytes: &[u8]) -> Result<Relationships> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut map = Relationships::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                if local_name(e.name().as_ref()) == "Relationship" {
                    let id = attr_val(&e, "Id");
                    let target = attr_val(&e, "Target");
                    if let (Some(id), Some(target)) = (id, target) {
                        map.insert(id, target);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(ViewerError::DocumentParse(format!("rels: {e}")));
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(map)
}

/// Parse `word/document.xml` into blocks + page setup.
///
/// # Errors
///
/// [`ViewerError::DocumentParse`] on malformed XML.
pub fn parse_document_xml(
    bytes: &[u8],
    styles: &StyleDefaults,
    numbering: &NumberingDefs,
    rels: &Relationships,
    media: &HashMap<String, Vec<u8>>,
) -> Result<(
    Vec<Block>,
    PageSetup,
    Vec<OpaqueXmlNode>,
    Vec<Bookmark>,
    Vec<CommentRange>,
)> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut blocks = Vec::new();
    let mut unsupported = Vec::new();
    let mut bookmarks: Vec<Bookmark> = Vec::new();
    let mut comment_ranges: Vec<CommentRange> = Vec::new();
    let mut open_comments: Vec<(u32, usize)> = Vec::new();
    let mut pending_body_bookmarks: Vec<String> = Vec::new();
    let mut plain_len = 0usize;
    let mut page_setup = PageSetup::default();
    let mut in_body = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "body" {
                    in_body = true;
                } else if in_body {
                    match local.as_str() {
                        "p" => {
                            let (p, images, local_bms, c_starts, c_ends) = parse_paragraph(
                                &mut reader,
                                &mut buf,
                                styles,
                                numbering,
                                rels,
                                media,
                            )?;
                            let has_text = p.runs.iter().any(|r| !r.text.is_empty());
                            let para_start = if plain_len > 0 { plain_len + 1 } else { 0 };
                            for name in pending_body_bookmarks.drain(..) {
                                if !bookmarks.iter().any(|b| b.name == name) {
                                    bookmarks.push(Bookmark {
                                        name,
                                        plain_offset: para_start,
                                    });
                                }
                            }
                            for (name, rel) in local_bms {
                                if !bookmarks.iter().any(|b| b.name == name) {
                                    bookmarks.push(Bookmark {
                                        name,
                                        plain_offset: para_start + rel,
                                    });
                                }
                            }
                            for (id, rel) in c_starts {
                                open_comments.push((id, para_start + rel));
                            }
                            for (id, rel) in c_ends {
                                let end = para_start + rel;
                                if let Some(pos) = open_comments.iter().rposition(|(i, _)| *i == id)
                                {
                                    let (_, start) = open_comments.remove(pos);
                                    comment_ranges.push(CommentRange {
                                        id,
                                        start_plain: start,
                                        end_plain: end.max(start),
                                    });
                                }
                                // Orphan end (e.g. duplicate marker) — ignore.
                            }
                            if has_text || images.is_empty() {
                                plain_len = para_start + p.plain_text().len();
                                blocks.push(Block::Paragraph(p));
                            }
                            for img in images {
                                blocks.push(Block::Image(img));
                            }
                        }
                        "tbl" => {
                            let t =
                                parse_table(&mut reader, &mut buf, styles, numbering, rels, media)?;
                            // Advance plain_len to match Document::plain_text after this table.
                            let before = Document {
                                blocks: blocks.clone(),
                                ..Default::default()
                            }
                            .plain_text()
                            .len();
                            blocks.push(Block::Table(t));
                            plain_len = Document {
                                blocks: blocks.clone(),
                                ..Default::default()
                            }
                            .plain_text()
                            .len();
                            let _ = before;
                        }
                        "bookmarkStart" => {
                            if let Some(name) = attr_val(&e, "name").filter(|n| !n.is_empty()) {
                                pending_body_bookmarks.push(name);
                            }
                        }
                        "sectPr" => {
                            page_setup = parse_sect_pr(&mut reader, &mut buf)?;
                        }
                        other => {
                            let name = other.to_string();
                            let start = e.into_owned();
                            buf.clear();
                            let raw = capture_element(&mut reader, &mut buf, &start)?;
                            unsupported.push(OpaqueXmlNode {
                                position_hint: format!("w:body/w:{name}"),
                                raw_xml: raw,
                            });
                        }
                    }
                }
            }
            Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                if in_body && local == "bookmarkStart" {
                    if let Some(name) = attr_val(&e, "name").filter(|n| !n.is_empty()) {
                        pending_body_bookmarks.push(name);
                    }
                } else if in_body && local == "sectPr" {
                    // Empty sectPr — keep defaults.
                } else if in_body && local != "body" {
                    // Self-closing unknown — skip.
                }
            }
            Ok(Event::End(e)) => {
                if local_name(e.name().as_ref()) == "body" {
                    in_body = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(ViewerError::DocumentParse(format!("document.xml: {e}")));
            }
            _ => {}
        }
        buf.clear();
    }

    // Trailing body bookmarks land at end-of-document.
    for name in pending_body_bookmarks.drain(..) {
        if !bookmarks.iter().any(|b| b.name == name) {
            bookmarks.push(Bookmark {
                name,
                plain_offset: plain_len,
            });
        }
    }
    for (id, start) in open_comments.drain(..) {
        comment_ranges.push(CommentRange {
            id,
            start_plain: start,
            end_plain: plain_len.max(start),
        });
    }

    Ok((blocks, page_setup, unsupported, bookmarks, comment_ranges))
}

/// Parse a header or footer story (`w:hdr` / `w:ftr`) into paragraphs.
///
/// # Errors
///
/// [`ViewerError::DocumentParse`] on malformed XML.
pub fn parse_story_xml(
    bytes: &[u8],
    root_local: &str,
    styles: &StyleDefaults,
    numbering: &NumberingDefs,
) -> Result<Vec<Paragraph>> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut paragraphs = Vec::new();
    let mut in_root = false;
    let empty_rels = Relationships::new();
    let empty_media = HashMap::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if local == root_local {
                    in_root = true;
                } else if in_root && local == "p" {
                    let (p, _images, _bms, _, _) = parse_paragraph(
                        &mut reader,
                        &mut buf,
                        styles,
                        numbering,
                        &empty_rels,
                        &empty_media,
                    )?;
                    paragraphs.push(p);
                }
            }
            Ok(Event::End(e)) => {
                if local_name(e.name().as_ref()) == root_local {
                    break;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(ViewerError::DocumentParse(format!("{root_local}: {e}")));
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(paragraphs)
}

pub(super) fn parse_paragraph(
    reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
    styles: &StyleDefaults,
    numbering: &NumberingDefs,
    rels: &Relationships,
    media: &HashMap<String, Vec<u8>>,
) -> Result<(
    Paragraph,
    Vec<InlineImage>,
    Vec<(String, usize)>,
    Vec<(u32, usize)>,
    Vec<(u32, usize)>,
)> {
    let mut p = Paragraph {
        runs: Vec::new(),
        alignment: Alignment::Left,
        list: ListKind::None,
        list_level: 0,
        num_id: None,
        page_break_before: false,
        keep_next: false,
        keep_lines: false,
        widow_control: false,
        contextual_spacing: false,
        bidi: false,
        suppress_auto_hyphens: false,
        outline_level: None,
        style_id: None,
        space_before_twips: 0,
        space_after_twips: 0,
        line_spacing: 0,
        line_spacing_rule: LineSpacingRule::Auto,
        indent_left_twips: 0,
        indent_first_line_twips: 0,
        indent_right_twips: 0,
        shade_fill: None,
        border_sides: 0,
        unsupported: Vec::new(),
        section_properties: None,
    };
    let mut images = Vec::new();
    let mut local_bookmarks: Vec<(String, usize)> = Vec::new();
    let mut local_comment_starts: Vec<(u32, usize)> = Vec::new();
    let mut local_comment_ends: Vec<(u32, usize)> = Vec::new();
    let mut para_plain_len = 0usize;
    let mut in_p_pr = false;
    let mut in_p_bdr = false;
    let mut in_r = false;
    let mut in_t = false;
    let mut current_run: Option<Run> = None;
    let mut active_link: Option<Hyperlink> = None;
    // Complex field collapse (`w:fldChar` / `w:instrText`) for PAGE / NUMPAGES.
    let mut cx_field: ComplexFieldParse = ComplexFieldParse::Off;
    let mut in_instr_text = false;

    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                match local.as_str() {
                    "pPr" => in_p_pr = true,
                    "jc" if in_p_pr => {
                        if let Some(val) = attr_val(&e, "val") {
                            p.alignment = parse_alignment(&val);
                        }
                    }
                    "numPr" if in_p_pr => {}
                    "ilvl" if in_p_pr => {
                        if let Some(val) = attr_val(&e, "val") {
                            p.list_level = val.parse().unwrap_or(0);
                        }
                    }
                    "numId" if in_p_pr => {
                        if let Some(val) = attr_val(&e, "val") {
                            if let Ok(id) = val.parse::<u32>() {
                                p.num_id = Some(id);
                                p.list = numbering.kind_of(id);
                            }
                        }
                    }
                    "pageBreakBefore" if in_p_pr => {
                        p.page_break_before = true;
                    }
                    "keepNext" if in_p_pr => {
                        p.keep_next = true;
                    }
                    "keepLines" if in_p_pr => {
                        p.keep_lines = true;
                    }
                    "widowControl" if in_p_pr => {
                        p.widow_control = true;
                    }
                    "contextualSpacing" if in_p_pr => {
                        p.contextual_spacing = true;
                    }
                    "bidi" if in_p_pr => {
                        p.bidi = true;
                    }
                    "suppressAutoHyphens" if in_p_pr => {
                        p.suppress_auto_hyphens = true;
                    }
                    "outlineLvl" if in_p_pr => {
                        if let Some(val) = attr_val(&e, "val") {
                            if let Ok(lvl) = val.parse::<u8>() {
                                p.outline_level = Some(lvl.min(8));
                            }
                        }
                    }
                    "pStyle" if in_p_pr => {
                        if let Some(val) = attr_val(&e, "val").filter(|v| !v.is_empty()) {
                            if p.outline_level.is_none() {
                                if let Some(ns) = styles.paragraph_styles.get(&val) {
                                    p.outline_level = ns.outline_level;
                                }
                            }
                            p.style_id = Some(val);
                        }
                    }
                    "spacing" if in_p_pr => {
                        apply_paragraph_spacing(&e, &mut p);
                    }
                    "ind" if in_p_pr => {
                        apply_paragraph_indent(&e, &mut p);
                    }
                    "shd" if in_p_pr => {
                        apply_paragraph_shading(&e, &mut p);
                    }
                    "pBdr" if in_p_pr => {
                        in_p_bdr = true;
                    }
                    "top" if in_p_bdr => {
                        apply_paragraph_border_side(&mut p, CELL_BORDER_TOP, &e);
                    }
                    "left" if in_p_bdr => {
                        apply_paragraph_border_side(&mut p, CELL_BORDER_LEFT, &e);
                    }
                    "bottom" if in_p_bdr => {
                        apply_paragraph_border_side(&mut p, CELL_BORDER_BOTTOM, &e);
                    }
                    "right" if in_p_bdr => {
                        apply_paragraph_border_side(&mut p, CELL_BORDER_RIGHT, &e);
                    }
                    "sectPr" if in_p_pr => {
                        p.section_properties = Some(parse_sect_pr(reader, buf)?);
                    }
                    "hyperlink" => {
                        active_link = resolve_hyperlink(&e, rels);
                    }
                    "bookmarkStart" => {
                        if let Some(name) = attr_val(&e, "name").filter(|n| !n.is_empty()) {
                            local_bookmarks.push((name, para_plain_len));
                        }
                    }
                    "commentRangeStart" => {
                        if let Some(id) = attr_val(&e, "id").and_then(|v| v.parse().ok()) {
                            local_comment_starts.push((id, para_plain_len));
                        }
                    }
                    "commentRangeEnd" => {
                        if let Some(id) = attr_val(&e, "id").and_then(|v| v.parse().ok()) {
                            local_comment_ends.push((id, para_plain_len));
                        }
                    }
                    "fldSimple" => {
                        let instr = attr_val(&e, "instr").unwrap_or_default();
                        let link = active_link.clone();
                        let run = parse_fld_simple(reader, buf, styles, link, &instr)?;
                        para_plain_len += run.text.len();
                        p.runs.push(run);
                    }
                    "r" => {
                        in_r = true;
                        current_run = Some(Run {
                            text: String::new(),
                            style: styles.run.clone(),
                            style_id: None,
                            hyperlink: active_link.clone(),
                            field: None,
                        });
                    }
                    "rPr" if in_r => {
                        if let Some(ref mut run) = current_run {
                            parse_r_pr_into(reader, buf, &mut run.style, &mut run.style_id)?;
                        }
                    }
                    "t" if in_r => {
                        in_t = true;
                    }
                    "drawing" if in_r => {
                        if let Some(img) = parse_drawing_image(reader, buf, rels, media)? {
                            images.push(img);
                        }
                    }
                    "br" if in_r => {
                        if attr_val(&e, "type").as_deref() == Some("page") {
                            p.page_break_before = true;
                        } else if let Some(ref mut run) = current_run {
                            run.text.push('\n');
                        }
                    }
                    "fldChar" => {
                        apply_fld_char(
                            &e,
                            &mut cx_field,
                            &mut current_run,
                            &mut p,
                            &mut para_plain_len,
                            styles,
                            active_link.as_ref(),
                        );
                    }
                    "instrText" if in_r => {
                        in_instr_text = true;
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                if in_p_bdr {
                    match local.as_str() {
                        "top" => apply_paragraph_border_side(&mut p, CELL_BORDER_TOP, &e),
                        "left" => apply_paragraph_border_side(&mut p, CELL_BORDER_LEFT, &e),
                        "bottom" => apply_paragraph_border_side(&mut p, CELL_BORDER_BOTTOM, &e),
                        "right" => apply_paragraph_border_side(&mut p, CELL_BORDER_RIGHT, &e),
                        _ => {}
                    }
                }
                if in_p_pr {
                    match local.as_str() {
                        "jc" => {
                            if let Some(val) = attr_val(&e, "val") {
                                p.alignment = parse_alignment(&val);
                            }
                        }
                        "ilvl" => {
                            if let Some(val) = attr_val(&e, "val") {
                                p.list_level = val.parse().unwrap_or(0);
                            }
                        }
                        "numId" => {
                            if let Some(val) = attr_val(&e, "val") {
                                if let Ok(id) = val.parse::<u32>() {
                                    p.num_id = Some(id);
                                    p.list = numbering.kind_of(id);
                                }
                            }
                        }
                        "pageBreakBefore" => {
                            p.page_break_before = true;
                        }
                        "keepNext" => {
                            p.keep_next = true;
                        }
                        "keepLines" => {
                            p.keep_lines = true;
                        }
                        "widowControl" => {
                            p.widow_control = true;
                        }
                        "contextualSpacing" => {
                            p.contextual_spacing = true;
                        }
                        "bidi" => {
                            p.bidi = true;
                        }
                        "suppressAutoHyphens" => {
                            p.suppress_auto_hyphens = true;
                        }
                        "outlineLvl" => {
                            if let Some(val) = attr_val(&e, "val") {
                                if let Ok(lvl) = val.parse::<u8>() {
                                    p.outline_level = Some(lvl.min(8));
                                }
                            }
                        }
                        "pStyle" => {
                            if let Some(val) = attr_val(&e, "val").filter(|v| !v.is_empty()) {
                                if p.outline_level.is_none() {
                                    if let Some(ns) = styles.paragraph_styles.get(&val) {
                                        p.outline_level = ns.outline_level;
                                    }
                                }
                                p.style_id = Some(val);
                            }
                        }
                        "spacing" => {
                            apply_paragraph_spacing(&e, &mut p);
                        }
                        "ind" => {
                            apply_paragraph_indent(&e, &mut p);
                        }
                        "shd" => {
                            apply_paragraph_shading(&e, &mut p);
                        }
                        _ => {}
                    }
                }
                if in_r && local == "br" {
                    if attr_val(&e, "type").as_deref() == Some("page") {
                        p.page_break_before = true;
                    } else if let Some(ref mut run) = current_run {
                        run.text.push('\n');
                    }
                }
                if local == "fldChar" {
                    apply_fld_char(
                        &e,
                        &mut cx_field,
                        &mut current_run,
                        &mut p,
                        &mut para_plain_len,
                        styles,
                        active_link.as_ref(),
                    );
                }
                if local == "bookmarkStart" {
                    if let Some(name) = attr_val(&e, "name").filter(|n| !n.is_empty()) {
                        local_bookmarks.push((name, para_plain_len));
                    }
                }
                if local == "commentRangeStart" {
                    if let Some(id) = attr_val(&e, "id").and_then(|v| v.parse().ok()) {
                        local_comment_starts.push((id, para_plain_len));
                    }
                }
                if local == "commentRangeEnd" {
                    if let Some(id) = attr_val(&e, "id").and_then(|v| v.parse().ok()) {
                        local_comment_ends.push((id, para_plain_len));
                    }
                }
                if in_r
                    && matches!(
                        local.as_str(),
                        "b" | "i" | "u" | "caps" | "smallCaps" | "color" | "rFonts" | "sz"
                    )
                {
                    if let Some(ref mut run) = current_run {
                        apply_r_pr_attr(&local, &e, &mut run.style);
                    }
                }
            }
            Ok(Event::Text(t)) => {
                let text = t.as_ref();
                if in_instr_text {
                    if let ComplexFieldParse::Instr { instr, .. } = &mut cx_field {
                        instr.push_str(text);
                    }
                } else if in_t {
                    if let Some(ref mut run) = current_run {
                        run.text.push_str(text);
                    }
                    if let ComplexFieldParse::Result { text: result, .. } = &mut cx_field {
                        result.push_str(text);
                    }
                }
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                match local.as_str() {
                    "pPr" => in_p_pr = false,
                    "pBdr" => in_p_bdr = false,
                    "t" => in_t = false,
                    "instrText" => in_instr_text = false,
                    "r" => {
                        in_r = false;
                        in_t = false;
                        in_instr_text = false;
                        if matches!(
                            cx_field,
                            ComplexFieldParse::Instr { .. } | ComplexFieldParse::Result { .. }
                        ) {
                            // Discard shell runs that only carry fldChar / instrText / result
                            // fragments; the collapsed field is pushed on fldChar end.
                            current_run = None;
                        } else if let Some(run) = current_run.take() {
                            para_plain_len += run.text.len();
                            p.runs.push(run);
                        }
                    }
                    "hyperlink" => active_link = None,
                    "p" => {
                        return Ok((
                            p,
                            images,
                            local_bookmarks,
                            local_comment_starts,
                            local_comment_ends,
                        ))
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => {
                return Err(ViewerError::DocumentParse(
                    "unexpected EOF inside paragraph".into(),
                ));
            }
            Err(e) => return Err(ViewerError::DocumentParse(format!("paragraph: {e}"))),
            _ => {}
        }
        buf.clear();
    }
}

pub(super) fn media_part_path(target: &str) -> String {
    let t = target.replace('\\', "/");
    if t.starts_with("word/") {
        t
    } else {
        format!("word/{t}")
    }
}

pub(super) fn apply_fld_char(
    e: &BytesStart<'_>,
    cx_field: &mut ComplexFieldParse,
    current_run: &mut Option<Run>,
    p: &mut Paragraph,
    para_plain_len: &mut usize,
    styles: &StyleDefaults,
    active_link: Option<&Hyperlink>,
) {
    let ty = attr_val(e, "fldCharType").unwrap_or_default();
    match ty.as_str() {
        "begin" => {
            let style = current_run
                .as_ref()
                .map(|r| r.style.clone())
                .unwrap_or_else(|| styles.run.clone());
            *cx_field = ComplexFieldParse::Instr {
                instr: String::new(),
                style,
                link: active_link.cloned(),
            };
            *current_run = None;
        }
        "separate" => {
            if let ComplexFieldParse::Instr { instr, style, link } =
                std::mem::replace(cx_field, ComplexFieldParse::Off)
            {
                *cx_field = ComplexFieldParse::Result {
                    field: DocField::from_instr(&instr),
                    text: String::new(),
                    style,
                    link,
                };
            }
            *current_run = None;
        }
        "end" => {
            if let ComplexFieldParse::Result {
                field,
                text,
                style,
                link,
            } = std::mem::replace(cx_field, ComplexFieldParse::Off)
            {
                let display = if text.is_empty() {
                    field.map(|f| f.display(1, 1, None)).unwrap_or_default()
                } else {
                    text
                };
                if field.is_some() || !display.is_empty() {
                    *para_plain_len += display.len();
                    p.runs.push(Run {
                        text: display,
                        style,
                        style_id: None,
                        hyperlink: link,
                        field,
                    });
                }
            }
            *current_run = None;
        }
        _ => {}
    }
}

pub(super) fn parse_fld_simple(
    reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
    styles: &StyleDefaults,
    link: Option<Hyperlink>,
    instr: &str,
) -> Result<Run> {
    let field = DocField::from_instr(instr);
    let mut text = String::new();
    let mut style = styles.run.clone();
    let mut in_t = false;
    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                match local.as_str() {
                    "rPr" => {
                        let mut sid = None;
                        parse_r_pr_into(reader, buf, &mut style, &mut sid)?;
                    }
                    "t" => in_t = true,
                    _ => {}
                }
            }
            Ok(Event::Empty(_)) => {}
            Ok(Event::Text(t)) => {
                if in_t {
                    text.push_str(t.as_ref());
                }
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "t" {
                    in_t = false;
                }
                if local == "fldSimple" {
                    break;
                }
            }
            Ok(Event::Eof) => {
                return Err(ViewerError::DocumentParse(
                    "unexpected EOF inside fldSimple".into(),
                ));
            }
            Err(e) => {
                return Err(ViewerError::DocumentParse(format!("fldSimple: {e}")));
            }
            _ => {}
        }
        buf.clear();
    }
    if text.is_empty() {
        text = field.map(|f| f.display(1, 1, None)).unwrap_or_default();
    }
    Ok(Run {
        text,
        style,
        style_id: None,
        hyperlink: link,
        field,
    })
}

/// Walk a `w:drawing` subtree and resolve the embedded blip to package media.
pub(super) fn parse_drawing_image(
    reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
    rels: &Relationships,
    media: &HashMap<String, Vec<u8>>,
) -> Result<Option<InlineImage>> {
    let mut embed: Option<String> = None;
    let mut extent: Option<(u64, u64)> = None;
    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                match local.as_str() {
                    "blip" => {
                        if let Some(id) = attr_val(&e, "embed") {
                            embed = Some(id);
                        }
                    }
                    "extent" => {
                        let cx = attr_val(&e, "cx").and_then(|s| s.parse::<u64>().ok());
                        let cy = attr_val(&e, "cy").and_then(|s| s.parse::<u64>().ok());
                        if let (Some(cx), Some(cy)) = (cx, cy) {
                            extent = Some((cx, cy));
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                if local_name(e.name().as_ref()) == "drawing" {
                    break;
                }
            }
            Ok(Event::Eof) => {
                return Err(ViewerError::DocumentParse(
                    "unexpected EOF inside drawing".into(),
                ));
            }
            Err(e) => {
                return Err(ViewerError::DocumentParse(format!("drawing: {e}")));
            }
            _ => {}
        }
        buf.clear();
    }

    let Some(r_id) = embed else {
        return Ok(None);
    };
    let Some(target) = rels.get(&r_id) else {
        return Ok(None);
    };
    let part_path = media_part_path(target);
    let Some(bytes) = media.get(&part_path).cloned() else {
        return Ok(None);
    };
    let mut img = image_from_part(&part_path, bytes, Some(r_id));
    if let Some((cx, cy)) = extent {
        img.width_px = emu_to_css_px(cx);
        img.height_px = emu_to_css_px(cy);
    }
    Ok(Some(img))
}

pub(super) fn parse_r_pr_into(
    reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
    style: &mut RunStyle,
    style_id: &mut Option<String>,
) -> Result<()> {
    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "rStyle" {
                    if let Some(val) = attr_val(&e, "val") {
                        if !val.is_empty() {
                            *style_id = Some(val);
                        }
                    }
                } else {
                    apply_r_pr_attr(&local, &e, style);
                }
            }
            Ok(Event::End(e)) => {
                if local_name(e.name().as_ref()) == "rPr" {
                    return Ok(());
                }
            }
            Ok(Event::Eof) => {
                return Err(ViewerError::DocumentParse(
                    "unexpected EOF inside rPr".into(),
                ));
            }
            Err(e) => return Err(ViewerError::DocumentParse(format!("rPr: {e}"))),
            _ => {}
        }
        buf.clear();
    }
}

pub(super) fn apply_r_pr_attr(local: &str, e: &BytesStart<'_>, style: &mut RunStyle) {
    match local {
        "b" => style.bold = !attr_val(e, "val").is_some_and(|v| v == "0" || v == "false"),
        "i" => style.italic = !attr_val(e, "val").is_some_and(|v| v == "0" || v == "false"),
        "u" => {
            let val = attr_val(e, "val").unwrap_or_else(|| "single".into());
            style.underline = val != "none";
        }
        "strike" => {
            style.strikethrough = !attr_val(e, "val").is_some_and(|v| v == "0" || v == "false");
        }
        "dstrike" => {
            style.double_strikethrough =
                !attr_val(e, "val").is_some_and(|v| v == "0" || v == "false");
        }
        "highlight" => {
            let val = attr_val(e, "val").unwrap_or_default();
            style.highlight = !val.is_empty() && val != "none";
        }
        "vertAlign" => {
            let val = attr_val(e, "val").unwrap_or_default();
            match val.as_str() {
                "superscript" => {
                    style.superscript = true;
                    style.subscript = false;
                }
                "subscript" => {
                    style.subscript = true;
                    style.superscript = false;
                }
                _ => {
                    style.superscript = false;
                    style.subscript = false;
                }
            }
        }
        "caps" => {
            style.all_caps = !attr_val(e, "val").is_some_and(|v| v == "0" || v == "false");
        }
        "smallCaps" => {
            style.small_caps = !attr_val(e, "val").is_some_and(|v| v == "0" || v == "false");
        }
        "vanish" => {
            style.vanish = !attr_val(e, "val").is_some_and(|v| v == "0" || v == "false");
        }
        "shadow" => {
            style.shadow = !attr_val(e, "val").is_some_and(|v| v == "0" || v == "false");
        }
        "emboss" => {
            style.emboss = !attr_val(e, "val").is_some_and(|v| v == "0" || v == "false");
        }
        "imprint" => {
            style.imprint = !attr_val(e, "val").is_some_and(|v| v == "0" || v == "false");
        }
        "color" => {
            if let Some(val) = attr_val(e, "val") {
                style.color = parse_rgb(&val);
            }
        }
        "rFonts" => {
            if let Some(ascii) = attr_val(e, "ascii").or_else(|| attr_val(e, "hAnsi")) {
                style.font_family = Some(ascii);
            }
        }
        "sz" => {
            if let Some(val) = attr_val(e, "val") {
                if let Ok(half) = val.parse::<f32>() {
                    style.font_size_pt = Some(half / 2.0);
                }
            }
        }
        _ => {}
    }
}

pub(super) fn parse_table(
    reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
    styles: &StyleDefaults,
    numbering: &NumberingDefs,
    rels: &Relationships,
    media: &HashMap<String, Vec<u8>>,
) -> Result<Table> {
    let mut table = Table::default();
    let mut current_row: Option<TableRow> = None;
    let mut current_cell: Option<TableCell> = None;
    // First-row `w:tcW` widths used when `w:tblGrid` is absent.
    let mut tcw_fallback: Vec<u32> = Vec::new();
    let mut in_tc_borders = false;

    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                match local.as_str() {
                    "tr" => current_row = Some(TableRow::default()),
                    "tc" => current_cell = Some(TableCell::default()),
                    "gridCol" => {
                        if let Some(w) = parse_grid_col_width(&e) {
                            table.column_widths_twips.push(w);
                        }
                    }
                    "tcW" => {
                        if table.rows.is_empty() {
                            if let Some(w) = parse_tc_width_dxa(&e) {
                                tcw_fallback.push(w);
                            }
                        }
                    }
                    "p" => {
                        let (p, images, _bms, _, _) =
                            parse_paragraph(reader, buf, styles, numbering, rels, media)?;
                        if let Some(ref mut cell) = current_cell {
                            cell.paragraphs.push(p);
                            let after = cell.paragraphs.len().saturating_sub(1);
                            for image in images {
                                cell.images.push(CellImage {
                                    after_paragraph: after,
                                    image,
                                });
                            }
                        }
                    }
                    "gridSpan" => {
                        if let Some(ref mut cell) = current_cell {
                            apply_grid_span(&e, cell);
                        }
                    }
                    "vMerge" => {
                        if let Some(ref mut cell) = current_cell {
                            apply_v_merge(&e, cell);
                        }
                    }
                    "shd" => {
                        if let Some(ref mut cell) = current_cell {
                            apply_cell_shading(&e, cell);
                        }
                    }
                    "tcBorders" => in_tc_borders = true,
                    "top" if in_tc_borders => {
                        if let Some(ref mut cell) = current_cell {
                            apply_cell_border_side(cell, CELL_BORDER_TOP, &e);
                        }
                    }
                    "left" if in_tc_borders => {
                        if let Some(ref mut cell) = current_cell {
                            apply_cell_border_side(cell, CELL_BORDER_LEFT, &e);
                        }
                    }
                    "bottom" if in_tc_borders => {
                        if let Some(ref mut cell) = current_cell {
                            apply_cell_border_side(cell, CELL_BORDER_BOTTOM, &e);
                        }
                    }
                    "right" if in_tc_borders => {
                        if let Some(ref mut cell) = current_cell {
                            apply_cell_border_side(cell, CELL_BORDER_RIGHT, &e);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                match local.as_str() {
                    "gridCol" => {
                        if let Some(w) = parse_grid_col_width(&e) {
                            table.column_widths_twips.push(w);
                        }
                    }
                    "tcW" => {
                        if table.rows.is_empty() {
                            if let Some(w) = parse_tc_width_dxa(&e) {
                                tcw_fallback.push(w);
                            }
                        }
                    }
                    "gridSpan" => {
                        if let Some(ref mut cell) = current_cell {
                            apply_grid_span(&e, cell);
                        }
                    }
                    "vMerge" => {
                        if let Some(ref mut cell) = current_cell {
                            apply_v_merge(&e, cell);
                        }
                    }
                    "shd" => {
                        if let Some(ref mut cell) = current_cell {
                            apply_cell_shading(&e, cell);
                        }
                    }
                    "top" if in_tc_borders => {
                        if let Some(ref mut cell) = current_cell {
                            apply_cell_border_side(cell, CELL_BORDER_TOP, &e);
                        }
                    }
                    "left" if in_tc_borders => {
                        if let Some(ref mut cell) = current_cell {
                            apply_cell_border_side(cell, CELL_BORDER_LEFT, &e);
                        }
                    }
                    "bottom" if in_tc_borders => {
                        if let Some(ref mut cell) = current_cell {
                            apply_cell_border_side(cell, CELL_BORDER_BOTTOM, &e);
                        }
                    }
                    "right" if in_tc_borders => {
                        if let Some(ref mut cell) = current_cell {
                            apply_cell_border_side(cell, CELL_BORDER_RIGHT, &e);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                match local.as_str() {
                    "tcBorders" => in_tc_borders = false,
                    "tc" => {
                        if let (Some(ref mut row), Some(cell)) =
                            (current_row.as_mut(), current_cell.take())
                        {
                            row.cells.push(cell);
                        }
                    }
                    "tr" => {
                        if let Some(row) = current_row.take() {
                            table.rows.push(row);
                        }
                    }
                    "tbl" => {
                        if table.column_widths_twips.is_empty() && !tcw_fallback.is_empty() {
                            table.column_widths_twips = tcw_fallback;
                        }
                        return Ok(table);
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => {
                return Err(ViewerError::DocumentParse(
                    "unexpected EOF inside table".into(),
                ));
            }
            Err(e) => return Err(ViewerError::DocumentParse(format!("table: {e}"))),
            _ => {}
        }
        buf.clear();
    }
}

pub(super) fn parse_sect_pr(reader: &mut Reader<&[u8]>, buf: &mut Vec<u8>) -> Result<PageSetup> {
    let mut setup = PageSetup::default();
    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                match local.as_str() {
                    "pgSz" => {
                        if let Some(w) = attr_val(&e, "w").and_then(|v| v.parse().ok()) {
                            setup.width_twips = w;
                        }
                        if let Some(h) = attr_val(&e, "h").and_then(|v| v.parse().ok()) {
                            setup.height_twips = h;
                        }
                        if attr_val(&e, "orient").as_deref() == Some("landscape")
                            && setup.width_twips < setup.height_twips
                        {
                            std::mem::swap(&mut setup.width_twips, &mut setup.height_twips);
                        }
                    }
                    "pgMar" => {
                        if let Some(v) = attr_val(&e, "top").and_then(|v| v.parse().ok()) {
                            setup.margin_top_twips = v;
                        }
                        if let Some(v) = attr_val(&e, "bottom").and_then(|v| v.parse().ok()) {
                            setup.margin_bottom_twips = v;
                        }
                        if let Some(v) = attr_val(&e, "left").and_then(|v| v.parse().ok()) {
                            setup.margin_left_twips = v;
                        }
                        if let Some(v) = attr_val(&e, "right").and_then(|v| v.parse().ok()) {
                            setup.margin_right_twips = v;
                        }
                        if let Some(v) = attr_val(&e, "header").and_then(|v| v.parse().ok()) {
                            setup.header_distance_twips = v;
                        }
                        if let Some(v) = attr_val(&e, "footer").and_then(|v| v.parse().ok()) {
                            setup.footer_distance_twips = v;
                        }
                    }
                    "headerReference" => {
                        let ty = attr_val(&e, "type").unwrap_or_else(|| "default".into());
                        if let Some(id) = attr_val(&e, "id").filter(|s| !s.is_empty()) {
                            match ty.as_str() {
                                "first" => setup.header_first_r_id = Some(id),
                                "even" => setup.header_even_r_id = Some(id),
                                "default" => setup.header_r_id = Some(id),
                                _ => {}
                            }
                        }
                    }
                    "footerReference" => {
                        let ty = attr_val(&e, "type").unwrap_or_else(|| "default".into());
                        if let Some(id) = attr_val(&e, "id").filter(|s| !s.is_empty()) {
                            match ty.as_str() {
                                "first" => setup.footer_first_r_id = Some(id),
                                "even" => setup.footer_even_r_id = Some(id),
                                "default" => setup.footer_r_id = Some(id),
                                _ => {}
                            }
                        }
                    }
                    "titlePg" => {
                        setup.title_page = true;
                    }
                    "evenAndOddHeaders" => {
                        setup.even_and_odd_headers = true;
                    }
                    "type" => {
                        let val = attr_val(&e, "val").unwrap_or_default();
                        setup.section_break = match val.as_str() {
                            "continuous" => SectionBreakType::Continuous,
                            // nextPage / oddPage / evenPage / omitted → next page band
                            _ => SectionBreakType::NextPage,
                        };
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                if local_name(e.name().as_ref()) == "sectPr" {
                    return Ok(setup);
                }
            }
            Ok(Event::Eof) => return Ok(setup),
            Err(e) => return Err(ViewerError::DocumentParse(format!("sectPr: {e}"))),
            _ => {}
        }
        buf.clear();
    }
}

pub(super) fn apply_paragraph_spacing(e: &BytesStart<'_>, p: &mut Paragraph) {
    if let Some(v) = attr_val(e, "before").and_then(|s| s.parse().ok()) {
        p.space_before_twips = v;
    }
    if let Some(v) = attr_val(e, "after").and_then(|s| s.parse().ok()) {
        p.space_after_twips = v;
    }
    let rule = attr_val(e, "lineRule").unwrap_or_default();
    p.line_spacing_rule = match rule.as_str() {
        "exact" => LineSpacingRule::Exact,
        "atLeast" => LineSpacingRule::AtLeast,
        _ => LineSpacingRule::Auto, // empty / "auto"
    };
    if let Some(v) = attr_val(e, "line").and_then(|s| s.parse().ok()) {
        p.line_spacing = v;
    }
}

pub(super) fn apply_paragraph_indent(e: &BytesStart<'_>, p: &mut Paragraph) {
    if let Some(v) = attr_val(e, "left")
        .or_else(|| attr_val(e, "start"))
        .and_then(|s| s.parse().ok())
    {
        p.indent_left_twips = v;
    }
    if let Some(v) = attr_val(e, "right")
        .or_else(|| attr_val(e, "end"))
        .and_then(|s| s.parse().ok())
    {
        p.indent_right_twips = v;
    }
    if let Some(v) = attr_val(e, "firstLine").and_then(|s| s.parse::<i32>().ok()) {
        p.indent_first_line_twips = v;
    } else if let Some(v) = attr_val(e, "hanging").and_then(|s| s.parse::<i32>().ok()) {
        p.indent_first_line_twips = -v.abs();
    }
}

pub(super) fn apply_paragraph_shading(e: &BytesStart<'_>, p: &mut Paragraph) {
    p.shade_fill = attr_val(e, "fill")
        .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("auto"))
        .and_then(|v| parse_rgb(&v));
}

pub(super) fn apply_cell_shading(e: &BytesStart<'_>, cell: &mut TableCell) {
    cell.shade_fill = attr_val(e, "fill")
        .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("auto"))
        .and_then(|v| parse_rgb(&v));
}

pub(super) fn border_side_visible(e: &BytesStart<'_>) -> bool {
    let val = attr_val(e, "val").unwrap_or_else(|| "single".to_string());
    !matches!(val.to_ascii_lowercase().as_str(), "nil" | "none" | "")
}

pub(super) fn apply_cell_border_side(cell: &mut TableCell, side_bit: u8, e: &BytesStart<'_>) {
    if border_side_visible(e) {
        cell.border_sides |= side_bit;
    }
}

pub(super) fn apply_paragraph_border_side(p: &mut Paragraph, side_bit: u8, e: &BytesStart<'_>) {
    if border_side_visible(e) {
        p.border_sides |= side_bit;
    }
}

pub(super) fn resolve_hyperlink(e: &BytesStart<'_>, rels: &Relationships) -> Option<Hyperlink> {
    if let Some(name) = attr_val(e, "anchor").filter(|n| !n.is_empty()) {
        return Some(Hyperlink {
            url: String::new(),
            r_id: None,
            bookmark: Some(name),
        });
    }
    let rid = attr_val(e, "id")?;
    let url = rels.get(&rid)?.clone();
    if let Some(name) = url.strip_prefix('#').filter(|n| !n.is_empty()) {
        return Some(Hyperlink {
            url: String::new(),
            r_id: Some(rid),
            bookmark: Some(name.to_string()),
        });
    }
    if url.is_empty() {
        return None;
    }
    Some(Hyperlink {
        url,
        r_id: Some(rid),
        bookmark: None,
    })
}

pub(super) fn apply_grid_span(e: &BytesStart<'_>, cell: &mut TableCell) {
    let span = attr_val(e, "val")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(1);
    if span > 1 {
        cell.grid_span = Some(span);
    }
}

pub(super) fn apply_v_merge(e: &BytesStart<'_>, cell: &mut TableCell) {
    let val = attr_val(e, "val").unwrap_or_default();
    cell.v_merge = Some(match val.as_str() {
        "restart" => VMerge::Restart,
        // Bare `<w:vMerge/>` and explicit `continue` both mean continuation.
        _ => VMerge::Continue,
    });
}

pub(super) fn parse_grid_col_width(e: &BytesStart<'_>) -> Option<u32> {
    attr_val(e, "w")
        .and_then(|v| v.parse().ok())
        .filter(|&w| w > 0)
}

pub(super) fn parse_tc_width_dxa(e: &BytesStart<'_>) -> Option<u32> {
    let typ = attr_val(e, "type").unwrap_or_else(|| "dxa".into());
    if typ != "dxa" {
        return None;
    }
    attr_val(e, "w")
        .and_then(|v| v.parse().ok())
        .filter(|&w| w > 0)
}

pub(super) fn capture_element(
    reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
    start: &BytesStart<'_>,
) -> Result<Vec<u8>> {
    let name = local_name(start.name().as_ref());
    let mut out = Vec::new();
    out.extend_from_slice(b"<");
    out.extend_from_slice(start.name().as_ref().as_bytes());
    for a in start.attributes().flatten() {
        out.push(b' ');
        out.extend_from_slice(a.key.as_ref().as_bytes());
        out.extend_from_slice(b"=\"");
        out.extend_from_slice(a.value.as_bytes());
        out.push(b'"');
    }
    out.extend_from_slice(b">");
    let mut depth = 1i32;
    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Start(e)) => {
                out.push(b'<');
                out.extend_from_slice(e.name().as_ref().as_bytes());
                out.push(b'>');
                if local_name(e.name().as_ref()) == name {
                    depth += 1;
                }
            }
            Ok(Event::End(e)) => {
                out.extend_from_slice(b"</");
                out.extend_from_slice(e.name().as_ref().as_bytes());
                out.push(b'>');
                if local_name(e.name().as_ref()) == name {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(out);
                    }
                }
            }
            Ok(Event::Text(t)) => out.extend_from_slice(t.as_ref().as_bytes()),
            Ok(Event::Empty(e)) => {
                out.push(b'<');
                out.extend_from_slice(e.name().as_ref().as_bytes());
                out.extend_from_slice(b"/>");
            }
            Ok(Event::Eof) => {
                return Err(ViewerError::DocumentParse(
                    "unexpected EOF capturing element".into(),
                ));
            }
            Err(e) => return Err(ViewerError::DocumentParse(e.to_string())),
            _ => {}
        }
        buf.clear();
    }
}
