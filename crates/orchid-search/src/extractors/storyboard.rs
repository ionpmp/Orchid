//! iOS storyboard and XIB extractor.
//!
//! Titles, label text, placeholders, and user labels are indexed. Class
//! names, object ids, and layout numbers are not.

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use quick_xml::XmlVersion;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable labels from `.storyboard` and `.xib` files.
#[derive(Debug, Default, Clone, Copy)]
pub struct StoryboardExtractor;

#[async_trait]
impl ContentExtractor for StoryboardExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "storyboard" | "xib"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(storyboard_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn storyboard_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut capture: Option<Capture> = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) => {
                if capture.is_none() {
                    take_attrs(&e, &mut out);
                }
            }
            Ok(Event::Start(e)) => {
                if capture.is_none() {
                    take_attrs(&e, &mut out);
                }
                let local = local_name(e.name().as_ref());
                if let Some(current) = capture.as_mut() {
                    if local == current.tag {
                        current.depth += 1;
                    }
                } else if local == "string" && string_key_is_visible(attr(&e, "key").as_deref()) {
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

fn string_key_is_visible(key: Option<&str>) -> bool {
    match key {
        None => true,
        Some(key) => matches!(
            key.to_ascii_lowercase().as_str(),
            "text" | "title" | "placeholder" | "normaltitle"
        ),
    }
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

fn is_label_attr(local: &str) -> bool {
    matches!(
        local.to_ascii_lowercase().as_str(),
        "title" | "text" | "placeholder" | "userlabel"
    )
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
    fn keeps_visible_labels_and_skips_ids() {
        let text = storyboard_text(
            r#"<document>
              <scene><objects>
                <viewController title="Settings" userLabel="Settings Screen" customClass="SettingsViewController" id="abc">
                  <label text="Hello" id="def">
                    <rect key="frame" x="0" y="0" width="100" height="21"/>
                  </label>
                  <button>
                    <state key="normal" title="Save"/>
                  </button>
                  <textField placeholder="Search"/>
                  <string key="text">Plain storyboard</string>
                  <string key="fontName">Helvetica</string>
                </viewController>
              </objects></scene>
            </document>"#,
        );
        assert!(text.contains("Settings"), "{text}");
        assert!(text.contains("Settings Screen"), "{text}");
        assert!(text.contains("Hello"), "{text}");
        assert!(text.contains("Save"), "{text}");
        assert!(text.contains("Search"), "{text}");
        assert!(text.contains("Plain storyboard"), "{text}");
        assert!(!text.contains("SettingsViewController"), "{text}");
        assert!(!text.contains("abc"), "{text}");
        assert!(!text.contains("100"), "{text}");
        assert!(!text.contains("Helvetica"), "{text}");
    }
}
