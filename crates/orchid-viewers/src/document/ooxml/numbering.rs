//! Parse / serialise `word/numbering.xml` into list-kind lookups.

use std::collections::HashMap;

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::document::model::{Block, Document, ListKind};
use crate::error::{Result, ViewerError};

/// Orchid default `numId` for bulleted paragraphs.
pub const ORCHID_BULLET_NUM_ID: u32 = 1;
/// Orchid default `numId` for numbered paragraphs.
pub const ORCHID_NUMBERED_NUM_ID: u32 = 2;

/// Mapping from `numId` → list kind (level 0).
#[derive(Debug, Clone, Default)]
pub struct NumberingDefs {
    /// `numId` → kind.
    pub by_num_id: HashMap<u32, ListKind>,
}

impl NumberingDefs {
    /// Resolve a numbering id; unknown ids yield [`ListKind::None`].
    #[must_use]
    pub fn kind_of(&self, num_id: u32) -> ListKind {
        self.by_num_id
            .get(&num_id)
            .copied()
            .unwrap_or(ListKind::None)
    }
}

/// Map a list kind to Orchid's fixed OOXML `numId`.
#[must_use]
pub fn num_id_for_kind(kind: ListKind) -> Option<u32> {
    match kind {
        ListKind::None => None,
        ListKind::Bullet => Some(ORCHID_BULLET_NUM_ID),
        ListKind::Numbered => Some(ORCHID_NUMBERED_NUM_ID),
    }
}

/// Whether any paragraph uses a list marker.
#[must_use]
pub fn document_uses_lists(doc: &Document) -> bool {
    doc.blocks.iter().any(|b| match b {
        Block::Paragraph(p) => p.list != ListKind::None,
        Block::Table(t) => t.rows.iter().any(|r| {
            r.cells
                .iter()
                .any(|c| c.paragraphs.iter().any(|p| p.list != ListKind::None))
        }),
        _ => false,
    })
}

/// Assign Orchid `numId` values from [`ListKind`] before serialising.
pub fn assign_orchid_list_ids(doc: &mut Document) {
    for block in &mut doc.blocks {
        match block {
            Block::Paragraph(p) => {
                p.num_id = num_id_for_kind(p.list);
            }
            Block::Table(t) => {
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        for p in &mut cell.paragraphs {
                            p.num_id = num_id_for_kind(p.list);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Minimal `word/numbering.xml` with bullet=`1` and decimal=`2`.
#[must_use]
pub fn write_numbering_xml() -> Vec<u8> {
    ORCHID_NUMBERING_XML.as_bytes().to_vec()
}

const ORCHID_NUMBERING_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">
    <w:nsid w:val="A1B2C301"/>
    <w:multiLevelType w:val="hybridMultilevel"/>
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="bullet"/>
      <w:lvlText w:val="•"/>
      <w:lvlJc w:val="left"/>
    </w:lvl>
  </w:abstractNum>
  <w:abstractNum w:abstractNumId="1">
    <w:nsid w:val="A1B2C302"/>
    <w:multiLevelType w:val="hybridMultilevel"/>
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="%1."/>
      <w:lvlJc w:val="left"/>
    </w:lvl>
  </w:abstractNum>
  <w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
  <w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>
</w:numbering>"#;

/// Parse numbering.xml.
///
/// # Errors
///
/// [`ViewerError::DocumentParse`] on malformed XML.
pub fn parse_numbering_xml(bytes: &[u8]) -> Result<NumberingDefs> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    let mut abstract_kinds: HashMap<u32, ListKind> = HashMap::new();
    let mut current_abstract: Option<u32> = None;
    let mut current_level: Option<u32> = None;
    let mut defs = NumberingDefs::default();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                match local.as_str() {
                    "abstractNum" => {
                        current_abstract =
                            attr_val(&e, "abstractNumId").and_then(|v| v.parse().ok());
                    }
                    "lvl" => {
                        current_level = attr_val(&e, "ilvl").and_then(|v| v.parse().ok());
                    }
                    "numFmt" => {
                        if current_abstract.is_some() && current_level == Some(0) {
                            if let Some(val) = attr_val(&e, "val") {
                                let kind = if val == "bullet" {
                                    ListKind::Bullet
                                } else {
                                    // decimal, lowerLetter, …
                                    ListKind::Numbered
                                };
                                if let Some(id) = current_abstract {
                                    abstract_kinds.insert(id, kind);
                                }
                            }
                        }
                    }
                    "num" => {
                        let num_id = attr_val(&e, "numId").and_then(|v| v.parse().ok());
                        // abstractNumId may be on a child — handled below via Empty/Start.
                        if let Some(nid) = num_id {
                            // Will be filled when we see abstractNumId.
                            defs.by_num_id.entry(nid).or_insert(ListKind::None);
                        }
                    }
                    "abstractNumId" => {
                        // Parent context: last `num` — we track via a side channel.
                        // Simpler: look for numId on parent by scanning attributes of
                        // recent num — use a pending_num_id.
                    }
                    _ => {}
                }
                // Pair num → abstractNumId when both present on nested empty tags.
                if local == "abstractNumId" {
                    // Handled with pending below.
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(ViewerError::DocumentParse(format!("numbering.xml: {e}")));
            }
            _ => {}
        }
        buf.clear();
    }

    // Second pass: map num → abstractNumId (more reliable).
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut pending_num: Option<u32> = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "num" {
                    pending_num = attr_val(&e, "numId").and_then(|v| v.parse().ok());
                } else if local == "abstractNumId" {
                    if let (Some(nid), Some(aid_s)) = (pending_num, attr_val(&e, "val")) {
                        if let Ok(aid) = aid_s.parse::<u32>() {
                            let kind = abstract_kinds.get(&aid).copied().unwrap_or(ListKind::None);
                            defs.by_num_id.insert(nid, kind);
                        }
                    }
                }
            }
            Ok(Event::End(e)) => {
                if local_name(e.name().as_ref()) == "num" {
                    pending_num = None;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(ViewerError::DocumentParse(format!("numbering.xml: {e}")));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(defs)
}

fn local_name(name: &[u8]) -> String {
    let s = String::from_utf8_lossy(name);
    s.rsplit(':').next().unwrap_or(&s).to_string()
}

fn attr_val(e: &quick_xml::events::BytesStart<'_>, key: &str) -> Option<String> {
    for a in e.attributes().flatten() {
        let local = local_name(a.key.as_ref());
        if local == key {
            return Some(String::from_utf8_lossy(&a.value).into_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bullet_and_decimal() {
        let xml = br#"<?xml version="1.0"?>
        <w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:abstractNum w:abstractNumId="0">
            <w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/></w:lvl>
          </w:abstractNum>
          <w:abstractNum w:abstractNumId="1">
            <w:lvl w:ilvl="0"><w:numFmt w:val="decimal"/></w:lvl>
          </w:abstractNum>
          <w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
          <w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>
        </w:numbering>"#;
        let defs = parse_numbering_xml(xml).unwrap();
        assert_eq!(defs.kind_of(1), ListKind::Bullet);
        assert_eq!(defs.kind_of(2), ListKind::Numbered);
        assert_eq!(defs.kind_of(99), ListKind::None);
    }

    #[test]
    fn orchid_numbering_roundtrips_parse() {
        let defs = parse_numbering_xml(&write_numbering_xml()).unwrap();
        assert_eq!(defs.kind_of(ORCHID_BULLET_NUM_ID), ListKind::Bullet);
        assert_eq!(defs.kind_of(ORCHID_NUMBERED_NUM_ID), ListKind::Numbered);
    }
}
