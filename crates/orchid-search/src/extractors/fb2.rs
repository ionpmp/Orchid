//! FictionBook (`.fb2` and `.fb2.zip`) text extractor.
//!
//! Title, authors, annotation, and body paragraphs are kept. Embedded
//! `<binary>` cover images are skipped so base64 does not enter the index.
//! Windows-1251 books are decoded from the XML encoding declaration.

use std::io::{Cursor, Read};

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use zip::ZipArchive;

use crate::error::{Result, SearchError};
use crate::extractors::text::MAX_CONTENT_BYTES;
use crate::extractors::ContentExtractor;

/// Extract readable text from FictionBook files.
#[derive(Debug, Default, Clone, Copy)]
pub struct Fb2Extractor;

#[async_trait]
impl ContentExtractor for Fb2Extractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            base.eq_ignore_ascii_case("application/x-fictionbook+xml")
                || base.eq_ignore_ascii_case("application/fictionbook2+xml")
        }) || extension.is_some_and(|e| e.eq_ignore_ascii_case("fb2"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let name = path.file_name().unwrap_or("");
        let zipped = name.to_ascii_lowercase().ends_with(".fb2.zip");
        let raw = if zipped {
            read_fb2_zip(provider, path).await?
        } else {
            orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?
        };
        let text = fb2_text(&decode_fb2(&raw));
        if text.trim().is_empty() {
            return Err(SearchError::Extraction {
                path: path.to_string(),
                reason: "no fictionbook text".into(),
            });
        }
        Ok(text)
    }
}

async fn read_fb2_zip(
    provider: &dyn orchid_fs::FsProvider,
    path: &orchid_fs::FsPath,
) -> Result<Vec<u8>> {
    let path_str = path.to_string();
    let bytes = provider.read(path).await?;
    tokio::task::spawn_blocking(move || fb2_member(&bytes))
        .await
        .map_err(|e| SearchError::Extraction {
            path: path_str.clone(),
            reason: format!("join: {e}"),
        })?
        .map_err(|reason| SearchError::Extraction {
            path: path_str,
            reason,
        })
}

fn fb2_member(bytes: &[u8]) -> std::result::Result<Vec<u8>, String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("zip: {e}"))?;
    let name = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .find(|n| n.to_ascii_lowercase().ends_with(".fb2"))
        .ok_or_else(|| "no .fb2 member".to_string())?;
    let entry = archive
        .by_name(&name)
        .map_err(|e| format!("zip entry: {e}"))?;
    let mut xml = Vec::new();
    entry
        .take(MAX_CONTENT_BYTES as u64)
        .read_to_end(&mut xml)
        .map_err(|e| e.to_string())?;
    Ok(xml)
}

pub(crate) fn fb2_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    reader.config_mut().check_end_names = false;
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut skip = 0i32;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if skip > 0 || local == "binary" {
                    skip += 1;
                } else if is_block(&local) {
                    push_break(&mut out);
                }
            }
            Ok(Event::Text(t)) if skip == 0 => push_words(&mut out, t.as_ref()),
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                if skip > 0 && (local == "binary" || skip > 1) {
                    skip -= 1;
                } else if is_block(&local) {
                    push_break(&mut out);
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
    out.trim().to_string()
}

fn decode_fb2(bytes: &[u8]) -> String {
    let head_len = bytes.len().min(240);
    let head = String::from_utf8_lossy(&bytes[..head_len]);
    if let Some(label) = encoding_label(&head) {
        if let Some(encoding) = encoding_rs::Encoding::for_label(label.as_bytes()) {
            return encoding.decode(bytes).0.into_owned();
        }
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn encoding_label(head: &str) -> Option<String> {
    let lower = head.to_ascii_lowercase();
    let key = "encoding=";
    let idx = lower.find(key)?;
    let rest = head.get(idx + key.len()..)?;
    let rest = rest.trim_start();
    let quote = rest.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let value = rest[quote.len_utf8()..].split(quote).next()?;
    Some(value.trim().to_string())
}

fn is_block(local: &str) -> bool {
    matches!(
        local,
        "p" | "v"
            | "subtitle"
            | "title"
            | "annotation"
            | "book-title"
            | "stanza"
            | "cite"
            | "poem"
            | "section"
            | "epigraph"
    )
}

fn local_name(name: &str) -> String {
    name.rsplit(':').next().unwrap_or(name).to_string()
}

fn push_break(out: &mut String) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
}

fn push_words(out: &mut String, raw: &str) {
    let words: Vec<&str> = raw.split_whitespace().collect();
    if words.is_empty() {
        return;
    }
    if !out.is_empty() && !out.ends_with('\n') && !out.ends_with(' ') {
        out.push(' ');
    }
    out.push_str(&words.join(" "));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    const BOOK: &str = r#"<?xml version="1.0" encoding="utf-8"?>
        <FictionBook>
          <description><title-info>
            <book-title>Northern Road</book-title>
            <author><first-name>Ada</first-name><last-name>Lovelace</last-name></author>
            <annotation><p>A short note</p></annotation>
          </title-info></description>
          <body><section><p>Hello chapter</p></section></body>
          <binary id="cover.jpg" content-type="image/jpeg">NOTINDEXEDBASE64</binary>
        </FictionBook>"#;

    #[test]
    fn keeps_title_and_body_and_skips_binary() {
        let text = fb2_text(BOOK);
        assert!(text.contains("Northern Road"));
        assert!(text.contains("Ada"));
        assert!(text.contains("Lovelace"));
        assert!(text.contains("A short note"));
        assert!(text.contains("Hello chapter"));
        assert!(!text.contains("NOTINDEXEDBASE64"));
    }

    #[test]
    fn decodes_windows_1251_declaration() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(
            br#"<?xml version="1.0" encoding="windows-1251"?><FictionBook><body><p>"#,
        );
        bytes.extend_from_slice(&[0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2]);
        bytes.extend_from_slice(b"</p></body></FictionBook>");
        let text = fb2_text(&decode_fb2(&bytes));
        assert!(text.contains("Привет"));
    }

    #[test]
    fn reads_fb2_member_from_zip() {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut cursor);
            let opts = SimpleFileOptions::default();
            zip.start_file("book.fb2", opts).unwrap();
            zip.write_all(BOOK.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        let xml = fb2_member(&cursor.into_inner()).unwrap();
        let text = fb2_text(&decode_fb2(&xml));
        assert!(text.contains("Hello chapter"));
        assert!(!text.contains("NOTINDEXEDBASE64"));
    }
}
