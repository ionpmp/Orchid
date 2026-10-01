//! Qt Linguist (`.ts`) extractor.
//!
//! A `.ts` file is TypeScript unless it opens as a Qt translation catalog.
//! Catalogs contribute context names, source strings, and translations.
//! TypeScript is indexed unchanged.

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract Qt translations, or pass TypeScript through as source text.
#[derive(Debug, Default, Clone, Copy)]
pub struct QtExtractor;

#[async_trait]
impl ContentExtractor for QtExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(|e| e.eq_ignore_ascii_case("ts"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(qt_or_source(&decode_best_effort(&raw)))
    }
}

pub(crate) fn qt_or_source(input: &str) -> String {
    if looks_like_qt(input) {
        qt_text(input)
    } else {
        input.to_string()
    }
}

pub(crate) fn looks_like_qt(input: &str) -> bool {
    let probe: String = input.chars().take(1200).collect();
    let probe = probe.to_ascii_lowercase();
    has_open_tag(&probe, "<ts")
        && (probe.contains("<!doctype ts")
            || has_open_tag(&probe, "<context")
            || has_open_tag(&probe, "<message"))
}

fn qt_text(xml: &str) -> String {
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
                } else if is_text_tag(&local) {
                    capture = Some(Capture::new(local));
                }
            }
            Ok(Event::Text(t)) => {
                if let Some(current) = capture.as_mut() {
                    current.buf.push_str(t.as_ref());
                }
            }
            Ok(Event::CData(t)) => {
                if let Some(current) = capture.as_mut() {
                    current.buf.push_str(t.as_ref());
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

fn is_text_tag(local: &str) -> bool {
    matches!(
        local,
        "name" | "source" | "translation" | "comment" | "extracomment" | "numerusform"
    )
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
    fn qt_catalog_keeps_messages_and_skips_locations() {
        let text = qt_or_source(
            r#"<?xml version="1.0"?>
            <!DOCTYPE TS>
            <TS version="2.1"><context>
              <name>MainWindow</name>
              <message>
                <location filename="main.cpp" line="10"/>
                <source>Hello</source>
                <translation>Привет</translation>
                <comment>greeting</comment>
              </message>
            </context></TS>"#,
        );
        assert!(text.contains("MainWindow"), "{text}");
        assert!(text.contains("Hello"), "{text}");
        assert!(text.contains("Привет"), "{text}");
        assert!(text.contains("greeting"), "{text}");
        assert!(!text.contains("main.cpp"), "{text}");
        assert!(!text.contains("line"), "{text}");
    }

    #[test]
    fn typescript_passes_through() {
        let src = "export const answer = 1;\n";
        assert!(!looks_like_qt(src));
        assert_eq!(qt_or_source(src), src);
    }
}
