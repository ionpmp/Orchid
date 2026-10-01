//! XAML text extractor.
//!
//! Titles, text, content, and headers are indexed. Element names and
//! `x:Name` ids are not. Markup inside `Code` or `Script` is skipped.

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use quick_xml::XmlVersion;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable labels from XAML documents.
#[derive(Debug, Default, Clone, Copy)]
pub struct XamlExtractor;

#[async_trait]
impl ContentExtractor for XamlExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(|e| e.eq_ignore_ascii_case("xaml"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(xaml_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn xaml_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut skip = 0i32;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                let hidden = is_hidden(&local);
                if skip == 0 && !hidden {
                    take_attrs(&e, &mut out);
                }
                if skip > 0 || hidden {
                    skip += 1;
                }
            }
            Ok(Event::Empty(e)) => {
                if skip == 0 {
                    take_attrs(&e, &mut out);
                }
            }
            Ok(Event::Text(t)) => {
                if skip == 0 {
                    push_line(&mut out, t.as_ref());
                }
            }
            Ok(Event::CData(t)) => {
                if skip == 0 {
                    push_line(&mut out, t.as_ref());
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if skip == 0 {
                    push_line(&mut out, &decode_ref(r.as_ref()).to_string());
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
        if out.len() >= MAX_CONTENT_BYTES {
            out.truncate(MAX_CONTENT_BYTES);
            break;
        }
    }
    out.trim().to_string()
}

fn take_attrs(e: &quick_xml::events::BytesStart<'_>, out: &mut String) {
    for attr in e.attributes().flatten() {
        let key = local_name(attr.key.as_ref());
        if !is_label_attr(&key) {
            continue;
        }
        let Ok(value) = attr.normalized_value(XmlVersion::Implicit1_0) else {
            continue;
        };
        push_line(out, value.as_ref());
    }
}

fn is_hidden(local: &str) -> bool {
    matches!(local.to_ascii_lowercase().as_str(), "code" | "script")
}

fn is_label_attr(local: &str) -> bool {
    matches!(
        local.to_ascii_lowercase().as_str(),
        "title"
            | "text"
            | "content"
            | "header"
            | "placeholdertext"
            | "tooltip"
            | "label"
            | "helptext"
    )
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
    let name = name.rsplit('}').next().unwrap_or(name);
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
    fn keeps_titles_and_text_and_skips_names() {
        let text = xaml_text(
            r#"<Window Title="Night Drive" xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation">
              <Button Content="OK" x:Name="okButton"/>
              <TextBlock Text="Hello"/>
              <TextBlock>Plain label</TextBlock>
              <Code>SECRET</Code>
            </Window>"#,
        );
        assert!(text.contains("Night Drive"), "{text}");
        assert!(text.contains("OK"), "{text}");
        assert!(text.contains("Hello"), "{text}");
        assert!(text.contains("Plain label"), "{text}");
        assert!(!text.contains("okButton"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("schemas.microsoft.com"), "{text}");
    }
}
