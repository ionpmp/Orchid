//! WiX installer source extractor.
//!
//! Product names, feature titles, dialog text, and localization strings are
//! indexed. Component ids, versions, and property values are not.

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use quick_xml::XmlVersion;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable labels from WiX sources and localization files.
#[derive(Debug, Default, Clone, Copy)]
pub struct WixExtractor;

#[async_trait]
impl ContentExtractor for WixExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "wxs" | "wxl"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(wix_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn wix_text(xml: &str) -> String {
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
                    take_attrs(&local, &e, &mut out);
                }
                if skip > 0 || hidden {
                    skip += 1;
                }
            }
            Ok(Event::Empty(e)) => {
                if skip == 0 {
                    let local = local_name(e.name().as_ref());
                    if !is_hidden(&local) {
                        take_attrs(&local, &e, &mut out);
                    }
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

fn take_attrs(element: &str, e: &quick_xml::events::BytesStart<'_>, out: &mut String) {
    for attr in e.attributes().flatten() {
        let key = local_name(attr.key.as_ref());
        if !is_label_attr(element, &key) {
            continue;
        }
        let Ok(value) = attr.normalized_value(XmlVersion::Implicit1_0) else {
            continue;
        };
        push_line(out, value.as_ref());
    }
}

fn is_hidden(local: &str) -> bool {
    matches!(local, "binary" | "customaction" | "condition")
}

fn is_label_attr(element: &str, key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    if key == "value" {
        return element == "string";
    }
    matches!(
        key.as_str(),
        "name"
            | "title"
            | "description"
            | "text"
            | "manufacturer"
            | "comments"
            | "helptext"
            | "message"
    )
}

fn local_name(name: &str) -> String {
    let name = name.rsplit('}').next().unwrap_or(name);
    name.rsplit(':').next().unwrap_or(name).to_ascii_lowercase()
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
    fn keeps_labels_and_skips_ids_and_property_values() {
        let text = wix_text(
            r#"<Wix>
              <Product Name="Orchid" Id="SECRET-GUID" Version="1.0.0">
                <Package Description="Installs the player" />
                <Property Id="API_TOKEN" Value="SECRET" />
                <Feature Title="Main" Description="Core files" />
                <File Name="orchid.exe" Id="fil1" />
                <Dialog>
                  <Control Text="Install &amp; run" />
                </Dialog>
                <Binary>SECRET2</Binary>
              </Product>
              <String Id="DlgTitle" Value="Welcome" />
              <String Id="DlgBody">Ready to install</String>
            </Wix>"#,
        );
        assert!(text.contains("Orchid"), "{text}");
        assert!(text.contains("Installs the player"), "{text}");
        assert!(text.contains("Core files"), "{text}");
        assert!(text.contains("orchid.exe"), "{text}");
        assert!(text.contains("Install & run"), "{text}");
        assert!(text.contains("Welcome"), "{text}");
        assert!(text.contains("Ready to install"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("1.0.0"), "{text}");
        assert!(!text.contains("fil1"), "{text}");
        assert!(!text.contains("DlgTitle"), "{text}");
    }
}
