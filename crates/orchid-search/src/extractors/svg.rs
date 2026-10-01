//! SVG text extractor.
//!
//! Visible labels from `title`, `desc`, and `text` are indexed. Scripts,
//! styles, and metadata (Inkscape RDF and similar) are skipped.

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable labels from SVG drawings.
#[derive(Debug, Default, Clone, Copy)]
pub struct SvgExtractor;

#[async_trait]
impl ContentExtractor for SvgExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            base.eq_ignore_ascii_case("image/svg+xml")
        }) || extension.is_some_and(|e| e.eq_ignore_ascii_case("svg"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(svg_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn svg_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut skip = 0i32;
    let mut chunk = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if skip > 0 || is_hidden(&local) {
                    skip += 1;
                }
            }
            Ok(Event::Text(t)) => {
                if skip == 0 {
                    push_chunk(&mut chunk, t.as_ref());
                }
            }
            Ok(Event::CData(t)) => {
                if skip == 0 {
                    push_chunk(&mut chunk, t.as_ref());
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if skip == 0 {
                    chunk.push(decode_ref(r.as_ref()));
                }
            }
            Ok(Event::End(_)) => {
                if skip > 0 {
                    skip -= 1;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
        if chunk.len() >= MAX_CONTENT_BYTES {
            break;
        }
    }
    push_line(&mut out, chunk.trim());
    out.trim().to_string()
}

fn is_hidden(local: &str) -> bool {
    matches!(local, "script" | "style" | "metadata")
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

fn decode_ref(name: &str) -> char {
    if let Some(num) = name.strip_prefix('#') {
        let code = if let Some(hex) = num.strip_prefix(['x', 'X']) {
            u32::from_str_radix(hex, 16).ok()
        } else {
            num.parse().ok()
        };
        if let Some(ch) = code.and_then(char::from_u32) {
            return ch;
        }
    }
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
    fn keeps_labels_and_skips_script_and_metadata() {
        let text = svg_text(
            r#"<svg xmlns="http://www.w3.org/2000/svg">
              <title>Night Icon</title>
              <desc>A &amp; B</desc>
              <text>Hello <tspan>world</tspan></text>
              <script>SECRET</script>
              <metadata><dc:title>INKSCAPE</dc:title></metadata>
            </svg>"#,
        );
        assert!(text.contains("Night Icon"), "{text}");
        assert!(text.contains("A & B"), "{text}");
        assert!(text.contains("Hello"), "{text}");
        assert!(text.contains("world"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("INKSCAPE"), "{text}");
    }
}
