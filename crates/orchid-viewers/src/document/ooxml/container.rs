//! ZIP package open / save for `.docx`.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::document::model::Document;
use crate::document::ooxml::document_xml::{
    parse_document_xml, parse_relationships, write_document_xml,
};
use crate::document::ooxml::numbering::{
    assign_orchid_list_ids, document_uses_lists, parse_numbering_xml, write_numbering_xml,
};
use crate::document::ooxml::styles::parse_styles_xml;
use crate::error::{Result, ViewerError};

/// In-memory OOXML package parts keyed by archive path.
#[derive(Debug, Default)]
pub struct OoxmlPackage {
    /// Part path → bytes.
    pub parts: HashMap<String, Vec<u8>>,
}

impl OoxmlPackage {
    /// Open a `.docx` zip from disk.
    ///
    /// # Errors
    ///
    /// [`ViewerError::DocumentParse`] / IO errors.
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let mut archive = ZipArchive::new(file)
            .map_err(|e| ViewerError::DocumentParse(format!("zip open: {e}")))?;
        let mut parts = HashMap::new();
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| ViewerError::DocumentParse(format!("zip entry: {e}")))?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().replace('\\', "/");
            let mut data = Vec::new();
            entry
                .read_to_end(&mut data)
                .map_err(|e| ViewerError::DocumentParse(format!("zip read: {e}")))?;
            parts.insert(name, data);
        }
        Ok(Self { parts })
    }

    /// Fetch a part by path.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.parts.get(name).map(|v| v.as_slice())
    }
}

/// Open a `.docx` and build a [`Document`].
///
/// # Errors
///
/// Propagates zip / XML failures.
pub fn open_document(path: &Path) -> Result<Document> {
    let package = OoxmlPackage::open(path)?;
    let document_xml = package
        .get("word/document.xml")
        .ok_or_else(|| ViewerError::DocumentParse("missing word/document.xml".into()))?;

    let styles = package
        .get("word/styles.xml")
        .map(parse_styles_xml)
        .transpose()?
        .unwrap_or_default();
    let numbering = package
        .get("word/numbering.xml")
        .map(parse_numbering_xml)
        .transpose()?
        .unwrap_or_default();
    let rels = package
        .get("word/_rels/document.xml.rels")
        .map(parse_relationships)
        .transpose()?
        .unwrap_or_default();

    let mut media = HashMap::new();
    for (name, bytes) in &package.parts {
        if name.starts_with("word/media/") {
            media.insert(name.clone(), bytes.clone());
        }
    }

    let (blocks, page_setup, unsupported) =
        parse_document_xml(document_xml, &styles, &numbering, &rels, &media)?;

    let mut retained = Vec::new();
    for (name, bytes) in &package.parts {
        if name == "word/document.xml" {
            continue;
        }
        retained.push((name.clone(), bytes.clone()));
    }

    Ok(Document {
        blocks,
        page_setup,
        unsupported,
        retained_parts: retained,
        content_types: package.get("[Content_Types].xml").map(|b| b.to_vec()),
        package_rels: package.get("_rels/.rels").map(|b| b.to_vec()),
        document_rels: package
            .get("word/_rels/document.xml.rels")
            .map(|b| b.to_vec()),
    })
}

/// Save a [`Document`] to `output_path` atomically (`.tmp` + rename).
///
/// # Errors
///
/// Propagates IO / zip failures.
pub async fn save_document(doc: &Document, output_path: &Path) -> Result<()> {
    let doc = doc.clone();
    let output_path = output_path.to_path_buf();
    tokio::task::spawn_blocking(move || save_document_sync(&doc, &output_path))
        .await
        .map_err(|e| ViewerError::DocumentSave(format!("join: {e}")))?
}

fn save_document_sync(doc: &Document, output_path: &Path) -> Result<()> {
    let mut doc = doc.clone();
    let uses_lists = document_uses_lists(&doc);
    if uses_lists {
        assign_orchid_list_ids(&mut doc);
    }

    let tmp = tmp_path(output_path);
    {
        let file = File::create(&tmp)?;
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        let document_xml = write_document_xml(&doc)?;
        zip.start_file("word/document.xml", opts)
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        zip.write_all(&document_xml)?;

        let mut content_types = doc
            .content_types
            .clone()
            .unwrap_or_else(|| MINIMAL_CONTENT_TYPES.as_bytes().to_vec());
        if uses_lists {
            content_types = ensure_numbering_content_type(&content_types);
        }
        zip.start_file("[Content_Types].xml", opts)
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        zip.write_all(&content_types)?;

        if let Some(ref rels) = doc.package_rels {
            zip.start_file("_rels/.rels", opts)
                .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
            zip.write_all(rels)?;
        } else {
            zip.start_file("_rels/.rels", opts)
                .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
            zip.write_all(MINIMAL_PACKAGE_RELS.as_bytes())?;
        }

        let mut written = std::collections::HashSet::new();
        written.insert("word/document.xml".to_string());
        written.insert("[Content_Types].xml".to_string());
        written.insert("_rels/.rels".to_string());

        if uses_lists {
            let numbering = write_numbering_xml();
            zip.start_file("word/numbering.xml", opts)
                .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
            zip.write_all(&numbering)?;
            written.insert("word/numbering.xml".to_string());
        }

        let mut document_rels = doc
            .document_rels
            .clone()
            .unwrap_or_else(|| MINIMAL_DOCUMENT_RELS.as_bytes().to_vec());
        if uses_lists {
            document_rels = ensure_numbering_document_rel(&document_rels);
        }
        zip.start_file("word/_rels/document.xml.rels", opts)
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
        zip.write_all(&document_rels)?;
        written.insert("word/_rels/document.xml.rels".to_string());

        for (name, bytes) in &doc.retained_parts {
            if written.contains(name) {
                continue;
            }
            zip.start_file(name.as_str(), opts)
                .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
            zip.write_all(bytes)?;
            written.insert(name.clone());
        }

        zip.finish()
            .map_err(|e| ViewerError::DocumentSave(e.to_string()))?;
    }

    std::fs::rename(&tmp, output_path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        ViewerError::Io(e)
    })?;
    Ok(())
}

fn ensure_numbering_content_type(bytes: &[u8]) -> Vec<u8> {
    let s = String::from_utf8_lossy(bytes);
    if s.contains("/word/numbering.xml") {
        return bytes.to_vec();
    }
    let injection = r#"  <Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>
"#;
    if let Some(idx) = s.rfind("</Types>") {
        let mut out = String::with_capacity(s.len() + injection.len());
        out.push_str(&s[..idx]);
        out.push_str(injection);
        out.push_str(&s[idx..]);
        out.into_bytes()
    } else {
        bytes.to_vec()
    }
}

fn ensure_numbering_document_rel(bytes: &[u8]) -> Vec<u8> {
    let s = String::from_utf8_lossy(bytes);
    if s.contains("relationships/numbering") || s.contains("Target=\"numbering.xml\"") {
        return bytes.to_vec();
    }
    let rid = next_relationship_id(&s);
    let injection = format!(
        r#"  <Relationship Id="{rid}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>
"#
    );
    if let Some(idx) = s.rfind("</Relationships>") {
        let mut out = String::with_capacity(s.len() + injection.len());
        out.push_str(&s[..idx]);
        out.push_str(&injection);
        out.push_str(&s[idx..]);
        out.into_bytes()
    } else {
        bytes.to_vec()
    }
}

fn next_relationship_id(rels_xml: &str) -> String {
    let mut max = 0u32;
    for (i, piece) in rels_xml.split("Id=\"rId").enumerate() {
        if i == 0 {
            continue;
        }
        let digits: String = piece.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = digits.parse::<u32>() {
            max = max.max(n);
        }
    }
    format!("rId{}", max + 1)
}

fn tmp_path(output: &Path) -> PathBuf {
    let mut tmp = output.as_os_str().to_owned();
    tmp.push(".tmp");
    PathBuf::from(tmp)
}

const MINIMAL_CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;

const MINIMAL_PACKAGE_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

const MINIMAL_DOCUMENT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
</Relationships>"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::model::{Block, Paragraph, Run, RunStyle};
    use std::io::Write;

    fn write_minimal_docx(path: &Path, document_xml: &[u8]) {
        let file = File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default();
        zip.start_file("[Content_Types].xml", opts).unwrap();
        zip.write_all(MINIMAL_CONTENT_TYPES.as_bytes()).unwrap();
        zip.start_file("_rels/.rels", opts).unwrap();
        zip.write_all(MINIMAL_PACKAGE_RELS.as_bytes()).unwrap();
        zip.start_file("word/document.xml", opts).unwrap();
        zip.write_all(document_xml).unwrap();
        zip.start_file("word/_rels/document.xml.rels", opts)
            .unwrap();
        zip.write_all(MINIMAL_DOCUMENT_RELS.as_bytes()).unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn open_minimal_package() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.docx");
        let xml = br#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Bold</w:t></w:r></w:p>
          </w:body>
        </w:document>"#;
        write_minimal_docx(&path, xml);
        let doc = open_document(&path).unwrap();
        match &doc.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(p.plain_text(), "Bold");
                assert!(p.runs[0].style.bold);
            }
            _ => panic!("paragraph"),
        }
    }

    #[test]
    fn save_and_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.docx");
        let doc = Document {
            blocks: vec![Block::Paragraph(Paragraph {
                runs: vec![Run {
                    text: "Saved".into(),
                    style: RunStyle {
                        italic: true,
                        ..Default::default()
                    },
                }],
                ..Default::default()
            })],
            ..Default::default()
        };
        save_document_sync(&doc, &path).unwrap();
        let loaded = open_document(&path).unwrap();
        match &loaded.blocks[0] {
            Block::Paragraph(p) => {
                assert_eq!(p.plain_text(), "Saved");
                assert!(p.runs[0].style.italic);
            }
            _ => panic!("paragraph"),
        }
    }
}
