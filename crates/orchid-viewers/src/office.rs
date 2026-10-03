//! Read-only preview for Excel and PowerPoint packages.
//!
//! Spreadsheets become a sheet and cell table. Presentations become one
//! HTML card per slide. The workbook is not edited.

use std::any::Any;
use std::io::{Cursor, Read, Seek};
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use zip::ZipArchive;

use crate::error::{Result, ViewerError};
use crate::snapshot::{HtmlSnapshot, SheetCell, SheetPage, SheetSnapshot, ViewerSnapshot};
use crate::viewer_trait::Viewer;

const MAX_ROWS: usize = 400;
const MAX_COLS: usize = 32;

/// Spreadsheet and presentation preview.
#[derive(Debug, Default)]
pub struct OfficeViewer {
    path: RwLock<Option<orchid_fs::FsPath>>,
    html: RwLock<Arc<str>>,
    info: RwLock<String>,
    sheets: RwLock<Vec<SheetPage>>,
    /// `true` for a slide deck. Workbooks use [`Self::sheets`].
    slides: RwLock<bool>,
}

impl OfficeViewer {
    /// Empty preview.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Viewer for OfficeViewer {
    fn type_id(&self) -> &'static str {
        "office"
    }

    async fn open(
        &mut self,
        path: orchid_fs::FsPath,
        registry: Arc<orchid_fs::FsProviderRegistry>,
    ) -> Result<()> {
        let provider = registry
            .for_path(&path)
            .ok_or_else(|| orchid_fs::FsError::ProviderNotFound(path.scheme().to_string()))?;
        let bytes = provider.read(&path).await?;
        let slides = is_slide_path(&path);
        let preview = tokio::task::spawn_blocking(move || render_office(&bytes, slides))
            .await
            .map_err(|err| ViewerError::DocumentParse(err.to_string()))?
            .map_err(ViewerError::DocumentParse)?;
        match preview {
            OfficePreview::Slides(preview) => {
                *self.html.write() = Arc::from(preview.html);
                *self.info.write() = preview.info;
                *self.sheets.write() = Vec::new();
                *self.slides.write() = true;
            }
            OfficePreview::Sheets(book) => {
                *self.html.write() = Arc::from("");
                *self.info.write() = book.info;
                *self.sheets.write() = book.sheets;
                *self.slides.write() = false;
            }
        }
        *self.path.write() = Some(path);
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        *self.path.write() = None;
        *self.html.write() = Arc::from("");
        *self.info.write() = String::new();
        *self.sheets.write() = Vec::new();
        *self.slides.write() = false;
        Ok(())
    }

    fn snapshot(&self) -> ViewerSnapshot {
        let path_display = self
            .path
            .read()
            .as_ref()
            .map(|path| path.as_str().to_string())
            .unwrap_or_default();
        if *self.slides.read() {
            ViewerSnapshot::Html(HtmlSnapshot {
                path_display,
                source_preview: Arc::clone(&self.html.read()),
                local_path: None,
                info_text: self.info.read().clone(),
            })
        } else {
            ViewerSnapshot::Sheet(SheetSnapshot {
                path_display,
                info_text: self.info.read().clone(),
                sheets: self.sheets.read().clone(),
            })
        }
    }

    fn current_path(&self) -> Option<&orchid_fs::FsPath> {
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

pub(crate) struct Preview {
    html: String,
    info: String,
}

struct SheetBook {
    sheets: Vec<SheetPage>,
    info: String,
}

enum OfficePreview {
    Slides(Preview),
    Sheets(SheetBook),
}

pub(crate) fn render_office(
    bytes: &[u8],
    slides: bool,
) -> std::result::Result<OfficePreview, String> {
    let cursor = Cursor::new(bytes.to_vec());
    let mut archive = ZipArchive::new(cursor).map_err(|err| format!("zip: {err}"))?;
    if slides {
        render_slides(&mut archive).map(OfficePreview::Slides)
    } else {
        render_sheets(&mut archive).map(OfficePreview::Sheets)
    }
}

fn render_sheets<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> std::result::Result<SheetBook, String> {
    let shared = read_entry(archive, "xl/sharedStrings.xml")
        .map(|xml| shared_strings(&xml))
        .unwrap_or_default();
    let sheets = sheet_entries(archive);
    let mut pages = Vec::new();
    for (name, path) in &sheets {
        let Some(xml) = read_entry(archive, path) else {
            continue;
        };
        let (rows, truncated) = parse_sheet(&xml, &shared);
        if rows.is_empty() {
            continue;
        }
        pages.push(SheetPage {
            name: name.clone(),
            rows,
            truncated,
        });
    }
    let shown = pages.len();
    Ok(SheetBook {
        sheets: pages,
        info: format!("{shown} sheets"),
    })
}

fn render_slides<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> std::result::Result<Preview, String> {
    let mut slides = slide_names(archive);
    slides.sort_by_key(|name| slide_number(name));
    let mut body = String::new();
    let mut shown = 0usize;
    for name in &slides {
        let Some(xml) = read_entry(archive, name) else {
            continue;
        };
        shown += 1;
        let paragraphs = text_paragraphs(&xml);
        body.push_str("<section class=\"slide\"><h2>Slide ");
        body.push_str(&shown.to_string());
        body.push_str("</h2>");
        if paragraphs.is_empty() {
            body.push_str("<p></p>");
        }
        for paragraph in &paragraphs {
            body.push_str("<p>");
            body.push_str(&escape(paragraph));
            body.push_str("</p>");
        }
        let notes_path = notes_for(name);
        if let Some(notes) = read_entry(archive, &notes_path) {
            let notes = text_paragraphs(&notes);
            if !notes.is_empty() {
                body.push_str("<p class=\"notes\">");
                body.push_str(&escape(&notes.join(" ")));
                body.push_str("</p>");
            }
        }
        body.push_str("</section>");
    }
    if shown == 0 {
        body.push_str("<p>This presentation has no slides.</p>");
    }
    Ok(Preview {
        html: page("Slides", &body),
        info: format!("{shown} slides"),
    })
}

fn page(title: &str, body: &str) -> String {
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>{title}</title><style>\
body{{font:14px/1.45 'Segoe UI',sans-serif;margin:24px;color:#1a1a1a;background:#f4f4f4}}\
h2{{font-size:15px;margin:20px 0 8px}}\
table{{border-collapse:collapse;background:#fff;margin-bottom:8px}}\
td{{border:1px solid #ccc;padding:4px 8px;vertical-align:top}}\
.slide{{background:#fff;border:1px solid #ccc;border-radius:8px;padding:16px 20px;margin:12px 0}}\
.notes{{color:#555;font-size:13px}}\
</style></head><body>{body}</body></html>"
    )
}

fn sheet_entries<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Vec<(String, String)> {
    let book = read_entry(archive, "xl/workbook.xml").unwrap_or_default();
    let rels = read_entry(archive, "xl/_rels/workbook.xml.rels").unwrap_or_default();
    let mut out = Vec::new();
    for (name, id) in sheet_refs(&book) {
        let target = rel_target(&rels, &id);
        let path = if target.is_empty() {
            String::new()
        } else {
            normalize_part(&target)
        };
        if !path.is_empty() {
            out.push((name, path));
        }
    }
    if out.is_empty() {
        for name in zip_names(archive) {
            let lower = name.to_ascii_lowercase();
            if lower.starts_with("xl/worksheets/sheet") && lower.ends_with(".xml") {
                out.push((name.clone(), name));
            }
        }
        out.sort_by(|left, right| left.1.cmp(&right.1));
    }
    out
}

fn sheet_refs(xml: &str) -> Vec<(String, String)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut out = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event) | Event::Empty(event)) => {
                if local_name(event.name().as_ref()) == "sheet" {
                    let name = attr(&event, "name");
                    let id = attr(&event, "r:id");
                    let id = if id.is_empty() {
                        attr_local(&event, "id")
                    } else {
                        id
                    };
                    if !name.is_empty() && !id.is_empty() {
                        out.push((name, id));
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

fn rel_target(xml: &str, id: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event) | Event::Empty(event)) => {
                if local_name(event.name().as_ref()) == "Relationship" && attr(&event, "Id") == id {
                    return attr(&event, "Target");
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    String::new()
}

fn normalize_part(target: &str) -> String {
    let target = target.trim_start_matches('/');
    if target.starts_with("xl/") {
        target.to_string()
    } else {
        format!("xl/{target}")
    }
}

fn parse_sheet(xml: &str, shared: &[String]) -> (Vec<Vec<SheetCell>>, bool) {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut kind = String::new();
    let mut cell_ref = String::new();
    let mut value = String::new();
    let mut in_v = false;
    let mut in_t = false;
    let mut truncated = false;
    loop {
        if rows.len() >= MAX_ROWS {
            truncated = true;
            break;
        }
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "c" {
                    kind = attr(&event, "t");
                    cell_ref = attr(&event, "r");
                    value.clear();
                } else if name == "v" || name == "t" {
                    if name == "v" {
                        in_v = true;
                    } else {
                        in_t = true;
                    }
                    value.clear();
                }
            }
            Ok(Event::Text(text)) => {
                if in_v || in_t {
                    value.push_str(text.as_ref());
                }
            }
            Ok(Event::End(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "v" || name == "t" {
                    in_v = false;
                    in_t = false;
                } else if name == "c" {
                    if let Some(text) = cell_text(&kind, &value, shared) {
                        let col = col_index(&cell_ref);
                        if col >= MAX_COLS {
                            truncated = true;
                        } else {
                            place(&mut row, col, text, cell_ref.clone());
                        }
                    }
                    kind.clear();
                    value.clear();
                } else if name == "row" {
                    fill_gap_addresses(&mut row);
                    if row.iter().any(|cell| !cell.text.is_empty()) {
                        rows.push(std::mem::take(&mut row));
                    } else {
                        row.clear();
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    (rows, truncated)
}

fn cell_text(kind: &str, value: &str, shared: &[String]) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if kind == "s" {
        return value
            .parse::<usize>()
            .ok()
            .and_then(|index| shared.get(index))
            .cloned()
            .filter(|text| !text.is_empty());
    }
    if kind == "b" {
        return Some(if value == "1" {
            "TRUE".to_string()
        } else {
            "FALSE".to_string()
        });
    }
    Some(value.to_string())
}

fn place(row: &mut Vec<SheetCell>, col: usize, text: String, address: String) {
    if row.len() <= col {
        row.resize(col + 1, SheetCell::default());
    }
    row[col] = SheetCell { text, address };
}

fn fill_gap_addresses(row: &mut [SheetCell]) {
    let Some(row_num) = row.iter().find_map(|cell| trailing_number(&cell.address)) else {
        return;
    };
    for (col, cell) in row.iter_mut().enumerate() {
        if cell.address.is_empty() {
            cell.address = format!("{}{row_num}", col_letters(col));
        }
    }
}

fn trailing_number(address: &str) -> Option<usize> {
    let digits: String = address
        .chars()
        .skip_while(|ch| ch.is_ascii_alphabetic())
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn col_letters(index: usize) -> String {
    let mut n = index + 1;
    let mut out = Vec::new();
    while n > 0 {
        n -= 1;
        out.push(b'A' + (n % 26) as u8);
        n /= 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

fn col_index(cell_ref: &str) -> usize {
    let mut index = 0usize;
    let mut seen = false;
    for ch in cell_ref.chars() {
        if !ch.is_ascii_alphabetic() {
            break;
        }
        seen = true;
        index = index * 26 + (ch.to_ascii_uppercase() as usize - 'A' as usize + 1);
    }
    if seen {
        index.saturating_sub(1)
    } else {
        0
    }
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
            Ok(Event::Start(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "si" {
                    in_si = true;
                    current.clear();
                } else if in_si && name == "t" {
                    in_t = true;
                }
            }
            Ok(Event::Text(text)) if in_t => {
                current.push_str(text.as_ref());
            }
            Ok(Event::End(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "t" {
                    in_t = false;
                } else if name == "si" {
                    strings.push(std::mem::take(&mut current));
                    in_si = false;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    strings
}

fn text_paragraphs(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut paragraphs = Vec::new();
    let mut current = String::new();
    let mut in_t = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event)) => {
                if local_name(event.name().as_ref()) == "t" {
                    in_t = true;
                }
            }
            Ok(Event::Text(text)) if in_t => {
                current.push_str(text.as_ref());
            }
            Ok(Event::End(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "t" {
                    in_t = false;
                } else if name == "p" {
                    let paragraph = current.trim().to_string();
                    current.clear();
                    if !paragraph.is_empty() {
                        paragraphs.push(paragraph);
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    paragraphs
}

fn slide_names<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Vec<String> {
    zip_names(archive)
        .into_iter()
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            lower.starts_with("ppt/slides/slide") && lower.ends_with(".xml")
        })
        .collect()
}

fn slide_number(name: &str) -> u32 {
    let stem = name.rsplit('/').next().unwrap_or(name);
    let digits: String = stem.chars().filter(|ch| ch.is_ascii_digit()).collect();
    digits.parse().unwrap_or(0)
}

fn notes_for(slide_path: &str) -> String {
    format!("ppt/notesSlides/notesSlide{}.xml", slide_number(slide_path))
}

fn zip_names<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Vec<String> {
    (0..archive.len())
        .filter_map(|index| {
            archive
                .by_index(index)
                .ok()
                .map(|file| file.name().to_string())
        })
        .collect()
}

fn read_entry<R: Read + Seek>(archive: &mut ZipArchive<R>, name: &str) -> Option<String> {
    let mut entry = archive.by_name(name).ok()?;
    let mut xml = String::new();
    entry.read_to_string(&mut xml).ok()?;
    Some(xml)
}

fn attr(event: &quick_xml::events::BytesStart<'_>, key: &str) -> String {
    event
        .try_get_attribute(key)
        .ok()
        .flatten()
        .map(|attribute| attribute.value.into_owned())
        .unwrap_or_default()
}

fn attr_local(event: &quick_xml::events::BytesStart<'_>, key: &str) -> String {
    for attribute in event.attributes().flatten() {
        if local_name(attribute.key.as_ref()) == key {
            return attribute.value.into_owned();
        }
    }
    String::new()
}

fn local_name(name: &str) -> String {
    name.rsplit([':', '}']).next().unwrap_or(name).to_string()
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

fn is_slide_path(path: &orchid_fs::FsPath) -> bool {
    matches!(
        path.extension()
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("pptx") | Some("pptm") | Some("ppsx")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
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
    fn workbook_table_keeps_sheet_name_and_cells() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/sharedStrings.xml",
                r#"<sst><si><t>Orchid</t></si></sst>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1"><v>42</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let preview = render_office(&bytes, false).unwrap();
        let OfficePreview::Sheets(book) = preview else {
            panic!("workbook should be a sheet table");
        };
        assert_eq!(book.sheets.len(), 1, "{}", book.info);
        assert_eq!(book.sheets[0].name, "Budgets");
        let row = &book.sheets[0].rows[0];
        assert_eq!(row[0].text, "Orchid");
        assert_eq!(row[0].address, "A1");
        assert_eq!(row[1].text, "42");
        assert_eq!(row[1].address, "B1");
        assert!(book.info.contains("1 sheets"), "{}", book.info);
    }

    #[test]
    fn slides_html_keeps_order_and_notes() {
        let bytes = zip_bytes(&[
            (
                "ppt/slides/slide2.xml",
                r#"<p:sld><p:cSld><p:spTree><a:p><a:t>Second</a:t></a:p></p:spTree></p:cSld></p:sld>"#,
            ),
            (
                "ppt/slides/slide1.xml",
                r#"<p:sld><p:cSld><p:spTree><a:p><a:t>Hello orchid</a:t></a:p></p:spTree></p:cSld></p:sld>"#,
            ),
            (
                "ppt/notesSlides/notesSlide1.xml",
                r#"<p:notes><a:p><a:t>Speaker note</a:t></a:p></p:notes>"#,
            ),
        ]);
        let preview = render_office(&bytes, true).unwrap();
        let OfficePreview::Slides(preview) = preview else {
            panic!("slides should stay an HTML preview");
        };
        let hello = preview.html.find("Hello orchid").unwrap();
        let second = preview.html.find("Second").unwrap();
        assert!(hello < second, "{}", preview.html);
        assert!(preview.html.contains("Speaker note"), "{}", preview.html);
        assert!(preview.info.contains("2 slides"), "{}", preview.info);
    }
}
