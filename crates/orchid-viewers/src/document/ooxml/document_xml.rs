//! Parse / serialise `word/document.xml`.

use std::collections::HashMap;
use std::io::Cursor;

use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;

use crate::document::model::{
    Alignment, Block, Document, ImageFormat, InlineImage, ListKind, OpaqueXmlNode, PageSetup,
    Paragraph, Run, RunStyle, Table, TableCell, TableRow,
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
) -> Result<(Vec<Block>, PageSetup, Vec<OpaqueXmlNode>)> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut blocks = Vec::new();
    let mut unsupported = Vec::new();
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
                            let (p, images) = parse_paragraph(
                                &mut reader,
                                &mut buf,
                                styles,
                                numbering,
                                rels,
                                media,
                            )?;
                            let has_text = p.runs.iter().any(|r| !r.text.is_empty());
                            if has_text || images.is_empty() {
                                blocks.push(Block::Paragraph(p));
                            }
                            for img in images {
                                blocks.push(Block::Image(img));
                            }
                        }
                        "tbl" => {
                            let t = parse_table(
                                &mut reader,
                                &mut buf,
                                styles,
                                numbering,
                                rels,
                                media,
                            )?;
                            blocks.push(Block::Table(t));
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
                if in_body && local == "sectPr" {
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

    Ok((blocks, page_setup, unsupported))
}

fn parse_paragraph(
    reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
    styles: &StyleDefaults,
    numbering: &NumberingDefs,
    rels: &Relationships,
    media: &HashMap<String, Vec<u8>>,
) -> Result<(Paragraph, Vec<InlineImage>)> {
    let mut p = Paragraph {
        runs: Vec::new(),
        alignment: Alignment::Left,
        list: ListKind::None,
        list_level: 0,
        num_id: None,
        unsupported: Vec::new(),
    };
    let mut images = Vec::new();
    let mut in_p_pr = false;
    let mut in_r = false;
    let mut in_t = false;
    let mut current_run: Option<Run> = None;

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
                    "r" => {
                        in_r = true;
                        current_run = Some(Run {
                            text: String::new(),
                            style: styles.run.clone(),
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
                        if let Some(ref mut run) = current_run {
                            run.text.push('\n');
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref());
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
                        _ => {}
                    }
                }
                if in_r && local == "br" {
                    if let Some(ref mut run) = current_run {
                        run.text.push('\n');
                    }
                }
                if in_r && matches!(local.as_str(), "b" | "i" | "u" | "color" | "rFonts" | "sz") {
                    if let Some(ref mut run) = current_run {
                        apply_r_pr_attr(&local, &e, &mut run.style);
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if in_t {
                    if let Some(ref mut run) = current_run {
                        let text = t.decode().unwrap_or_default();
                        run.text.push_str(&text);
                    }
                }
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                match local.as_str() {
                    "pPr" => in_p_pr = false,
                    "t" => in_t = false,
                    "r" => {
                        in_r = false;
                        in_t = false;
                        if let Some(run) = current_run.take() {
                            p.runs.push(run);
                        }
                    }
                    "p" => return Ok((p, images)),
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

    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                match local.as_str() {
                    "tr" => current_row = Some(TableRow::default()),
                    "tc" => current_cell = Some(TableCell::default()),
                    "p" => {
                        // Cell model is paragraph-only; drawings in cells are skipped for Tier-1.
                        let (p, _images) =
                            parse_paragraph(reader, buf, styles, numbering, rels, media)?;
                        if let Some(ref mut cell) = current_cell {
                            cell.paragraphs.push(p);
                        }
                    }
                    "vMerge" | "gridSpan" => {
                        tracing::warn!(
                            element = local.as_str(),
                            "table cell merge not supported in Tier 1; preserving as opaque"
                        );
                        table.unsupported.push(OpaqueXmlNode {
                            position_hint: format!("w:tbl/w:{local}"),
                            raw_xml: format!("<w:{local}/>").into_bytes(),
                        });
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                if matches!(local.as_str(), "vMerge" | "gridSpan") {
                    tracing::warn!(
                        element = local.as_str(),
                        "table cell merge not supported in Tier 1"
                    );
                    table.unsupported.push(OpaqueXmlNode {
                        position_hint: format!("w:tbl/w:{local}"),
                        raw_xml: format!("<w:{local}/>").into_bytes(),
                    });
                }
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                match local.as_str() {
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
                    "tbl" => return Ok(table),
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
    writer
        .write_event(Event::Start(doc_start))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    writer
        .write_event(Event::Start(BytesStart::new("w:body")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;

    for block in &doc.blocks {
        match block {
            Block::Paragraph(p) => write_paragraph(&mut writer, p)?,
            Block::Table(t) => write_table(&mut writer, t)?,
            Block::Image(_) => {
                // Full `w:drawing` rewrite is out of scope. Skip the block so
                // plain-text round-trips stay stable; media remains in retained parts.
            }
        }
    }

    for node in &doc.unsupported {
        // Best-effort: write raw XML bytes as-is.
        let _ = writer.get_mut().get_mut().extend_from_slice(&node.raw_xml);
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

fn write_paragraph(writer: &mut Writer<Cursor<Vec<u8>>>, p: &Paragraph) -> Result<()> {
    writer
        .write_event(Event::Start(BytesStart::new("w:p")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    writer
        .write_event(Event::Start(BytesStart::new("w:pPr")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    let mut jc = BytesStart::new("w:jc");
    jc.push_attribute(("w:val", alignment_val(p.alignment)));
    writer
        .write_event(Event::Empty(jc))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
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

    for run in &p.runs {
        write_run(writer, run)?;
    }
    for node in &p.unsupported {
        writer.get_mut().get_mut().extend_from_slice(&node.raw_xml);
    }
    writer
        .write_event(Event::End(BytesEnd::new("w:p")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(())
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

    writer
        .write_event(Event::Start(BytesStart::new("w:t")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    writer
        .write_event(Event::Text(BytesText::new(&run.text)))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    writer
        .write_event(Event::End(BytesEnd::new("w:t")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    writer
        .write_event(Event::End(BytesEnd::new("w:r")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(())
}

fn write_table(writer: &mut Writer<Cursor<Vec<u8>>>, t: &Table) -> Result<()> {
    writer
        .write_event(Event::Start(BytesStart::new("w:tbl")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    for row in &t.rows {
        writer
            .write_event(Event::Start(BytesStart::new("w:tr")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        for cell in &row.cells {
            writer
                .write_event(Event::Start(BytesStart::new("w:tc")))
                .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
            if cell.paragraphs.is_empty() {
                write_paragraph(writer, &Paragraph::default())?;
            } else {
                for p in &cell.paragraphs {
                    write_paragraph(writer, p)?;
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

fn write_sect_pr(writer: &mut Writer<Cursor<Vec<u8>>>, setup: &PageSetup) -> Result<()> {
    writer
        .write_event(Event::Start(BytesStart::new("w:sectPr")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    let mut sz = BytesStart::new("w:pgSz");
    sz.push_attribute(("w:w", setup.width_twips.to_string().as_str()));
    sz.push_attribute(("w:h", setup.height_twips.to_string().as_str()));
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

fn local_name(name: &[u8]) -> String {
    let s = String::from_utf8_lossy(name);
    s.rsplit(':').next().unwrap_or(&s).to_string()
}

fn attr_val(e: &BytesStart<'_>, key: &str) -> Option<String> {
    for a in e.attributes().flatten() {
        if local_name(a.key.as_ref()) == key {
            return Some(String::from_utf8_lossy(&a.value).into_owned());
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
    out.extend_from_slice(start.name().as_ref());
    for a in start.attributes().flatten() {
        out.push(b' ');
        out.extend_from_slice(a.key.as_ref());
        out.extend_from_slice(b"=\"");
        out.extend_from_slice(&a.value);
        out.push(b'"');
    }
    out.extend_from_slice(b">");
    let mut depth = 1i32;
    loop {
        match reader.read_event_into(buf) {
            Ok(Event::Start(e)) => {
                out.push(b'<');
                out.extend_from_slice(e.name().as_ref());
                out.push(b'>');
                if local_name(e.name().as_ref()) == name {
                    depth += 1;
                }
            }
            Ok(Event::End(e)) => {
                out.extend_from_slice(b"</");
                out.extend_from_slice(e.name().as_ref());
                out.push(b'>');
                if local_name(e.name().as_ref()) == name {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(out);
                    }
                }
            }
            Ok(Event::Text(t)) => out.extend_from_slice(&t),
            Ok(Event::Empty(e)) => {
                out.push(b'<');
                out.extend_from_slice(e.name().as_ref());
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
        let (blocks, setup, _) = parse_document_xml(
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
    fn write_round_trip_model() {
        let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p>
              <w:r><w:rPr><w:i/></w:rPr><w:t>Hi</w:t></w:r>
            </w:p>
          </w:body>
        </w:document>"#;
        let (blocks, page_setup, unsupported) = parse_document_xml(
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
        let (blocks2, _, _) = parse_document_xml(
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
        let (blocks, _, _) = parse_document_xml(
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
            }
            _ => panic!("expected table"),
        }
    }

    #[test]
    fn parse_inline_drawing_to_image_block() {
        let mut png = Vec::new();
        {
            let img = image::RgbaImage::from_pixel(4, 2, image::Rgba([10, 20, 30, 255]));
            img.write_to(
                &mut std::io::Cursor::new(&mut png),
                image::ImageFormat::Png,
            )
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
        let (blocks, _, _) = parse_document_xml(
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
                assert_eq!(
                    img.part_path.as_deref(),
                    Some("word/media/dot.png")
                );
            }
            _ => panic!("expected image block"),
        }
    }
}
