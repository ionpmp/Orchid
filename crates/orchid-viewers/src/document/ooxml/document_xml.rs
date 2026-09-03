//! Parse / serialise `word/document.xml`.

use std::collections::HashMap;
use std::io::Cursor;

use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;

use crate::document::model::{
    Alignment, Block, Bookmark, CellImage, Document, Hyperlink, ImageFormat, InlineImage,
    LineSpacingRule, ListKind, OpaqueXmlNode, PageSetup, Paragraph, Run, RunStyle, Table, TableCell,
    TableRow, VMerge, CELL_BORDER_BOTTOM, CELL_BORDER_LEFT, CELL_BORDER_RIGHT,
    CELL_BORDER_TOP,
};
use crate::document::ooxml::numbering::NumberingDefs;
use crate::document::ooxml::styles::StyleDefaults;
use crate::error::{Result, ViewerError};

/// Relationship map: `rId` → target path relative to `word/`.
pub type Relationships = HashMap<String, String>;

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
) -> Result<(Vec<Block>, PageSetup, Vec<OpaqueXmlNode>, Vec<Bookmark>)> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut blocks = Vec::new();
    let mut unsupported = Vec::new();
    let mut bookmarks: Vec<Bookmark> = Vec::new();
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
                            let (p, images, local_bms) = parse_paragraph(
                                &mut reader,
                                &mut buf,
                                styles,
                                numbering,
                                rels,
                                media,
                            )?;
                            let has_text = p.runs.iter().any(|r| !r.text.is_empty());
                            let para_start = if plain_len > 0 {
                                plain_len + 1
                            } else {
                                0
                            };
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

    Ok((blocks, page_setup, unsupported, bookmarks))
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
                    let (p, _images, _bms) = parse_paragraph(
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
        write_paragraph(&mut writer, &Paragraph::default(), &[], 0, &mut bookmark_id)?;
    } else {
        for p in paragraphs {
            write_paragraph(&mut writer, p, &[], 0, &mut bookmark_id)?;
        }
    }

    writer
        .write_event(Event::End(BytesEnd::new(root_tag.as_str())))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(writer.into_inner().into_inner())
}

/// Resolve a document relationship target to a package part path under `word/`.
#[must_use]
pub fn word_part_path(target: &str) -> String {
    let t = target.replace('\\', "/");
    if t.starts_with("word/") {
        t
    } else if let Some(rest) = t.strip_prefix("./") {
        format!("word/{rest}")
    } else if t.starts_with('/') {
        t.trim_start_matches('/').to_string()
    } else {
        format!("word/{t}")
    }
}

fn parse_paragraph(
    reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
    styles: &StyleDefaults,
    numbering: &NumberingDefs,
    rels: &Relationships,
    media: &HashMap<String, Vec<u8>>,
) -> Result<(Paragraph, Vec<InlineImage>, Vec<(String, usize)>)> {
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
    };
    let mut images = Vec::new();
    let mut local_bookmarks: Vec<(String, usize)> = Vec::new();
    let mut para_plain_len = 0usize;
    let mut in_p_pr = false;
    let mut in_p_bdr = false;
    let mut in_r = false;
    let mut in_t = false;
    let mut current_run: Option<Run> = None;
    let mut active_link: Option<Hyperlink> = None;

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
                    "hyperlink" => {
                        active_link = resolve_hyperlink(&e, rels);
                    }
                    "bookmarkStart" => {
                        if let Some(name) = attr_val(&e, "name").filter(|n| !n.is_empty()) {
                            local_bookmarks.push((name, para_plain_len));
                        }
                    }
                    "r" => {
                        in_r = true;
                        current_run = Some(Run {
                            text: String::new(),
                            style: styles.run.clone(),
                            hyperlink: active_link.clone(),
                        });
                    }
                    "rPr" if in_r => {
                        if let Some(ref mut run) = current_run {
                            parse_r_pr_into(reader, buf, &mut run.style)?;
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
                if local == "bookmarkStart" {
                    if let Some(name) = attr_val(&e, "name").filter(|n| !n.is_empty()) {
                        local_bookmarks.push((name, para_plain_len));
                    }
                }
                if in_r && matches!(local.as_str(), "b" | "i" | "u" | "caps" | "smallCaps" | "color" | "rFonts" | "sz") {
                    if let Some(ref mut run) = current_run {
                        apply_r_pr_attr(&local, &e, &mut run.style);
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if in_t {
                    if let Some(ref mut run) = current_run {
                        let text = t.as_ref();
                        run.text.push_str(text);
                    }
                }
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                match local.as_str() {
                    "pPr" => in_p_pr = false,
                    "pBdr" => in_p_bdr = false,
                    "t" => in_t = false,
                    "r" => {
                        in_r = false;
                        in_t = false;
                        if let Some(run) = current_run.take() {
                            para_plain_len += run.text.len();
                            p.runs.push(run);
                        }
                    }
                    "hyperlink" => active_link = None,
                    "p" => return Ok((p, images, local_bookmarks)),
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

/// EMUs → CSS pixels at 96 DPI (`emu * 96 / 914400`).
fn emu_to_css_px(emu: u64) -> u32 {
    ((emu.saturating_mul(96)) / 914_400).max(1) as u32
}

fn media_part_path(target: &str) -> String {
    let t = target.replace('\\', "/");
    if t.starts_with("word/") {
        t
    } else {
        format!("word/{t}")
    }
}

/// Walk a `w:drawing` subtree and resolve the embedded blip to package media.
fn parse_drawing_image(
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

fn parse_r_pr_into(
    reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
    style: &mut RunStyle,
) -> Result<()> {
    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                apply_r_pr_attr(&local, &e, style);
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

fn apply_r_pr_attr(local: &str, e: &BytesStart<'_>, style: &mut RunStyle) {
    match local {
        "b" => style.bold = !attr_val(e, "val").is_some_and(|v| v == "0" || v == "false"),
        "i" => style.italic = !attr_val(e, "val").is_some_and(|v| v == "0" || v == "false"),
        "u" => {
            let val = attr_val(e, "val").unwrap_or_else(|| "single".into());
            style.underline = val != "none";
        }
        "strike" | "dstrike" => {
            style.strikethrough = !attr_val(e, "val").is_some_and(|v| v == "0" || v == "false");
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

fn parse_table(
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
                        let (p, images, _bms) =
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

fn parse_sect_pr(reader: &mut Reader<&[u8]>, buf: &mut Vec<u8>) -> Result<PageSetup> {
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
                    }
                    "headerReference" => {
                        let ty = attr_val(&e, "type").unwrap_or_else(|| "default".into());
                        if let Some(id) = attr_val(&e, "id").filter(|s| !s.is_empty()) {
                            match ty.as_str() {
                                "first" => setup.header_first_r_id = Some(id),
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
                                "default" => setup.footer_r_id = Some(id),
                                _ => {}
                            }
                        }
                    }
                    "titlePg" => {
                        setup.title_page = true;
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

/// Build an [`InlineImage`] from package media (used by container after scan).
#[must_use]
pub fn image_from_part(part_path: &str, bytes: Vec<u8>, r_id: Option<String>) -> InlineImage {
    let ext = part_path.rsplit('.').next().unwrap_or("");
    let format = ImageFormat::from_extension(ext);
    let (width_px, height_px) = image::load_from_memory(&bytes)
        .map(|img| (img.width(), img.height()))
        .unwrap_or((0, 0));
    InlineImage {
        bytes,
        format,
        width_px,
        height_px,
        r_id,
        part_path: Some(part_path.to_string()),
    }
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

fn write_collapsed_bookmark(
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

fn write_paragraph(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    p: &Paragraph,
    bookmarks: &[Bookmark],
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
        } else {
            write_run(writer, &p.runs[i])?;
            run_rel += p.runs[i].text.len();
            i += 1;
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

fn apply_paragraph_spacing(e: &BytesStart<'_>, p: &mut Paragraph) {
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

fn apply_paragraph_indent(e: &BytesStart<'_>, p: &mut Paragraph) {
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

fn apply_paragraph_shading(e: &BytesStart<'_>, p: &mut Paragraph) {
    p.shade_fill = attr_val(e, "fill")
        .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("auto"))
        .and_then(|v| parse_rgb(&v));
}

fn apply_cell_shading(e: &BytesStart<'_>, cell: &mut TableCell) {
    cell.shade_fill = attr_val(e, "fill")
        .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("auto"))
        .and_then(|v| parse_rgb(&v));
}

fn border_side_visible(e: &BytesStart<'_>) -> bool {
    let val = attr_val(e, "val").unwrap_or_else(|| "single".to_string());
    !matches!(
        val.to_ascii_lowercase().as_str(),
        "nil" | "none" | ""
    )
}

fn apply_cell_border_side(cell: &mut TableCell, side_bit: u8, e: &BytesStart<'_>) {
    if border_side_visible(e) {
        cell.border_sides |= side_bit;
    }
}

fn apply_paragraph_border_side(p: &mut Paragraph, side_bit: u8, e: &BytesStart<'_>) {
    if border_side_visible(e) {
        p.border_sides |= side_bit;
    }
}

fn write_paragraph_border_side(writer: &mut Writer<Cursor<Vec<u8>>>, tag: &str) -> Result<()> {
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

fn write_paragraph_borders(writer: &mut Writer<Cursor<Vec<u8>>>, sides: u8) -> Result<()> {
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

fn resolve_hyperlink(e: &BytesStart<'_>, rels: &Relationships) -> Option<Hyperlink> {
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

fn write_run(writer: &mut Writer<Cursor<Vec<u8>>>, run: &Run) -> Result<()> {
    writer
        .write_event(Event::Start(BytesStart::new("w:r")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    writer
        .write_event(Event::Start(BytesStart::new("w:rPr")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
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

fn write_text_element(writer: &mut Writer<Cursor<Vec<u8>>>, text: &str) -> Result<()> {
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

fn apply_grid_span(e: &BytesStart<'_>, cell: &mut TableCell) {
    let span = attr_val(e, "val")
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(1);
    if span > 1 {
        cell.grid_span = Some(span);
    }
}

fn apply_v_merge(e: &BytesStart<'_>, cell: &mut TableCell) {
    let val = attr_val(e, "val").unwrap_or_default();
    cell.v_merge = Some(match val.as_str() {
        "restart" => VMerge::Restart,
        // Bare `<w:vMerge/>` and explicit `continue` both mean continuation.
        _ => VMerge::Continue,
    });
}

fn parse_grid_col_width(e: &BytesStart<'_>) -> Option<u32> {
    attr_val(e, "w")
        .and_then(|v| v.parse().ok())
        .filter(|&w| w > 0)
}

fn parse_tc_width_dxa(e: &BytesStart<'_>) -> Option<u32> {
    let typ = attr_val(e, "type").unwrap_or_else(|| "dxa".into());
    if typ != "dxa" {
        return None;
    }
    attr_val(e, "w")
        .and_then(|v| v.parse().ok())
        .filter(|&w| w > 0)
}

fn write_tc_border_side(writer: &mut Writer<Cursor<Vec<u8>>>, tag: &str) -> Result<()> {
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

fn write_tc_borders(writer: &mut Writer<Cursor<Vec<u8>>>, sides: u8) -> Result<()> {
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

fn write_table(
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
                write_paragraph(writer, &Paragraph::default(), &[], 0, &mut bm_id)?;
                for ci in cell.images.iter().filter(|c| c.after_paragraph == 0) {
                    write_image_paragraph(writer, &ci.image, drawing_id)?;
                }
            } else {
                for (i, p) in cell.paragraphs.iter().enumerate() {
                    let mut bm_id = 0u32;
                    write_paragraph(writer, p, &[], 0, &mut bm_id)?;
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

fn css_px_to_emu(px: u32) -> u64 {
    u64::from(px.max(1)).saturating_mul(914_400) / 96
}

fn write_image_paragraph(
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

fn write_sect_pr(writer: &mut Writer<Cursor<Vec<u8>>>, setup: &PageSetup) -> Result<()> {
    writer
        .write_event(Event::Start(BytesStart::new("w:sectPr")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
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
    if setup.title_page {
        writer
            .write_event(Event::Empty(BytesStart::new("w:titlePg")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    writer
        .write_event(Event::End(BytesEnd::new("w:sectPr")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(())
}

fn parse_alignment(val: &str) -> Alignment {
    match val {
        "center" => Alignment::Center,
        "right" | "end" => Alignment::Right,
        "both" | "distribute" => Alignment::Justify,
        _ => Alignment::Left,
    }
}

fn alignment_val(a: Alignment) -> &'static str {
    match a {
        Alignment::Left => "left",
        Alignment::Center => "center",
        Alignment::Right => "right",
        Alignment::Justify => "both",
    }
}

fn parse_rgb(val: &str) -> Option<[u8; 3]> {
    let v = val.trim();
    if v.len() == 6 {
        let r = u8::from_str_radix(&v[0..2], 16).ok()?;
        let g = u8::from_str_radix(&v[2..4], 16).ok()?;
        let b = u8::from_str_radix(&v[4..6], 16).ok()?;
        Some([r, g, b])
    } else {
        None
    }
}

fn local_name(name: &str) -> String {
    name.rsplit(':').next().unwrap_or(name).to_string()
}

fn attr_val(e: &BytesStart<'_>, key: &str) -> Option<String> {
    for a in e.attributes().flatten() {
        if local_name(a.key.as_ref()) == key {
            return Some(a.value.into_owned());
        }
    }
    None
}

fn capture_element(
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let (blocks, setup, _, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks2, _, _, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks2, _, _, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks2, _, _, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks2, _, _, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks2, _, _, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks2, _, _, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks2, _, _, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, bookmarks) = parse_document_xml(
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
        let (blocks2, _, _, bookmarks2) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks2, _, _, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks, _, _, _) = parse_document_xml(
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
        let (blocks, _, _, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks2, _, _, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks2, _, _, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks2, _, _, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks2, _, _, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks2, _, _, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks2, _, _, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks2, _, _, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (_, page_setup, _, _) = parse_document_xml(
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
        let (_, page_setup, _, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks2, _, _, _) = parse_document_xml(
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
        let (blocks, page_setup, unsupported, _) = parse_document_xml(
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
        let (blocks2, _, _, _) = parse_document_xml(
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
        let (blocks, _, _, _) = parse_document_xml(
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
        let (blocks, _, _, _) = parse_document_xml(
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
        let (blocks, _, _, _) = parse_document_xml(
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
        let (blocks, _, _, _) = parse_document_xml(
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
        let (blocks, _, _, _) = parse_document_xml(
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
        let (blocks, _, _, _) = parse_document_xml(
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
}
