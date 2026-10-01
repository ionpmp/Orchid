//! NuGet package manifest extractor.
//!
//! Package ids, titles, descriptions, authors, and dependency ids are
//! indexed. Versions, repository commits, and packed file lists are not.

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use quick_xml::XmlVersion;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from `.nuspec` manifests.
#[derive(Debug, Default, Clone, Copy)]
pub struct NuspecExtractor;

#[async_trait]
impl ContentExtractor for NuspecExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(|ext| ext.eq_ignore_ascii_case("nuspec"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(nuspec_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn nuspec_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut skip = 0i32;
    let mut capture: Option<Capture> = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if skip > 0 || is_hidden(&local) {
                    skip += 1;
                } else if local == "dependency" {
                    take_attr(&e, &mut out, "id");
                } else if local == "repository" {
                    take_attr(&e, &mut out, "url");
                } else if let Some(current) = capture.as_mut() {
                    current.depth += 1;
                } else if is_label(&local) {
                    capture = Some(Capture {
                        depth: 1,
                        text: String::new(),
                    });
                }
            }
            Ok(Event::Empty(e)) => {
                if skip == 0 {
                    let local = local_name(e.name().as_ref());
                    if local == "dependency" {
                        take_attr(&e, &mut out, "id");
                    } else if local == "repository" {
                        take_attr(&e, &mut out, "url");
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if skip == 0 {
                    if let Some(current) = capture.as_mut() {
                        append(&mut current.text, t.as_ref());
                    }
                }
            }
            Ok(Event::CData(t)) => {
                if skip == 0 {
                    if let Some(current) = capture.as_mut() {
                        append(&mut current.text, t.as_ref());
                    }
                }
            }
            Ok(Event::End(_)) => {
                if skip > 0 {
                    skip -= 1;
                } else {
                    let flush = capture.as_ref().is_some_and(|current| current.depth == 1);
                    if let Some(current) = capture.as_mut() {
                        current.depth -= 1;
                    }
                    if flush {
                        if let Some(current) = capture.take() {
                            push_line(&mut out, current.text.trim());
                        }
                    }
                }
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

struct Capture {
    depth: i32,
    text: String,
}

fn is_label(local: &str) -> bool {
    matches!(
        local,
        "id" | "title"
            | "authors"
            | "owners"
            | "description"
            | "summary"
            | "tags"
            | "releasenotes"
            | "copyright"
            | "language"
            | "license"
    )
}

fn is_hidden(local: &str) -> bool {
    matches!(local, "files" | "file" | "frameworkassemblies")
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

fn append(buf: &mut String, chunk: &str) {
    if chunk.is_empty() || buf.len() >= MAX_CONTENT_BYTES {
        return;
    }
    let room = MAX_CONTENT_BYTES.saturating_sub(buf.len());
    buf.push_str(&chunk.chars().take(room).collect::<String>());
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
    fn indexes_metadata_and_skips_versions_and_files() {
        let text = nuspec_text(
            r#"<package>
              <metadata>
                <id>Orchid.Core</id>
                <version>1.2.3</version>
                <title>Orchid</title>
                <authors>Ada</authors>
                <description>File manager</description>
                <tags>search files</tags>
                <releaseNotes>Fixed search</releaseNotes>
                <repository type="git" url="https://example.com/orchid" commit="deadbeef" />
                <dependencies>
                  <group targetFramework="net8.0">
                    <dependency id="Tantivy" version="0.26.0" />
                  </group>
                </dependencies>
              </metadata>
              <files>
                <file src="SECRET.dll" target="lib" />
              </files>
            </package>"#,
        );
        assert!(text.contains("Orchid.Core"), "{text}");
        assert!(text.contains("File manager"), "{text}");
        assert!(text.contains("Ada"), "{text}");
        assert!(text.contains("search files"), "{text}");
        assert!(text.contains("Fixed search"), "{text}");
        assert!(text.contains("https://example.com/orchid"), "{text}");
        assert!(text.contains("Tantivy"), "{text}");
        assert!(!text.contains("1.2.3"), "{text}");
        assert!(!text.contains("0.26.0"), "{text}");
        assert!(!text.contains("deadbeef"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("net8.0"), "{text}");
    }
}
