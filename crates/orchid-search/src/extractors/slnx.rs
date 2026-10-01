//! Visual Studio XML solution extractor.
//!
//! Project paths, folder names, and solution-item paths are indexed.
//! Build configurations are not.

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use quick_xml::XmlVersion;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable paths from `.slnx` solutions.
#[derive(Debug, Default, Clone, Copy)]
pub struct SlnxExtractor;

#[async_trait]
impl ContentExtractor for SlnxExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(|ext| ext.eq_ignore_ascii_case("slnx"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(slnx_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn slnx_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                consider(local_name(e.name().as_ref()).as_str(), &e, &mut out);
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
        if out.len() >= MAX_CONTENT_BYTES {
            break;
        }
    }
    out.trim().to_string()
}

fn consider(local: &str, e: &quick_xml::events::BytesStart<'_>, out: &mut String) {
    match local {
        "project" | "file" => take_attr(e, out, "path"),
        "folder" => take_attr(e, out, "name"),
        _ => {}
    }
}

fn take_attr(e: &quick_xml::events::BytesStart<'_>, out: &mut String, key: &str) {
    for attr in e.attributes().flatten() {
        if local_name(attr.key.as_ref()) != key {
            continue;
        }
        let Ok(value) = attr.normalized_value(XmlVersion::Implicit1_0) else {
            continue;
        };
        push_line(out, value.as_ref());
    }
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
    fn indexes_paths_and_skips_build_types() {
        let text = slnx_text(
            r#"<Solution>
              <Configurations>
                <BuildType Name="Debug" />
                <Platform Name="Any CPU" />
              </Configurations>
              <Project Path="src/Orchid.csproj" Default="true" />
              <Folder Name="Solution Items">
                <File Path="README.md" />
              </Folder>
            </Solution>"#,
        );
        assert!(text.contains("src/Orchid.csproj"), "{text}");
        assert!(text.contains("Solution Items"), "{text}");
        assert!(text.contains("README.md"), "{text}");
        assert!(!text.contains("Debug"), "{text}");
        assert!(!text.contains("Any CPU"), "{text}");
        assert!(!text.contains("true"), "{text}");
    }
}
