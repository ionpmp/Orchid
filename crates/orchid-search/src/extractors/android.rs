//! Android string resource extractor.
//!
//! `strings.xml` and the other `<resources>` files contribute resource names
//! and visible text. Tags are not indexed. `.xml` dispatch lives in
//! [`super::Extractor::extract`]: only documents that look like a resource
//! file are handled here. Other XML stays plain text.

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::extractors::text::MAX_CONTENT_BYTES;

pub(crate) fn looks_like_android(xml: &str) -> bool {
    let probe: String = xml.chars().take(1500).collect();
    let probe = probe.to_ascii_lowercase();
    has_open_tag(&probe, "<resources")
        && (has_open_tag(&probe, "<string")
            || has_open_tag(&probe, "<plurals")
            || has_open_tag(&probe, "<string-array"))
}

pub(crate) fn android_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut capture: Option<Capture> = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if let Some(current) = capture.as_mut() {
                    if local == current.tag {
                        current.depth += 1;
                    }
                } else if is_named_group(&local) {
                    if let Some(name) = attr(&e, "name") {
                        push_line(&mut out, &name);
                    }
                } else if is_value_tag(&local) {
                    if local == "string" {
                        if let Some(name) = attr(&e, "name") {
                            push_line(&mut out, &name);
                        }
                    }
                    capture = Some(Capture::new(local));
                }
            }
            Ok(Event::Text(t)) => {
                if let Some(current) = capture.as_mut() {
                    push_chunk(&mut current.buf, t.as_ref());
                }
            }
            Ok(Event::CData(t)) => {
                if let Some(current) = capture.as_mut() {
                    push_chunk(&mut current.buf, t.as_ref());
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if let Some(current) = capture.as_mut() {
                    current.buf.push(decode_ref(r.as_ref()));
                }
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                let done = capture
                    .as_ref()
                    .is_some_and(|current| current.tag == local && current.depth == 0);
                if done {
                    if let Some(finished) = capture.take() {
                        push_line(&mut out, finished.buf.trim());
                    }
                } else if let Some(current) = capture.as_mut() {
                    if current.tag == local && current.depth > 0 {
                        current.depth -= 1;
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
        if out.len() >= MAX_CONTENT_BYTES {
            out.truncate(MAX_CONTENT_BYTES);
            break;
        }
    }
    out.trim().to_string()
}

struct Capture {
    tag: String,
    depth: i32,
    buf: String,
}

impl Capture {
    fn new(tag: String) -> Self {
        Self {
            tag,
            depth: 0,
            buf: String::new(),
        }
    }
}

fn is_named_group(local: &str) -> bool {
    matches!(local, "plurals" | "string-array")
}

fn is_value_tag(local: &str) -> bool {
    matches!(local, "string" | "item")
}

fn has_open_tag(probe: &str, tag: &str) -> bool {
    let mut rest = probe;
    while let Some(index) = rest.find(tag) {
        let after = index + tag.len();
        let boundary = rest[after..].chars().next();
        if matches!(boundary, None | Some('>' | ' ' | '\t' | '\n' | '\r' | '/')) {
            return true;
        }
        rest = &rest[after..];
    }
    false
}

fn push_chunk(buf: &mut String, chunk: &str) {
    if chunk.is_empty() || buf.len() >= MAX_CONTENT_BYTES {
        return;
    }
    let needs_space = !buf.is_empty()
        && !buf.ends_with(|c: char| c.is_whitespace())
        && !chunk.starts_with(|c: char| c.is_whitespace());
    if needs_space {
        buf.push(' ');
    }
    let room = MAX_CONTENT_BYTES.saturating_sub(buf.len());
    buf.push_str(&chunk.chars().take(room).collect::<String>());
}

fn attr(e: &quick_xml::events::BytesStart<'_>, key: &str) -> Option<String> {
    let value = e
        .try_get_attribute(key)
        .ok()
        .flatten()
        .map(|a| a.value.into_owned())?;
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn decode_ref(name: &str) -> char {
    match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        _ => ' ',
    }
}

fn local_name(name: &str) -> String {
    name.rsplit(':').next().unwrap_or(name).to_string()
}

fn push_line(out: &mut String, value: &str) {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.is_empty() || out.len() >= MAX_CONTENT_BYTES {
        return;
    }
    if !out.is_empty() {
        out.push('\n');
    }
    let room = MAX_CONTENT_BYTES.saturating_sub(out.len());
    out.push_str(&value.chars().take(room).collect::<String>());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_names_and_text_including_xliff_placeholders() {
        let text = android_text(
            r#"<resources>
              <string name="title">Night &amp; Drive</string>
              <string name="hello">Hello <xliff:g id="x">Ada</xliff:g></string>
              <plurals name="items">
                <item quantity="one">One item</item>
              </plurals>
            </resources>"#,
        );
        assert!(text.contains("title"), "{text}");
        assert!(text.contains("Night & Drive"), "{text}");
        assert!(text.contains("hello"), "{text}");
        assert!(text.contains("Hello"), "{text}");
        assert!(text.contains("Ada"), "{text}");
        assert!(text.contains("items"), "{text}");
        assert!(text.contains("One item"), "{text}");
        assert!(!text.contains("xliff"), "{text}");
        assert!(!text.contains("quantity"), "{text}");
    }

    #[test]
    fn sniff_rejects_unrelated_xml() {
        assert!(looks_like_android(
            "<resources><string name=\"a\">b</string></resources>"
        ));
        assert!(!looks_like_android(
            "<resources><color name=\"a\">#fff</color></resources>"
        ));
        assert!(!looks_like_android("<root><string>nope</string></root>"));
    }
}
