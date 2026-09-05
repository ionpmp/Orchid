//! Parse / serialise `word/comments.xml`.

use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;
use std::io::Cursor;

use crate::document::model::DocComment;
use crate::error::{Result, ViewerError};

/// Parse `word/comments.xml` into comment payloads.
///
/// # Errors
///
/// [`ViewerError::DocumentParse`] on malformed XML.
pub fn parse_comments_xml(bytes: &[u8]) -> Result<Vec<DocComment>> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut comments = Vec::new();
    let mut in_comment = false;
    let mut cur = DocComment::default();
    let mut in_t = false;
    let mut text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                match local.as_str() {
                    "comment" => {
                        in_comment = true;
                        cur = DocComment {
                            id: attr_val(&e, "id").and_then(|v| v.parse().ok()).unwrap_or(0),
                            author: attr_val(&e, "author").unwrap_or_default(),
                            initials: attr_val(&e, "initials").unwrap_or_default(),
                            date: attr_val(&e, "date").unwrap_or_default(),
                            text: String::new(),
                        };
                        text.clear();
                    }
                    "t" if in_comment => in_t = true,
                    _ => {}
                }
            }
            Ok(Event::Text(t)) if in_t => {
                text.push_str(t.as_ref());
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                match local.as_str() {
                    "t" => in_t = false,
                    "comment" => {
                        cur.text = text.trim().to_string();
                        comments.push(std::mem::take(&mut cur));
                        in_comment = false;
                        text.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(ViewerError::DocumentParse(format!("comments.xml: {e}")));
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(comments)
}

/// Serialise comments to `word/comments.xml`.
///
/// # Errors
///
/// [`ViewerError::DocumentSave`] on write failure.
pub fn write_comments_xml(comments: &[DocComment]) -> Result<Vec<u8>> {
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut root = BytesStart::new("w:comments");
    root.push_attribute((
        "xmlns:w",
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
    ));
    writer
        .write_event(Event::Start(root))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    for c in comments {
        let id = c.id.to_string();
        let mut start = BytesStart::new("w:comment");
        start.push_attribute(("w:id", id.as_str()));
        if !c.author.is_empty() {
            start.push_attribute(("w:author", c.author.as_str()));
        }
        if !c.initials.is_empty() {
            start.push_attribute(("w:initials", c.initials.as_str()));
        }
        if !c.date.is_empty() {
            start.push_attribute(("w:date", c.date.as_str()));
        }
        writer
            .write_event(Event::Start(start))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        writer
            .write_event(Event::Start(BytesStart::new("w:p")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        writer
            .write_event(Event::Start(BytesStart::new("w:r")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        writer
            .write_event(Event::Start(BytesStart::new("w:t")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        writer
            .write_event(Event::Text(BytesText::new(&c.text)))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        writer
            .write_event(Event::End(BytesEnd::new("w:t")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        writer
            .write_event(Event::End(BytesEnd::new("w:r")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        writer
            .write_event(Event::End(BytesEnd::new("w:p")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        writer
            .write_event(Event::End(BytesEnd::new("w:comment")))
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }
    writer
        .write_event(Event::End(BytesEnd::new("w:comments")))
        .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    Ok(writer.into_inner().into_inner())
}

fn local_name(name: &str) -> String {
    name.rsplit(':').next().unwrap_or(name).to_string()
}

fn attr_val(e: &quick_xml::events::BytesStart<'_>, key: &str) -> Option<String> {
    for a in e.attributes().flatten() {
        if local_name(a.key.as_ref()) == key {
            return Some(a.value.into_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_write_comment() {
        let xml = br#"<?xml version="1.0"?>
        <w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:comment w:id="0" w:author="Ada" w:initials="AL" w:date="2026-01-02T03:04:05Z">
            <w:p><w:r><w:t>Hello note</w:t></w:r></w:p>
          </w:comment>
        </w:comments>"#;
        let comments = parse_comments_xml(xml).unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].id, 0);
        assert_eq!(comments[0].author, "Ada");
        assert_eq!(comments[0].initials, "AL");
        assert_eq!(comments[0].text, "Hello note");
        let out = write_comments_xml(&comments).unwrap();
        let again = parse_comments_xml(&out).unwrap();
        assert_eq!(again, comments);
    }
}
