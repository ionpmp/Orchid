//! `.resx` and XLIFF text extractor.
//!
//! Resource names, values, comments, and translation units are indexed.
//! Embedded binary `.resx` values and XLIFF `bin-unit` data are skipped.

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable strings from .NET `.resx` and XLIFF files.
#[derive(Debug, Default, Clone, Copy)]
pub struct ResourceXmlExtractor;

#[async_trait]
impl ContentExtractor for ResourceXmlExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            base.eq_ignore_ascii_case("application/xliff+xml")
                || base.eq_ignore_ascii_case("application/x-xliff+xml")
        }) || extension
            .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "resx" | "xlf" | "xliff"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        let text = decode_best_effort(&raw);
        let ext = path.extension().unwrap_or("");
        Ok(resource_text(&text, ext))
    }
}

pub(crate) fn resource_text(input: &str, extension: &str) -> String {
    let ext = extension.to_ascii_lowercase();
    if matches!(ext.as_str(), "xlf" | "xliff") || looks_like_xliff(input) {
        xliff_text(input)
    } else {
        resx_text(input)
    }
}

fn looks_like_xliff(input: &str) -> bool {
    let probe: String = input.chars().take(1500).collect();
    probe.to_ascii_lowercase().contains("<xliff")
}

fn resx_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut skip = 0i32;
    let mut in_data = 0i32;
    let mut capture: Option<String> = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if skip > 0 {
                    skip += 1;
                } else if local == "data" && attr(&e, "mimetype").is_some() {
                    skip = 1;
                } else if local == "data" {
                    in_data += 1;
                    if let Some(name) = attr(&e, "name") {
                        push_line(&mut out, &name);
                    }
                } else if in_data > 0 && matches!(local.as_str(), "value" | "comment") {
                    capture = Some(String::new());
                }
            }
            Ok(Event::Text(t)) => {
                if skip == 0 {
                    if let Some(current) = capture.as_mut() {
                        current.push_str(t.as_ref());
                    }
                }
            }
            Ok(Event::CData(t)) => {
                if skip == 0 {
                    if let Some(current) = capture.as_mut() {
                        current.push_str(t.as_ref());
                    }
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if skip == 0 {
                    if let Some(current) = capture.as_mut() {
                        current.push(decode_ref(r.as_ref()));
                    }
                }
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                if skip > 0 {
                    skip -= 1;
                } else if local == "data" && in_data > 0 {
                    in_data -= 1;
                } else if capture.is_some() && matches!(local.as_str(), "value" | "comment") {
                    if let Some(text) = capture.take() {
                        push_line(&mut out, text.trim());
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

fn xliff_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut skip = 0i32;
    let mut capture: Option<String> = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if skip > 0 || is_xliff_binary(&local) {
                    skip += 1;
                } else if matches!(local.as_str(), "source" | "target" | "note") {
                    capture = Some(String::new());
                }
            }
            Ok(Event::Text(t)) => {
                if skip == 0 {
                    if let Some(current) = capture.as_mut() {
                        push_chunk(current, t.as_ref());
                    }
                }
            }
            Ok(Event::CData(t)) => {
                if skip == 0 {
                    if let Some(current) = capture.as_mut() {
                        push_chunk(current, t.as_ref());
                    }
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if skip == 0 {
                    if let Some(current) = capture.as_mut() {
                        current.push(decode_ref(r.as_ref()));
                    }
                }
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                if skip > 0 {
                    skip -= 1;
                } else if capture.is_some()
                    && matches!(local.as_str(), "source" | "target" | "note")
                {
                    if let Some(text) = capture.take() {
                        push_line(&mut out, text.trim());
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

fn is_xliff_binary(local: &str) -> bool {
    matches!(
        local,
        "bin-unit" | "bin-source" | "bin-target" | "internal-file" | "skeleton"
    )
}

fn push_chunk(buf: &mut String, chunk: &str) {
    if chunk.is_empty() {
        return;
    }
    let needs_space = !buf.is_empty()
        && !buf.ends_with(|c: char| c.is_whitespace())
        && !chunk.starts_with(|c: char| c.is_whitespace());
    if needs_space {
        buf.push(' ');
    }
    buf.push_str(chunk);
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
    fn resx_keeps_strings_and_skips_binary() {
        let text = resource_text(
            r#"<root>
              <resheader name="reader"><value>System.Resources.ResXResourceReader</value></resheader>
              <data name="Title" xml:space="preserve">
                <value>Night &amp; Drive</value>
                <comment>Window title</comment>
              </data>
              <data name="Icon" mimetype="application/x-microsoft.net.object.binary.base64">
                <value>Qk0SECRET</value>
              </data>
            </root>"#,
            "resx",
        );
        assert!(text.contains("Title"), "{text}");
        assert!(text.contains("Night & Drive"), "{text}");
        assert!(text.contains("Window title"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("ResXResourceReader"), "{text}");
    }

    #[test]
    fn xliff_keeps_source_target_and_note() {
        let text = resource_text(
            r#"<xliff version="1.2"><file><body>
              <trans-unit id="greeting">
                <source>Hello</source>
                <target>Привет</target>
                <note>welcome</note>
              </trans-unit>
              <bin-unit><bin-source>SECRET</bin-source></bin-unit>
            </body></file></xliff>"#,
            "xliff",
        );
        assert!(text.contains("Hello"), "{text}");
        assert!(text.contains("Привет"), "{text}");
        assert!(text.contains("welcome"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("greeting"), "{text}");
    }
}
