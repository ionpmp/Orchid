//! DOCX content extractor (Office Open XML).

use std::io::Read;

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use zip::ZipArchive;

use crate::error::{Result, SearchError};
use crate::extractors::text::MAX_CONTENT_BYTES;
use crate::extractors::ContentExtractor;

/// Extract plain text from `.docx` packages.
#[derive(Debug, Default, Clone, Copy)]
pub struct DocxExtractor;

#[async_trait]
impl ContentExtractor for DocxExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.map(|m| m == "application/vnd.openxmlformats-officedocument.wordprocessingml.document")
            .unwrap_or(false)
            || extension
                .map(|e| e.eq_ignore_ascii_case("docx") || e.eq_ignore_ascii_case("docm"))
                .unwrap_or(false)
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let path_str = path.to_string();
        if path.is_local() {
            let os_path = path.to_local()?;
            let path_for_err = path_str.clone();
            return tokio::task::spawn_blocking(move || extract_local(&os_path))
                .await
                .map_err(|e| SearchError::Extraction {
                    path: path_for_err,
                    reason: format!("join: {e}"),
                })?;
        }
        let bytes = provider.read(path).await.map_err(SearchError::from)?;
        let path_for_err = path_str.clone();
        tokio::task::spawn_blocking(move || extract_bytes(&bytes))
            .await
            .map_err(|e| SearchError::Extraction {
                path: path_for_err,
                reason: format!("join: {e}"),
            })?
    }
}

fn extract_local(path: &std::path::Path) -> Result<String> {
    let file = std::fs::File::open(path).map_err(|e| SearchError::Extraction {
        path: path.display().to_string(),
        reason: e.to_string(),
    })?;
    let mut archive = ZipArchive::new(file).map_err(|e| SearchError::Extraction {
        path: path.display().to_string(),
        reason: format!("zip: {e}"),
    })?;
    read_document_xml(&mut archive, &path.display().to_string())
}

fn extract_bytes(bytes: &[u8]) -> Result<String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|e| SearchError::Extraction {
        path: "<bytes>".into(),
        reason: format!("zip: {e}"),
    })?;
    read_document_xml(&mut archive, "<bytes>")
}

fn read_document_xml<R: std::io::Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    path_label: &str,
) -> Result<String> {
    let mut entry = archive
        .by_name("word/document.xml")
        .map_err(|e| SearchError::Extraction {
            path: path_label.into(),
            reason: format!("missing word/document.xml: {e}"),
        })?;
    let mut xml = String::new();
    entry
        .read_to_string(&mut xml)
        .map_err(|e| SearchError::Extraction {
            path: path_label.into(),
            reason: e.to_string(),
        })?;
    Ok(plain_text_from_document_xml(&xml))
}

fn plain_text_from_document_xml(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut in_t = false;
    let mut at_para_start = true;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "t" {
                    in_t = true;
                } else if local == "p" {
                    if !out.is_empty() && !at_para_start {
                        out.push_str("\n\n");
                    }
                    at_para_start = true;
                } else if local == "tab" {
                    out.push('\t');
                    at_para_start = false;
                }
            }
            Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "tab" || local == "br" {
                    out.push(if local == "tab" { '\t' } else { '\n' });
                    at_para_start = false;
                }
            }
            Ok(Event::Text(t)) if in_t => {
                out.push_str(t.as_ref());
                at_para_start = false;
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "t" {
                    in_t = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
        if out.len() >= MAX_CONTENT_BYTES {
            out.truncate(MAX_CONTENT_BYTES);
            break;
        }
    }
    out
}

fn local_name(name: &str) -> String {
    name.rsplit(':').next().unwrap_or(name).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_text_from_minimal_document_xml() {
        let xml = r#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body>
            <w:p><w:r><w:t>Hello</w:t></w:r></w:p>
            <w:p><w:r><w:t>World</w:t></w:r></w:p>
          </w:body>
        </w:document>"#;
        let text = plain_text_from_document_xml(xml);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
    }
}
