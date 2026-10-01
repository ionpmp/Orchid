//! MSBuild project extractor.
//!
//! Assembly names, descriptions, SDK ids, and package or project references
//! are indexed. Versions and source-file lists are not.

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use quick_xml::XmlVersion;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable labels from MSBuild projects.
#[derive(Debug, Default, Clone, Copy)]
pub struct MsbuildExtractor;

#[async_trait]
impl ContentExtractor for MsbuildExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "csproj" | "fsproj" | "vbproj" | "vcxproj"
            )
        })
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(msbuild_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn msbuild_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut capture: Option<Capture> = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "project" {
                    take_attr(&e, &mut out, "sdk");
                }
                if is_reference(&local) {
                    take_ref(&e, &mut out);
                } else if capture.is_some() {
                    if let Some(current) = capture.as_mut() {
                        current.depth += 1;
                    }
                } else if is_label(&local) {
                    capture = Some(Capture {
                        depth: 1,
                        text: String::new(),
                    });
                }
            }
            Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "project" {
                    take_attr(&e, &mut out, "sdk");
                }
                if is_reference(&local) {
                    take_ref(&e, &mut out);
                }
            }
            Ok(Event::Text(t)) => {
                if let Some(current) = capture.as_mut() {
                    append(&mut current.text, t.as_ref());
                }
            }
            Ok(Event::CData(t)) => {
                if let Some(current) = capture.as_mut() {
                    append(&mut current.text, t.as_ref());
                }
            }
            Ok(Event::End(_)) => {
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
        "assemblyname"
            | "rootnamespace"
            | "projectname"
            | "product"
            | "title"
            | "description"
            | "copyright"
            | "company"
            | "authors"
            | "packageid"
            | "packagetags"
            | "packagereleasenotes"
    )
}

fn is_reference(local: &str) -> bool {
    matches!(local, "packagereference" | "projectreference" | "reference")
}

fn take_ref(e: &quick_xml::events::BytesStart<'_>, out: &mut String) {
    take_attr(e, out, "include");
    take_attr(e, out, "update");
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
    fn indexes_names_and_references_and_skips_versions() {
        let text = msbuild_text(
            r#"<Project Sdk="Microsoft.NET.Sdk">
              <PropertyGroup>
                <AssemblyName>Orchid</AssemblyName>
                <Version>1.2.3</Version>
                <Description>File manager</Description>
              </PropertyGroup>
              <ItemGroup>
                <PackageReference Include="Tantivy" Version="0.26.0" />
                <ProjectReference Include="orchid-fs.csproj" />
                <Compile Include="SECRET.cs" />
              </ItemGroup>
            </Project>"#,
        );
        assert!(text.contains("Microsoft.NET.Sdk"), "{text}");
        assert!(text.contains("Orchid"), "{text}");
        assert!(text.contains("File manager"), "{text}");
        assert!(text.contains("Tantivy"), "{text}");
        assert!(text.contains("orchid-fs.csproj"), "{text}");
        assert!(!text.contains("1.2.3"), "{text}");
        assert!(!text.contains("0.26.0"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
    }
}
