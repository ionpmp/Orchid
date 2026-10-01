//! EPUB and OpenDocument text extractors.
//!
//! EPUB chapters are read in spine order from the OPF package. OpenDocument
//! text, spreadsheets, and presentations all store readable runs in
//! `content.xml`.

use std::collections::HashMap;
use std::io::{Read, Seek};

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use zip::ZipArchive;

use crate::error::{Result, SearchError};
use crate::extractors::ooxml::extract_zip;
use crate::extractors::text::MAX_CONTENT_BYTES;
use crate::extractors::ContentExtractor;

/// Extract chapter text from `.epub` packages.
#[derive(Debug, Default, Clone, Copy)]
pub struct EpubExtractor;

/// Extract text from `.odt` / `.ods` / `.odp` packages.
#[derive(Debug, Default, Clone, Copy)]
pub struct OdfExtractor;

#[async_trait]
impl ContentExtractor for EpubExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| m == "application/epub+zip")
            || extension.is_some_and(|e| e.eq_ignore_ascii_case("epub"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        extract_zip(provider, path, extract_epub_archive).await
    }
}

#[async_trait]
impl ContentExtractor for OdfExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            m == "application/vnd.oasis.opendocument.text"
                || m == "application/vnd.oasis.opendocument.spreadsheet"
                || m == "application/vnd.oasis.opendocument.presentation"
        }) || extension.is_some_and(|e| {
            e.eq_ignore_ascii_case("odt")
                || e.eq_ignore_ascii_case("ods")
                || e.eq_ignore_ascii_case("odp")
        })
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        extract_zip(provider, path, extract_odf_archive).await
    }
}

fn extract_epub_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path_label: &str,
) -> Result<String> {
    let names = zip_names(archive);
    let opf_path = container_opf(archive).or_else(|| {
        names
            .iter()
            .find(|n| n.to_ascii_lowercase().ends_with(".opf"))
            .cloned()
    });
    let mut chapters = Vec::new();
    if let Some(opf) = opf_path.as_deref() {
        if let Some(xml) = read_entry(archive, opf) {
            chapters = spine_chapters(&xml, opf);
        }
    }
    if chapters.is_empty() {
        chapters = names.iter().filter(|n| is_html_name(n)).cloned().collect();
        chapters.sort();
    }
    let mut out = String::new();
    for chapter in chapters {
        if let Some(xml) = read_entry(archive, &chapter) {
            push_capped(&mut out, &html_text(&xml));
        }
        if out.len() >= MAX_CONTENT_BYTES {
            break;
        }
    }
    if out.trim().is_empty() {
        return Err(SearchError::Extraction {
            path: path_label.into(),
            reason: "no chapter text".into(),
        });
    }
    Ok(out)
}

fn extract_odf_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path_label: &str,
) -> Result<String> {
    let Some(xml) = read_entry(archive, "content.xml") else {
        return Err(SearchError::Extraction {
            path: path_label.into(),
            reason: "missing content.xml".into(),
        });
    };
    let text = odf_text(&xml);
    if text.trim().is_empty() {
        return Err(SearchError::Extraction {
            path: path_label.into(),
            reason: "no document text".into(),
        });
    }
    Ok(text)
}

fn container_opf<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Option<String> {
    let xml = read_entry(archive, "META-INF/container.xml")?;
    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e) | Event::Empty(e)) => {
                if local_name(e.name().as_ref()) == "rootfile" {
                    let path = attr(&e, "full-path");
                    if !path.is_empty() {
                        return Some(path);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    None
}

fn spine_chapters(opf: &str, opf_path: &str) -> Vec<String> {
    let mut reader = Reader::from_str(opf);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut manifest: HashMap<String, String> = HashMap::new();
    let mut spine = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e) | Event::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                if local == "item" {
                    let id = attr(&e, "id");
                    let href = attr(&e, "href");
                    let media = attr(&e, "media-type");
                    if !id.is_empty()
                        && !href.is_empty()
                        && (media.contains("html") || is_html_name(&href))
                    {
                        manifest.insert(id, join_zip_path(opf_path, &href));
                    }
                } else if local == "itemref" {
                    let idref = attr(&e, "idref");
                    if !idref.is_empty() {
                        spine.push(idref);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    spine
        .into_iter()
        .filter_map(|id| manifest.get(&id).cloned())
        .collect()
}

fn html_text(xml: &str) -> String {
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
                if skip > 0 || local == "script" || local == "style" {
                    skip += 1;
                } else if is_block(&local) {
                    push_break(&mut out);
                }
            }
            Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                if skip == 0 && (local == "br" || is_block(&local)) {
                    push_break(&mut out);
                }
            }
            Ok(Event::Text(t)) if skip == 0 => {
                push_words(&mut out, t.as_ref());
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                if skip > 0 && (local == "script" || local == "style" || skip > 1) {
                    skip -= 1;
                }
                if skip == 0 && is_block(&local) {
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

fn odf_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e) | Event::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                if matches!(local.as_str(), "p" | "h" | "table-row") {
                    push_break(&mut out);
                } else if local == "s" {
                    out.push(' ');
                } else if local == "tab" || local == "line-break" {
                    out.push(if local == "tab" { '\t' } else { '\n' });
                }
            }
            Ok(Event::Text(t)) => push_words(&mut out, t.as_ref()),
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

fn is_block(local: &str) -> bool {
    matches!(
        local,
        "p" | "div"
            | "li"
            | "tr"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "blockquote"
            | "section"
    )
}

fn is_html_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".xhtml") || lower.ends_with(".html") || lower.ends_with(".htm")
}

fn join_zip_path(base_file: &str, href: &str) -> String {
    let href = percent_decode(href);
    let href = href.split(['#', '?']).next().unwrap_or(href.as_str());
    let href = href.trim_start_matches('/');
    let joined = if let Some((dir, _)) = base_file.rsplit_once('/') {
        if href.is_empty() {
            dir.to_string()
        } else {
            format!("{dir}/{href}")
        }
    } else {
        href.to_string()
    };
    normalize_zip_path(&joined)
}

fn normalize_zip_path(path: &str) -> String {
    let mut stack = Vec::new();
    for part in path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            stack.pop();
            continue;
        }
        stack.push(part);
    }
    stack.join("/")
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(value) = u8::from_str_radix(hex, 16) {
                    out.push(value);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
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
    if !out.is_empty() && !out.ends_with('\n') && !out.ends_with(' ') && !out.ends_with('\t') {
        out.push(' ');
    }
    out.push_str(&words.join(" "));
}

fn push_capped(out: &mut String, piece: &str) {
    let piece = piece.trim();
    if piece.is_empty() || out.len() >= MAX_CONTENT_BYTES {
        return;
    }
    push_break(out);
    let room = MAX_CONTENT_BYTES.saturating_sub(out.len());
    let take = piece.chars().take(room).collect::<String>();
    out.push_str(&take);
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
    fn epub_reads_spine_order_and_skips_script() {
        let bytes = zip_bytes(&[
            (
                "META-INF/container.xml",
                r#"<container><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>"#,
            ),
            (
                "OEBPS/content.opf",
                r#"<package>
                  <manifest>
                    <item id="c2" href="c2.xhtml" media-type="application/xhtml+xml"/>
                    <item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>
                    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml"/>
                  </manifest>
                  <spine>
                    <itemref idref="c1"/>
                    <itemref idref="c2"/>
                  </spine>
                </package>"#,
            ),
            (
                "OEBPS/c1.xhtml",
                r#"<html><body><p>Alpha</p><script>secret()</script><p>chapter</p></body></html>"#,
            ),
            (
                "OEBPS/c2.xhtml",
                r#"<html><body><h1>Beta</h1></body></html>"#,
            ),
            (
                "OEBPS/nav.xhtml",
                r#"<html><body><p>Contents</p></body></html>"#,
            ),
        ]);
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let text = extract_epub_archive(&mut archive, "book.epub").unwrap();
        let alpha = text.find("Alpha").unwrap();
        let beta = text.find("Beta").unwrap();
        assert!(alpha < beta);
        assert!(text.contains("chapter"));
        assert!(!text.contains("secret"));
        assert!(!text.contains("Contents"));
    }

    #[test]
    fn odf_reads_paragraphs_and_cells() {
        let bytes = zip_bytes(&[(
            "content.xml",
            r#"<office:document-content>
                <office:body>
                  <text:h>Title</text:h>
                  <text:p>Hello <text:s/>there</text:p>
                  <table:table-row>
                    <table:table-cell><text:p>Revenue</text:p></table:table-cell>
                  </table:table-row>
                </office:body>
              </office:document-content>"#,
        )]);
        let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let text = extract_odf_archive(&mut archive, "note.odt").unwrap();
        assert!(text.contains("Title"));
        assert!(text.contains("Hello"));
        assert!(text.contains("there"));
        assert!(text.contains("Revenue"));
    }
}
