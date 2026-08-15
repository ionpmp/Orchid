//! OOXML `docProps/core.xml` (title, author, keywords, …).

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::error::{Result, ViewerError};

/// Dublin Core / core-properties used by Word, Excel, and PowerPoint.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OfficeCoreProps {
    /// `dc:title`
    pub title: String,
    /// `dc:subject`
    pub subject: String,
    /// `dc:creator`
    pub creator: String,
    /// `cp:keywords`
    pub keywords: String,
    /// `dc:description`
    pub description: String,
    /// `cp:lastModifiedBy` (read-only in the editor).
    pub last_modified_by: String,
}

/// Whether `ext` (lowercase, no dot) is an OOXML Office package.
#[must_use]
pub fn is_office_extension(ext: &str) -> bool {
    matches!(ext, "docx" | "xlsx" | "pptx")
}

/// Read `docProps/core.xml` from an OOXML package.
///
/// # Errors
///
/// Missing zip, missing part, or XML parse failure.
pub fn read_office_core_props(path: &Path) -> Result<OfficeCoreProps> {
    let file = File::open(path)?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| ViewerError::Metadata(format!("zip: {e}")))?;
    let mut entry = archive
        .by_name("docProps/core.xml")
        .map_err(|e| ViewerError::Metadata(format!("core.xml: {e}")))?;
    let mut xml = String::new();
    entry.read_to_string(&mut xml)?;
    Ok(parse_core_xml(&xml))
}

/// Write `docProps/core.xml`, copying every other zip entry unchanged.
///
/// # Errors
///
/// Zip I/O failures.
pub fn write_office_core_props(path: &Path, props: &OfficeCoreProps) -> Result<()> {
    let file = File::open(path)?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| ViewerError::Metadata(format!("zip: {e}")))?;
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| ViewerError::Metadata(format!("zip entry: {e}")))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        let mut data = Vec::new();
        entry.read_to_end(&mut data)?;
        parts.push((name, data));
    }
    drop(archive);

    let xml = write_core_xml(props);
    if let Some((_, data)) = parts.iter_mut().find(|(n, _)| n == "docProps/core.xml") {
        *data = xml.into_bytes();
    } else {
        parts.push(("docProps/core.xml".into(), xml.into_bytes()));
    }

    let tmp = path.with_extension("orchid-core.tmp");
    {
        let out = File::create(&tmp)?;
        let mut zip = ZipWriter::new(out);
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in &parts {
            zip.start_file(name, opts)
                .map_err(|e| ViewerError::Metadata(e.to_string()))?;
            zip.write_all(data)?;
        }
        zip.finish()
            .map_err(|e| ViewerError::Metadata(e.to_string()))?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Format core properties as a report body.
#[must_use]
pub fn format_office_report(props: &OfficeCoreProps) -> String {
    let mut body = String::new();
    for (label, value) in [
        ("Title", props.title.as_str()),
        ("Subject", props.subject.as_str()),
        ("Creator", props.creator.as_str()),
        ("Keywords", props.keywords.as_str()),
        ("Description", props.description.as_str()),
        ("Last modified by", props.last_modified_by.as_str()),
    ] {
        if !value.is_empty() {
            body.push_str(label);
            body.push_str(": ");
            body.push_str(value);
            body.push('\n');
        }
    }
    body
}

/// Packed editor line: `title | subject | creator | keywords | description`.
#[must_use]
pub fn pack_office_props(props: &OfficeCoreProps) -> String {
    format!(
        "{} | {} | {} | {} | {}",
        props.title, props.subject, props.creator, props.keywords, props.description
    )
}

/// Parse the packed editor line.
#[must_use]
pub fn unpack_office_props(input: &str, base: OfficeCoreProps) -> OfficeCoreProps {
    let parts: Vec<&str> = input.split('|').map(str::trim).collect();
    let get = |i: usize| parts.get(i).copied().unwrap_or("").to_string();
    OfficeCoreProps {
        title: get(0),
        subject: get(1),
        creator: get(2),
        keywords: get(3),
        description: get(4),
        last_modified_by: base.last_modified_by,
    }
}

fn parse_core_xml(xml: &str) -> OfficeCoreProps {
    let mut props = OfficeCoreProps::default();
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut current = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e) | quick_xml::events::Event::Empty(e)) => {
                current = local_name(e.name().as_ref()).to_ascii_lowercase();
            }
            Ok(quick_xml::events::Event::Text(t)) => {
                let text = t.decode().unwrap_or_default().into_owned();
                match current.as_str() {
                    "title" => props.title = text,
                    "subject" => props.subject = text,
                    "creator" => props.creator = text,
                    "keywords" => props.keywords = text,
                    "description" => props.description = text,
                    "lastmodifiedby" => props.last_modified_by = text,
                    _ => {}
                }
            }
            Ok(quick_xml::events::Event::End(_)) => current.clear(),
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    props
}

fn local_name(name: &[u8]) -> &str {
    let s = std::str::from_utf8(name).unwrap_or("");
    s.rsplit(':').next().unwrap_or(s)
}

fn write_core_xml(props: &OfficeCoreProps) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <dc:title>{}</dc:title>
  <dc:subject>{}</dc:subject>
  <dc:creator>{}</dc:creator>
  <cp:keywords>{}</cp:keywords>
  <dc:description>{}</dc:description>
  <cp:lastModifiedBy>{}</cp:lastModifiedBy>
</cp:coreProperties>
"#,
        xml_escape(&props.title),
        xml_escape(&props.subject),
        xml_escape(&props.creator),
        xml_escape(&props.keywords),
        xml_escape(&props.description),
        xml_escape(&props.last_modified_by),
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    #[test]
    fn parse_and_pack_roundtrip() {
        let xml = r#"<cp:coreProperties xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties">
  <dc:title>T</dc:title><dc:creator>Ann</dc:creator><cp:keywords>a,b</cp:keywords>
</cp:coreProperties>"#;
        let p = parse_core_xml(xml);
        assert_eq!(p.title, "T");
        assert_eq!(p.creator, "Ann");
        assert_eq!(p.keywords, "a,b");
        let packed = pack_office_props(&p);
        let back = unpack_office_props(&packed, OfficeCoreProps::default());
        assert_eq!(back.title, "T");
        assert_eq!(back.creator, "Ann");
    }

    #[test]
    fn zip_read_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.docx");
        {
            let f = File::create(&path).unwrap();
            let mut zip = ZipWriter::new(f);
            let opts = SimpleFileOptions::default();
            zip.start_file("docProps/core.xml", opts).unwrap();
            zip.write_all(
                write_core_xml(&OfficeCoreProps {
                    title: "Old".into(),
                    creator: "A".into(),
                    ..OfficeCoreProps::default()
                })
                .as_bytes(),
            )
            .unwrap();
            zip.finish().unwrap();
        }
        let mut props = read_office_core_props(&path).unwrap();
        assert_eq!(props.title, "Old");
        props.title = "New".into();
        write_office_core_props(&path, &props).unwrap();
        let again = read_office_core_props(&path).unwrap();
        assert_eq!(again.title, "New");
        assert_eq!(again.creator, "A");
    }
}
