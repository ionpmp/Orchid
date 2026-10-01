//! Spreadsheet and presentation text extractors (Office Open XML).
//!
//! Excel cell text comes from `xl/sharedStrings.xml` plus worksheet `<v>` /
//! inline strings. PowerPoint text is the `<a:t>` runs on each slide.

use std::io::{Read, Seek};

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use zip::ZipArchive;

use crate::error::{Result, SearchError};
use crate::extractors::text::MAX_CONTENT_BYTES;
use crate::extractors::ContentExtractor;

/// Extract plain text from `.xlsx` / `.xlsm` packages.
#[derive(Debug, Default, Clone, Copy)]
pub struct XlsxExtractor;

/// Extract plain text from `.pptx` / `.pptm` / `.ppsx` packages.
#[derive(Debug, Default, Clone, Copy)]
pub struct PptxExtractor;

#[async_trait]
impl ContentExtractor for XlsxExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            m == "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                || m == "application/vnd.ms-excel.sheet.macroEnabled.12"
        }) || extension
            .is_some_and(|e| e.eq_ignore_ascii_case("xlsx") || e.eq_ignore_ascii_case("xlsm"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        extract_zip(provider, path, extract_xlsx_archive).await
    }
}

#[async_trait]
impl ContentExtractor for PptxExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            m == "application/vnd.openxmlformats-officedocument.presentationml.presentation"
                || m == "application/vnd.ms-powerpoint.presentation.macroEnabled.12"
        }) || extension.is_some_and(|e| {
            e.eq_ignore_ascii_case("pptx")
                || e.eq_ignore_ascii_case("pptm")
                || e.eq_ignore_ascii_case("ppsx")
        })
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        extract_zip(provider, path, extract_pptx_archive).await
    }
}

pub(crate) async fn extract_zip<F>(
    provider: &dyn orchid_fs::FsProvider,
    path: &orchid_fs::FsPath,
    parse: F,
) -> Result<String>
where
    F: FnOnce(&mut ZipArchive<std::io::Cursor<Vec<u8>>>, &str) -> Result<String> + Send + 'static,
{
    let path_str = path.to_string();
    let bytes = if path.is_local() {
        let os_path = path.to_local()?;
        let path_for_err = path_str.clone();
        tokio::task::spawn_blocking(move || {
            std::fs::read(&os_path).map_err(|e| SearchError::Extraction {
                path: path_for_err,
                reason: e.to_string(),
            })
        })
        .await
        .map_err(|e| SearchError::Extraction {
            path: path_str.clone(),
            reason: format!("join: {e}"),
        })??
    } else {
        provider.read(path).await.map_err(SearchError::from)?
    };
    let path_for_err = path_str.clone();
    tokio::task::spawn_blocking(move || {
        let cursor = std::io::Cursor::new(bytes);
        let mut archive = ZipArchive::new(cursor).map_err(|e| SearchError::Extraction {
            path: path_for_err.clone(),
            reason: format!("zip: {e}"),
        })?;
        parse(&mut archive, &path_for_err)
    })
    .await
    .map_err(|e| SearchError::Extraction {
        path: path_str,
        reason: format!("join: {e}"),
    })?
}

fn extract_xlsx_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path_label: &str,
) -> Result<String> {
    let shared = read_entry(archive, "xl/sharedStrings.xml")
        .map(|xml| shared_strings(&xml))
        .unwrap_or_default();
    let mut out = String::new();
    if let Some(book) = read_entry(archive, "xl/workbook.xml") {
        push_capped(&mut out, &attr_values(&book, "sheet", "name"));
    }
    let names = zip_names(archive);
    for name in names {
        let lower = name.to_ascii_lowercase();
        if lower.starts_with("xl/worksheets/sheet") && lower.ends_with(".xml") {
            if let Some(xml) = read_entry(archive, &name) {
                push_capped(&mut out, &sheet_text(&xml, &shared));
            }
        }
    }
    if out.is_empty() {
        return Err(SearchError::Extraction {
            path: path_label.into(),
            reason: "no worksheet text".into(),
        });
    }
    Ok(out)
}

fn extract_pptx_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path_label: &str,
) -> Result<String> {
    let mut out = String::new();
    let names = zip_names(archive);
    for name in names {
        let lower = name.to_ascii_lowercase();
        let slide = lower.starts_with("ppt/slides/slide") && lower.ends_with(".xml");
        let notes = lower.starts_with("ppt/notesslides/notesslide") && lower.ends_with(".xml");
        if slide || notes {
            if let Some(xml) = read_entry(archive, &name) {
                push_capped(&mut out, &local_text_runs(&xml, "t"));
            }
        }
    }
    if out.is_empty() {
        return Err(SearchError::Extraction {
            path: path_label.into(),
            reason: "no slide text".into(),
        });
    }
    Ok(out)
}

fn zip_names<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Vec<String> {
    (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .collect()
}

fn read_entry<R: Read + Seek>(archive: &mut ZipArchive<R>, name: &str) -> Option<String> {
    let mut entry = archive.by_name(name).ok()?;
    let mut xml = String::new();
    entry.read_to_string(&mut xml).ok()?;
    Some(xml)
}

fn push_capped(out: &mut String, piece: &str) {
    if piece.is_empty() || out.len() >= MAX_CONTENT_BYTES {
        return;
    }
    if !out.is_empty() {
        out.push('\n');
    }
    let room = MAX_CONTENT_BYTES.saturating_sub(out.len());
    let take = piece.chars().take(room).collect::<String>();
    out.push_str(&take);
}

fn shared_strings(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut strings = Vec::new();
    let mut current = String::new();
    let mut in_si = false;
    let mut in_t = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "si" {
                    in_si = true;
                    current.clear();
                } else if in_si && local == "t" {
                    in_t = true;
                }
            }
            Ok(Event::Text(t)) if in_t => {
                current.push_str(t.as_ref());
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "t" {
                    in_t = false;
                } else if local == "si" {
                    strings.push(std::mem::take(&mut current));
                    in_si = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    strings
}

fn sheet_text(xml: &str, shared: &[String]) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut cell_kind = String::new();
    let mut in_v = false;
    let mut in_t = false;
    let mut value = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "c" {
                    cell_kind = attr(&e, "t");
                    value.clear();
                } else if local == "v" {
                    in_v = true;
                    value.clear();
                } else if local == "t" {
                    in_t = true;
                }
            }
            Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "c" {
                    cell_kind = attr(&e, "t");
                }
            }
            Ok(Event::Text(t)) => {
                if in_v || in_t {
                    value.push_str(t.as_ref());
                }
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "v" || (local == "c" && cell_kind == "inlineStr") {
                    emit_cell(&mut out, &cell_kind, &value, shared);
                    in_v = false;
                    value.clear();
                } else if local == "t" {
                    in_t = false;
                } else if local == "c" {
                    cell_kind.clear();
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
        if out.len() >= MAX_CONTENT_BYTES {
            break;
        }
    }
    out
}

fn emit_cell(out: &mut String, kind: &str, value: &str, shared: &[String]) {
    let text = if kind == "s" {
        value
            .trim()
            .parse::<usize>()
            .ok()
            .and_then(|i| shared.get(i))
            .map(String::as_str)
    } else if kind == "inlineStr" || kind.is_empty() || kind == "n" || kind == "str" {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    } else {
        None
    };
    let Some(text) = text else {
        return;
    };
    if text.is_empty() {
        return;
    }
    if !out.is_empty() {
        out.push('\t');
    }
    out.push_str(text);
}

fn local_text_runs(xml: &str, tag: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut in_tag = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                if local_name(e.name().as_ref()) == tag {
                    in_tag = true;
                }
            }
            Ok(Event::Text(t)) if in_tag => {
                let piece = t.as_ref();
                if !piece.is_empty() {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(piece);
                }
            }
            Ok(Event::End(e)) => {
                if local_name(e.name().as_ref()) == tag {
                    in_tag = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
        if out.len() >= MAX_CONTENT_BYTES {
            break;
        }
    }
    out
}

fn attr_values(xml: &str, element: &str, attribute: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut out = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e) | Event::Empty(e)) => {
                if local_name(e.name().as_ref()) == element {
                    let value = attr(&e, attribute);
                    if !value.is_empty() {
                        if !out.is_empty() {
                            out.push('\n');
                        }
                        out.push_str(&value);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

fn attr(e: &quick_xml::events::BytesStart<'_>, key: &str) -> String {
    e.try_get_attribute(key)
        .ok()
        .flatten()
        .map(|a| a.value.into_owned())
        .unwrap_or_default()
}

fn local_name(name: &str) -> String {
    name.rsplit(':').next().unwrap_or(name).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn zip_bytes(files: &[(&str, &str)]) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut cursor);
            let opts = SimpleFileOptions::default();
            for (name, body) in files {
                zip.start_file(*name, opts).unwrap();
                zip.write_all(body.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        cursor.into_inner()
    }

    #[test]
    fn xlsx_reads_shared_strings_numbers_and_sheet_names() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Sales"/></sheets></workbook>"#,
            ),
            (
                "xl/sharedStrings.xml",
                r#"<sst><si><t>Revenue</t></si><si><t>Q1</t></si></sst>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row>
                    <c t="s"><v>0</v></c>
                    <c t="s"><v>1</v></c>
                    <c><v>1500</v></c>
                    <c t="inlineStr"><is><t>Inline</t></is></c>
                </row></sheetData></worksheet>"#,
            ),
        ]);
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let text = extract_xlsx_archive(&mut archive, "book.xlsx").unwrap();
        assert!(text.contains("Sales"));
        assert!(text.contains("Revenue"));
        assert!(text.contains("Q1"));
        assert!(text.contains("1500"));
        assert!(text.contains("Inline"));
    }

    #[test]
    fn pptx_reads_slide_and_notes_text() {
        let bytes = zip_bytes(&[
            (
                "ppt/slides/slide1.xml",
                r#"<p:sld><a:t>Quarterly</a:t><a:t>review</a:t></p:sld>"#,
            ),
            (
                "ppt/notesSlides/notesSlide1.xml",
                r#"<p:notes><a:t>Speaker note</a:t></p:notes>"#,
            ),
        ]);
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let text = extract_pptx_archive(&mut archive, "deck.pptx").unwrap();
        assert!(text.contains("Quarterly"));
        assert!(text.contains("review"));
        assert!(text.contains("Speaker note"));
    }
}
