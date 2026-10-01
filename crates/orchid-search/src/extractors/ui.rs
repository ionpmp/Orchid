//! Qt Designer and GTK Glade (`.ui`) extractor.
//!
//! Window titles, labels, tooltips, and other visible strings are indexed.
//! Geometry and object ids are not.

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable labels from `.ui` interface files.
#[derive(Debug, Default, Clone, Copy)]
pub struct UiExtractor;

#[async_trait]
impl ContentExtractor for UiExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(|e| e.eq_ignore_ascii_case("ui"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(ui_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn ui_text(xml: &str) -> String {
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
                } else if local == "property" && attr(&e, "name").is_some_and(|n| is_text_prop(&n))
                {
                    capture = Some(Capture::new(local));
                } else if local == "string" {
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

fn is_text_prop(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "windowtitle"
            | "windowicontext"
            | "title"
            | "text"
            | "label"
            | "tooltip"
            | "tooltip-text"
            | "whatsthis"
            | "placeholdertext"
            | "placeholder-text"
            | "accessiblename"
            | "accessibledescription"
            | "statustip"
            | "shortcut"
            | "comment"
            | "subtitle"
            | "message"
            | "html"
            | "plaintext"
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
    fn qt_and_glade_keep_labels_and_skip_geometry() {
        let qt = ui_text(
            r#"<ui version="4.0">
              <widget class="QMainWindow" name="MainWindow">
                <property name="windowTitle"><string>Night Drive</string></property>
                <property name="geometry"><rect><width>100</width></rect></property>
                <widget class="QPushButton" name="okButton">
                  <property name="text"><string>OK</string></property>
                </widget>
              </widget>
            </ui>"#,
        );
        assert!(qt.contains("Night Drive"), "{qt}");
        assert!(qt.contains("OK"), "{qt}");
        assert!(!qt.contains("okButton"), "{qt}");
        assert!(!qt.contains("100"), "{qt}");

        let glade = ui_text(
            r#"<interface>
              <object class="GtkWindow">
                <property name="title">Main &amp; Window</property>
                <property name="label">_Open</property>
              </object>
            </interface>"#,
        );
        assert!(glade.contains("Main & Window"), "{glade}");
        assert!(glade.contains("_Open"), "{glade}");
        assert!(!glade.contains("GtkWindow"), "{glade}");
    }
}
