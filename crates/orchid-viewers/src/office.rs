//! Spreadsheet and presentation preview.
//!
//! Spreadsheets become a sheet and cell table. One stored cell can be
//! written back into the package. A formula cell is left unchanged. After a
//! value edit, the functions named in the spreadsheet section of
//! `docs/user/viewers.md` are recalculated on that sheet. Other formulas
//! keep their stored value. Presentations become one HTML card per slide.

use std::any::Any;
use std::io::{Cursor, Read, Seek, Write};
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
    sheets: RwLock<Arc<Vec<SheetPage>>>,
    /// `true` for a slide deck. Workbooks use [`Self::sheets`].
    slides: RwLock<bool>,
}

impl OfficeViewer {
    /// Empty preview.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace one stored cell and write the package back to `path`.
    ///
    /// A formula cell is refused. Drawings and the other zip parts are
    /// copied through. Nothing is recalculated. The in-memory table is
    /// refreshed only when this viewer still has the same path open.
    ///
    /// # Errors
    ///
    /// A slide deck, a missing sheet or cell, a formula cell, or a
    /// provider write failure.
    pub async fn edit_cell(
        &self,
        registry: Arc<orchid_fs::FsProviderRegistry>,
        sheet: &str,
        address: &str,
        text: &str,
    ) -> Result<()> {
        if *self.slides.read() {
            return Err(ViewerError::SheetEdit("viewer-sheet-not-workbook".into()));
        }
        let path = self
            .path
            .read()
            .clone()
            .ok_or_else(|| ViewerError::SheetEdit("viewer-sheet-not-workbook".into()))?;
        let provider = registry
            .for_path(&path)
            .ok_or_else(|| orchid_fs::FsError::ProviderNotFound(path.scheme().to_string()))?;
        let bytes = provider.read(&path).await?;
        let sheet = sheet.to_string();
        let address = address.to_string();
        let text = text.to_string();
        let saved =
            tokio::task::spawn_blocking(move || set_sheet_cell(&bytes, &sheet, &address, &text))
                .await
                .map_err(|err| ViewerError::SheetEdit(err.to_string()))?
                .map_err(ViewerError::SheetEdit)?;
        let tmp = orchid_fs::FsPath::new(format!("{}.orchid-save", path.as_str()))?;
        provider.write(&tmp, &saved).await?;
        provider.rename(&tmp, &path).await?;
        let preview = tokio::task::spawn_blocking(move || render_office(&saved, false))
            .await
            .map_err(|err| ViewerError::SheetEdit(err.to_string()))?
            .map_err(ViewerError::DocumentParse)?;
        if self.path.read().as_ref().map(orchid_fs::FsPath::as_str) != Some(path.as_str()) {
            return Ok(());
        }
        if let OfficePreview::Sheets(book) = preview {
            *self.html.write() = Arc::from("");
            *self.info.write() = book.info;
            *self.sheets.write() = Arc::new(book.sheets);
            *self.slides.write() = false;
        }
        Ok(())
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
                *self.sheets.write() = Arc::new(Vec::new());
                *self.slides.write() = true;
            }
            OfficePreview::Sheets(book) => {
                *self.html.write() = Arc::from("");
                *self.info.write() = book.info;
                *self.sheets.write() = Arc::new(book.sheets);
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
        *self.sheets.write() = Arc::new(Vec::new());
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
                sheets: Arc::clone(&self.sheets.read()),
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

/// Replace one cell's stored value and return the new package.
///
/// A formula cell is left unchanged. After a value edit, simple formulas on
/// that sheet (`+`, `-`, `*`, `/`, comparisons, parentheses, cell references,
/// `SUM`, `AVERAGE`, `MIN`, `MAX`, `COUNT`, `IF`, `ROUND`, `ABS`, `INT`,
/// `&`, `CONCAT`, `LEN`, `LEFT`, `RIGHT`, `MID`, `UPPER`, and `LOWER`) are
/// written back. A number stays in `<v>`. Text is written as an inline
/// string. Length and slices count Unicode scalar values. An unsupported
/// formula keeps its previous value. Other zip parts, including drawings,
/// are copied through.
pub(crate) fn set_sheet_cell(
    bytes: &[u8],
    sheet_name: &str,
    address: &str,
    text: &str,
) -> std::result::Result<Vec<u8>, String> {
    let address = address.trim().to_ascii_uppercase();
    if !is_cell_address(&address) {
        return Err("viewer-sheet-bad-address".into());
    }
    if text.len() > 32_768 {
        return Err("viewer-sheet-too-long".into());
    }
    let mut archive =
        ZipArchive::new(Cursor::new(bytes.to_vec())).map_err(|err| err.to_string())?;
    let sheets = sheet_entries(&mut archive);
    let path = sheets
        .iter()
        .find(|(name, _)| name == sheet_name)
        .map(|(_, path)| path.clone())
        .ok_or_else(|| "viewer-sheet-missing-sheet".to_string())?;
    let xml =
        read_entry(&mut archive, &path).ok_or_else(|| "viewer-sheet-unreadable".to_string())?;
    let shared =
        shared_strings(&read_entry(&mut archive, "xl/sharedStrings.xml").unwrap_or_default());
    let mut other_sheets = Vec::new();
    for (name, sheet_path) in &sheets {
        if sheet_path == &path {
            continue;
        }
        if let Some(body) = read_entry(&mut archive, sheet_path) {
            other_sheets.push((name.to_ascii_lowercase(), body));
        }
    }
    let xml = replace_cell_xml(&xml, &address, text)?;
    let mut foreign = std::collections::HashMap::new();
    foreign.insert(sheet_name.to_ascii_lowercase(), stored_sheet(&xml, &shared));
    for (name, body) in other_sheets {
        foreign.insert(name, stored_sheet(&body, &shared));
    }
    let workbook = read_entry(&mut archive, "xl/workbook.xml").unwrap_or_default();
    let date1904 = workbook_date1904(&workbook);
    let names = workbook_names(&workbook);
    let sheet_key = sheet_name.to_ascii_lowercase();
    let xml = recalculate_sheet(&xml, &shared, &foreign, &names, &sheet_key, date1904);
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut out = zip::ZipWriter::new(&mut cursor);
        let mut archive =
            ZipArchive::new(Cursor::new(bytes.to_vec())).map_err(|err| err.to_string())?;
        for index in 0..archive.len() {
            let file = archive.by_index(index).map_err(|err| err.to_string())?;
            let name = file.name().to_string();
            if name == path {
                out.start_file(name, zip::write::SimpleFileOptions::default())
                    .map_err(|err| err.to_string())?;
                out.write_all(xml.as_bytes())
                    .map_err(|err| err.to_string())?;
            } else {
                out.raw_copy_file(file).map_err(|err| err.to_string())?;
            }
        }
        out.finish().map_err(|err| err.to_string())?;
    }
    Ok(cursor.into_inner())
}

fn is_cell_address(address: &str) -> bool {
    let mut letters = 0usize;
    let mut digits = 0usize;
    for ch in address.chars() {
        if ch.is_ascii_alphabetic() {
            if digits > 0 || letters >= 3 {
                return false;
            }
            letters += 1;
        } else if ch.is_ascii_digit() {
            if letters == 0 || digits >= 7 {
                return false;
            }
            digits += 1;
        } else {
            return false;
        }
    }
    letters > 0 && digits > 0
}

fn replace_cell_xml(xml: &str, address: &str, text: &str) -> std::result::Result<String, String> {
    let (start, end) = find_cell(xml, address)?;
    let element = &xml[start..end];
    if element_has_formula(element) {
        return Err("viewer-sheet-formula".into());
    }
    let mut out = String::with_capacity(xml.len() + text.len());
    out.push_str(&xml[..start]);
    out.push_str(&cell_element(address, text));
    out.push_str(&xml[end..]);
    Ok(out)
}

fn find_cell(xml: &str, address: &str) -> std::result::Result<(usize, usize), String> {
    let bytes = xml.as_bytes();
    let mut index = 0usize;
    while index + 2 < bytes.len() {
        if bytes[index] == b'<' && bytes[index + 1] == b'c' {
            let boundary = bytes[index + 2];
            if matches!(boundary, b' ' | b'>' | b'/') {
                let Some(tag_end) = xml[index..].find('>') else {
                    break;
                };
                let open_end = index + tag_end + 1;
                let open = &xml[index..open_end];
                if cell_ref_matches(open, address) {
                    if open.ends_with("/>") {
                        return Ok((index, open_end));
                    }
                    let Some(close) = xml[open_end..].find("</c>") else {
                        return Err("viewer-sheet-broken".into());
                    };
                    return Ok((index, open_end + close + 4));
                }
                index = open_end;
                continue;
            }
        }
        index += 1;
    }
    Err("viewer-sheet-missing-cell".into())
}

fn cell_ref_matches(open_tag: &str, address: &str) -> bool {
    open_tag.contains(&format!("r=\"{address}\"")) || open_tag.contains(&format!("r='{address}'"))
}

fn element_has_formula(element: &str) -> bool {
    element.contains("<f>") || element.contains("<f ") || element.contains("<f/>")
}

struct SheetCellRef {
    address: String,
    formula: Option<String>,
    value: Option<f64>,
    text: Option<String>,
    value_span: Option<(usize, usize)>,
    insert_at: Option<usize>,
    open_span: (usize, usize),
    text_cell: bool,
}

struct Spill {
    values: Vec<f64>,
    columns: u32,
    all_or_nothing: bool,
}

fn try_spill(formula: &str, env: &mut CalcEnv<'_>) -> Option<Spill> {
    let mut parser = CalcParser {
        bytes: formula.as_bytes(),
        index: 0,
    };
    parser.skip();
    let word = parser.word()?;
    parser.skip();
    if parser.bytes.get(parser.index) != Some(&b'(') {
        return None;
    }
    let name = word.to_ascii_uppercase();
    if !matches!(
        name.as_str(),
        "FREQUENCY" | "LINEST" | "TREND" | "MODE.MULT"
    ) {
        return None;
    }
    parser.index += 1;
    let spill = match name.as_str() {
        "FREQUENCY" => parser.frequency_spill(env)?,
        "MODE.MULT" => parser.mode_mult_spill(env)?,
        "LINEST" => parser.linest_spill(env)?,
        "TREND" => parser.trend_spill(env)?,
        _ => return None,
    };
    parser.skip();
    if parser.index == parser.bytes.len() {
        Some(spill)
    } else {
        None
    }
}

fn place_spill(
    origin: &str,
    spill: &Spill,
    cells: &[SheetCellRef],
    taken: &std::collections::HashSet<String>,
) -> Option<Vec<(String, f64)>> {
    if spill.columns == 0 {
        return None;
    }
    let mut writes = Vec::new();
    for (step, value) in spill.values.iter().enumerate() {
        let row = step as u32 / spill.columns;
        let col = step as u32 % spill.columns;
        let address = if row == 0 && col == 0 {
            origin.to_string()
        } else {
            let Some(shifted) = shift_address(origin, row, col) else {
                if spill.all_or_nothing {
                    return None;
                }
                break;
            };
            shifted
        };
        if row > 0 || col > 0 {
            let blocked = taken.contains(&address)
                || cells
                    .iter()
                    .find(|cell| cell.address == address)
                    .is_none_or(|cell| cell.formula.is_some() || cell.text_cell);
            if blocked {
                if spill.all_or_nothing {
                    return None;
                }
                break;
            }
        }
        writes.push((address, *value));
    }
    if spill.all_or_nothing && writes.len() != spill.values.len() {
        None
    } else if writes.is_empty() {
        None
    } else {
        Some(writes)
    }
}

fn shift_address(address: &str, down: u32, right: u32) -> Option<String> {
    let (col, row) = split_address(address)?;
    let col = col.checked_add(right)?;
    let row = row.checked_add(down)?;
    if row == 0 || row > 9_999_999 {
        return None;
    }
    let name = column_name(col);
    if name.len() > 3 || name.is_empty() {
        return None;
    }
    Some(format!("{name}{row}"))
}

fn recalculate_sheet(
    xml: &str,
    shared: &[String],
    foreign: &std::collections::HashMap<String, ForeignSheet>,
    names: &std::collections::HashMap<String, DefinedRef>,
    sheet: &str,
    date1904: bool,
) -> String {
    let cells = sheet_cells(xml, shared);
    let mut literals = std::collections::HashMap::<String, f64>::new();
    let mut texts = std::collections::HashMap::<String, String>::new();
    let mut formulas = std::collections::HashMap::<String, String>::new();
    for cell in &cells {
        if let Some(formula) = &cell.formula {
            formulas.insert(cell.address.clone(), formula.clone());
        } else if cell.text_cell {
            if let Some(text) = &cell.text {
                texts.insert(cell.address.clone(), text.clone());
            }
        } else if let Some(value) = cell.value {
            literals.insert(cell.address.clone(), value);
        }
    }
    let mut edits = Vec::new();
    let mut spilled = std::collections::HashSet::<String>::new();
    for cell in &cells {
        let Some(formula) = &cell.formula else {
            continue;
        };
        if cell.text_cell {
            continue;
        }
        let mut visiting = std::collections::HashSet::new();
        {
            let mut spill_env = CalcEnv {
                formulas: &formulas,
                literals: &literals,
                texts: &texts,
                foreign,
                names,
                sheet,
                date1904,
                visiting: &mut visiting,
            };
            if let Some(spill) = try_spill(formula, &mut spill_env) {
                if let Some(writes) = place_spill(&cell.address, &spill, &cells, &spilled) {
                    for (address, number) in writes {
                        spilled.insert(address.clone());
                        let Some(target) = cells.iter().find(|item| item.address == address) else {
                            continue;
                        };
                        let rendered = format_calc(number);
                        if rendered.is_empty() {
                            continue;
                        }
                        if let Some((start, end)) = target.value_span {
                            edits.push((start, end, rendered));
                        } else if let Some(at) = target.insert_at {
                            edits.push((at, at, format!("<v>{rendered}</v>")));
                        }
                    }
                }
                continue;
            }
        }
        let Some(value) = eval_formula(
            formula,
            &formulas,
            &literals,
            &texts,
            foreign,
            names,
            sheet,
            date1904,
            &mut visiting,
        ) else {
            continue;
        };
        match value {
            CalcValue::Num(number) => {
                let rendered = format_calc(number);
                if rendered.is_empty() {
                    continue;
                }
                if let Some((start, end)) = cell.value_span {
                    edits.push((start, end, rendered));
                } else if let Some(at) = cell.insert_at {
                    edits.push((at, at, format!("<v>{rendered}</v>")));
                }
            }
            CalcValue::Text(text) => {
                let inline = format!("<is><t>{}</t></is>", escape(&text));
                let (open_start, open_end) = cell.open_span;
                let Some(open) = open_tag_inline(&xml[open_start..open_end]) else {
                    continue;
                };
                edits.push((open_start, open_end, open));
                if let Some((start, end)) = cell.value_span {
                    edits.push((start.saturating_sub(3), end + 4, inline));
                } else if let Some(at) = cell.insert_at {
                    edits.push((at, at, inline));
                }
            }
        }
    }
    edits.sort_by(|a, b| b.0.cmp(&a.0));
    let mut out = xml.to_string();
    for (start, end, text) in edits {
        out.replace_range(start..end, &text);
    }
    out
}

fn stored_sheet(xml: &str, shared: &[String]) -> ForeignSheet {
    let mut literals = std::collections::HashMap::new();
    let mut texts = std::collections::HashMap::new();
    for cell in sheet_cells(xml, shared) {
        if cell.formula.is_some() {
            if let Some(value) = cell.value {
                literals.insert(cell.address, value);
            }
            continue;
        }
        if cell.text_cell {
            if let Some(text) = cell.text {
                texts.insert(cell.address, text);
            }
        } else if let Some(value) = cell.value {
            literals.insert(cell.address, value);
        }
    }
    ForeignSheet { literals, texts }
}

fn sheet_cells(xml: &str, shared: &[String]) -> Vec<SheetCellRef> {
    let bytes = xml.as_bytes();
    let mut cells = Vec::new();
    let mut index = 0usize;
    while index + 2 < bytes.len() {
        if bytes[index] == b'<' && bytes[index + 1] == b'c' {
            let boundary = bytes[index + 2];
            if matches!(boundary, b' ' | b'>' | b'/') {
                let Some(tag_end) = xml[index..].find('>') else {
                    break;
                };
                let open_end = index + tag_end + 1;
                let open = &xml[index..open_end];
                let address = cell_address_from_tag(open);
                if open.ends_with("/>") {
                    index = open_end;
                    continue;
                }
                let Some(close) = xml[open_end..].find("</c>") else {
                    break;
                };
                let end = open_end + close + 4;
                if let Some(address) = address {
                    let body = &xml[open_end..open_end + close];
                    let formula = formula_text(body);
                    let value_span = value_span(xml, open_end, open_end + close);
                    let text_cell = open.contains("t=\"s\"")
                        || open.contains("t=\"str\"")
                        || open.contains("t=\"inlineStr\"");
                    let value = if text_cell {
                        None
                    } else {
                        value_span.and_then(|(start, end)| xml[start..end].parse::<f64>().ok())
                    };
                    let text = if open.contains("t=\"s\"") {
                        value_span.and_then(|(start, end)| {
                            let index = xml[start..end].trim().parse::<usize>().ok()?;
                            shared.get(index).cloned()
                        })
                    } else if text_cell {
                        inline_text(body)
                    } else {
                        None
                    };
                    cells.push(SheetCellRef {
                        address,
                        formula,
                        value,
                        text,
                        value_span,
                        insert_at: Some(open_end + close),
                        open_span: (index, open_end),
                        text_cell,
                    });
                }
                index = end;
                continue;
            }
        }
        index += 1;
    }
    cells
}

fn cell_address_from_tag(open: &str) -> Option<String> {
    for mark in ["r=\"", "r='"] {
        let Some(start) = open.find(mark) else {
            continue;
        };
        let rest = &open[start + mark.len()..];
        let end = rest.find(['"', '\'']).unwrap_or(rest.len());
        let address = rest[..end].trim().to_ascii_uppercase();
        if is_cell_address(&address) {
            return Some(address);
        }
    }
    None
}

fn formula_text(body: &str) -> Option<String> {
    let start = body.find("<f>")?;
    let rest = &body[start + 3..];
    let end = rest.find("</f>")?;
    let text = rest[..end].trim();
    let text = text.strip_prefix('=').unwrap_or(text).trim();
    if text.is_empty() {
        None
    } else {
        Some(unescape_xml(text))
    }
}

fn value_span(xml: &str, from: usize, to: usize) -> Option<(usize, usize)> {
    let body = &xml[from..to];
    let start = body.find("<v>")?;
    let rest = &body[start + 3..];
    let end = rest.find("</v>")?;
    Some((from + start + 3, from + start + 3 + end))
}

fn stdev_excel(args: &[f64], sample: bool) -> Option<f64> {
    if args.iter().any(|number| !number.is_finite()) {
        return None;
    }
    let count = args.len();
    if sample {
        if count < 2 {
            return None;
        }
    } else if count == 0 {
        return None;
    }
    let mean = args.iter().sum::<f64>() / count as f64;
    let squared = args
        .iter()
        .map(|number| {
            let delta = number - mean;
            delta * delta
        })
        .sum::<f64>();
    let denom = if sample {
        (count - 1) as f64
    } else {
        count as f64
    };
    let value = (squared / denom).sqrt();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn var_excel(args: &[f64], sample: bool) -> Option<f64> {
    if args.iter().any(|number| !number.is_finite()) {
        return None;
    }
    let count = args.len();
    if sample {
        if count < 2 {
            return None;
        }
    } else if count == 0 {
        return None;
    }
    let mean = args.iter().sum::<f64>() / count as f64;
    let squared = args
        .iter()
        .map(|number| {
            let delta = number - mean;
            delta * delta
        })
        .sum::<f64>();
    let denom = if sample {
        (count - 1) as f64
    } else {
        count as f64
    };
    let value = squared / denom;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn avedev_excel(args: &[f64]) -> Option<f64> {
    if args.is_empty() || args.iter().any(|number| !number.is_finite()) {
        return None;
    }
    let mean = args.iter().sum::<f64>() / args.len() as f64;
    let value = args.iter().map(|number| (number - mean).abs()).sum::<f64>() / args.len() as f64;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn devsq_excel(args: &[f64]) -> Option<f64> {
    if args.is_empty() || args.iter().any(|number| !number.is_finite()) {
        return None;
    }
    let mean = args.iter().sum::<f64>() / args.len() as f64;
    let value = args
        .iter()
        .map(|number| {
            let delta = number - mean;
            delta * delta
        })
        .sum::<f64>();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn geomean_excel(args: &[f64]) -> Option<f64> {
    if args.is_empty()
        || args
            .iter()
            .any(|number| !number.is_finite() || *number <= 0.0)
    {
        return None;
    }
    let sum_ln = args.iter().map(|number| number.ln()).sum::<f64>();
    let value = (sum_ln / args.len() as f64).exp();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn harmean_excel(args: &[f64]) -> Option<f64> {
    if args.is_empty()
        || args
            .iter()
            .any(|number| !number.is_finite() || *number <= 0.0)
    {
        return None;
    }
    let reciprocals = args.iter().map(|number| 1.0 / number).sum::<f64>();
    if !reciprocals.is_finite() || reciprocals == 0.0 {
        return None;
    }
    let value = args.len() as f64 / reciprocals;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn slope_excel(pairs: &[(f64, f64)]) -> Option<f64> {
    if pairs.len() < 2 {
        return None;
    }
    let count = pairs.len() as f64;
    let mean_y = pairs.iter().map(|(y, _)| *y).sum::<f64>() / count;
    let mean_x = pairs.iter().map(|(_, x)| *x).sum::<f64>() / count;
    let mut numerator = 0.0;
    let mut denominator = 0.0;
    for (y, x) in pairs {
        let dx = x - mean_x;
        let dy = y - mean_y;
        numerator += dx * dy;
        denominator += dx * dx;
    }
    if !numerator.is_finite() || !denominator.is_finite() || denominator == 0.0 {
        return None;
    }
    let value = numerator / denominator;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn intercept_excel(pairs: &[(f64, f64)]) -> Option<f64> {
    let slope = slope_excel(pairs)?;
    let count = pairs.len() as f64;
    let mean_y = pairs.iter().map(|(y, _)| *y).sum::<f64>() / count;
    let mean_x = pairs.iter().map(|(_, x)| *x).sum::<f64>() / count;
    let value = mean_y - slope * mean_x;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn linest_stats(pairs: &[(f64, f64)]) -> Option<Vec<f64>> {
    if pairs.len() < 3 || pairs.len() > 256 {
        return None;
    }
    let slope = slope_excel(pairs)?;
    let intercept = intercept_excel(pairs)?;
    let count = pairs.len() as f64;
    let mean_y = pairs.iter().map(|(y, _)| *y).sum::<f64>() / count;
    let mean_x = pairs.iter().map(|(_, x)| *x).sum::<f64>() / count;
    let mut ssx = 0.0;
    let mut ssresid = 0.0;
    let mut sstotal = 0.0;
    for (y, x) in pairs {
        let dx = x - mean_x;
        ssx += dx * dx;
        let error = y - (intercept + slope * x);
        ssresid += error * error;
        let dy = y - mean_y;
        sstotal += dy * dy;
    }
    let df = count - 2.0;
    if !ssx.is_finite() || !ssresid.is_finite() || !sstotal.is_finite() || ssx == 0.0 || df <= 0.0 {
        return None;
    }
    if ssresid.abs() <= 1e-9 * sstotal.max(1.0) || sstotal == 0.0 {
        return None;
    }
    let mut ssreg = sstotal - ssresid;
    if ssreg.abs() <= 1e-9 * sstotal.max(1.0) {
        ssreg = 0.0;
    }
    if ssreg < 0.0 {
        return None;
    }
    let sey = (ssresid / df).sqrt();
    let se_slope = sey / ssx.sqrt();
    let se_intercept = sey * (1.0 / count + mean_x * mean_x / ssx).sqrt();
    let r2 = 1.0 - ssresid / sstotal;
    let f_stat = ssreg * df / ssresid;
    let values = vec![
        slope,
        intercept,
        se_slope,
        se_intercept,
        r2,
        sey,
        f_stat,
        df,
        ssreg,
        ssresid,
    ];
    if values.iter().all(|value| value.is_finite()) {
        Some(values)
    } else {
        None
    }
}

fn correl_excel(pairs: &[(f64, f64)]) -> Option<f64> {
    if pairs.len() < 2 {
        return None;
    }
    let count = pairs.len() as f64;
    let mean_y = pairs.iter().map(|(y, _)| *y).sum::<f64>() / count;
    let mean_x = pairs.iter().map(|(_, x)| *x).sum::<f64>() / count;
    let mut numerator = 0.0;
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    for (y, x) in pairs {
        let dx = x - mean_x;
        let dy = y - mean_y;
        numerator += dx * dy;
        sum_x += dx * dx;
        sum_y += dy * dy;
    }
    if !numerator.is_finite()
        || !sum_x.is_finite()
        || !sum_y.is_finite()
        || sum_x == 0.0
        || sum_y == 0.0
    {
        return None;
    }
    let value = numerator / (sum_x * sum_y).sqrt();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn rsq_excel(pairs: &[(f64, f64)]) -> Option<f64> {
    let correl = correl_excel(pairs)?;
    let value = correl * correl;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn forecast_excel(x_value: f64, pairs: &[(f64, f64)]) -> Option<f64> {
    if !x_value.is_finite() {
        return None;
    }
    let slope = slope_excel(pairs)?;
    let intercept = intercept_excel(pairs)?;
    let value = intercept + slope * x_value;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn steyx_excel(pairs: &[(f64, f64)]) -> Option<f64> {
    if pairs.len() < 3 {
        return None;
    }
    let slope = slope_excel(pairs)?;
    let intercept = intercept_excel(pairs)?;
    let mut residual = 0.0;
    for (y, x) in pairs {
        let delta = y - (intercept + slope * x);
        residual += delta * delta;
    }
    let value = (residual / (pairs.len() - 2) as f64).sqrt();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn covariance_excel(pairs: &[(f64, f64)], sample: bool) -> Option<f64> {
    if pairs.len() < 2 {
        return None;
    }
    let count = pairs.len() as f64;
    let mean_y = pairs.iter().map(|(y, _)| *y).sum::<f64>() / count;
    let mean_x = pairs.iter().map(|(_, x)| *x).sum::<f64>() / count;
    let mut numerator = 0.0;
    for (y, x) in pairs {
        numerator += (y - mean_y) * (x - mean_x);
    }
    if !numerator.is_finite() {
        return None;
    }
    let denom = if sample {
        (pairs.len() - 1) as f64
    } else {
        count
    };
    let value = numerator / denom;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn rank_excel(number: f64, values: &[f64], ascending: bool) -> Option<f64> {
    if !number.is_finite() || !values.iter().any(|value| (value - number).abs() < 1e-9) {
        return None;
    }
    let ahead = values
        .iter()
        .filter(|value| {
            if ascending {
                **value + 1e-9 < number
            } else {
                **value > number + 1e-9
            }
        })
        .count();
    Some((ahead + 1) as f64)
}

fn rank_avg_excel(number: f64, values: &[f64], ascending: bool) -> Option<f64> {
    if !number.is_finite() {
        return None;
    }
    let mut ahead = 0.0;
    let mut ties = 0.0;
    for value in values {
        if (value - number).abs() < 1e-9 {
            ties += 1.0;
        } else if ascending {
            if *value + 1e-9 < number {
                ahead += 1.0;
            }
        } else if *value > number + 1e-9 {
            ahead += 1.0;
        }
    }
    if ties == 0.0 {
        return None;
    }
    let value: f64 = ahead + (ties + 1.0) / 2.0;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn percentile_inc_excel(values: &mut [f64], k: f64) -> Option<f64> {
    if values.is_empty() || !k.is_finite() || !(0.0..=1.0).contains(&k) {
        return None;
    }
    values.sort_by(|left, right| left.total_cmp(right));
    if values.len() == 1 {
        return Some(values[0]);
    }
    let index = k * (values.len() - 1) as f64;
    let lower = index.floor() as usize;
    let upper = index.ceil() as usize;
    if lower == upper || upper >= values.len() {
        return values.get(lower).copied();
    }
    let fraction = index - lower as f64;
    let value = values[lower] + fraction * (values[upper] - values[lower]);
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn percentile_exc_excel(values: &mut [f64], k: f64) -> Option<f64> {
    if values.is_empty() || !k.is_finite() || k <= 0.0 || k >= 1.0 {
        return None;
    }
    values.sort_by(|left, right| left.total_cmp(right));
    let count = values.len() as f64;
    let mut rank = k * (count + 1.0);
    if !rank.is_finite() {
        return None;
    }
    if (rank - 1.0).abs() < 1e-9 {
        rank = 1.0;
    } else if (rank - count).abs() < 1e-9 {
        rank = count;
    }
    if rank < 1.0 || rank > count {
        return None;
    }
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    if lower == 0 || upper > values.len() {
        return None;
    }
    if lower == upper {
        return Some(values[lower - 1]);
    }
    let fraction = rank - lower as f64;
    let value = values[lower - 1] + fraction * (values[upper - 1] - values[lower - 1]);
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn modes_excel(values: &[f64]) -> Option<Vec<f64>> {
    if values.is_empty() || values.len() > 256 {
        return None;
    }
    let mut best_count = 1usize;
    let mut modes = Vec::new();
    for (index, value) in values.iter().enumerate() {
        if values[..index]
            .iter()
            .any(|earlier| (earlier - value).abs() < 1e-9)
        {
            continue;
        }
        let count = values
            .iter()
            .filter(|other| (*other - value).abs() < 1e-9)
            .count();
        if count > best_count {
            best_count = count;
            modes.clear();
            modes.push(*value);
        } else if count == best_count && count > 1 {
            modes.push(*value);
        }
    }
    if modes.is_empty() || modes.len() > 16 {
        None
    } else {
        Some(modes)
    }
}

fn mode_excel(values: &[f64]) -> Option<f64> {
    let mut best_count = 1usize;
    let mut best = None;
    for value in values {
        let count = values
            .iter()
            .filter(|other| (**other - *value).abs() < 1e-9)
            .count();
        if count > best_count {
            best_count = count;
            best = Some(*value);
        }
    }
    best
}

fn percent_rank_inc(values: &mut [f64], x_value: f64) -> Option<f64> {
    if values.len() < 2 || !x_value.is_finite() {
        return None;
    }
    values.sort_by(|left, right| left.total_cmp(right));
    let last = values.len() - 1;
    if x_value < values[0] - 1e-9 || x_value > values[last] + 1e-9 {
        return None;
    }
    let span = last as f64;
    if let Some(index) = values
        .iter()
        .position(|value| (value - x_value).abs() < 1e-9)
    {
        return Some(index as f64 / span);
    }
    let upper = values.iter().position(|value| *value > x_value)?;
    if upper == 0 {
        return None;
    }
    let lower = upper - 1;
    let gap = values[upper] - values[lower];
    if gap == 0.0 {
        return None;
    }
    let fraction = (x_value - values[lower]) / gap;
    let value = (lower as f64 + fraction) / span;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn percent_rank_exc(values: &mut [f64], x_value: f64) -> Option<f64> {
    if values.is_empty() || !x_value.is_finite() {
        return None;
    }
    values.sort_by(|left, right| left.total_cmp(right));
    let last = values.len() - 1;
    if x_value < values[0] - 1e-9 || x_value > values[last] + 1e-9 {
        return None;
    }
    let span = (values.len() + 1) as f64;
    if let Some(index) = values
        .iter()
        .position(|value| (*value - x_value).abs() < 1e-9)
    {
        return Some((index as f64 + 1.0) / span);
    }
    let upper = values.iter().position(|value| *value > x_value)?;
    if upper == 0 {
        return None;
    }
    let lower = upper - 1;
    let gap = values[upper] - values[lower];
    if gap == 0.0 {
        return None;
    }
    let fraction = (x_value - values[lower]) / gap;
    let value = (lower as f64 + 1.0 + fraction) / span;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn standardize_excel(x_value: f64, mean: f64, scale: f64) -> Option<f64> {
    if !x_value.is_finite() || !mean.is_finite() || !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let value = (x_value - mean) / scale;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn skew_excel(args: &[f64], population: bool) -> Option<f64> {
    if args.len() < 3 || args.iter().any(|number| !number.is_finite()) {
        return None;
    }
    let count = args.len() as f64;
    let mean = args.iter().sum::<f64>() / count;
    let mut square = 0.0;
    for number in args {
        let delta = number - mean;
        square += delta * delta;
    }
    if !square.is_finite() {
        return None;
    }
    let denom = if population { count } else { count - 1.0 };
    let scale = (square / denom).sqrt();
    if scale == 0.0 || !scale.is_finite() {
        return None;
    }
    let mut cubes = 0.0;
    for number in args {
        let zed = (number - mean) / scale;
        cubes += zed * zed * zed;
    }
    if !cubes.is_finite() {
        return None;
    }
    let value = if population {
        cubes / count
    } else {
        cubes * count / ((count - 1.0) * (count - 2.0))
    };
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn kurt_excel(args: &[f64]) -> Option<f64> {
    if args.len() < 4 || args.iter().any(|number| !number.is_finite()) {
        return None;
    }
    let count = args.len() as f64;
    let mean = args.iter().sum::<f64>() / count;
    let mut square = 0.0;
    for number in args {
        let delta = number - mean;
        square += delta * delta;
    }
    if !square.is_finite() {
        return None;
    }
    let scale = (square / (count - 1.0)).sqrt();
    if scale == 0.0 || !scale.is_finite() {
        return None;
    }
    let mut fourth = 0.0;
    for number in args {
        let zed = (number - mean) / scale;
        let square_zed = zed * zed;
        fourth += square_zed * square_zed;
    }
    if !fourth.is_finite() {
        return None;
    }
    let lead = count * (count + 1.0) / ((count - 1.0) * (count - 2.0) * (count - 3.0));
    let adjust = 3.0 * (count - 1.0) * (count - 1.0) / ((count - 2.0) * (count - 3.0));
    let value = lead * fourth - adjust;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn trimmean_excel(values: &mut [f64], percent: f64) -> Option<f64> {
    if values.is_empty()
        || !percent.is_finite()
        || percent < 0.0
        || percent >= 1.0
        || values.iter().any(|number| !number.is_finite())
    {
        return None;
    }
    values.sort_by(|left, right| left.total_cmp(right));
    let drop = (percent * values.len() as f64 / 2.0).floor() as usize;
    if drop.saturating_mul(2) >= values.len() {
        return None;
    }
    let end = values.len() - drop;
    let sum = values[drop..end].iter().sum::<f64>();
    let value = sum / (end - drop) as f64;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn fisher_excel(number: f64) -> Option<f64> {
    if !number.is_finite() || number <= -1.0 || number >= 1.0 {
        return None;
    }
    let value = number.atanh();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn fisher_inv_excel(number: f64) -> Option<f64> {
    if !number.is_finite() {
        return None;
    }
    let value = number.tanh();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn sqrt_pi_excel(number: f64) -> Option<f64> {
    if !number.is_finite() || number < 0.0 {
        return None;
    }
    let value = (number * std::f64::consts::PI).sqrt();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn sum_pair_excel(pairs: &[(f64, f64)], kind: u8) -> Option<f64> {
    let mut acc = 0.0;
    for (left, right) in pairs {
        let term = if kind == 0 {
            left * left - right * right
        } else if kind == 1 {
            left * left + right * right
        } else {
            let delta = left - right;
            delta * delta
        };
        if !term.is_finite() {
            return None;
        }
        acc += term;
        if !acc.is_finite() {
            return None;
        }
    }
    Some(acc)
}

fn gestep_excel(number: f64, step: f64) -> Option<f64> {
    if !number.is_finite() || !step.is_finite() {
        return None;
    }
    Some(if number >= step { 1.0 } else { 0.0 })
}

fn delta_excel(left: f64, right: f64) -> Option<f64> {
    if !left.is_finite() || !right.is_finite() {
        return None;
    }
    Some(if left == right { 1.0 } else { 0.0 })
}

fn multinomial_excel(args: &[f64]) -> Option<f64> {
    if args.is_empty()
        || args
            .iter()
            .any(|number| !number.is_finite() || *number < 0.0 || *number >= 1_000_000.0)
    {
        return None;
    }
    let mut whole = Vec::with_capacity(args.len());
    let mut total = 0u64;
    for number in args {
        let value = number.trunc() as u64;
        total = total.checked_add(value)?;
        if total >= 1_000_000 {
            return None;
        }
        whole.push(value);
    }
    let mut acc = 1.0;
    let mut cursor = 0u64;
    for count in whole {
        for index in 1..=count {
            cursor += 1;
            acc *= cursor as f64;
            acc /= index as f64;
            if !acc.is_finite() {
                return None;
            }
        }
    }
    if acc < 1e15 {
        Some(acc.round())
    } else {
        Some(acc)
    }
}

fn format_calc(value: f64) -> String {
    if !value.is_finite() {
        return String::new();
    }
    if (value - value.round()).abs() < 1e-9 && value.abs() < 1e15 {
        return format!("{}", value.round() as i64);
    }
    let text = format!("{value:.8}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn eval_formula(
    formula: &str,
    formulas: &std::collections::HashMap<String, String>,
    literals: &std::collections::HashMap<String, f64>,
    texts: &std::collections::HashMap<String, String>,
    foreign: &std::collections::HashMap<String, ForeignSheet>,
    names: &std::collections::HashMap<String, DefinedRef>,
    sheet: &str,
    date1904: bool,
    visiting: &mut std::collections::HashSet<String>,
) -> Option<CalcValue> {
    let mut parser = CalcParser {
        bytes: formula.as_bytes(),
        index: 0,
    };
    let mut env = CalcEnv {
        formulas,
        literals,
        texts,
        foreign,
        names,
        sheet,
        date1904,
        visiting,
    };
    let value = parser.compare(&mut env)?;
    parser.skip();
    if parser.index == parser.bytes.len() {
        Some(value)
    } else {
        None
    }
}

fn kept_calc(value: CalcValue) -> Option<CalcValue> {
    match value {
        CalcValue::Num(number) if number.is_finite() => Some(CalcValue::Num(number)),
        CalcValue::Num(_) => None,
        text @ CalcValue::Text(_) => Some(text),
    }
}

fn values_match(left: &CalcValue, right: &CalcValue) -> bool {
    match (left, right) {
        (CalcValue::Num(left), CalcValue::Num(right)) => {
            left.is_finite() && right.is_finite() && (left - right).abs() < 1e-9
        }
        (CalcValue::Text(left), CalcValue::Text(right)) => left == right,
        _ => false,
    }
}

fn eval_slice(bytes: &[u8], env: &mut CalcEnv<'_>) -> Option<CalcValue> {
    let mut parser = CalcParser { bytes, index: 0 };
    let value = parser.compare(env)?;
    parser.skip();
    if parser.index == parser.bytes.len() {
        kept_calc(value)
    } else {
        None
    }
}

/// Splits the arguments of the call whose opening `(` is already consumed.
/// Returns each argument's byte range and the index just past the closing `)`.
fn split_top_args(bytes: &[u8], start: usize) -> Option<(Vec<(usize, usize)>, usize)> {
    let mut index = start;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut arg_start = start;
    let mut args = Vec::new();
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            if byte == b'"' {
                if bytes.get(index + 1) == Some(&b'"') {
                    index += 2;
                    continue;
                }
                in_string = false;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'(' => depth += 1,
            b')' if depth == 0 => {
                args.push((arg_start, index));
                return Some((args, index + 1));
            }
            b')' => depth -= 1,
            b',' if depth == 0 => {
                args.push((arg_start, index));
                index += 1;
                arg_start = index;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    None
}

enum CalcValue {
    Num(f64),
    Text(String),
}

struct CalcEnv<'a> {
    formulas: &'a std::collections::HashMap<String, String>,
    literals: &'a std::collections::HashMap<String, f64>,
    texts: &'a std::collections::HashMap<String, String>,
    foreign: &'a std::collections::HashMap<String, ForeignSheet>,
    names: &'a std::collections::HashMap<String, DefinedRef>,
    sheet: &'a str,
    date1904: bool,
    visiting: &'a mut std::collections::HashSet<String>,
}

#[derive(Clone)]
struct DefinedRef {
    sheet: String,
    cells: Vec<String>,
    rows: u32,
    cols: u32,
}

enum NameArg {
    Absent,
    Cells(Vec<String>),
    Invalid,
}

struct ForeignSheet {
    literals: std::collections::HashMap<String, f64>,
    texts: std::collections::HashMap<String, String>,
}

fn calc_num(value: CalcValue) -> Option<f64> {
    match value {
        CalcValue::Num(number) => Some(number),
        CalcValue::Text(_) => None,
    }
}

fn calc_text(value: &CalcValue) -> String {
    match value {
        CalcValue::Num(number) => format_calc(*number),
        CalcValue::Text(text) => text.clone(),
    }
}

fn round_excel(value: f64, digits: f64) -> Option<f64> {
    if !value.is_finite() || !digits.is_finite() {
        return None;
    }
    let places = digits.trunc();
    if places < -10.0 || places > 10.0 {
        return None;
    }
    let scale = 10f64.powi(places as i32);
    if !scale.is_finite() {
        return None;
    }
    let scaled = value * scale;
    if !scaled.is_finite() {
        return None;
    }
    Some(scaled.round() / scale)
}

fn math_step_excel(number: f64, significance: f64, mode: f64, floor: bool) -> Option<f64> {
    if !number.is_finite() || !significance.is_finite() || !mode.is_finite() {
        return None;
    }
    if significance == 0.0 {
        return Some(0.0);
    }
    let step = significance.abs();
    if !step.is_finite() {
        return None;
    }
    let quotient = number / step;
    if !quotient.is_finite() {
        return None;
    }
    let away = mode != 0.0 && number < 0.0;
    let scaled = if floor == away {
        quotient.ceil()
    } else {
        quotient.floor()
    };
    let value = scaled * step;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn finite_positive_log(number: f64) -> Option<f64> {
    if number.is_finite() && number > 0.0 {
        let value = number.ln();
        if value.is_finite() {
            Some(value)
        } else {
            None
        }
    } else {
        None
    }
}

fn log_base(number: f64, base: f64) -> Option<f64> {
    if !base.is_finite() || base <= 0.0 || base == 1.0 {
        return None;
    }
    let value = finite_positive_log(number)? / base.ln();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn exp_excel(number: f64) -> Option<f64> {
    if !number.is_finite() {
        return None;
    }
    let value = number.exp();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn fact_excel(number: f64) -> Option<f64> {
    if !number.is_finite() || number < 0.0 || number >= 171.0 {
        return None;
    }
    let count = number.trunc() as u32;
    let mut acc = 1.0;
    for step in 2..=count {
        acc *= f64::from(step);
        if !acc.is_finite() {
            return None;
        }
    }
    Some(acc)
}

fn fact_double_excel(number: f64) -> Option<f64> {
    if !number.is_finite() || number < 0.0 || number >= 301.0 {
        return None;
    }
    let mut count = number.trunc() as u32;
    let mut acc = 1.0;
    while count > 1 {
        acc *= f64::from(count);
        if !acc.is_finite() {
            return None;
        }
        count -= 2;
    }
    Some(acc)
}

fn poisson_excel(x_value: f64, mean: f64, cumulative: bool) -> Option<f64> {
    if !x_value.is_finite()
        || !mean.is_finite()
        || x_value < 0.0
        || mean < 0.0
        || x_value >= 171.0
        || mean >= 700.0
    {
        return None;
    }
    let count = x_value.trunc() as u32;
    let mut term = (-mean).exp();
    if !term.is_finite() {
        return None;
    }
    if !cumulative {
        for step in 1..=count {
            term *= mean / f64::from(step);
            if !term.is_finite() {
                return None;
            }
        }
        return Some(term);
    }
    let mut sum = term;
    for step in 1..=count {
        term *= mean / f64::from(step);
        if !term.is_finite() {
            return None;
        }
        sum += term;
        if !sum.is_finite() {
            return None;
        }
    }
    Some(sum)
}

fn binom_dist_excel(
    successes: f64,
    trials: f64,
    probability: f64,
    cumulative: bool,
) -> Option<f64> {
    if !successes.is_finite()
        || !trials.is_finite()
        || !probability.is_finite()
        || successes < 0.0
        || trials < 0.0
        || probability < 0.0
        || probability > 1.0
        || trials >= 171.0
    {
        return None;
    }
    let k = successes.trunc() as u32;
    let n = trials.trunc() as u32;
    if k > n {
        return None;
    }
    if probability == 0.0 {
        let point = if k == 0 { 1.0 } else { 0.0 };
        return Some(if cumulative { 1.0 } else { point });
    }
    if probability == 1.0 {
        let point = if k == n { 1.0 } else { 0.0 };
        return Some(if cumulative {
            if k == n {
                1.0
            } else {
                0.0
            }
        } else {
            point
        });
    }
    let mut term = (1.0 - probability).powi(n as i32);
    if !term.is_finite() || term == 0.0 {
        return None;
    }
    let ratio = probability / (1.0 - probability);
    if !cumulative {
        for step in 0..k {
            term *= f64::from(n - step) / f64::from(step + 1) * ratio;
            if !term.is_finite() {
                return None;
            }
        }
        return Some(term);
    }
    let mut sum = term;
    for step in 0..k {
        term *= f64::from(n - step) / f64::from(step + 1) * ratio;
        if !term.is_finite() {
            return None;
        }
        sum += term;
        if !sum.is_finite() {
            return None;
        }
    }
    Some(sum)
}

fn expon_dist_excel(x_value: f64, lambda: f64, cumulative: bool) -> Option<f64> {
    if !x_value.is_finite() || !lambda.is_finite() || x_value < 0.0 || lambda <= 0.0 {
        return None;
    }
    let exponent = -lambda * x_value;
    if !exponent.is_finite() {
        return None;
    }
    let decay = exponent.exp();
    if !decay.is_finite() {
        return None;
    }
    let value = if cumulative {
        1.0 - decay
    } else {
        lambda * decay
    };
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn negbinom_dist_excel(
    failures: f64,
    successes: f64,
    probability: f64,
    cumulative: bool,
) -> Option<f64> {
    if !failures.is_finite()
        || !successes.is_finite()
        || !probability.is_finite()
        || failures < 0.0
        || successes < 1.0
        || failures >= 171.0
        || successes >= 171.0
        || !(probability > 0.0 && probability < 1.0)
    {
        return None;
    }
    let failures = failures.trunc() as u32;
    let successes = successes.trunc() as u32;
    let mut term = probability.powi(successes as i32);
    if term == 0.0 || !term.is_finite() {
        return None;
    }
    let mut sum = term;
    let miss = 1.0 - probability;
    let mut count = 0u32;
    while count < failures {
        term *= f64::from(count + successes) / f64::from(count + 1) * miss;
        if !term.is_finite() {
            return None;
        }
        sum += term;
        if !sum.is_finite() {
            return None;
        }
        count += 1;
    }
    let value = if cumulative { sum } else { term };
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn hypgeom_dist_excel(
    sample_s: f64,
    number_sample: f64,
    population_s: f64,
    number_pop: f64,
    cumulative: bool,
) -> Option<f64> {
    if !sample_s.is_finite()
        || !number_sample.is_finite()
        || !population_s.is_finite()
        || !number_pop.is_finite()
        || sample_s < 0.0
        || number_sample < 0.0
        || population_s < 0.0
        || number_pop < 0.0
        || sample_s >= 171.0
        || number_sample >= 171.0
        || population_s >= 171.0
        || number_pop >= 171.0
    {
        return None;
    }
    let drawn = sample_s.trunc() as u32;
    let sample = number_sample.trunc() as u32;
    let marked = population_s.trunc() as u32;
    let population = number_pop.trunc() as u32;
    if sample > population || marked > population || drawn > sample || drawn > marked {
        return None;
    }
    let plain = population - marked;
    if sample - drawn > plain {
        return None;
    }
    let denom = combin_excel(f64::from(population), f64::from(sample))?;
    if denom == 0.0 {
        return None;
    }
    let low = if cumulative {
        sample.saturating_sub(plain)
    } else {
        drawn
    };
    let mut sum = 0.0;
    let mut count = low;
    while count <= drawn {
        let ways = combin_excel(f64::from(marked), f64::from(count))?
            * combin_excel(f64::from(plain), f64::from(sample - count))?;
        if !ways.is_finite() {
            return None;
        }
        sum += ways / denom;
        if !sum.is_finite() {
            return None;
        }
        count += 1;
    }
    if sum.is_finite() {
        Some(sum)
    } else {
        None
    }
}

fn weibull_dist_excel(x_value: f64, alpha: f64, beta: f64, cumulative: bool) -> Option<f64> {
    if !x_value.is_finite()
        || !alpha.is_finite()
        || !beta.is_finite()
        || x_value < 0.0
        || alpha <= 0.0
        || beta <= 0.0
    {
        return None;
    }
    let ratio = x_value / beta;
    if !ratio.is_finite() {
        return None;
    }
    let shape = ratio.powf(alpha);
    if !shape.is_finite() {
        return None;
    }
    let decay = (-shape).exp();
    if !decay.is_finite() {
        return None;
    }
    let value = if cumulative {
        1.0 - decay
    } else if x_value == 0.0 && alpha < 1.0 {
        return None;
    } else {
        let power = ratio.powf(alpha - 1.0);
        if !power.is_finite() {
            return None;
        }
        (alpha / beta) * power * decay
    };
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn ln_gamma_lanczos(number: f64) -> Option<f64> {
    const COEFFS: [f64; 9] = [
        0.99999999999980993,
        676.5203681218851,
        -1259.1392167224028,
        771.32342877765313,
        -176.61502916214059,
        12.507343278686905,
        -0.13857109526572012,
        9.9843695780195716e-6,
        1.5056327351493116e-7,
    ];
    let shifted = number - 1.0;
    let mut acc = COEFFS[0];
    let mut index = 1.0;
    for coeff in COEFFS.iter().skip(1) {
        acc += coeff / (shifted + index);
        index += 1.0;
    }
    if !acc.is_finite() || acc <= 0.0 {
        return None;
    }
    let base = shifted + 7.5;
    if !base.is_finite() || base <= 0.0 {
        return None;
    }
    let value =
        0.5 * (2.0 * std::f64::consts::PI).ln() + (shifted + 0.5) * base.ln() - base + acc.ln();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn factorial_below(number: f64) -> Option<f64> {
    if !(number > 0.0 && number < 171.0 && number.fract() == 0.0) {
        return None;
    }
    let count = number as u32;
    let mut acc = 1.0;
    let mut step = 2u32;
    while step < count {
        acc *= f64::from(step);
        if !acc.is_finite() {
            return None;
        }
        step += 1;
    }
    Some(acc)
}

fn ln_gamma_excel(number: f64) -> Option<f64> {
    if !number.is_finite() || number <= 0.0 {
        return None;
    }
    if let Some(fact) = factorial_below(number) {
        return if fact > 0.0 { Some(fact.ln()) } else { None };
    }
    let value = if number < 0.5 {
        let sine = (number * std::f64::consts::PI).sin();
        if sine <= 0.0 || !sine.is_finite() {
            return None;
        }
        std::f64::consts::PI.ln() - sine.ln() - ln_gamma_lanczos(1.0 - number)?
    } else {
        ln_gamma_lanczos(number)?
    };
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn gamma_excel(number: f64) -> Option<f64> {
    if !number.is_finite() {
        return None;
    }
    if number.fract() == 0.0 {
        if number <= 0.0 || number > 170.0 {
            return None;
        }
        return factorial_below(number);
    }
    if number > 0.0 {
        let value = ln_gamma_excel(number)?.exp();
        return if value.is_finite() { Some(value) } else { None };
    }
    let sine = (number * std::f64::consts::PI).sin();
    if sine == 0.0 || !sine.is_finite() {
        return None;
    }
    let denom = sine * gamma_excel(1.0 - number)?;
    if denom == 0.0 || !denom.is_finite() {
        return None;
    }
    let value = std::f64::consts::PI / denom;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn gamma_cdf_series(shape: f64, x_value: f64) -> Option<f64> {
    if x_value == 0.0 {
        return Some(0.0);
    }
    let log_prefix = -x_value + shape * x_value.ln() - ln_gamma_excel(shape)?;
    if !log_prefix.is_finite() {
        return None;
    }
    let mut term = 1.0 / shape;
    if !term.is_finite() {
        return None;
    }
    let mut sum = term;
    let mut settled = false;
    let mut step = 0.0;
    while step < 200.0 {
        step += 1.0;
        term *= x_value / (shape + step);
        if !term.is_finite() {
            return None;
        }
        sum += term;
        if !sum.is_finite() {
            return None;
        }
        let scale = if sum.abs() > 1.0 { sum.abs() } else { 1.0 };
        if term.abs() <= 1e-12 * scale {
            settled = true;
            break;
        }
    }
    if !settled {
        return None;
    }
    let value = log_prefix.exp() * sum;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn gamma_dist_excel(x_value: f64, alpha: f64, beta: f64, cumulative: bool) -> Option<f64> {
    if !x_value.is_finite()
        || !alpha.is_finite()
        || !beta.is_finite()
        || x_value < 0.0
        || alpha <= 0.0
        || beta <= 0.0
    {
        return None;
    }
    if cumulative {
        if x_value == 0.0 {
            return Some(0.0);
        }
        let lambda = x_value / beta;
        if !lambda.is_finite() {
            return None;
        }
        if alpha.fract() == 0.0 {
            let mass = poisson_excel(alpha - 1.0, lambda, true)?;
            let value = 1.0 - mass;
            return if value.is_finite() { Some(value) } else { None };
        }
        return gamma_cdf_series(alpha, lambda);
    }
    if x_value == 0.0 {
        if alpha < 1.0 {
            return None;
        }
        if alpha > 1.0 {
            return Some(0.0);
        }
        let value = 1.0 / beta;
        return if value.is_finite() { Some(value) } else { None };
    }
    let log_pdf = (alpha - 1.0) * x_value.ln()
        - (x_value / beta)
        - alpha * beta.ln()
        - ln_gamma_excel(alpha)?;
    if !log_pdf.is_finite() {
        return None;
    }
    let value = log_pdf.exp();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn binom_inv_excel(trials: f64, probability: f64, alpha: f64) -> Option<f64> {
    if !trials.is_finite()
        || !probability.is_finite()
        || !alpha.is_finite()
        || trials < 0.0
        || trials >= 171.0
        || !(probability > 0.0 && probability < 1.0)
        || !(alpha > 0.0 && alpha < 1.0)
    {
        return None;
    }
    let trials = trials.trunc() as u32;
    let mut count = 0u32;
    while count <= trials {
        let cdf = binom_dist_excel(f64::from(count), f64::from(trials), probability, true)?;
        if cdf >= alpha {
            return Some(f64::from(count));
        }
        count += 1;
    }
    None
}

fn chisq_dist_excel(x_value: f64, degrees: f64, cumulative: bool) -> Option<f64> {
    if !x_value.is_finite() || !degrees.is_finite() || x_value < 0.0 || degrees < 1.0 {
        return None;
    }
    let degrees = degrees.trunc();
    if degrees < 1.0 {
        return None;
    }
    gamma_dist_excel(x_value, degrees / 2.0, 2.0, cumulative)
}

fn chisq_rt_excel(x_value: f64, degrees: f64) -> Option<f64> {
    let value = 1.0 - chisq_dist_excel(x_value, degrees, true)?;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn chisq_test_stat(pairs: &[(f64, f64)]) -> Option<f64> {
    if pairs.is_empty() {
        return None;
    }
    let mut sum = 0.0;
    for (actual, expected) in pairs {
        if !actual.is_finite() || !expected.is_finite() || *expected <= 0.0 {
            return None;
        }
        let delta = actual - expected;
        sum += delta * delta / expected;
        if !sum.is_finite() {
            return None;
        }
    }
    Some(sum)
}

fn invert_unit_cdf(probability: f64, mut cdf: impl FnMut(f64) -> Option<f64>) -> Option<f64> {
    if !probability.is_finite() || probability <= 0.0 || probability >= 1.0 {
        return None;
    }
    let mut low = 0.0;
    let mut high = 1.0;
    loop {
        let value = cdf(high)?;
        if value >= probability {
            break;
        }
        if high >= 1.0e6 {
            return None;
        }
        high *= 2.0;
    }
    for _ in 0..80 {
        let mid = (low + high) / 2.0;
        let value = cdf(mid)?;
        if value < probability {
            low = mid;
        } else {
            high = mid;
        }
    }
    if format_calc(low) != format_calc(high) {
        return None;
    }
    let value = (low + high) / 2.0;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn gamma_inv_excel(probability: f64, alpha: f64, beta: f64) -> Option<f64> {
    if !alpha.is_finite() || !beta.is_finite() || alpha <= 0.0 || beta <= 0.0 {
        return None;
    }
    invert_unit_cdf(probability, |x_value| {
        gamma_dist_excel(x_value, alpha, beta, true)
    })
}

fn chisq_inv_excel(probability: f64, degrees: f64) -> Option<f64> {
    if !degrees.is_finite() || degrees < 1.0 {
        return None;
    }
    let degrees = degrees.trunc();
    if degrees < 1.0 {
        return None;
    }
    invert_unit_cdf(probability, |x_value| {
        chisq_dist_excel(x_value, degrees, true)
    })
}

fn chisq_inv_rt_excel(probability: f64, degrees: f64) -> Option<f64> {
    if !probability.is_finite() || probability <= 0.0 || probability >= 1.0 {
        return None;
    }
    chisq_inv_excel(1.0 - probability, degrees)
}

fn norms_dist_excel(x_value: f64, cumulative: bool) -> Option<f64> {
    if !x_value.is_finite() {
        return None;
    }
    if !cumulative {
        let exponent = -0.5 * x_value * x_value;
        if !exponent.is_finite() {
            return None;
        }
        let value = exponent.exp() / (2.0 * std::f64::consts::PI).sqrt();
        return if value.is_finite() { Some(value) } else { None };
    }
    if x_value == 0.0 {
        return Some(0.5);
    }
    let half = 0.5 * x_value * x_value;
    if !half.is_finite() {
        return None;
    }
    let tail = gamma_cdf_series(0.5, half)?;
    let value = if x_value > 0.0 {
        0.5 * (1.0 + tail)
    } else {
        0.5 * (1.0 - tail)
    };
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn norm_dist_excel(x_value: f64, mean: f64, scale: f64, cumulative: bool) -> Option<f64> {
    if !x_value.is_finite() || !mean.is_finite() || !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let z = (x_value - mean) / scale;
    if !z.is_finite() {
        return None;
    }
    let value = norms_dist_excel(z, cumulative)?;
    if cumulative {
        return Some(value);
    }
    let density = value / scale;
    if density.is_finite() {
        Some(density)
    } else {
        None
    }
}

fn norms_inv_excel(probability: f64) -> Option<f64> {
    if !probability.is_finite() || probability <= 0.0 || probability >= 1.0 {
        return None;
    }
    if (probability - 0.5).abs() < 1e-12 {
        return Some(0.0);
    }
    let (target, sign) = if probability > 0.5 {
        (probability, 1.0)
    } else {
        (1.0 - probability, -1.0)
    };
    let mut low = 0.0;
    let mut high = 1.0;
    loop {
        let cdf = norms_dist_excel(high, true)?;
        if cdf >= target {
            break;
        }
        if high >= 8.0 {
            return None;
        }
        high *= 2.0;
    }
    for _ in 0..60 {
        let mid = (low + high) / 2.0;
        let cdf = norms_dist_excel(mid, true)?;
        if cdf < target {
            low = mid;
        } else {
            high = mid;
        }
    }
    let value = sign * ((low + high) / 2.0);
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn norm_inv_excel(probability: f64, mean: f64, scale: f64) -> Option<f64> {
    if !mean.is_finite() || !(scale > 0.0) || !scale.is_finite() {
        return None;
    }
    let value = mean + scale * norms_inv_excel(probability)?;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn lognorm_inv_excel(probability: f64, mean: f64, scale: f64) -> Option<f64> {
    if !mean.is_finite() || !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let value = (mean + scale * norms_inv_excel(probability)?).exp();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn confidence_norm_excel(alpha: f64, stdev: f64, size: f64) -> Option<f64> {
    if !alpha.is_finite()
        || alpha <= 0.0
        || alpha >= 1.0
        || !stdev.is_finite()
        || stdev <= 0.0
        || !size.is_finite()
    {
        return None;
    }
    let count = size.trunc();
    if count < 1.0 {
        return None;
    }
    let z = norms_inv_excel(1.0 - alpha / 2.0)?;
    let value = z * stdev / count.sqrt();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn erf_excel(x_value: f64) -> Option<f64> {
    if !x_value.is_finite() {
        return None;
    }
    if x_value == 0.0 {
        return Some(0.0);
    }
    let square = x_value * x_value;
    if !square.is_finite() {
        return None;
    }
    let magnitude = gamma_cdf_series(0.5, square)?;
    let value = if x_value > 0.0 { magnitude } else { -magnitude };
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn lognorm_dist_excel(x_value: f64, mean: f64, scale: f64, cumulative: bool) -> Option<f64> {
    if !x_value.is_finite()
        || !mean.is_finite()
        || !scale.is_finite()
        || x_value <= 0.0
        || scale <= 0.0
    {
        return None;
    }
    let z = (x_value.ln() - mean) / scale;
    if !z.is_finite() {
        return None;
    }
    let value = norms_dist_excel(z, cumulative)?;
    if cumulative {
        return Some(value);
    }
    let density = value / (x_value * scale);
    if density.is_finite() {
        Some(density)
    } else {
        None
    }
}

fn binom_range_excel(trials: f64, probability: f64, low: f64, high: f64) -> Option<f64> {
    if !trials.is_finite()
        || !low.is_finite()
        || !high.is_finite()
        || trials < 0.0
        || trials >= 171.0
        || low < 0.0
        || high < 0.0
    {
        return None;
    }
    let limit = trials.trunc();
    let low = low.trunc();
    let high = high.trunc();
    if low > limit || high > limit || high < low {
        return None;
    }
    let mut sum = 0.0;
    let mut count = low;
    while count <= high {
        sum += binom_dist_excel(count, trials, probability, false)?;
        if !sum.is_finite() {
            return None;
        }
        count += 1.0;
    }
    Some(sum)
}

fn z_test_excel(values: &[f64], target: f64, sigma: Option<f64>) -> Option<f64> {
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) || !target.is_finite() {
        return None;
    }
    let count = values.len() as f64;
    let mean = values.iter().sum::<f64>() / count;
    let scale = match sigma {
        Some(sigma) => {
            if !sigma.is_finite() || sigma <= 0.0 {
                return None;
            }
            sigma
        }
        None => stdev_excel(values, true)?,
    };
    if scale == 0.0 {
        return None;
    }
    let z = (mean - target) / (scale / count.sqrt());
    if !z.is_finite() {
        return None;
    }
    let value = 1.0 - norms_dist_excel(z, true)?;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn prob_excel(pairs: &[(f64, f64)], lower: f64, upper: f64) -> Option<f64> {
    if pairs.is_empty() || !lower.is_finite() || !upper.is_finite() || upper < lower {
        return None;
    }
    let mut sum = 0.0;
    for (x_value, probability) in pairs {
        if !x_value.is_finite() || !probability.is_finite() || *probability < 0.0 {
            return None;
        }
        if *x_value >= lower && *x_value <= upper {
            sum += *probability;
        }
    }
    if sum.is_finite() {
        Some(sum)
    } else {
        None
    }
}

fn series_sum_excel(x_value: f64, first: f64, step: f64, coefficients: &[f64]) -> Option<f64> {
    if !x_value.is_finite() || !first.is_finite() || !step.is_finite() {
        return None;
    }
    let mut sum = 0.0;
    for (index, coefficient) in coefficients.iter().enumerate() {
        if !coefficient.is_finite() {
            return None;
        }
        let power = first + (index as f64) * step;
        if !power.is_finite() {
            return None;
        }
        let term = coefficient * x_value.powf(power);
        if !term.is_finite() {
            return None;
        }
        sum += term;
    }
    if sum.is_finite() {
        Some(sum)
    } else {
        None
    }
}

fn civil_to_unix(year: i32, month: i32, day: i32) -> i64 {
    let y = year as i64 - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp as u64 + 2) / 5 + day as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i64 - 719468
}

fn unix_to_civil(days: i64) -> (i32, i32, i32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month as i32, day as i32)
}

fn excel_normalize_month(mut year: i32, month: i32) -> Option<(i32, i32)> {
    if year < 0 || year > 9999 {
        return None;
    }
    if year < 1900 {
        year += 1900;
    }
    let zero = month as i64 - 1;
    let year_delta = if zero >= 0 {
        zero / 12
    } else {
        (zero - 11) / 12
    };
    let month0 = zero - year_delta * 12;
    let year = year as i64 + year_delta;
    if !(1900..=9999).contains(&year) {
        return None;
    }
    Some((year as i32, month0 as i32 + 1))
}

fn workbook_date1904(xml: &str) -> bool {
    xml.contains("date1904=\"1\"") || xml.contains("date1904=\"true\"")
}

fn workbook_names(xml: &str) -> std::collections::HashMap<String, DefinedRef> {
    let mut names = std::collections::HashMap::new();
    let mut index = 0usize;
    while let Some(at) = xml[index..].find("<definedName ") {
        let start = index + at;
        let Some(tag_rel) = xml[start..].find('>') else {
            break;
        };
        let tag = &xml[start..start + tag_rel];
        let empty = tag.ends_with('/');
        let body_start = start + tag_rel + 1;
        if tag.contains("localSheetId=") || empty {
            index = if empty {
                body_start
            } else if let Some(close) = xml[body_start..].find("</definedName>") {
                body_start + close + "</definedName>".len()
            } else {
                break;
            };
            continue;
        }
        let Some(name) = xml_attr(tag, "name") else {
            index = body_start;
            continue;
        };
        let Some(close) = xml[body_start..].find("</definedName>") else {
            break;
        };
        let body = &xml[body_start..body_start + close];
        index = body_start + close + "</definedName>".len();
        let name = unescape_xml(&name);
        let count = name.chars().count();
        if count == 0
            || count > 255
            || is_cell_address(&name)
            || name
                .chars()
                .any(|ch| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
        {
            continue;
        }
        if let Some(refer) = parse_defined_ref(&body) {
            names.insert(name.to_ascii_lowercase(), refer);
        }
    }
    names
}

fn xml_attr(tag: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=\"");
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn parse_defined_ref(text: &str) -> Option<DefinedRef> {
    let text = unescape_xml(text.trim());
    if text.contains('[') || text.contains(',') {
        return None;
    }
    let mut parser = CalcParser {
        bytes: text.as_bytes(),
        index: 0,
    };
    parser.skip();
    if parser.bytes.get(parser.index) == Some(&b'=') {
        parser.index += 1;
        parser.skip();
    }
    let sheet = if parser.bytes.get(parser.index) == Some(&b'\'') {
        parser.quoted_sheet()?
    } else {
        parser.word()?
    };
    if sheet.is_empty() || sheet.chars().count() > 31 {
        return None;
    }
    parser.skip();
    if parser.bytes.get(parser.index) != Some(&b'!') {
        return None;
    }
    parser.index += 1;
    let start = parser.cell_token()?;
    parser.skip();
    let (cells, rows, cols) = if parser.bytes.get(parser.index) == Some(&b':') {
        parser.index += 1;
        let end = parser.cell_token()?;
        let (c1, r1) = split_address(&start)?;
        let (c2, r2) = split_address(&end)?;
        let cells = cells_in_range(&start, &end)?;
        (cells, r1.abs_diff(r2) + 1, c1.abs_diff(c2) + 1)
    } else {
        (vec![start.to_ascii_uppercase()], 1, 1)
    };
    parser.skip();
    if parser.index != parser.bytes.len() || cells.is_empty() {
        return None;
    }
    Some(DefinedRef {
        sheet: sheet.to_ascii_lowercase(),
        cells,
        rows,
        cols,
    })
}

fn named_addresses(defined: &DefinedRef, env: &CalcEnv<'_>) -> Vec<String> {
    if defined.sheet == env.sheet {
        defined.cells.clone()
    } else {
        defined
            .cells
            .iter()
            .map(|cell| format!("{}!{cell}", defined.sheet))
            .collect()
    }
}

fn as_1900(serial: f64, date1904: bool) -> Option<f64> {
    shift_serial(serial, date1904, 1462.0)
}

fn as_weekday_serial(serial: f64, date1904: bool) -> Option<f64> {
    shift_serial(serial, date1904, 1461.0)
}

fn from_1900(serial: f64, date1904: bool) -> Option<f64> {
    unshift_serial(serial, date1904, 1462.0)
}

fn from_weekday_serial(serial: f64, date1904: bool) -> Option<f64> {
    unshift_serial(serial, date1904, 1461.0)
}

fn shift_serial(serial: f64, date1904: bool, shift: f64) -> Option<f64> {
    if !serial.is_finite() {
        return None;
    }
    let shifted = if date1904 { serial + shift } else { serial };
    if (0.0..=2_958_465.0).contains(&shifted) {
        Some(shifted)
    } else {
        None
    }
}

fn unshift_serial(serial: f64, date1904: bool, shift: f64) -> Option<f64> {
    if !serial.is_finite() {
        return None;
    }
    let shifted = if date1904 { serial - shift } else { serial };
    if (0.0..=2_958_465.0).contains(&shifted) {
        Some(shifted)
    } else {
        None
    }
}

fn date_excel(year: f64, month: f64, day: f64) -> Option<f64> {
    if !year.is_finite() || !month.is_finite() || !day.is_finite() {
        return None;
    }
    if !(year >= 0.0 && year <= 9999.0) || month.abs() >= 1.0e9 || day.abs() >= 1.0e9 {
        return None;
    }
    let (year, month) = excel_normalize_month(year.trunc() as i32, month.trunc() as i32)?;
    let day = day.trunc() as i64;
    let serial = if year == 1900 && month == 2 && day >= 29 {
        60 + (day - 29)
    } else {
        let unix = civil_to_unix(year, month, 1) + (day - 1);
        let epoch = civil_to_unix(1899, 12, 30);
        let mut serial = unix - epoch;
        if unix < civil_to_unix(1900, 3, 1) {
            serial -= 1;
        }
        serial
    };
    if (0..=2_958_465).contains(&serial) {
        Some(serial as f64)
    } else {
        None
    }
}

fn excel_parts(serial: f64) -> Option<(i32, i32, i32)> {
    if !serial.is_finite() || serial < 0.0 || serial > 2_958_465.0 {
        return None;
    }
    let serial = serial.trunc() as i64;
    if serial == 0 {
        return Some((1900, 1, 0));
    }
    if serial == 60 {
        return Some((1900, 2, 29));
    }
    let epoch = civil_to_unix(1899, 12, 30);
    let unix = if serial < 60 {
        epoch + serial + 1
    } else {
        epoch + serial
    };
    Some(unix_to_civil(unix))
}

fn base_digit(byte: u8) -> Option<u32> {
    match byte {
        b'0'..=b'9' => Some(u32::from(byte - b'0')),
        b'A'..=b'Z' => Some(u32::from(byte - b'A') + 10),
        b'a'..=b'z' => Some(u32::from(byte - b'a') + 10),
        _ => None,
    }
}

fn format_base(mut number: u64, radix: u32, width: Option<usize>) -> Option<String> {
    if !(2..=36).contains(&radix) {
        return None;
    }
    let alphabet = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut chars = Vec::new();
    if number == 0 {
        chars.push(b'0');
    } else {
        while number > 0 {
            let rem = (number % u64::from(radix)) as usize;
            chars.push(alphabet[rem]);
            number /= u64::from(radix);
        }
        chars.reverse();
    }
    if let Some(width) = width {
        if chars.len() > width {
            return None;
        }
        let pad = width - chars.len();
        let mut padded = vec![b'0'; pad];
        padded.append(&mut chars);
        chars = padded;
    }
    String::from_utf8(chars).ok()
}

fn complement_text(number: f64, radix: u32, bits: u32, places: Option<f64>) -> Option<String> {
    if !number.is_finite() || bits == 0 || bits >= 64 {
        return None;
    }
    let number = number.trunc();
    let sign = 1u64 << (bits - 1);
    let min = -(sign as f64);
    let max = (sign - 1) as f64;
    if number < min || number > max {
        return None;
    }
    let width = match places {
        None => None,
        Some(places) => {
            if !places.is_finite() {
                return None;
            }
            let places = places.trunc();
            if !(1.0..=10.0).contains(&places) {
                return None;
            }
            Some(places as usize)
        }
    };
    if number < 0.0 {
        if width.is_some_and(|width| width != 10) {
            return None;
        }
        let unsigned = (number as i128) + (1i128 << bits);
        return format_base(unsigned as u64, radix, Some(10));
    }
    format_base(number as u64, radix, width)
}

fn from_complement(text: &str, radix: u32, bits: u32) -> Option<f64> {
    if text.is_empty() || text.len() > 10 || bits == 0 || bits >= 64 || !(2..=36).contains(&radix) {
        return None;
    }
    let mut value: u64 = 0;
    for byte in text.bytes() {
        let digit = base_digit(byte)?;
        if digit >= radix {
            return None;
        }
        value = value
            .checked_mul(u64::from(radix))?
            .checked_add(u64::from(digit))?;
    }
    let full = 1u64 << bits;
    let sign = 1u64 << (bits - 1);
    if text.len() == 10 && value >= sign {
        Some((value as i128 - full as i128) as f64)
    } else if value < full {
        Some(value as f64)
    } else {
        None
    }
}

fn base_excel(number: f64, radix: f64, min_length: Option<f64>) -> Option<String> {
    if !number.is_finite() || !radix.is_finite() || number < 0.0 || number >= (1u64 << 53) as f64 {
        return None;
    }
    let radix = radix.trunc();
    if !(2.0..=36.0).contains(&radix) {
        return None;
    }
    let width = match min_length {
        None => None,
        Some(length) => {
            if !length.is_finite() {
                return None;
            }
            let length = length.trunc();
            if !(0.0..=255.0).contains(&length) {
                return None;
            }
            Some(length as usize)
        }
    };
    format_base(number.trunc() as u64, radix as u32, width)
}

fn decimal_excel(text: &str, radix: f64) -> Option<f64> {
    if !radix.is_finite() || text.is_empty() || text.len() > 255 {
        return None;
    }
    let radix = radix.trunc();
    if !(2.0..=36.0).contains(&radix) {
        return None;
    }
    let mut value: u64 = 0;
    let limit = 1u64 << 53;
    for byte in text.bytes() {
        let digit = base_digit(byte)?;
        if (digit as f64) >= radix {
            return None;
        }
        value = value
            .checked_mul(radix as u64)?
            .checked_add(u64::from(digit))?;
        if value >= limit {
            return None;
        }
    }
    Some(value as f64)
}

fn bessel_excel(x: f64, order: f64, modified: bool) -> Option<f64> {
    if !x.is_finite() || !order.is_finite() || order < 0.0 || order > 40.0 || x.abs() >= 40.0 {
        return None;
    }
    let n = order.trunc() as u32;
    if x == 0.0 {
        return Some(if n == 0 { 1.0 } else { 0.0 });
    }
    let half = x / 2.0;
    let mut term = 1.0;
    for k in 1..=n {
        term *= half / f64::from(k);
        if !term.is_finite() {
            return None;
        }
    }
    let mut sum = 0.0;
    let half_sq = half * half;
    for k in 0..200 {
        if !term.is_finite() {
            return None;
        }
        sum += term;
        let step = half_sq / (f64::from(k + 1) * f64::from(n + k + 1));
        term *= if modified { step } else { -step };
        if term.abs() <= 1e-16 * sum.abs().max(1.0) {
            return if sum.is_finite() { Some(sum) } else { None };
        }
    }
    None
}

fn mdeterm_excel(values: &[f64], n: usize) -> Option<f64> {
    if n == 0 || n > 10 || values.len() != n * n {
        return None;
    }
    let mut matrix = values.to_vec();
    let mut det = 1.0;
    for col in 0..n {
        let mut pivot = col;
        let mut best = matrix[col * n + col].abs();
        for row in (col + 1)..n {
            let value = matrix[row * n + col].abs();
            if value > best {
                best = value;
                pivot = row;
            }
        }
        if best <= 1e-12 {
            return Some(0.0);
        }
        if pivot != col {
            for index in 0..n {
                matrix.swap(col * n + index, pivot * n + index);
            }
            det = -det;
        }
        let pivot_value = matrix[col * n + col];
        det *= pivot_value;
        if !det.is_finite() {
            return None;
        }
        for row in (col + 1)..n {
            let factor = matrix[row * n + col] / pivot_value;
            for index in col..n {
                matrix[row * n + index] -= factor * matrix[col * n + index];
            }
        }
    }
    if det.is_finite() {
        Some(det)
    } else {
        None
    }
}

fn exact_lookup(lookup: &CalcValue, cell: Option<&CalcValue>) -> bool {
    match (lookup, cell) {
        (CalcValue::Num(left), Some(CalcValue::Num(right))) => {
            left.is_finite() && right.is_finite() && left == right
        }
        (CalcValue::Text(left), Some(CalcValue::Text(right))) => text_pattern(left, right),
        _ => false,
    }
}

fn text_pattern(pattern: &str, text: &str) -> bool {
    if !pattern.chars().any(|ch| matches!(ch, '*' | '?' | '~')) {
        return pattern.eq_ignore_ascii_case(text);
    }
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    if pattern.len() > 64 || text.len() > 256 {
        return false;
    }
    wildcard_match(&pattern, &text)
}

fn wildcard_match(pattern: &[char], text: &[char]) -> bool {
    if pattern.is_empty() {
        return text.is_empty();
    }
    if pattern[0] == '~' {
        return pattern.len() > 1
            && !text.is_empty()
            && pattern[1].eq_ignore_ascii_case(&text[0])
            && wildcard_match(&pattern[2..], &text[1..]);
    }
    if pattern[0] == '?' {
        return !text.is_empty() && wildcard_match(&pattern[1..], &text[1..]);
    }
    if pattern[0] == '*' {
        let mut rest = &pattern[1..];
        while rest.first() == Some(&'*') {
            rest = &rest[1..];
        }
        if rest.is_empty() {
            return true;
        }
        for start in 0..=text.len() {
            if wildcard_match(rest, &text[start..]) {
                return true;
            }
        }
        return false;
    }
    !text.is_empty()
        && pattern[0].eq_ignore_ascii_case(&text[0])
        && wildcard_match(&pattern[1..], &text[1..])
}

fn lookup_cmp(lookup: &CalcValue, cell: Option<&CalcValue>) -> Option<std::cmp::Ordering> {
    match (lookup, cell) {
        (CalcValue::Num(left), Some(CalcValue::Num(right)))
            if left.is_finite() && right.is_finite() =>
        {
            Some(left.total_cmp(right))
        }
        (CalcValue::Text(left), Some(CalcValue::Text(right))) => {
            Some(left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase()))
        }
        _ => None,
    }
}

fn approximate_index(
    keys: &[Option<CalcValue>],
    lookup: &CalcValue,
    descending: bool,
) -> Option<usize> {
    let mut found = None;
    for (index, cell) in keys.iter().enumerate() {
        let Some(order) = lookup_cmp(lookup, cell.as_ref()) else {
            break;
        };
        let keep = if descending {
            order != std::cmp::Ordering::Greater
        } else {
            order != std::cmp::Ordering::Less
        };
        if keep {
            found = Some(index);
        } else {
            break;
        }
    }
    found
}

fn beta_fraction(a: f64, b: f64, x: f64) -> Option<f64> {
    const STEPS: i32 = 200;
    let tiny = 1e-30;
    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < tiny {
        d = tiny;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..=STEPS {
        let m = f64::from(m);
        let twice = 2.0 * m;
        let mut aa = m * (b - m) * x / ((qam + twice) * (a + twice));
        d = 1.0 + aa * d;
        if d.abs() < tiny {
            d = tiny;
        }
        c = 1.0 + aa / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        h *= d * c;
        aa = -(a + m) * (qab + m) * x / ((a + twice) * (qap + twice));
        d = 1.0 + aa * d;
        if d.abs() < tiny {
            d = tiny;
        }
        c = 1.0 + aa / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        let delta = d * c;
        h *= delta;
        if !h.is_finite() {
            return None;
        }
        if (delta - 1.0).abs() < 1e-12 {
            return Some(h);
        }
    }
    None
}

fn beta_cdf(x: f64, alpha: f64, beta: f64) -> Option<f64> {
    if !x.is_finite() || !alpha.is_finite() || !beta.is_finite() || alpha <= 0.0 || beta <= 0.0 {
        return None;
    }
    if !(0.0..=1.0).contains(&x) {
        return None;
    }
    if x == 0.0 || x == 1.0 {
        return Some(x);
    }
    let ln_beta = ln_gamma_excel(alpha)? + ln_gamma_excel(beta)? - ln_gamma_excel(alpha + beta)?;
    let front = (-ln_beta + alpha * x.ln() + beta * (1.0 - x).ln()).exp();
    if !front.is_finite() {
        return None;
    }
    let value = if x < (alpha + 1.0) / (alpha + beta + 2.0) {
        front * beta_fraction(alpha, beta, x)? / alpha
    } else {
        1.0 - front * beta_fraction(beta, alpha, 1.0 - x)? / beta
    };
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn beta_pdf(x: f64, alpha: f64, beta: f64) -> Option<f64> {
    if !x.is_finite() || !alpha.is_finite() || !beta.is_finite() || alpha <= 0.0 || beta <= 0.0 {
        return None;
    }
    if !(0.0..=1.0).contains(&x) {
        return None;
    }
    let ln_beta = ln_gamma_excel(alpha)? + ln_gamma_excel(beta)? - ln_gamma_excel(alpha + beta)?;
    if x == 0.0 {
        if alpha > 1.0 {
            return Some(0.0);
        }
        if alpha == 1.0 {
            let value = (-ln_beta).exp();
            return if value.is_finite() { Some(value) } else { None };
        }
        return None;
    }
    if x == 1.0 {
        if beta > 1.0 {
            return Some(0.0);
        }
        if beta == 1.0 {
            let value = (-ln_beta).exp();
            return if value.is_finite() { Some(value) } else { None };
        }
        return None;
    }
    let value = ((alpha - 1.0) * x.ln() + (beta - 1.0) * (1.0 - x).ln() - ln_beta).exp();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn t_cdf(x: f64, df: f64) -> Option<f64> {
    if !x.is_finite() || !df.is_finite() || df <= 0.0 {
        return None;
    }
    let z = df / (df + x * x);
    if !z.is_finite() {
        return None;
    }
    let tail = 0.5 * beta_cdf(z, df / 2.0, 0.5)?;
    let value = if x >= 0.0 { 1.0 - tail } else { tail };
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn t_pdf(x: f64, df: f64) -> Option<f64> {
    if !x.is_finite() || !df.is_finite() || df <= 0.0 {
        return None;
    }
    let ln = ln_gamma_excel((df + 1.0) / 2.0)?
        - ln_gamma_excel(df / 2.0)?
        - 0.5 * (df * std::f64::consts::PI).ln();
    let base = 1.0 + (x * x) / df;
    if base <= 0.0 || !ln.is_finite() {
        return None;
    }
    let value = (ln - ((df + 1.0) / 2.0) * base.ln()).exp();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn f_cdf(x: f64, df1: f64, df2: f64) -> Option<f64> {
    if !x.is_finite() || !df1.is_finite() || !df2.is_finite() || x < 0.0 || df1 <= 0.0 || df2 <= 0.0
    {
        return None;
    }
    if x == 0.0 {
        return Some(0.0);
    }
    let z = (df1 * x) / (df1 * x + df2);
    if !z.is_finite() {
        return None;
    }
    beta_cdf(z, df1 / 2.0, df2 / 2.0)
}

fn f_pdf(x: f64, df1: f64, df2: f64) -> Option<f64> {
    if !x.is_finite() || !df1.is_finite() || !df2.is_finite() || x < 0.0 || df1 <= 0.0 || df2 <= 0.0
    {
        return None;
    }
    if x == 0.0 {
        if df1 > 2.0 {
            return Some(0.0);
        }
        if df1 != 2.0 {
            return None;
        }
    }
    let ln = ln_gamma_excel((df1 + df2) / 2.0)?
        - ln_gamma_excel(df1 / 2.0)?
        - ln_gamma_excel(df2 / 2.0)?
        + (df1 / 2.0) * df1.ln()
        + (df2 / 2.0) * df2.ln()
        + if x == 0.0 {
            0.0
        } else {
            (df1 / 2.0 - 1.0) * x.ln()
        }
        - ((df1 + df2) / 2.0) * (df2 + df1 * x).ln();
    let value = ln.exp();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn student_tail(stat: f64, df: f64, tails: f64) -> Option<f64> {
    if !stat.is_finite() || !df.is_finite() || df <= 0.0 || !tails.is_finite() {
        return None;
    }
    let tails = tails.trunc();
    if tails != 1.0 && tails != 2.0 {
        return None;
    }
    let upper = 1.0 - t_cdf(stat.abs(), df)?;
    let value = if tails == 1.0 {
        upper
    } else {
        (2.0 * upper).min(1.0)
    };
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn mean_sample_var(values: &[f64]) -> Option<(f64, f64, f64)> {
    let variance = var_excel(values, true)?;
    let count = values.len() as f64;
    let mean = values.iter().sum::<f64>() / count;
    if mean.is_finite() {
        Some((mean, variance, count))
    } else {
        None
    }
}

fn paired_t_test(diffs: &[f64], tails: f64) -> Option<f64> {
    let (mean, variance, count) = mean_sample_var(diffs)?;
    if variance <= 0.0 {
        return None;
    }
    let stat = mean / (variance.sqrt() / count.sqrt());
    student_tail(stat, count - 1.0, tails)
}

fn t_test_excel(left: &[f64], right: &[f64], tails: f64, kind: f64) -> Option<f64> {
    if !kind.is_finite() {
        return None;
    }
    let kind = kind.trunc();
    let (mean_left, var_left, n_left) = mean_sample_var(left)?;
    let (mean_right, var_right, n_right) = mean_sample_var(right)?;
    if kind == 2.0 {
        let df = n_left + n_right - 2.0;
        if df <= 0.0 {
            return None;
        }
        let pooled = ((n_left - 1.0) * var_left + (n_right - 1.0) * var_right) / df;
        if pooled <= 0.0 {
            return None;
        }
        let stat = (mean_left - mean_right) / (pooled * (1.0 / n_left + 1.0 / n_right)).sqrt();
        return student_tail(stat, df, tails);
    }
    if kind == 3.0 {
        let left_term = var_left / n_left;
        let right_term = var_right / n_right;
        let se2 = left_term + right_term;
        if se2 <= 0.0 || n_left <= 1.0 || n_right <= 1.0 {
            return None;
        }
        let df = (se2 * se2)
            / (left_term.powi(2) / (n_left - 1.0) + right_term.powi(2) / (n_right - 1.0));
        let stat = (mean_left - mean_right) / se2.sqrt();
        return student_tail(stat, df, tails);
    }
    None
}

fn f_test_excel(left: &[f64], right: &[f64]) -> Option<f64> {
    let (_, var_left, n_left) = mean_sample_var(left)?;
    let (_, var_right, n_right) = mean_sample_var(right)?;
    if var_left <= 0.0 || var_right <= 0.0 {
        return None;
    }
    let cdf = f_cdf(var_left / var_right, n_left - 1.0, n_right - 1.0)?;
    let value = (2.0 * cdf.min(1.0 - cdf)).min(1.0);
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn t_quantile(probability: f64, df: f64) -> Option<f64> {
    if !(probability > 0.5 && probability < 1.0) || !df.is_finite() || df <= 0.0 {
        return None;
    }
    let mut high = 1.0;
    while t_cdf(high, df)? < probability {
        high *= 2.0;
        if high > 1.0e8 {
            return None;
        }
    }
    let mut low = 0.0;
    for _ in 0..80 {
        let mid = (low + high) / 2.0;
        if t_cdf(mid, df)? < probability {
            low = mid;
        } else {
            high = mid;
        }
    }
    let value = (low + high) / 2.0;
    if value.is_finite() && high - low <= 1e-10 * value.abs().max(1.0) {
        Some(value)
    } else {
        None
    }
}

fn confidence_t_excel(alpha: f64, stdev: f64, size: f64) -> Option<f64> {
    if !alpha.is_finite() || !stdev.is_finite() || !size.is_finite() {
        return None;
    }
    if !(alpha > 0.0 && alpha < 1.0) || stdev <= 0.0 {
        return None;
    }
    let size = size.trunc();
    if !(2.0..=1.0e6).contains(&size) {
        return None;
    }
    let critical = t_quantile(1.0 - alpha / 2.0, size - 1.0)?;
    let value = critical * stdev / size.sqrt();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn period_type(value: f64) -> Option<f64> {
    if !value.is_finite() {
        return None;
    }
    Some(if value.trunc() == 0.0 { 0.0 } else { 1.0 })
}

fn annuity_pow(rate: f64, nper: f64) -> Option<f64> {
    if !rate.is_finite() || !nper.is_finite() || nper <= 0.0 || nper > 1.0e6 || rate <= -1.0 {
        return None;
    }
    let value = (1.0 + rate).powf(nper);
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn pmt_excel(rate: f64, nper: f64, pv: f64, fv: f64, typ: f64) -> Option<f64> {
    if !pv.is_finite() || !fv.is_finite() {
        return None;
    }
    let typ = period_type(typ)?;
    if rate == 0.0 {
        if nper <= 0.0 || nper > 1.0e6 {
            return None;
        }
        let value = -(pv + fv) / nper;
        return if value.is_finite() { Some(value) } else { None };
    }
    let factor = annuity_pow(rate, nper)?;
    let value = -(pv * factor + fv) * rate / ((1.0 + rate * typ) * (factor - 1.0));
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn fv_excel(rate: f64, nper: f64, pmt: f64, pv: f64, typ: f64) -> Option<f64> {
    if !pmt.is_finite() || !pv.is_finite() {
        return None;
    }
    let typ = period_type(typ)?;
    if rate == 0.0 {
        if nper <= 0.0 || nper > 1.0e6 {
            return None;
        }
        let value = -pv - pmt * nper;
        return if value.is_finite() { Some(value) } else { None };
    }
    let factor = annuity_pow(rate, nper)?;
    let value = -pv * factor - pmt * (1.0 + rate * typ) * (factor - 1.0) / rate;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn pv_excel(rate: f64, nper: f64, pmt: f64, fv: f64, typ: f64) -> Option<f64> {
    if !pmt.is_finite() || !fv.is_finite() {
        return None;
    }
    let typ = period_type(typ)?;
    if rate == 0.0 {
        if nper <= 0.0 || nper > 1.0e6 {
            return None;
        }
        let value = -fv - pmt * nper;
        return if value.is_finite() { Some(value) } else { None };
    }
    let factor = annuity_pow(rate, nper)?;
    let value = -(fv + pmt * (1.0 + rate * typ) * (factor - 1.0) / rate) / factor;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn nper_excel(rate: f64, pmt: f64, pv: f64, fv: f64, typ: f64) -> Option<f64> {
    if !rate.is_finite() || !pmt.is_finite() || !pv.is_finite() || !fv.is_finite() || rate <= -1.0 {
        return None;
    }
    let typ = period_type(typ)?;
    if rate == 0.0 {
        if pmt == 0.0 {
            return None;
        }
        let value = -(pv + fv) / pmt;
        return if value.is_finite() && value > 0.0 {
            Some(value)
        } else {
            None
        };
    }
    let payment = pmt * (1.0 + rate * typ);
    let numerator = payment - fv * rate;
    let denominator = payment + pv * rate;
    if denominator == 0.0 || numerator / denominator <= 0.0 || (1.0 + rate) <= 0.0 {
        return None;
    }
    let value = (numerator / denominator).ln() / (1.0 + rate).ln();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn rate_balance(rate: f64, nper: f64, pmt: f64, pv: f64, fv: f64, typ: f64) -> Option<f64> {
    if rate.abs() < 1e-12 {
        let value = pv + pmt * nper + fv;
        return if value.is_finite() { Some(value) } else { None };
    }
    let factor = annuity_pow(rate, nper)?;
    let value = pv * factor + pmt * (1.0 + rate * typ) * (factor - 1.0) / rate + fv;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn rate_excel(nper: f64, pmt: f64, pv: f64, fv: f64, typ: f64, guess: f64) -> Option<f64> {
    if !nper.is_finite()
        || !pmt.is_finite()
        || !pv.is_finite()
        || !fv.is_finite()
        || !guess.is_finite()
    {
        return None;
    }
    let typ = period_type(typ)?;
    if nper <= 0.0 || nper > 1.0e6 || guess <= -1.0 {
        return None;
    }
    let mut rate = guess;
    for _ in 0..40 {
        let step = 1e-6 * rate.abs().max(1.0);
        let value = rate_balance(rate, nper, pmt, pv, fv, typ)?;
        let shifted = rate_balance(rate + step, nper, pmt, pv, fv, typ)?;
        let slope = (shifted - value) / step;
        if slope.abs() < 1e-14 {
            return None;
        }
        let next = rate - value / slope;
        if !next.is_finite() || next <= -1.0 {
            return None;
        }
        if (next - rate).abs() <= 1e-8 * next.abs().max(1.0) {
            return Some(next);
        }
        rate = next;
    }
    None
}

fn npv_excel(rate: f64, values: &[f64]) -> Option<f64> {
    if !rate.is_finite() || rate <= -1.0 || values.is_empty() || values.len() > 4096 {
        return None;
    }
    if values.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let mut total = 0.0;
    let mut discount = 1.0 + rate;
    for value in values {
        total += value / discount;
        discount *= 1.0 + rate;
        if !discount.is_finite() {
            return None;
        }
    }
    if total.is_finite() {
        Some(total)
    } else {
        None
    }
}

fn irr_excel(values: &[f64], guess: f64) -> Option<f64> {
    if !guess.is_finite() || guess <= -1.0 || values.len() < 2 || values.len() > 128 {
        return None;
    }
    if values.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let mut rate = guess;
    for _ in 0..40 {
        let mut total = 0.0;
        let mut slope = 0.0;
        let mut discount = 1.0;
        for (index, value) in values.iter().enumerate() {
            total += value / discount;
            if index > 0 {
                slope += -((index as f64) * value) / (discount * (1.0 + rate));
            }
            discount *= 1.0 + rate;
            if !discount.is_finite() {
                return None;
            }
        }
        if slope.abs() < 1e-14 || !total.is_finite() {
            return None;
        }
        let next = rate - total / slope;
        if !next.is_finite() || next <= -1.0 {
            return None;
        }
        if (next - rate).abs() <= 1e-8 * next.abs().max(1.0) {
            return Some(next);
        }
        rate = next;
    }
    None
}

fn ipmt_excel(rate: f64, per: f64, nper: f64, pv: f64, fv: f64, typ: f64) -> Option<f64> {
    if !per.is_finite() {
        return None;
    }
    let per = per.trunc();
    if per < 1.0 || per > nper {
        return None;
    }
    let payment = pmt_excel(rate, nper, pv, fv, typ)?;
    let typ = period_type(typ)?;
    if rate == 0.0 || (typ == 1.0 && per == 1.0) {
        return Some(0.0);
    }
    let balance = if per == 1.0 {
        -pv
    } else {
        fv_excel(rate, per - 1.0, payment, pv, typ)?
    };
    let value = balance * rate;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn ppmt_excel(rate: f64, per: f64, nper: f64, pv: f64, fv: f64, typ: f64) -> Option<f64> {
    let interest = ipmt_excel(rate, per, nper, pv, fv, typ)?;
    let payment = pmt_excel(rate, nper, pv, fv, typ)?;
    let value = payment - interest;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn cumulative_payment(
    rate: f64,
    nper: f64,
    pv: f64,
    start: f64,
    end: f64,
    typ: f64,
    principal: bool,
) -> Option<f64> {
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    let start = start.trunc();
    let end = end.trunc();
    if start < 1.0 || end < start || end > nper || end - start > 10_000.0 {
        return None;
    }
    let mut total = 0.0;
    let mut period = start;
    while period <= end {
        let part = if principal {
            ppmt_excel(rate, period, nper, pv, 0.0, typ)?
        } else {
            ipmt_excel(rate, period, nper, pv, 0.0, typ)?
        };
        total += part;
        if !total.is_finite() {
            return None;
        }
        period += 1.0;
    }
    Some(total)
}

fn xnpv_excel(rate: f64, values: &[f64], dates: &[f64]) -> Option<f64> {
    if !rate.is_finite()
        || rate <= -1.0
        || values.is_empty()
        || values.len() > 128
        || values.len() != dates.len()
    {
        return None;
    }
    let base = dates[0];
    if dates.iter().any(|date| *date < base) {
        return None;
    }
    let mut total = 0.0;
    for (value, date) in values.iter().zip(dates) {
        let years = (date - base) / 365.0;
        let discount = (1.0 + rate).powf(years);
        if !discount.is_finite() || discount == 0.0 {
            return None;
        }
        total += value / discount;
    }
    if total.is_finite() {
        Some(total)
    } else {
        None
    }
}

fn xirr_excel(values: &[f64], dates: &[f64], guess: f64) -> Option<f64> {
    if !guess.is_finite()
        || guess <= -1.0
        || values.len() < 2
        || values.len() > 128
        || values.len() != dates.len()
        || !values.iter().any(|value| *value > 0.0)
        || !values.iter().any(|value| *value < 0.0)
    {
        return None;
    }
    let base = dates[0];
    let mut previous = base;
    for date in &dates[1..] {
        if *date < previous {
            return None;
        }
        previous = *date;
    }
    if (dates[dates.len() - 1] - base).abs() < 1e-9 {
        return None;
    }
    let times: Vec<f64> = dates.iter().map(|date| (date - base) / 365.0).collect();
    let mut rate = guess;
    for _ in 0..40 {
        let mut total = 0.0;
        let mut slope = 0.0;
        for (value, time) in values.iter().zip(&times) {
            let discount = (1.0 + rate).powf(*time);
            let next = (1.0 + rate).powf(time + 1.0);
            if !discount.is_finite() || !next.is_finite() || discount == 0.0 || next == 0.0 {
                return None;
            }
            total += value / discount;
            slope += value * (-time) / next;
        }
        if slope.abs() < 1e-14 || !total.is_finite() {
            return None;
        }
        let next = rate - total / slope;
        if !next.is_finite() || next <= -1.0 {
            return None;
        }
        if (next - rate).abs() <= 1e-8 * next.abs().max(1.0) {
            return Some(next);
        }
        rate = next;
    }
    None
}

fn mirr_excel(values: &[f64], finance: f64, reinvest: f64) -> Option<f64> {
    if !finance.is_finite()
        || !reinvest.is_finite()
        || finance <= -1.0
        || reinvest <= -1.0
        || values.len() < 2
        || values.len() > 128
    {
        return None;
    }
    let mut positive = 0.0;
    let mut negative = 0.0;
    let mut has_positive = false;
    let mut has_negative = false;
    for (index, value) in values.iter().enumerate() {
        if *value > 0.0 {
            has_positive = true;
            let discount = (1.0 + reinvest).powf(index as f64);
            if !discount.is_finite() || discount == 0.0 {
                return None;
            }
            positive += value / discount;
        } else if *value < 0.0 {
            has_negative = true;
            let discount = (1.0 + finance).powf(index as f64);
            if !discount.is_finite() || discount == 0.0 {
                return None;
            }
            negative += value / discount;
        }
    }
    if !has_positive || !has_negative || !positive.is_finite() || !negative.is_finite() {
        return None;
    }
    let ratio = positive.abs() / negative.abs();
    let value = ratio.powf(1.0 / (values.len() as f64 - 1.0)) * (1.0 + reinvest) - 1.0;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn convert_excel(number: f64, from: &str, to: &str) -> Option<f64> {
    if !number.is_finite() {
        return None;
    }
    if is_temperature(from) && is_temperature(to) {
        let kelvin = to_kelvin(from, number)?;
        if kelvin < 0.0 {
            return None;
        }
        return from_kelvin(to, kelvin);
    }
    let (left_kind, left_factor) = linear_unit(from)?;
    let (right_kind, right_factor) = linear_unit(to)?;
    if left_kind != right_kind || right_factor == 0.0 {
        return None;
    }
    let value = number * left_factor / right_factor;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn is_temperature(unit: &str) -> bool {
    matches!(unit, "C" | "F" | "K")
}

fn to_kelvin(unit: &str, number: f64) -> Option<f64> {
    let value = match unit {
        "C" => number + 273.15,
        "F" => (number - 32.0) * 5.0 / 9.0 + 273.15,
        "K" => number,
        _ => return None,
    };
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn from_kelvin(unit: &str, kelvin: f64) -> Option<f64> {
    let value = match unit {
        "C" => kelvin - 273.15,
        "F" => (kelvin - 273.15) * 9.0 / 5.0 + 32.0,
        "K" => kelvin,
        _ => return None,
    };
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn linear_unit(unit: &str) -> Option<(&'static str, f64)> {
    Some(match unit {
        "m" => ("length", 1.0),
        "cm" => ("length", 0.01),
        "mm" => ("length", 0.001),
        "km" => ("length", 1000.0),
        "in" => ("length", 0.0254),
        "ft" => ("length", 0.3048),
        "yd" => ("length", 0.9144),
        "mi" => ("length", 1609.344),
        "g" => ("mass", 1.0),
        "kg" => ("mass", 1000.0),
        "mg" => ("mass", 0.001),
        "lbm" => ("mass", 453.59237),
        "ozm" => ("mass", 28.349523125),
        "sec" => ("time", 1.0),
        "mn" => ("time", 60.0),
        "hr" => ("time", 3600.0),
        "day" => ("time", 86_400.0),
        "yr" => ("time", 365.25 * 86_400.0),
        _ => return None,
    })
}

fn text_excel(value: CalcValue, format: &str, date1904: bool) -> Option<CalcValue> {
    if format.is_empty() || format.len() > 64 || format.contains(';') {
        return None;
    }
    if format == "@" {
        let text = match value {
            CalcValue::Text(text) => text,
            CalcValue::Num(number) => format_calc(number),
        };
        return limited_text(text);
    }
    let unquoted = unquoted_format(format)?;
    let date = unquoted
        .chars()
        .any(|ch| matches!(ch, 'y' | 'Y' | 'd' | 'D' | 'm' | 'M'));
    let number_code = unquoted
        .chars()
        .any(|ch| matches!(ch, '0' | '#' | '%' | '.'));
    if date && number_code {
        return None;
    }
    if date {
        let CalcValue::Num(number) = value else {
            return None;
        };
        let serial = as_1900(number, date1904)?;
        return format_excel_date(serial, format).and_then(limited_text);
    }
    let CalcValue::Num(number) = value else {
        return None;
    };
    format_excel_number(number, format).and_then(limited_text)
}

fn unquoted_format(format: &str) -> Option<String> {
    let chars: Vec<char> = format.chars().collect();
    let mut index = 0usize;
    let mut out = String::new();
    while index < chars.len() {
        if chars[index] == '"' {
            index += 1;
            while index < chars.len() && chars[index] != '"' {
                index += 1;
            }
            if index >= chars.len() {
                return None;
            }
            index += 1;
            continue;
        }
        out.push(chars[index]);
        index += 1;
    }
    Some(out)
}

fn limited_text(text: String) -> Option<CalcValue> {
    if text.chars().count() > 32_767 {
        None
    } else {
        Some(CalcValue::Text(text))
    }
}

fn format_excel_date(serial: f64, format: &str) -> Option<String> {
    let (year, month, day) = excel_parts(serial)?;
    let chars: Vec<char> = format.chars().collect();
    let mut index = 0usize;
    let mut out = String::new();
    let mut saw_token = false;
    while index < chars.len() {
        if chars[index] == '"' {
            index += 1;
            let start = index;
            while index < chars.len() && chars[index] != '"' {
                index += 1;
            }
            if index >= chars.len() {
                return None;
            }
            out.extend(chars[start..index].iter());
            index += 1;
            continue;
        }
        let rest: String = chars[index..]
            .iter()
            .collect::<String>()
            .to_ascii_lowercase();
        let (token, text) = if rest.starts_with("yyyy") {
            ("yyyy", format!("{year:04}"))
        } else if rest.starts_with("yy") {
            ("yy", format!("{:02}", year.rem_euclid(100)))
        } else if rest.starts_with("mm") {
            ("mm", format!("{month:02}"))
        } else if rest.starts_with('m') {
            ("m", month.to_string())
        } else if rest.starts_with("dd") {
            ("dd", format!("{day:02}"))
        } else if rest.starts_with('d') {
            ("d", day.to_string())
        } else if matches!(chars[index], '-' | '/' | '.' | ' ' | ':') {
            out.push(chars[index]);
            index += 1;
            continue;
        } else {
            return None;
        };
        saw_token = true;
        index += token.len();
        out.push_str(&text);
    }
    if saw_token {
        Some(out)
    } else {
        None
    }
}

fn format_excel_number(number: f64, format: &str) -> Option<String> {
    if !number.is_finite() || number.abs() >= 1e15 {
        return None;
    }
    let chars: Vec<char> = format.chars().collect();
    let mut index = 0usize;
    let mut prefix = String::new();
    let mut suffix = String::new();
    let mut pattern = false;
    let mut after = false;
    let mut int_zeros = 0usize;
    let mut frac_zeros = 0usize;
    let mut frac_places = 0usize;
    let mut thousands = false;
    let mut percent = false;
    let mut dotted = false;
    let mut saw_placeholder = false;
    while index < chars.len() {
        let ch = chars[index];
        if ch == '"' {
            index += 1;
            let start = index;
            while index < chars.len() && chars[index] != '"' {
                index += 1;
            }
            if index >= chars.len() {
                return None;
            }
            let literal: String = chars[start..index].iter().collect();
            index += 1;
            if pattern || after {
                after = true;
                suffix.push_str(&literal);
            } else {
                prefix.push_str(&literal);
            }
            continue;
        }
        if after {
            if ch == '%' && !percent {
                percent = true;
                suffix.push('%');
                index += 1;
                continue;
            }
            return None;
        }
        match ch {
            '#' | '0' => {
                pattern = true;
                saw_placeholder = true;
                if dotted {
                    frac_places += 1;
                    if ch == '0' {
                        frac_zeros += 1;
                    }
                } else if ch == '0' {
                    int_zeros += 1;
                }
            }
            ',' => {
                if !pattern || dotted {
                    return None;
                }
                thousands = true;
            }
            '.' => {
                if dotted {
                    return None;
                }
                pattern = true;
                dotted = true;
            }
            '%' => {
                if percent {
                    return None;
                }
                percent = true;
                after = true;
                suffix.push('%');
            }
            _ => return None,
        }
        index += 1;
    }
    if !saw_placeholder || int_zeros > 16 || frac_places > 8 {
        return None;
    }
    let scaled_number = if percent { number * 100.0 } else { number };
    if !scaled_number.is_finite() || scaled_number.abs() >= 1e15 {
        return None;
    }
    let scale = 10f64.powi(frac_places as i32);
    let scaled = (scaled_number.abs() * scale).round();
    if !scaled.is_finite() {
        return None;
    }
    let mut frac = if frac_places == 0 {
        0.0
    } else {
        scaled % scale
    };
    let mut int_part = if frac_places == 0 {
        scaled
    } else {
        (scaled / scale).floor()
    };
    if frac_places > 0 && (frac - scale).abs() < 1e-6 {
        int_part += 1.0;
        frac = 0.0;
    }
    if int_part >= 1e15 {
        return None;
    }
    let mut int_text = format!("{}", int_part as i64);
    if int_text.len() < int_zeros {
        int_text = format!("{int_text:0>int_zeros$}");
    }
    if thousands {
        int_text = group_thousands(&int_text);
    }
    let mut out = String::new();
    if scaled_number < 0.0 && (int_part > 0.0 || frac > 0.0 || frac_places > 0) {
        out.push('-');
    }
    out.push_str(&prefix);
    out.push_str(&int_text);
    if frac_places > 0 {
        let frac_text = format!("{frac_digits:0>frac_places$}", frac_digits = frac as i64);
        let mut keep = frac_places;
        while keep > frac_zeros && frac_text.as_bytes()[keep - 1] == b'0' {
            keep -= 1;
        }
        if keep > 0 {
            out.push('.');
            out.push_str(&frac_text[..keep]);
        }
    }
    out.push_str(&suffix);
    Some(out)
}

fn group_thousands(digits: &str) -> String {
    let mut out = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn weekday_code(serial: i64, kind: i64) -> Option<f64> {
    if !(0..=2_958_465).contains(&serial) {
        return None;
    }
    let value = match kind {
        1 => {
            let day = (serial + 1) % 7;
            if day == 0 {
                7
            } else {
                day
            }
        }
        2 => {
            let day = serial % 7;
            if day == 0 {
                7
            } else {
                day
            }
        }
        3 => (serial - 1).rem_euclid(7),
        _ => return None,
    };
    Some(value as f64)
}

fn month_length(year: i32, month: i32) -> Option<i32> {
    let serial = date_excel(f64::from(year), f64::from(month) + 1.0, 0.0)?;
    let (_, _, day) = excel_parts(serial)?;
    if day <= 0 {
        None
    } else {
        Some(day)
    }
}

fn shift_months(serial: f64, months: f64, end_of_month: bool) -> Option<f64> {
    if !serial.is_finite() || !months.is_finite() || serial < 1.0 || serial > 2_958_465.0 {
        return None;
    }
    if months.abs() > 120_000.0 {
        return None;
    }
    let (year, month, day) = excel_parts(serial.trunc())?;
    let (year, month) = excel_normalize_month(year, month + months.trunc() as i32)?;
    let length = month_length(year, month)?;
    let day = if end_of_month {
        length
    } else {
        day.min(length)
    };
    date_excel(f64::from(year), f64::from(month), f64::from(day))
}

fn datedif_excel(start: f64, end: f64, unit: &str) -> Option<f64> {
    if !start.is_finite() || !end.is_finite() || start < 1.0 || end < start || end > 2_958_465.0 {
        return None;
    }
    let start = start.trunc();
    let end = end.trunc();
    let (sy, sm, sd) = excel_parts(start)?;
    let (ey, em, ed) = excel_parts(end)?;
    let unit = unit.to_ascii_uppercase();
    let value = match unit.as_str() {
        "D" => end - start,
        "Y" => {
            let mut years = ey - sy;
            if (em, ed) < (sm, sd) {
                years -= 1;
            }
            f64::from(years)
        }
        "M" => {
            let mut months = (ey - sy) * 12 + (em - sm);
            if ed < sd {
                months -= 1;
            }
            f64::from(months)
        }
        "YM" => {
            let mut months = em - sm;
            if ed < sd {
                months -= 1;
            }
            if months < 0 {
                months += 12;
            }
            f64::from(months)
        }
        "MD" => {
            if ed >= sd {
                f64::from(ed - sd)
            } else {
                let previous = if em == 1 { 12 } else { em - 1 };
                let year = if em == 1 { ey - 1 } else { ey };
                let length = month_length(year, previous)?;
                f64::from(length - sd + ed)
            }
        }
        "YD" => {
            let mut anchor = date_excel(f64::from(ey), f64::from(sm), f64::from(sd))?;
            if anchor > end {
                anchor = date_excel(f64::from(ey - 1), f64::from(sm), f64::from(sd))?;
            }
            end - anchor
        }
        _ => return None,
    };
    if value.is_finite() && value >= 0.0 {
        Some(value)
    } else {
        None
    }
}

fn is_workday_serial(serial: i64) -> bool {
    let day = serial.rem_euclid(7);
    let day = if day == 0 { 7 } else { day };
    (1..=5).contains(&day)
}

fn networkdays_excel(start: f64, end: f64, holidays: &[f64]) -> Option<f64> {
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    let mut start = start.trunc() as i64;
    let mut end = end.trunc() as i64;
    let sign = if start <= end { 1.0 } else { -1.0 };
    if start > end {
        std::mem::swap(&mut start, &mut end);
    }
    if start < 0 || end > 2_958_465 || end - start > 100_000 {
        return None;
    }
    let mut count = 0.0;
    for day in start..=end {
        if is_workday_serial(day) && !holiday_hit(day, holidays) {
            count += 1.0;
        }
    }
    Some(sign * count)
}

fn holiday_hit(day: i64, holidays: &[f64]) -> bool {
    holidays.iter().any(|holiday| holiday.trunc() as i64 == day)
}

fn workday_excel(start: f64, days: f64, holidays: &[f64]) -> Option<f64> {
    if !start.is_finite() || !days.is_finite() || start < 0.0 || start > 2_958_465.0 {
        return None;
    }
    let days = days.trunc();
    if days.abs() > 10_000.0 {
        return None;
    }
    if days == 0.0 {
        return Some(start.trunc());
    }
    let step: i64 = if days > 0.0 { 1 } else { -1 };
    let mut left = days.abs() as i64;
    let mut day = start.trunc() as i64;
    let mut guard = 0i64;
    while left > 0 {
        day += step;
        guard += 1;
        if guard > 20_000 || !(0..=2_958_465).contains(&day) {
            return None;
        }
        if is_workday_serial(day) && !holiday_hit(day, holidays) {
            left -= 1;
        }
    }
    Some(day as f64)
}

fn trig_excel(number: f64, kind: &str) -> Option<f64> {
    if !number.is_finite() {
        return None;
    }
    let value = match kind {
        "SIN" => number.sin(),
        "COS" => number.cos(),
        "TAN" => number.tan(),
        _ => return None,
    };
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn reciprocal_trig_excel(number: f64, kind: &str) -> Option<f64> {
    if !number.is_finite() {
        return None;
    }
    let value = match kind {
        "SEC" => {
            let denominator = number.cos();
            if denominator == 0.0 {
                return None;
            }
            1.0 / denominator
        }
        "CSC" => {
            let denominator = number.sin();
            if denominator == 0.0 {
                return None;
            }
            1.0 / denominator
        }
        "COT" => {
            let denominator = number.sin();
            if denominator == 0.0 {
                return None;
            }
            number.cos() / denominator
        }
        _ => return None,
    };
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn radians_excel(number: f64) -> Option<f64> {
    if !number.is_finite() {
        return None;
    }
    let value = number * std::f64::consts::PI / 180.0;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn degrees_excel(number: f64) -> Option<f64> {
    if !number.is_finite() {
        return None;
    }
    let value = number * 180.0 / std::f64::consts::PI;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn asin_excel(number: f64) -> Option<f64> {
    if !number.is_finite() || !(-1.0..=1.0).contains(&number) {
        return None;
    }
    let value = number.asin();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn acos_excel(number: f64) -> Option<f64> {
    if !number.is_finite() || !(-1.0..=1.0).contains(&number) {
        return None;
    }
    let value = number.acos();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn atan_excel(number: f64) -> Option<f64> {
    if !number.is_finite() {
        return None;
    }
    let value = number.atan();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn hyper_excel(number: f64, kind: &str) -> Option<f64> {
    if !number.is_finite() {
        return None;
    }
    let value = match kind {
        "SINH" => number.sinh(),
        "COSH" => number.cosh(),
        "TANH" => number.tanh(),
        _ => return None,
    };
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn inverse_hyper_excel(number: f64, kind: &str) -> Option<f64> {
    if !number.is_finite() {
        return None;
    }
    let value = match kind {
        "ASINH" => number.asinh(),
        "ACOSH" => {
            if number < 1.0 {
                return None;
            }
            number.acosh()
        }
        "ATANH" => {
            if number <= -1.0 || number >= 1.0 {
                return None;
            }
            number.atanh()
        }
        _ => return None,
    };
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn reciprocal_hyper_excel(number: f64, kind: &str) -> Option<f64> {
    if !number.is_finite() {
        return None;
    }
    let base = match kind {
        "SECH" => number.cosh(),
        "CSCH" => number.sinh(),
        "COTH" => number.tanh(),
        _ => return None,
    };
    if base == 0.0 || !base.is_finite() {
        return None;
    }
    let value = 1.0 / base;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn acot_excel(number: f64) -> Option<f64> {
    if !number.is_finite() {
        return None;
    }
    let value = if number == 0.0 {
        std::f64::consts::PI / 2.0
    } else if number > 0.0 {
        (1.0 / number).atan()
    } else {
        (1.0 / number).atan() + std::f64::consts::PI
    };
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn acoth_excel(number: f64) -> Option<f64> {
    if !number.is_finite() || number.abs() <= 1.0 {
        return None;
    }
    let value = (1.0 / number).atanh();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn combin_excel(n: f64, k: f64) -> Option<f64> {
    if !n.is_finite() || !k.is_finite() || n < 0.0 || k < 0.0 || n >= 1_000_000.0 {
        return None;
    }
    let n = n.trunc() as u64;
    let k = k.trunc() as u64;
    if k > n {
        return None;
    }
    let k = k.min(n - k);
    let mut acc = 1.0;
    for index in 0..k {
        acc *= (n - index) as f64;
        acc /= (index + 1) as f64;
        if !acc.is_finite() {
            return None;
        }
    }
    if acc < 1e15 {
        Some(acc.round())
    } else {
        Some(acc)
    }
}

fn combina_excel(n: f64, k: f64) -> Option<f64> {
    if !n.is_finite()
        || !k.is_finite()
        || n < 0.0
        || k < 0.0
        || n >= 1_000_000.0
        || k >= 1_000_000.0
    {
        return None;
    }
    let n = n.trunc();
    let k = k.trunc();
    if k == 0.0 {
        return Some(1.0);
    }
    if n == 0.0 {
        return Some(0.0);
    }
    combin_excel(n + k - 1.0, k)
}

fn count_pair(n: f64, k: f64) -> Option<(u64, u64)> {
    if !n.is_finite()
        || !k.is_finite()
        || n < 0.0
        || k < 0.0
        || n >= 1_000_000.0
        || k >= 1_000_000.0
    {
        return None;
    }
    Some((n.trunc() as u64, k.trunc() as u64))
}

fn round_count(acc: f64) -> Option<f64> {
    if !acc.is_finite() {
        return None;
    }
    if acc < 1e15 {
        Some(acc.round())
    } else {
        Some(acc)
    }
}

fn permut_excel(n: f64, k: f64) -> Option<f64> {
    let (n, k) = count_pair(n, k)?;
    if k > n {
        return None;
    }
    let mut acc = 1.0;
    for index in 0..k {
        acc *= (n - index) as f64;
        if !acc.is_finite() {
            return None;
        }
    }
    round_count(acc)
}

fn permutationa_excel(n: f64, k: f64) -> Option<f64> {
    let (n, k) = count_pair(n, k)?;
    if k == 0 {
        return Some(1.0);
    }
    if n == 0 {
        return Some(0.0);
    }
    let mut acc = 1.0;
    for _ in 0..k {
        acc *= n as f64;
        if !acc.is_finite() {
            return None;
        }
    }
    round_count(acc)
}

fn ranked_excel(values: &mut [f64], rank: f64, small: bool) -> Option<f64> {
    if values.is_empty()
        || !rank.is_finite()
        || rank < 1.0
        || rank > values.len() as f64
        || values.iter().any(|number| !number.is_finite())
    {
        return None;
    }
    let rank = rank.trunc() as usize;
    if rank < 1 || rank > values.len() {
        return None;
    }
    values.sort_by(|left, right| left.total_cmp(right));
    let index = if small { rank - 1 } else { values.len() - rank };
    Some(values[index])
}

fn atan2_excel(x_coord: f64, y_coord: f64) -> Option<f64> {
    if !x_coord.is_finite() || !y_coord.is_finite() || (x_coord == 0.0 && y_coord == 0.0) {
        return None;
    }
    let value = y_coord.atan2(x_coord);
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn round_directed(value: f64, digits: f64, away: bool) -> Option<f64> {
    if !value.is_finite() || !digits.is_finite() {
        return None;
    }
    let places = digits.trunc();
    if !(-10.0..=10.0).contains(&places) {
        return None;
    }
    let scale = 10f64.powi(places as i32);
    if !scale.is_finite() {
        return None;
    }
    let scaled = value * scale;
    if !scaled.is_finite() {
        return None;
    }
    let rounded = if away {
        if scaled >= 0.0 {
            scaled.ceil()
        } else {
            scaled.floor()
        }
    } else {
        scaled.trunc()
    };
    let result = rounded / scale;
    if result.is_finite() {
        Some(result)
    } else {
        None
    }
}

fn power_excel(base: f64, exponent: f64) -> Option<f64> {
    if !base.is_finite() || !exponent.is_finite() {
        return None;
    }
    if base < 0.0 && exponent.fract() != 0.0 {
        return None;
    }
    let value = base.powf(exponent);
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn mod_excel(number: f64, divisor: f64) -> Option<f64> {
    if !number.is_finite() || !divisor.is_finite() || divisor == 0.0 {
        return None;
    }
    let quotient = (number / divisor).floor();
    let value = number - divisor * quotient;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn sign_excel(number: f64) -> Option<f64> {
    if !number.is_finite() {
        return None;
    }
    if number == 0.0 {
        Some(0.0)
    } else if number > 0.0 {
        Some(1.0)
    } else {
        Some(-1.0)
    }
}

fn quotient_excel(number: f64, divisor: f64) -> Option<f64> {
    if !number.is_finite()
        || !divisor.is_finite()
        || divisor == 0.0
        || number.abs() >= 1e15
        || divisor.abs() >= 1e15
    {
        return None;
    }
    let value = (number / divisor).trunc();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

/// Round away from zero. `odd` selects the next odd integer; otherwise the next even one.
fn even_odd_excel(number: f64, odd: bool) -> Option<f64> {
    if !number.is_finite() || number.abs() >= 1e15 {
        return None;
    }
    let sign = if number < 0.0 { -1.0 } else { 1.0 };
    let mut magnitude = number.abs().ceil();
    let even_magnitude = magnitude % 2.0 == 0.0;
    if odd == even_magnitude {
        magnitude += 1.0;
    }
    let value = sign * magnitude;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn parity_excel(number: f64, odd: bool) -> Option<f64> {
    if !number.is_finite() || number.abs() >= 1e15 {
        return None;
    }
    let is_odd = number.trunc() as i64 % 2 != 0;
    Some(if is_odd == odd { 1.0 } else { 0.0 })
}

fn bit_whole(number: f64) -> Option<u64> {
    if !number.is_finite() || number < 0.0 || number >= 281_474_976_710_656.0 {
        return None;
    }
    Some(number.trunc() as u64)
}

fn bit_excel(left: f64, right: f64, and: bool, xor: bool) -> Option<f64> {
    let left = bit_whole(left)?;
    let right = bit_whole(right)?;
    let value = if and {
        left & right
    } else if xor {
        left ^ right
    } else {
        left | right
    };
    Some(value as f64)
}

fn bit_shift(number: f64, shift: f64, right: bool) -> Option<f64> {
    let number = bit_whole(number)?;
    if !shift.is_finite() || shift.trunc().abs() > 53.0 {
        return None;
    }
    let mut places = shift.trunc() as i64;
    if right {
        places = -places;
    }
    let value = if places >= 0 {
        if places >= 48 && number != 0 {
            return None;
        }
        let shifted = number.checked_shl(places as u32)?;
        if shifted >= 281_474_976_710_656 {
            return None;
        }
        shifted
    } else {
        number >> ((-places) as u32)
    };
    Some(value as f64)
}

fn mround_excel(number: f64, multiple: f64) -> Option<f64> {
    if !number.is_finite()
        || !multiple.is_finite()
        || number.abs() >= 1e15
        || multiple.abs() >= 1e15
    {
        return None;
    }
    if multiple == 0.0 {
        return if number == 0.0 { Some(0.0) } else { None };
    }
    if number == 0.0 {
        return Some(0.0);
    }
    if number.signum() != multiple.signum() {
        return None;
    }
    let steps = (number / multiple).abs();
    let value = steps.round() * multiple.abs() * number.signum();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn step_multiple(number: f64, significance: f64, away: bool) -> Option<f64> {
    if !number.is_finite()
        || !significance.is_finite()
        || number.abs() >= 1e15
        || significance.abs() >= 1e15
    {
        return None;
    }
    if significance == 0.0 {
        return if away { Some(0.0) } else { None };
    }
    if number == 0.0 {
        return Some(0.0);
    }
    if number.signum() != significance.signum() {
        return None;
    }
    let steps = (number / significance).abs();
    let rounded = if away { steps.ceil() } else { steps.floor() };
    let value = rounded * significance.abs() * number.signum();
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

fn roman_excel(number: f64) -> Option<String> {
    if !number.is_finite() {
        return None;
    }
    let number = number.trunc();
    if number < 1.0 || number > 3999.0 {
        return None;
    }
    let mut remaining = number as i32;
    let glyphs = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (value, glyph) in glyphs {
        while remaining >= value {
            out.push_str(glyph);
            remaining -= value;
        }
    }
    Some(out)
}

fn arabic_excel(text: &str) -> Option<f64> {
    if text.is_empty() || text.len() > 15 || !text.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        return None;
    }
    let upper = text.to_ascii_uppercase();
    let mut number = 1i32;
    while number <= 3999 {
        let roman = roman_excel(f64::from(number))?;
        if roman == upper {
            return Some(f64::from(number));
        }
        number += 1;
    }
    None
}

fn code_excel(text: &str) -> Option<f64> {
    text.chars().next().map(|ch| u32::from(ch) as f64)
}

fn char_excel(code: f64) -> Option<String> {
    if !code.is_finite() || code < 1.0 || code > 0x10_FFFF as f64 {
        return None;
    }
    let code = code.trunc() as u32;
    char::from_u32(code).map(|ch| ch.to_string())
}

fn clean_excel(text: &str) -> String {
    text.chars().filter(|ch| u32::from(*ch) >= 32).collect()
}

fn proper_excel(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word_start = true;
    for ch in text.chars() {
        if ch.is_alphabetic() {
            if word_start {
                out.extend(ch.to_uppercase());
            } else {
                out.extend(ch.to_lowercase());
            }
            word_start = false;
        } else {
            out.push(ch);
            word_start = true;
        }
    }
    out
}

fn text_join(delim: &str, ignore_empty: bool, parts: &[String]) -> Option<String> {
    let mut out = String::new();
    let mut count = 0usize;
    let mut started = false;
    for part in parts {
        if ignore_empty && part.is_empty() {
            continue;
        }
        if started {
            for ch in delim.chars() {
                count += 1;
                if count > 32_767 {
                    return None;
                }
                out.push(ch);
            }
        }
        started = true;
        for ch in part.chars() {
            count += 1;
            if count > 32_767 {
                return None;
            }
            out.push(ch);
        }
    }
    Some(out)
}

fn note_presence(value: &CalcValue, present: &mut f64, blank: &mut f64) -> Option<()> {
    match value {
        CalcValue::Num(number) if number.is_finite() => *present += 1.0,
        CalcValue::Num(_) => return None,
        CalcValue::Text(text) if text.is_empty() => *blank += 1.0,
        CalcValue::Text(_) => *present += 1.0,
    }
    Some(())
}

fn gcd_u64(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let next = left % right;
        left = right;
        right = next;
    }
    left
}

fn whole_arg(number: f64) -> Option<u64> {
    if !number.is_finite() || number < 0.0 || number >= 1e15 {
        return None;
    }
    Some(number.trunc() as u64)
}

fn gcd_excel(args: &[f64]) -> Option<f64> {
    if args.is_empty() {
        return None;
    }
    let mut acc = 0u64;
    for number in args {
        acc = gcd_u64(acc, whole_arg(*number)?);
    }
    Some(acc as f64)
}

fn lcm_excel(args: &[f64]) -> Option<f64> {
    if args.is_empty() {
        return None;
    }
    let mut acc = 1u64;
    for number in args {
        let next = whole_arg(*number)?;
        if next == 0 || acc == 0 {
            return Some(0.0);
        }
        let divisor = gcd_u64(acc, next);
        acc = acc / divisor * next;
        if acc as f64 >= 1e15 {
            return None;
        }
    }
    Some(acc as f64)
}

fn text_count(count: f64) -> Option<usize> {
    if !count.is_finite() || count < 0.0 || count > 32_767.0 {
        return None;
    }
    Some(count.trunc() as usize)
}

fn slice_text(text: &str, start: usize, count: f64) -> Option<String> {
    let count = text_count(count)?;
    Some(text.chars().skip(start).take(count).collect())
}

fn slice_mid(text: &str, start: f64, count: f64) -> Option<String> {
    if !start.is_finite() || start < 1.0 || start > 32_767.0 {
        return None;
    }
    let start = (start.trunc() as usize).saturating_sub(1);
    slice_text(text, start, count)
}

/// Excel `TRIM`: drop leading and trailing U+0020 and collapse inner runs of that space.
fn trim_spaces(text: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    let mut started = false;
    for ch in text.chars() {
        if ch == ' ' {
            if started {
                gap = true;
            }
        } else {
            if gap {
                out.push(' ');
                gap = false;
            }
            started = true;
            out.push(ch);
        }
    }
    out
}

fn substitute_text(text: &str, old: &str, new: &str, instance: Option<f64>) -> Option<String> {
    if old.is_empty() {
        return None;
    }
    let nth = match instance {
        None => None,
        Some(number) => {
            let count = text_count(number)?;
            if count < 1 {
                return None;
            }
            Some(count)
        }
    };
    let Some(nth) = nth else {
        return Some(text.replace(old, new));
    };
    let mut seen = 0usize;
    let mut rest = text;
    let mut out = String::new();
    while let Some(at) = rest.find(old) {
        seen += 1;
        if seen == nth {
            out.push_str(&rest[..at]);
            out.push_str(new);
            out.push_str(&rest[at + old.len()..]);
            return Some(out);
        }
        let next = at + old.len();
        out.push_str(&rest[..next]);
        rest = &rest[next..];
    }
    Some(text.to_string())
}

fn find_scalar(haystack: &str, needle: &str, start: f64, ignore_ascii_case: bool) -> Option<f64> {
    if needle.is_empty() || !start.is_finite() || start < 1.0 || start > 32_767.0 {
        return None;
    }
    let start = (start.trunc() as usize).saturating_sub(1);
    let hay: Vec<char> = haystack.chars().collect();
    let ned: Vec<char> = needle.chars().collect();
    if start > hay.len() || ned.len() > hay.len().saturating_sub(start) {
        return None;
    }
    let last = hay.len() - ned.len();
    for index in start..=last {
        let matched = hay[index..index + ned.len()]
            .iter()
            .zip(&ned)
            .all(|(left, right)| {
                if ignore_ascii_case {
                    left.eq_ignore_ascii_case(right)
                } else {
                    left == right
                }
            });
        if matched {
            return Some((index + 1) as f64);
        }
    }
    None
}

fn rept_text(text: &str, count: f64) -> Option<String> {
    let count = text_count(count)?;
    if text.is_empty() || count == 0 {
        return Some(String::new());
    }
    let chars = text.chars().count();
    if chars.saturating_mul(count) > 32_767 {
        return None;
    }
    Some(text.repeat(count))
}

fn replace_span(text: &str, start: f64, count: f64, new: &str) -> Option<String> {
    if !start.is_finite() || start < 1.0 || start > 32_767.0 {
        return None;
    }
    let count = text_count(count)?;
    let chars: Vec<char> = text.chars().collect();
    let from = (start.trunc() as usize).saturating_sub(1);
    if from > chars.len() {
        return None;
    }
    let to = (from + count).min(chars.len());
    let mut out = String::new();
    out.extend(chars[..from].iter());
    out.push_str(new);
    out.extend(chars[to..].iter());
    if out.chars().count() > 32_767 {
        return None;
    }
    Some(out)
}

fn value_excel(text: &str) -> Option<f64> {
    let text = text.trim_matches(' ');
    if text.is_empty() {
        return None;
    }
    let bytes = text.as_bytes();
    let (sign, digits) = match bytes.first() {
        Some(b'+') => (1.0, &bytes[1..]),
        Some(b'-') => (-1.0, &bytes[1..]),
        _ => (1.0, bytes),
    };
    if digits.is_empty() || !digits.iter().any(u8::is_ascii_digit) {
        return None;
    }
    let mut saw_dot = false;
    for byte in digits {
        if *byte == b'.' {
            if saw_dot {
                return None;
            }
            saw_dot = true;
            continue;
        }
        if !byte.is_ascii_digit() {
            return None;
        }
    }
    let parsed = std::str::from_utf8(digits).ok()?.parse::<f64>().ok()?;
    if !parsed.is_finite() {
        return None;
    }
    let value = sign * parsed;
    if value.is_finite() {
        Some(value)
    } else {
        None
    }
}

#[derive(Clone, Copy)]
enum CriterionOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

fn criterion_op(text: &str) -> Option<(CriterionOp, &str)> {
    let text = text.trim_matches(' ');
    for (prefix, op) in [
        (">=", CriterionOp::Ge),
        ("<=", CriterionOp::Le),
        ("<>", CriterionOp::Ne),
        (">", CriterionOp::Gt),
        ("<", CriterionOp::Lt),
        ("=", CriterionOp::Eq),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return Some((op, rest));
        }
    }
    None
}

fn number_matches(cell: f64, op: CriterionOp, target: f64) -> bool {
    let same = (cell - target).abs() < 1e-9;
    match op {
        CriterionOp::Eq => same,
        CriterionOp::Ne => !same,
        CriterionOp::Gt => !same && cell > target,
        CriterionOp::Lt => !same && cell < target,
        CriterionOp::Ge => same || cell > target,
        CriterionOp::Le => same || cell < target,
    }
}

fn compile_criterion(criteria: &CalcValue) -> Option<(CriterionOp, f64)> {
    match criteria {
        CalcValue::Num(target) if target.is_finite() => Some((CriterionOp::Eq, *target)),
        CalcValue::Num(_) => None,
        CalcValue::Text(text) => {
            let (op, rest) = criterion_op(text)?;
            Some((op, value_excel(rest)?))
        }
    }
}

fn open_tag_inline(open: &str) -> Option<String> {
    if open.contains("t=") {
        return None;
    }
    let end = open.rfind('>')?;
    let mut out = String::new();
    out.push_str(&open[..end]);
    out.push_str(" t=\"inlineStr\">");
    Some(out)
}

fn inline_text(body: &str) -> Option<String> {
    let start = if let Some(at) = body.find("<t>") {
        at + 3
    } else {
        let at = body.find("<t ")?;
        let rest = &body[at..];
        let close = rest.find('>')?;
        at + close + 1
    };
    let rest = &body[start..];
    let end = rest.find("</t>")?;
    Some(unescape_xml(&rest[..end]))
}

fn unescape_xml(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';') else {
            out.push_str(rest);
            return out;
        };
        match &rest[..=end] {
            "&amp;" => out.push('&'),
            "&lt;" => out.push('<'),
            "&gt;" => out.push('>'),
            "&quot;" => out.push('"'),
            "&apos;" => out.push('\''),
            _ => {
                out.push('&');
                rest = &rest[1..];
                continue;
            }
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

struct CalcParser<'a> {
    bytes: &'a [u8],
    index: usize,
}

enum CmpOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}

impl<'a> CalcParser<'a> {
    fn skip(&mut self) {
        while self
            .bytes
            .get(self.index)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            self.index += 1;
        }
    }

    fn compare(&mut self, env: &mut CalcEnv<'_>) -> Option<CalcValue> {
        let left = self.join(env)?;
        self.skip();
        let Some(op) = self.cmp_op() else {
            return Some(left);
        };
        let right = self.join(env)?;
        let left = calc_num(left)?;
        let right = calc_num(right)?;
        let same = (left - right).abs() < 1e-9;
        let flag = match op {
            CmpOp::Eq => same,
            CmpOp::Ne => !same,
            CmpOp::Lt => left < right && !same,
            CmpOp::Gt => left > right && !same,
            CmpOp::Le => left < right || same,
            CmpOp::Ge => left > right || same,
        };
        Some(CalcValue::Num(if flag { 1.0 } else { 0.0 }))
    }

    fn join(&mut self, env: &mut CalcEnv<'_>) -> Option<CalcValue> {
        let mut value = self.expr(env)?;
        loop {
            self.skip();
            if self.bytes.get(self.index) != Some(&b'&') {
                break;
            }
            self.index += 1;
            let right = self.expr(env)?;
            value = CalcValue::Text(format!("{}{}", calc_text(&value), calc_text(&right)));
        }
        Some(value)
    }

    fn cmp_op(&mut self) -> Option<CmpOp> {
        match self.bytes.get(self.index).copied() {
            Some(b'=') => {
                self.index += 1;
                Some(CmpOp::Eq)
            }
            Some(b'<') => {
                self.index += 1;
                if self.bytes.get(self.index) == Some(&b'>') {
                    self.index += 1;
                    Some(CmpOp::Ne)
                } else if self.bytes.get(self.index) == Some(&b'=') {
                    self.index += 1;
                    Some(CmpOp::Le)
                } else {
                    Some(CmpOp::Lt)
                }
            }
            Some(b'>') => {
                self.index += 1;
                if self.bytes.get(self.index) == Some(&b'=') {
                    self.index += 1;
                    Some(CmpOp::Ge)
                } else {
                    Some(CmpOp::Gt)
                }
            }
            _ => None,
        }
    }

    fn expr(&mut self, env: &mut CalcEnv<'_>) -> Option<CalcValue> {
        let mut value = self.term(env)?;
        loop {
            self.skip();
            match self.bytes.get(self.index).copied() {
                Some(b'+') => {
                    self.index += 1;
                    let right = self.term(env)?;
                    value = CalcValue::Num(calc_num(value)? + calc_num(right)?);
                }
                Some(b'-') => {
                    self.index += 1;
                    let right = self.term(env)?;
                    value = CalcValue::Num(calc_num(value)? - calc_num(right)?);
                }
                _ => break,
            }
        }
        Some(value)
    }

    fn term(&mut self, env: &mut CalcEnv<'_>) -> Option<CalcValue> {
        let mut value = self.factor(env)?;
        loop {
            self.skip();
            match self.bytes.get(self.index).copied() {
                Some(b'*') => {
                    self.index += 1;
                    let right = self.factor(env)?;
                    value = CalcValue::Num(calc_num(value)? * calc_num(right)?);
                }
                Some(b'/') => {
                    self.index += 1;
                    let right = calc_num(self.factor(env)?)?;
                    if right == 0.0 {
                        return None;
                    }
                    value = CalcValue::Num(calc_num(value)? / right);
                }
                _ => break,
            }
        }
        Some(value)
    }

    fn factor(&mut self, env: &mut CalcEnv<'_>) -> Option<CalcValue> {
        self.skip();
        if self.bytes.get(self.index) == Some(&b'+') {
            self.index += 1;
            return self.factor(env);
        }
        if self.bytes.get(self.index) == Some(&b'-') {
            self.index += 1;
            return Some(CalcValue::Num(-calc_num(self.factor(env)?)?));
        }
        if self.bytes.get(self.index) == Some(&b'(') {
            self.index += 1;
            let value = self.compare(env)?;
            self.skip();
            if self.bytes.get(self.index) != Some(&b')') {
                return None;
            }
            self.index += 1;
            return Some(value);
        }
        if self.bytes.get(self.index) == Some(&b'"') {
            return self.quoted().map(CalcValue::Text);
        }
        if self
            .bytes
            .get(self.index)
            .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'.')
        {
            return self.number().map(CalcValue::Num);
        }
        if self.bytes.get(self.index) == Some(&b'\'') {
            let name = self.quoted_sheet()?;
            return self.foreign_cell(&name, env);
        }
        let word = self.word()?;
        self.skip();
        if self.bytes.get(self.index) == Some(&b'(') {
            self.index += 1;
            if word.eq_ignore_ascii_case("IF") {
                let cond = calc_num(self.compare(env)?)?;
                self.skip();
                if self.bytes.get(self.index) != Some(&b',') {
                    return None;
                }
                self.index += 1;
                let yes = calc_num(self.compare(env)?)?;
                self.skip();
                if self.bytes.get(self.index) != Some(&b',') {
                    return None;
                }
                self.index += 1;
                let no = calc_num(self.compare(env)?)?;
                self.skip();
                if self.bytes.get(self.index) != Some(&b')') {
                    return None;
                }
                self.index += 1;
                return Some(CalcValue::Num(if cond != 0.0 { yes } else { no }));
            }
            if word.eq_ignore_ascii_case("IFERROR") {
                let (args, end) = split_top_args(self.bytes, self.index)?;
                if args.len() != 2 {
                    return None;
                }
                let first = args[0];
                let second = args[1];
                self.index = end;
                if let Some(value) = eval_slice(&self.bytes[first.0..first.1], env) {
                    return Some(value);
                }
                return eval_slice(&self.bytes[second.0..second.1], env);
            }
            if word.eq_ignore_ascii_case("CHOOSE") {
                let (args, end) = split_top_args(self.bytes, self.index)?;
                self.index = end;
                if args.len() < 2 || args.len() > 255 {
                    return None;
                }
                let index = args[0];
                let index = calc_num(eval_slice(&self.bytes[index.0..index.1], env)?)?;
                if !index.is_finite() {
                    return None;
                }
                let index = index.trunc() as i64;
                if index < 1 || index as usize >= args.len() {
                    return None;
                }
                let chosen = args[index as usize];
                return eval_slice(&self.bytes[chosen.0..chosen.1], env);
            }
            if word.eq_ignore_ascii_case("SWITCH") {
                let (args, end) = split_top_args(self.bytes, self.index)?;
                self.index = end;
                if args.len() < 3 || args.len() > 255 {
                    return None;
                }
                let expr_at = args[0];
                let expr = eval_slice(&self.bytes[expr_at.0..expr_at.1], env)?;
                let default_at = if args.len() % 2 == 0 {
                    Some(args.len() - 1)
                } else {
                    None
                };
                let pair_end = default_at.unwrap_or(args.len());
                let mut pair = 1usize;
                while pair + 1 < pair_end {
                    let match_at = args[pair];
                    let matched = eval_slice(&self.bytes[match_at.0..match_at.1], env)?;
                    if values_match(&expr, &matched) {
                        let result = args[pair + 1];
                        return eval_slice(&self.bytes[result.0..result.1], env);
                    }
                    pair += 2;
                }
                let Some(default_at) = default_at else {
                    return None;
                };
                let default = args[default_at];
                return eval_slice(&self.bytes[default.0..default.1], env);
            }
            if word.eq_ignore_ascii_case("IFS") {
                let (args, end) = split_top_args(self.bytes, self.index)?;
                self.index = end;
                if args.is_empty() || args.len() > 254 || args.len() % 2 == 1 {
                    return None;
                }
                let mut pair = 0usize;
                while pair + 1 < args.len() {
                    let cond_at = args[pair];
                    let cond = eval_slice(&self.bytes[cond_at.0..cond_at.1], env)?;
                    let Some(number) = calc_num(cond) else {
                        return None;
                    };
                    if !number.is_finite() {
                        return None;
                    }
                    if number != 0.0 {
                        let result = args[pair + 1];
                        return eval_slice(&self.bytes[result.0..result.1], env);
                    }
                    pair += 2;
                }
                return None;
            }
            if word.eq_ignore_ascii_case("ABS") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return Some(CalcValue::Num(number.abs()));
            }
            if word.eq_ignore_ascii_case("INT") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                if !number.is_finite() {
                    return None;
                }
                return Some(CalcValue::Num(number.floor()));
            }
            if word.eq_ignore_ascii_case("SQRT") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                if number < 0.0 || !number.is_finite() {
                    return None;
                }
                let value = number.sqrt();
                if !value.is_finite() {
                    return None;
                }
                return Some(CalcValue::Num(value));
            }
            if word.eq_ignore_ascii_case("FISHER") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return fisher_excel(number).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("FISHERINV") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return fisher_inv_excel(number).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("SQRTPI") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return sqrt_pi_excel(number).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("LN") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return finite_positive_log(number).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("LOG10") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return log_base(number, 10.0).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("LOG") {
                let number = calc_num(self.compare(env)?)?;
                let base = self.optional_number(env, 10.0)?;
                return log_base(number, base).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("EXP") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return exp_excel(number).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("FACT") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return fact_excel(number).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("FACTDOUBLE") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return fact_double_excel(number).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("POISSON") || word.eq_ignore_ascii_case("POISSON.DIST") {
                let x_value = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let mean = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let flag = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                if !flag.is_finite() {
                    return None;
                }
                return poisson_excel(x_value, mean, flag != 0.0).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("BINOM.DIST") || word.eq_ignore_ascii_case("BINOMDIST") {
                let successes = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let trials = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let probability = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let flag = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                if !flag.is_finite() {
                    return None;
                }
                return binom_dist_excel(successes, trials, probability, flag != 0.0)
                    .map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("EXPON.DIST") || word.eq_ignore_ascii_case("EXPONDIST") {
                let x_value = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let lambda = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let flag = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                if !flag.is_finite() {
                    return None;
                }
                return expon_dist_excel(x_value, lambda, flag != 0.0).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("NEGBINOM.DIST")
                || word.eq_ignore_ascii_case("NEGBINOMDIST")
            {
                let legacy = word.eq_ignore_ascii_case("NEGBINOMDIST");
                let failures = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let successes = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let probability = calc_num(self.compare(env)?)?;
                let cumulative = if legacy {
                    self.close_paren()?;
                    false
                } else {
                    self.require_comma()?;
                    let flag = calc_num(self.compare(env)?)?;
                    self.close_paren()?;
                    if !flag.is_finite() {
                        return None;
                    }
                    flag != 0.0
                };
                return negbinom_dist_excel(failures, successes, probability, cumulative)
                    .map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("HYPGEOM.DIST") || word.eq_ignore_ascii_case("HYPGEOMDIST")
            {
                let legacy = word.eq_ignore_ascii_case("HYPGEOMDIST");
                let sample_s = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let number_sample = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let population_s = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let number_pop = calc_num(self.compare(env)?)?;
                let cumulative = if legacy {
                    self.close_paren()?;
                    false
                } else {
                    self.require_comma()?;
                    let flag = calc_num(self.compare(env)?)?;
                    self.close_paren()?;
                    if !flag.is_finite() {
                        return None;
                    }
                    flag != 0.0
                };
                return hypgeom_dist_excel(
                    sample_s,
                    number_sample,
                    population_s,
                    number_pop,
                    cumulative,
                )
                .map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("WEIBULL.DIST") || word.eq_ignore_ascii_case("WEIBULL") {
                let x_value = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let alpha = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let beta = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let flag = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                if !flag.is_finite() {
                    return None;
                }
                return weibull_dist_excel(x_value, alpha, beta, flag != 0.0).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("GAMMALN") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return ln_gamma_excel(number).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("GAMMA") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return gamma_excel(number).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("GAMMA.DIST") || word.eq_ignore_ascii_case("GAMMADIST") {
                let x_value = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let alpha = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let beta = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let flag = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                if !flag.is_finite() {
                    return None;
                }
                return gamma_dist_excel(x_value, alpha, beta, flag != 0.0).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("GAMMA.INV") || word.eq_ignore_ascii_case("GAMMAINV") {
                let probability = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let alpha = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let beta = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return gamma_inv_excel(probability, alpha, beta).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("BINOM.INV") || word.eq_ignore_ascii_case("CRITBINOM") {
                let trials = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let probability = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let alpha = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return binom_inv_excel(trials, probability, alpha).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("CHISQ.DIST.RT") || word.eq_ignore_ascii_case("CHIDIST") {
                let x_value = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let degrees = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return chisq_rt_excel(x_value, degrees).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("CHISQ.DIST") {
                let x_value = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let degrees = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let flag = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                if !flag.is_finite() {
                    return None;
                }
                return chisq_dist_excel(x_value, degrees, flag != 0.0).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("CHISQ.INV.RT") || word.eq_ignore_ascii_case("CHIINV") {
                let probability = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let degrees = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return chisq_inv_rt_excel(probability, degrees).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("CHISQ.INV") {
                let probability = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let degrees = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return chisq_inv_excel(probability, degrees).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("CHISQ.TEST") || word.eq_ignore_ascii_case("CHITEST") {
                let (actual, rows, cols) = self.cell_block(env)?;
                self.require_comma()?;
                let (expected, expected_rows, expected_cols) = self.cell_block(env)?;
                self.close_paren()?;
                if rows != expected_rows || cols != expected_cols {
                    return None;
                }
                let degrees = if rows == 1 {
                    cols.saturating_sub(1)
                } else if cols == 1 {
                    rows.saturating_sub(1)
                } else {
                    rows.saturating_sub(1)
                        .saturating_mul(cols.saturating_sub(1))
                };
                if degrees < 1 {
                    return None;
                }
                let mut pairs = Vec::new();
                for (actual_address, expected_address) in actual.iter().zip(expected) {
                    let Some(CalcValue::Num(actual_n)) = self.cell_value(actual_address, env)
                    else {
                        return None;
                    };
                    let Some(CalcValue::Num(expected_n)) = self.cell_value(&expected_address, env)
                    else {
                        return None;
                    };
                    pairs.push((actual_n, expected_n));
                }
                let stat = chisq_test_stat(&pairs)?;
                return chisq_rt_excel(stat, f64::from(degrees)).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("NORM.S.DIST") || word.eq_ignore_ascii_case("NORMSDIST") {
                let legacy = word.eq_ignore_ascii_case("NORMSDIST");
                let x_value = calc_num(self.compare(env)?)?;
                let cumulative = if legacy {
                    self.close_paren()?;
                    true
                } else {
                    self.require_comma()?;
                    let flag = calc_num(self.compare(env)?)?;
                    self.close_paren()?;
                    if !flag.is_finite() {
                        return None;
                    }
                    flag != 0.0
                };
                return norms_dist_excel(x_value, cumulative).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("NORM.DIST") || word.eq_ignore_ascii_case("NORMDIST") {
                let x_value = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let mean = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let scale = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let flag = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                if !flag.is_finite() {
                    return None;
                }
                return norm_dist_excel(x_value, mean, scale, flag != 0.0).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("NORM.S.INV") || word.eq_ignore_ascii_case("NORMSINV") {
                let probability = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return norms_inv_excel(probability).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("NORM.INV") || word.eq_ignore_ascii_case("NORMINV") {
                let probability = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let mean = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let scale = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return norm_inv_excel(probability, mean, scale).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("LOGNORM.INV") || word.eq_ignore_ascii_case("LOGINV") {
                let probability = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let mean = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let scale = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return lognorm_inv_excel(probability, mean, scale).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("CONFIDENCE.NORM")
                || word.eq_ignore_ascii_case("CONFIDENCE")
            {
                let alpha = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let stdev = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let size = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return confidence_norm_excel(alpha, stdev, size).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("ERF") {
                let lower = calc_num(self.compare(env)?)?;
                self.skip();
                if self.bytes.get(self.index) == Some(&b')') {
                    self.index += 1;
                    return erf_excel(lower).map(CalcValue::Num);
                }
                self.require_comma()?;
                let upper = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                let value = erf_excel(upper)? - erf_excel(lower)?;
                return if value.is_finite() {
                    Some(CalcValue::Num(value))
                } else {
                    None
                };
            }
            if word.eq_ignore_ascii_case("ERFC") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                let value = 1.0 - erf_excel(number)?;
                return if value.is_finite() {
                    Some(CalcValue::Num(value))
                } else {
                    None
                };
            }
            if word.eq_ignore_ascii_case("GAUSS") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                let value = norms_dist_excel(number, true)? - 0.5;
                return if value.is_finite() {
                    Some(CalcValue::Num(value))
                } else {
                    None
                };
            }
            if word.eq_ignore_ascii_case("PHI") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return norms_dist_excel(number, false).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("LOGNORM.DIST") || word.eq_ignore_ascii_case("LOGNORMDIST")
            {
                let legacy = word.eq_ignore_ascii_case("LOGNORMDIST");
                let x_value = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let mean = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let scale = calc_num(self.compare(env)?)?;
                let cumulative = if legacy {
                    self.close_paren()?;
                    true
                } else {
                    self.require_comma()?;
                    let flag = calc_num(self.compare(env)?)?;
                    self.close_paren()?;
                    if !flag.is_finite() {
                        return None;
                    }
                    flag != 0.0
                };
                return lognorm_dist_excel(x_value, mean, scale, cumulative).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("BINOM.DIST.RANGE") {
                let trials = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let probability = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let low = calc_num(self.compare(env)?)?;
                self.skip();
                let high = if self.bytes.get(self.index) == Some(&b')') {
                    self.index += 1;
                    low
                } else {
                    self.require_comma()?;
                    let high = calc_num(self.compare(env)?)?;
                    self.close_paren()?;
                    high
                };
                return binom_range_excel(trials, probability, low, high).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("Z.TEST") || word.eq_ignore_ascii_case("ZTEST") {
                let addresses = self.cell_range(env)?;
                self.require_comma()?;
                let target = calc_num(self.compare(env)?)?;
                self.skip();
                let sigma = if self.bytes.get(self.index) == Some(&b')') {
                    self.index += 1;
                    None
                } else {
                    self.require_comma()?;
                    let sigma = calc_num(self.compare(env)?)?;
                    self.close_paren()?;
                    Some(sigma)
                };
                let mut values = Vec::new();
                for address in addresses {
                    if let Some(CalcValue::Num(value)) = self.cell_value(&address, env) {
                        if value.is_finite() {
                            values.push(value);
                        }
                    }
                }
                return z_test_excel(&values, target, sigma).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("PROB") {
                let xs = self.cell_range(env)?;
                self.require_comma()?;
                let probabilities = self.cell_range(env)?;
                if xs.len() != probabilities.len() {
                    return None;
                }
                self.require_comma()?;
                let lower = calc_num(self.compare(env)?)?;
                self.skip();
                let upper = if self.bytes.get(self.index) == Some(&b')') {
                    self.index += 1;
                    lower
                } else {
                    self.require_comma()?;
                    let upper = calc_num(self.compare(env)?)?;
                    self.close_paren()?;
                    upper
                };
                let mut pairs = Vec::new();
                for (x_address, probability_address) in xs.iter().zip(probabilities) {
                    let Some(CalcValue::Num(x_value)) = self.cell_value(x_address, env) else {
                        continue;
                    };
                    let Some(CalcValue::Num(probability)) =
                        self.cell_value(&probability_address, env)
                    else {
                        continue;
                    };
                    if x_value.is_finite() && probability.is_finite() {
                        pairs.push((x_value, probability));
                    }
                }
                return prob_excel(&pairs, lower, upper).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("SERIESSUM") {
                let x_value = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let first = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let step = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let addresses = self.cell_range(env)?;
                self.close_paren()?;
                let mut coefficients = Vec::new();
                for address in addresses {
                    match self.cell_value(&address, env) {
                        None => coefficients.push(0.0),
                        Some(CalcValue::Num(value)) if value.is_finite() => {
                            coefficients.push(value);
                        }
                        _ => return None,
                    }
                }
                return series_sum_excel(x_value, first, step, &coefficients).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("SIN")
                || word.eq_ignore_ascii_case("COS")
                || word.eq_ignore_ascii_case("TAN")
            {
                let kind = word.to_ascii_uppercase();
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return trig_excel(number, &kind).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("SEC")
                || word.eq_ignore_ascii_case("CSC")
                || word.eq_ignore_ascii_case("COT")
            {
                let kind = word.to_ascii_uppercase();
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return reciprocal_trig_excel(number, &kind).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("RADIANS") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return radians_excel(number).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("DEGREES") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return degrees_excel(number).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("ASIN") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return asin_excel(number).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("ACOS") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return acos_excel(number).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("ATAN2") {
                let x_coord = calc_num(self.compare(env)?)?;
                let y_coord = self.comma_number(env)?;
                return atan2_excel(x_coord, y_coord).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("ATAN") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return atan_excel(number).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("SINH")
                || word.eq_ignore_ascii_case("COSH")
                || word.eq_ignore_ascii_case("TANH")
            {
                let kind = word.to_ascii_uppercase();
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return hyper_excel(number, &kind).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("ASINH")
                || word.eq_ignore_ascii_case("ACOSH")
                || word.eq_ignore_ascii_case("ATANH")
            {
                let kind = word.to_ascii_uppercase();
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return inverse_hyper_excel(number, &kind).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("SECH")
                || word.eq_ignore_ascii_case("CSCH")
                || word.eq_ignore_ascii_case("COTH")
            {
                let kind = word.to_ascii_uppercase();
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return reciprocal_hyper_excel(number, &kind).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("ACOT") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return acot_excel(number).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("ACOTH") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return acoth_excel(number).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("COMBIN") {
                let n = calc_num(self.compare(env)?)?;
                let k = self.comma_number(env)?;
                return combin_excel(n, k).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("COMBINA") {
                let n = calc_num(self.compare(env)?)?;
                let k = self.comma_number(env)?;
                return combina_excel(n, k).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("PERMUTATIONA") {
                let n = calc_num(self.compare(env)?)?;
                let k = self.comma_number(env)?;
                return permutationa_excel(n, k).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("PERMUT") {
                let n = calc_num(self.compare(env)?)?;
                let k = self.comma_number(env)?;
                return permut_excel(n, k).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("POWER") {
                let base = calc_num(self.compare(env)?)?;
                let exponent = self.comma_number(env)?;
                return power_excel(base, exponent).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("MOD") {
                let number = calc_num(self.compare(env)?)?;
                let divisor = self.comma_number(env)?;
                return mod_excel(number, divisor).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("PI") {
                self.close_paren()?;
                return Some(CalcValue::Num(std::f64::consts::PI));
            }
            if word.eq_ignore_ascii_case("SIGN") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return sign_excel(number).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("GESTEP") {
                let number = calc_num(self.compare(env)?)?;
                let step = self.optional_number(env, 0.0)?;
                return gestep_excel(number, step).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("DELTA") {
                let number = calc_num(self.compare(env)?)?;
                let other = self.optional_number(env, 0.0)?;
                return delta_excel(number, other).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("QUOTIENT") {
                let number = calc_num(self.compare(env)?)?;
                let divisor = self.comma_number(env)?;
                return quotient_excel(number, divisor).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("EVEN") || word.eq_ignore_ascii_case("ODD") {
                let odd = word.eq_ignore_ascii_case("ODD");
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return even_odd_excel(number, odd).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("ROUND") {
                let number = calc_num(self.compare(env)?)?;
                self.skip();
                if self.bytes.get(self.index) != Some(&b',') {
                    return None;
                }
                self.index += 1;
                let digits = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return round_excel(number, digits).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("ROUNDUP") || word.eq_ignore_ascii_case("ROUNDDOWN") {
                let away = word.eq_ignore_ascii_case("ROUNDUP");
                let number = calc_num(self.compare(env)?)?;
                let digits = self.comma_number(env)?;
                return round_directed(number, digits, away).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("TRUNC") {
                let number = calc_num(self.compare(env)?)?;
                let digits = self.optional_number(env, 0.0)?;
                return round_directed(number, digits, false).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("ISEVEN") || word.eq_ignore_ascii_case("ISODD") {
                let odd = word.eq_ignore_ascii_case("ISODD");
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return parity_excel(number, odd).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("CODE") || word.eq_ignore_ascii_case("UNICODE") {
                let text = calc_text(&self.compare(env)?);
                self.close_paren()?;
                return code_excel(&text).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("CHAR") || word.eq_ignore_ascii_case("UNICHAR") {
                let code = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return char_excel(code).map(CalcValue::Text);
            }
            if word.eq_ignore_ascii_case("CLEAN") {
                let text = calc_text(&self.compare(env)?);
                self.close_paren()?;
                return Some(CalcValue::Text(clean_excel(&text)));
            }
            if word.eq_ignore_ascii_case("PROPER") {
                let text = calc_text(&self.compare(env)?);
                self.close_paren()?;
                return Some(CalcValue::Text(proper_excel(&text)));
            }
            if word.eq_ignore_ascii_case("CEILING") || word.eq_ignore_ascii_case("FLOOR") {
                let away = word.eq_ignore_ascii_case("CEILING");
                let number = calc_num(self.compare(env)?)?;
                let significance = self.comma_number(env)?;
                return step_multiple(number, significance, away).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("MROUND") {
                let number = calc_num(self.compare(env)?)?;
                let multiple = self.comma_number(env)?;
                return mround_excel(number, multiple).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("SUMIF") {
                return self.sum_if(env).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("COUNTIF") || word.eq_ignore_ascii_case("COUNTIFS") {
                let matched = self.matched_numbers(env)?;
                return Some(CalcValue::Num(matched.len() as f64));
            }
            if word.eq_ignore_ascii_case("AVERAGEIF") {
                let matched = self.matched_numbers(env)?;
                if matched.is_empty() {
                    return None;
                }
                let average = matched.iter().sum::<f64>() / matched.len() as f64;
                if !average.is_finite() {
                    return None;
                }
                return Some(CalcValue::Num(average));
            }
            if word.eq_ignore_ascii_case("SUMPRODUCT") {
                return self.sum_product(env).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("SUMX2MY2")
                || word.eq_ignore_ascii_case("SUMX2PY2")
                || word.eq_ignore_ascii_case("SUMXMY2")
            {
                let kind = if word.eq_ignore_ascii_case("SUMX2PY2") {
                    1
                } else if word.eq_ignore_ascii_case("SUMXMY2") {
                    2
                } else {
                    0
                };
                let pairs = self.paired_ranges(env)?;
                return sum_pair_excel(&pairs, kind).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("SLOPE") {
                let pairs = self.paired_ranges(env)?;
                return slope_excel(&pairs).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("INTERCEPT") {
                let pairs = self.paired_ranges(env)?;
                return intercept_excel(&pairs).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("CORREL") || word.eq_ignore_ascii_case("PEARSON") {
                let pairs = self.paired_ranges(env)?;
                return correl_excel(&pairs).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("RSQ") {
                let pairs = self.paired_ranges(env)?;
                return rsq_excel(&pairs).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("FORECAST") || word.eq_ignore_ascii_case("FORECAST.LINEAR")
            {
                let x_value = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let pairs = self.paired_ranges(env)?;
                return forecast_excel(x_value, &pairs).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("STEYX") {
                let pairs = self.paired_ranges(env)?;
                return steyx_excel(&pairs).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("COVARIANCE.P")
                || word.eq_ignore_ascii_case("COVAR")
                || word.eq_ignore_ascii_case("COVARIANCE.S")
            {
                let sample = word.eq_ignore_ascii_case("COVARIANCE.S");
                let pairs = self.paired_ranges(env)?;
                return covariance_excel(&pairs, sample).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("RANK") || word.eq_ignore_ascii_case("RANK.EQ") {
                return self.rank_call(env, false).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("RANK.AVG") {
                return self.rank_call(env, true).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("PERCENTILE")
                || word.eq_ignore_ascii_case("PERCENTILE.INC")
            {
                return self.percentile_call(env).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("PERCENTILE.EXC") {
                return self.percentile_exc_call(env).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("QUARTILE") || word.eq_ignore_ascii_case("QUARTILE.INC") {
                return self.quartile_call(env).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("QUARTILE.EXC") {
                return self.quartile_exc_call(env).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("MODE") || word.eq_ignore_ascii_case("MODE.SNGL") {
                return self.mode_call(env).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("PERCENTRANK")
                || word.eq_ignore_ascii_case("PERCENTRANK.INC")
            {
                return self.percent_rank_call(env).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("PERCENTRANK.EXC") {
                return self.percent_rank_exc_call(env).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("STANDARDIZE") {
                let x_value = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let mean = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let scale = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return standardize_excel(x_value, mean, scale).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("MINIFS") || word.eq_ignore_ascii_case("MAXIFS") {
                let max = word.eq_ignore_ascii_case("MAXIFS");
                let matched = self.ifs_values(env)?;
                let value = if matched.is_empty() {
                    0.0
                } else if max {
                    matched.into_iter().fold(f64::MIN, f64::max)
                } else {
                    matched.into_iter().fold(f64::MAX, f64::min)
                };
                return Some(CalcValue::Num(value));
            }
            if word.eq_ignore_ascii_case("SUMIFS") {
                let matched = self.ifs_values(env)?;
                let sum = matched.iter().sum::<f64>();
                if !sum.is_finite() {
                    return None;
                }
                return Some(CalcValue::Num(sum));
            }
            if word.eq_ignore_ascii_case("AVERAGEIFS") {
                let matched = self.ifs_values(env)?;
                if matched.is_empty() {
                    return None;
                }
                let average = matched.iter().sum::<f64>() / matched.len() as f64;
                if !average.is_finite() {
                    return None;
                }
                return Some(CalcValue::Num(average));
            }
            if word.eq_ignore_ascii_case("CEILING.MATH") || word.eq_ignore_ascii_case("FLOOR.MATH")
            {
                let floor = word.eq_ignore_ascii_case("FLOOR.MATH");
                let number = calc_num(self.compare(env)?)?;
                self.skip();
                if self.bytes.get(self.index) == Some(&b')') {
                    self.index += 1;
                    return math_step_excel(number, 1.0, 0.0, floor).map(CalcValue::Num);
                }
                self.require_comma()?;
                let significance = calc_num(self.compare(env)?)?;
                self.skip();
                if self.bytes.get(self.index) == Some(&b')') {
                    self.index += 1;
                    return math_step_excel(number, significance, 0.0, floor).map(CalcValue::Num);
                }
                self.require_comma()?;
                let mode = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return math_step_excel(number, significance, mode, floor).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("CONCAT") || word.eq_ignore_ascii_case("CONCATENATE") {
                return self.concat_args(env).map(CalcValue::Text);
            }
            if word.eq_ignore_ascii_case("ROMAN") {
                let number = calc_num(self.compare(env)?)?;
                self.skip();
                if self.bytes.get(self.index) == Some(&b')') {
                    self.index += 1;
                    return roman_excel(number).map(CalcValue::Text);
                }
                self.require_comma()?;
                let form = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                if !form.is_finite() || form != 0.0 {
                    return None;
                }
                return roman_excel(number).map(CalcValue::Text);
            }
            if word.eq_ignore_ascii_case("ARABIC") {
                let text = calc_text(&self.compare(env)?);
                self.close_paren()?;
                return arabic_excel(&text).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("DATE") {
                let year = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let month = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let day = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return date_excel(year, month, day)
                    .and_then(|serial| from_1900(serial, env.date1904))
                    .map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("YEAR")
                || word.eq_ignore_ascii_case("MONTH")
                || word.eq_ignore_ascii_case("DAY")
            {
                let part = word.to_ascii_uppercase();
                let serial = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                let serial = as_1900(serial, env.date1904)?;
                let (year, month, day) = excel_parts(serial)?;
                let value = if part == "YEAR" {
                    year
                } else if part == "MONTH" {
                    month
                } else {
                    day
                };
                return Some(CalcValue::Num(f64::from(value)));
            }
            if word.eq_ignore_ascii_case("DEC2BIN")
                || word.eq_ignore_ascii_case("DEC2HEX")
                || word.eq_ignore_ascii_case("DEC2OCT")
            {
                let kind = word.to_ascii_uppercase();
                let number = calc_num(self.compare(env)?)?;
                self.skip();
                let places = if self.bytes.get(self.index) == Some(&b')') {
                    self.index += 1;
                    None
                } else {
                    self.require_comma()?;
                    let places = calc_num(self.compare(env)?)?;
                    self.close_paren()?;
                    Some(places)
                };
                let (radix, bits) = if kind == "DEC2BIN" {
                    (2, 10)
                } else if kind == "DEC2HEX" {
                    (16, 40)
                } else {
                    (8, 30)
                };
                return complement_text(number, radix, bits, places).map(CalcValue::Text);
            }
            if word.eq_ignore_ascii_case("BIN2DEC")
                || word.eq_ignore_ascii_case("HEX2DEC")
                || word.eq_ignore_ascii_case("OCT2DEC")
            {
                let kind = word.to_ascii_uppercase();
                let text = calc_text(&self.compare(env)?);
                self.close_paren()?;
                let (radix, bits) = if kind == "BIN2DEC" {
                    (2, 10)
                } else if kind == "HEX2DEC" {
                    (16, 40)
                } else {
                    (8, 30)
                };
                return from_complement(&text, radix, bits).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("BASE") {
                let number = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let radix = calc_num(self.compare(env)?)?;
                self.skip();
                if self.bytes.get(self.index) == Some(&b')') {
                    self.index += 1;
                    return base_excel(number, radix, None).map(CalcValue::Text);
                }
                self.require_comma()?;
                let length = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return base_excel(number, radix, Some(length)).map(CalcValue::Text);
            }
            if word.eq_ignore_ascii_case("DECIMAL") {
                let text = calc_text(&self.compare(env)?);
                self.require_comma()?;
                let radix = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return decimal_excel(&text, radix).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("BESSELJ") || word.eq_ignore_ascii_case("BESSELI") {
                let modified = word.eq_ignore_ascii_case("BESSELI");
                let x = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let order = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return bessel_excel(x, order, modified).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("MDETERM") {
                let (cells, rows, cols) = self.cell_block(env)?;
                self.close_paren()?;
                if rows != cols || rows == 0 || rows > 10 {
                    return None;
                }
                let n = rows as usize;
                let mut values = Vec::with_capacity(n * n);
                for address in cells {
                    match self.cell_value(&address, env) {
                        None => values.push(0.0),
                        Some(CalcValue::Num(number)) if number.is_finite() => values.push(number),
                        _ => return None,
                    }
                }
                return mdeterm_excel(&values, n).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("INDEX") {
                let (cells, rows, cols) = self.cell_block(env)?;
                self.require_comma()?;
                let row = calc_num(self.compare(env)?)?;
                self.skip();
                let col = if self.bytes.get(self.index) == Some(&b')') {
                    self.index += 1;
                    if cols != 1 {
                        return None;
                    }
                    1.0
                } else {
                    self.require_comma()?;
                    let col = calc_num(self.compare(env)?)?;
                    self.close_paren()?;
                    col
                };
                if !row.is_finite() || !col.is_finite() {
                    return None;
                }
                let row = row.trunc();
                let col = col.trunc();
                if row < 1.0 || col < 1.0 || row > f64::from(rows) || col > f64::from(cols) {
                    return None;
                }
                let index = ((row as u32 - 1) * cols + (col as u32 - 1)) as usize;
                let address = cells.get(index)?;
                return match self.cell_value(address, env) {
                    None => Some(CalcValue::Num(0.0)),
                    Some(CalcValue::Num(number)) if number.is_finite() => {
                        Some(CalcValue::Num(number))
                    }
                    Some(CalcValue::Text(text)) => Some(CalcValue::Text(text)),
                    _ => None,
                };
            }
            if word.eq_ignore_ascii_case("MATCH") {
                let lookup = self.compare(env)?;
                self.require_comma()?;
                let (cells, rows, cols) = self.cell_block(env)?;
                self.skip();
                let kind = if self.bytes.get(self.index) == Some(&b')') {
                    self.index += 1;
                    1.0
                } else {
                    self.require_comma()?;
                    let kind = calc_num(self.compare(env)?)?;
                    self.close_paren()?;
                    kind
                };
                if !kind.is_finite() || (rows != 1 && cols != 1) {
                    return None;
                }
                let kind = kind.trunc();
                if kind == 0.0 {
                    for (index, address) in cells.iter().enumerate() {
                        let cell = self.cell_value(address, env);
                        if exact_lookup(&lookup, cell.as_ref()) {
                            return Some(CalcValue::Num((index + 1) as f64));
                        }
                    }
                    return None;
                }
                if kind != 1.0 && kind != -1.0 {
                    return None;
                }
                let mut keys = Vec::new();
                for address in &cells {
                    keys.push(self.cell_value(address, env));
                }
                let Some(found) = approximate_index(&keys, &lookup, kind < 0.0) else {
                    return None;
                };
                return Some(CalcValue::Num((found + 1) as f64));
            }
            if word.eq_ignore_ascii_case("VLOOKUP") || word.eq_ignore_ascii_case("HLOOKUP") {
                let horizontal = word.eq_ignore_ascii_case("HLOOKUP");
                let lookup = self.compare(env)?;
                self.require_comma()?;
                let (cells, rows, cols) = self.cell_block(env)?;
                self.require_comma()?;
                let index = calc_num(self.compare(env)?)?;
                self.skip();
                let range_lookup = if self.bytes.get(self.index) == Some(&b')') {
                    self.index += 1;
                    1.0
                } else {
                    self.require_comma()?;
                    let range_lookup = calc_num(self.compare(env)?)?;
                    self.close_paren()?;
                    range_lookup
                };
                if !index.is_finite() || !range_lookup.is_finite() || rows == 0 || cols == 0 {
                    return None;
                }
                let index = index.trunc();
                let limit = if horizontal { rows } else { cols };
                if index < 1.0 || index > f64::from(limit) {
                    return None;
                }
                let index = index as u32;
                let key_count = if horizontal { cols } else { rows };
                let mut keys = Vec::new();
                for key in 0..key_count {
                    let at = if horizontal {
                        key as usize
                    } else {
                        (key * cols) as usize
                    };
                    keys.push(self.cell_value(&cells[at], env));
                }
                let found = if range_lookup.trunc() == 0.0 {
                    keys.iter()
                        .position(|cell| exact_lookup(&lookup, cell.as_ref()))
                } else {
                    approximate_index(&keys, &lookup, false)
                };
                let Some(found) = found else {
                    return None;
                };
                let found = found as u32;
                let at = if horizontal {
                    ((index - 1) * cols + found) as usize
                } else {
                    (found * cols + (index - 1)) as usize
                };
                let address = cells.get(at)?;
                return match self.cell_value(address, env) {
                    None => Some(CalcValue::Num(0.0)),
                    Some(CalcValue::Num(number)) if number.is_finite() => {
                        Some(CalcValue::Num(number))
                    }
                    Some(CalcValue::Text(text)) => Some(CalcValue::Text(text)),
                    _ => None,
                };
            }
            if word.eq_ignore_ascii_case("BETA.DIST") {
                let x = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let alpha = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let beta = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let cumulative = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                if !cumulative.is_finite() {
                    return None;
                }
                let value = if cumulative == 0.0 {
                    beta_pdf(x, alpha, beta)
                } else {
                    beta_cdf(x, alpha, beta)
                };
                return value.map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("T.DIST.RT")
                || word.eq_ignore_ascii_case("T.DIST.2T")
                || word.eq_ignore_ascii_case("T.DIST")
                || word.eq_ignore_ascii_case("TDIST")
            {
                let kind = word.to_ascii_uppercase();
                let x = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let df = calc_num(self.compare(env)?)?;
                let cumulative = if kind == "T.DIST" {
                    self.require_comma()?;
                    let cumulative = calc_num(self.compare(env)?)?;
                    self.close_paren()?;
                    Some(cumulative)
                } else if kind == "TDIST" {
                    self.require_comma()?;
                    let tails = calc_num(self.compare(env)?)?;
                    self.close_paren()?;
                    Some(tails)
                } else {
                    self.close_paren()?;
                    None
                };
                if kind == "T.DIST" {
                    let cumulative = cumulative?;
                    if !cumulative.is_finite() {
                        return None;
                    }
                    let value = if cumulative == 0.0 {
                        t_pdf(x, df)
                    } else {
                        t_cdf(x, df)
                    };
                    return value.map(CalcValue::Num);
                }
                if x < 0.0 {
                    return None;
                }
                let cdf = t_cdf(x, df)?;
                let value = if kind == "T.DIST.RT" {
                    1.0 - cdf
                } else if kind == "T.DIST.2T" {
                    2.0 * (1.0 - cdf)
                } else {
                    let tails = cumulative?;
                    if !tails.is_finite() {
                        return None;
                    }
                    let tails = tails.trunc();
                    if tails == 1.0 {
                        1.0 - cdf
                    } else if tails == 2.0 {
                        2.0 * (1.0 - cdf)
                    } else {
                        return None;
                    }
                };
                return if value.is_finite() {
                    Some(CalcValue::Num(value))
                } else {
                    None
                };
            }
            if word.eq_ignore_ascii_case("F.DIST.RT")
                || word.eq_ignore_ascii_case("F.DIST")
                || word.eq_ignore_ascii_case("FDIST")
            {
                let kind = word.to_ascii_uppercase();
                let x = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let df1 = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let df2 = calc_num(self.compare(env)?)?;
                let cumulative = if kind == "F.DIST" {
                    self.require_comma()?;
                    let cumulative = calc_num(self.compare(env)?)?;
                    self.close_paren()?;
                    Some(cumulative)
                } else {
                    self.close_paren()?;
                    None
                };
                if kind == "F.DIST" {
                    let cumulative = cumulative?;
                    if !cumulative.is_finite() {
                        return None;
                    }
                    let value = if cumulative == 0.0 {
                        f_pdf(x, df1, df2)
                    } else {
                        f_cdf(x, df1, df2)
                    };
                    return value.map(CalcValue::Num);
                }
                let cdf = f_cdf(x, df1, df2)?;
                let value = 1.0 - cdf;
                return if value.is_finite() {
                    Some(CalcValue::Num(value))
                } else {
                    None
                };
            }
            if word.eq_ignore_ascii_case("T.TEST") || word.eq_ignore_ascii_case("TTEST") {
                let left_cells = self.cell_range(env)?;
                self.require_comma()?;
                let right_cells = self.cell_range(env)?;
                self.require_comma()?;
                let tails = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let kind = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                if !kind.is_finite() {
                    return None;
                }
                let value = if kind.trunc() == 1.0 {
                    if left_cells.len() != right_cells.len() {
                        return None;
                    }
                    let mut diffs = Vec::new();
                    for (left, right) in left_cells.iter().zip(&right_cells) {
                        let Some(CalcValue::Num(left)) = self.cell_value(left, env) else {
                            continue;
                        };
                        let Some(CalcValue::Num(right)) = self.cell_value(right, env) else {
                            continue;
                        };
                        if left.is_finite() && right.is_finite() {
                            diffs.push(left - right);
                        }
                    }
                    paired_t_test(&diffs, tails)
                } else {
                    let left = self.range_numbers(&left_cells, env);
                    let right = self.range_numbers(&right_cells, env);
                    t_test_excel(&left, &right, tails, kind)
                };
                return value.map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("F.TEST") || word.eq_ignore_ascii_case("FTEST") {
                let left_cells = self.cell_range(env)?;
                self.require_comma()?;
                let right_cells = self.cell_range(env)?;
                self.close_paren()?;
                let left = self.range_numbers(&left_cells, env);
                let right = self.range_numbers(&right_cells, env);
                return f_test_excel(&left, &right).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("CONFIDENCE.T") {
                let alpha = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let stdev = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let size = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return confidence_t_excel(alpha, stdev, size).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("PMT") {
                let rate = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let nper = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let pv = calc_num(self.compare(env)?)?;
                let extra = self.rest_numbers(env, 2)?;
                let fv = extra.first().copied().unwrap_or(0.0);
                let typ = extra.get(1).copied().unwrap_or(0.0);
                return pmt_excel(rate, nper, pv, fv, typ).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("FV") {
                let rate = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let nper = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let pmt = calc_num(self.compare(env)?)?;
                let extra = self.rest_numbers(env, 2)?;
                let pv = extra.first().copied().unwrap_or(0.0);
                let typ = extra.get(1).copied().unwrap_or(0.0);
                return fv_excel(rate, nper, pmt, pv, typ).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("PV") {
                let rate = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let nper = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let pmt = calc_num(self.compare(env)?)?;
                let extra = self.rest_numbers(env, 2)?;
                let fv = extra.first().copied().unwrap_or(0.0);
                let typ = extra.get(1).copied().unwrap_or(0.0);
                return pv_excel(rate, nper, pmt, fv, typ).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("NPER") {
                let rate = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let pmt = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let pv = calc_num(self.compare(env)?)?;
                let extra = self.rest_numbers(env, 2)?;
                let fv = extra.first().copied().unwrap_or(0.0);
                let typ = extra.get(1).copied().unwrap_or(0.0);
                return nper_excel(rate, pmt, pv, fv, typ).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("RATE") {
                let nper = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let pmt = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let pv = calc_num(self.compare(env)?)?;
                let extra = self.rest_numbers(env, 3)?;
                let fv = extra.first().copied().unwrap_or(0.0);
                let typ = extra.get(1).copied().unwrap_or(0.0);
                let guess = extra.get(2).copied().unwrap_or(0.1);
                return rate_excel(nper, pmt, pv, fv, typ, guess).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("NPV") {
                let rate = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let values = self.arg_list(env)?;
                return npv_excel(rate, &values).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("IRR") {
                let cells = self.cell_range(env)?;
                self.skip();
                let guess = if self.bytes.get(self.index) == Some(&b')') {
                    self.index += 1;
                    0.1
                } else {
                    self.require_comma()?;
                    let guess = calc_num(self.compare(env)?)?;
                    self.close_paren()?;
                    guess
                };
                let mut values = Vec::new();
                for address in cells {
                    match self.cell_value(&address, env) {
                        None => values.push(0.0),
                        Some(CalcValue::Num(number)) if number.is_finite() => values.push(number),
                        _ => return None,
                    }
                }
                return irr_excel(&values, guess).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("IPMT") || word.eq_ignore_ascii_case("PPMT") {
                let principal = word.eq_ignore_ascii_case("PPMT");
                let rate = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let per = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let nper = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let pv = calc_num(self.compare(env)?)?;
                let extra = self.rest_numbers(env, 2)?;
                let fv = extra.first().copied().unwrap_or(0.0);
                let typ = extra.get(1).copied().unwrap_or(0.0);
                let value = if principal {
                    ppmt_excel(rate, per, nper, pv, fv, typ)
                } else {
                    ipmt_excel(rate, per, nper, pv, fv, typ)
                };
                return value.map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("CUMIPMT") || word.eq_ignore_ascii_case("CUMPRINC") {
                let principal = word.eq_ignore_ascii_case("CUMPRINC");
                let rate = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let nper = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let pv = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let start = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let end = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let typ = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return cumulative_payment(rate, nper, pv, start, end, typ, principal)
                    .map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("XNPV") {
                let rate = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let values = self.finite_range(env)?;
                self.require_comma()?;
                let dates = self.finite_range(env)?;
                self.close_paren()?;
                return xnpv_excel(rate, &values, &dates).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("XIRR") {
                let values = self.finite_range(env)?;
                self.require_comma()?;
                let dates = self.finite_range(env)?;
                self.skip();
                let guess = if self.bytes.get(self.index) == Some(&b')') {
                    self.index += 1;
                    0.1
                } else {
                    self.require_comma()?;
                    let guess = calc_num(self.compare(env)?)?;
                    self.close_paren()?;
                    guess
                };
                return xirr_excel(&values, &dates, guess).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("MIRR") {
                let values = self.finite_range(env)?;
                self.require_comma()?;
                let finance = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let reinvest = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return mirr_excel(&values, finance, reinvest).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("WEEKDAY") {
                let serial = calc_num(self.compare(env)?)?;
                self.skip();
                let kind = if self.bytes.get(self.index) == Some(&b')') {
                    self.index += 1;
                    1.0
                } else {
                    self.require_comma()?;
                    let kind = calc_num(self.compare(env)?)?;
                    self.close_paren()?;
                    kind
                };
                if !serial.is_finite() || !kind.is_finite() {
                    return None;
                }
                let serial = as_weekday_serial(serial, env.date1904)?;
                return weekday_code(serial.trunc() as i64, kind.trunc() as i64)
                    .map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("EDATE") || word.eq_ignore_ascii_case("EOMONTH") {
                let end = word.eq_ignore_ascii_case("EOMONTH");
                let serial = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let months = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                let serial = as_1900(serial, env.date1904)?;
                return shift_months(serial, months, end)
                    .and_then(|serial| from_1900(serial, env.date1904))
                    .map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("DATEDIF") {
                let start = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let end = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let unit = calc_text(&self.compare(env)?);
                self.close_paren()?;
                let start = as_1900(start, env.date1904)?;
                let end = as_1900(end, env.date1904)?;
                return datedif_excel(start, end, &unit).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("NETWORKDAYS") {
                let start = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let end = calc_num(self.compare(env)?)?;
                self.skip();
                let holidays = if self.bytes.get(self.index) == Some(&b')') {
                    self.index += 1;
                    Vec::new()
                } else {
                    self.require_comma()?;
                    let cells = self.cell_range(env)?;
                    self.close_paren()?;
                    if cells.len() > 512 {
                        return None;
                    }
                    let mut holidays = Vec::new();
                    for address in cells {
                        if let Some(CalcValue::Num(number)) = self.cell_value(&address, env) {
                            if number.is_finite() {
                                holidays.push(number);
                            }
                        }
                    }
                    holidays
                };
                let start = as_weekday_serial(start, env.date1904)?;
                let end = as_weekday_serial(end, env.date1904)?;
                let holidays: Vec<f64> = holidays
                    .iter()
                    .filter_map(|number| as_weekday_serial(*number, env.date1904))
                    .collect();
                return networkdays_excel(start, end, &holidays).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("WORKDAY") {
                let start = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let days = calc_num(self.compare(env)?)?;
                self.skip();
                let holidays = if self.bytes.get(self.index) == Some(&b')') {
                    self.index += 1;
                    Vec::new()
                } else {
                    self.require_comma()?;
                    let cells = self.cell_range(env)?;
                    self.close_paren()?;
                    if cells.len() > 512 {
                        return None;
                    }
                    let mut holidays = Vec::new();
                    for address in cells {
                        if let Some(CalcValue::Num(number)) = self.cell_value(&address, env) {
                            if number.is_finite() {
                                holidays.push(number);
                            }
                        }
                    }
                    holidays
                };
                let start = as_weekday_serial(start, env.date1904)?;
                let holidays: Vec<f64> = holidays
                    .iter()
                    .filter_map(|number| as_weekday_serial(*number, env.date1904))
                    .collect();
                return workday_excel(start, days, &holidays)
                    .and_then(|serial| from_weekday_serial(serial, env.date1904))
                    .map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("TEXTJOIN") {
                let delim = calc_text(&self.compare(env)?);
                self.require_comma()?;
                let ignore = calc_num(self.compare(env)?)?;
                if !ignore.is_finite() {
                    return None;
                }
                self.skip();
                let parts = match self.bytes.get(self.index) {
                    Some(&b')') => {
                        self.index += 1;
                        Vec::new()
                    }
                    Some(&b',') => {
                        self.index += 1;
                        self.join_parts(env)?
                    }
                    _ => return None,
                };
                return text_join(&delim, ignore != 0.0, &parts).map(CalcValue::Text);
            }
            if word.eq_ignore_ascii_case("LEN") {
                let text = calc_text(&self.compare(env)?);
                self.close_paren()?;
                return Some(CalcValue::Num(text.chars().count() as f64));
            }
            if word.eq_ignore_ascii_case("UPPER") {
                let text = calc_text(&self.compare(env)?);
                self.close_paren()?;
                return Some(CalcValue::Text(text.to_uppercase()));
            }
            if word.eq_ignore_ascii_case("LOWER") {
                let text = calc_text(&self.compare(env)?);
                self.close_paren()?;
                return Some(CalcValue::Text(text.to_lowercase()));
            }
            if word.eq_ignore_ascii_case("LEFT") {
                let text = calc_text(&self.compare(env)?);
                let count = self.comma_number(env)?;
                return slice_text(&text, 0, count).map(CalcValue::Text);
            }
            if word.eq_ignore_ascii_case("RIGHT") {
                let text = calc_text(&self.compare(env)?);
                let count = self.comma_number(env)?;
                let count = text_count(count)?;
                let chars: Vec<char> = text.chars().collect();
                let start = chars.len().saturating_sub(count);
                return Some(CalcValue::Text(chars[start..].iter().collect()));
            }
            if word.eq_ignore_ascii_case("MID") {
                let text = calc_text(&self.compare(env)?);
                self.skip();
                if self.bytes.get(self.index) != Some(&b',') {
                    return None;
                }
                self.index += 1;
                let start = calc_num(self.compare(env)?)?;
                let count = self.comma_number(env)?;
                return slice_mid(&text, start, count).map(CalcValue::Text);
            }
            if word.eq_ignore_ascii_case("TRIM") {
                let text = calc_text(&self.compare(env)?);
                self.close_paren()?;
                return Some(CalcValue::Text(trim_spaces(&text)));
            }
            if word.eq_ignore_ascii_case("EXACT") {
                let left = calc_text(&self.compare(env)?);
                self.require_comma()?;
                let right = calc_text(&self.compare(env)?);
                self.close_paren()?;
                return Some(CalcValue::Num(if left == right { 1.0 } else { 0.0 }));
            }
            if word.eq_ignore_ascii_case("REPT") {
                let text = calc_text(&self.compare(env)?);
                let count = self.comma_number(env)?;
                return rept_text(&text, count).map(CalcValue::Text);
            }
            if word.eq_ignore_ascii_case("FIND") || word.eq_ignore_ascii_case("SEARCH") {
                let ignore_ascii_case = word.eq_ignore_ascii_case("SEARCH");
                let needle = calc_text(&self.compare(env)?);
                self.require_comma()?;
                let haystack = calc_text(&self.compare(env)?);
                let start = self.optional_number(env, 1.0)?;
                return find_scalar(&haystack, &needle, start, ignore_ascii_case)
                    .map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("SUBSTITUTE") {
                let text = calc_text(&self.compare(env)?);
                self.require_comma()?;
                let old = calc_text(&self.compare(env)?);
                self.require_comma()?;
                let new = calc_text(&self.compare(env)?);
                let instance = self.optional_number(env, f64::NAN)?;
                let instance = if instance.is_nan() {
                    None
                } else {
                    Some(instance)
                };
                return substitute_text(&text, &old, &new, instance).map(CalcValue::Text);
            }
            if word.eq_ignore_ascii_case("REPLACE") {
                let text = calc_text(&self.compare(env)?);
                self.require_comma()?;
                let start = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let count = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let new = calc_text(&self.compare(env)?);
                self.close_paren()?;
                return replace_span(&text, start, count, &new).map(CalcValue::Text);
            }
            if word.eq_ignore_ascii_case("VALUE") {
                let text = calc_text(&self.compare(env)?);
                self.close_paren()?;
                return value_excel(&text).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("CONVERT") {
                let number = calc_num(self.compare(env)?)?;
                self.require_comma()?;
                let from = calc_text(&self.compare(env)?);
                self.require_comma()?;
                let to = calc_text(&self.compare(env)?);
                self.close_paren()?;
                return convert_excel(number, &from, &to).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("TEXT") {
                let value = self.compare(env)?;
                self.require_comma()?;
                let format = calc_text(&self.compare(env)?);
                self.close_paren()?;
                return text_excel(value, &format, env.date1904);
            }
            if word.eq_ignore_ascii_case("T") {
                let value = self.compare(env)?;
                self.close_paren()?;
                let text = match value {
                    CalcValue::Text(text) => text,
                    CalcValue::Num(_) => String::new(),
                };
                return Some(CalcValue::Text(text));
            }
            if word.eq_ignore_ascii_case("N") {
                let value = self.compare(env)?;
                self.close_paren()?;
                let number = match value {
                    CalcValue::Num(number) if number.is_finite() => number,
                    CalcValue::Num(_) => return None,
                    CalcValue::Text(_) => 0.0,
                };
                return Some(CalcValue::Num(number));
            }
            if word.eq_ignore_ascii_case("ISNUMBER") || word.eq_ignore_ascii_case("ISTEXT") {
                let want_text = word.eq_ignore_ascii_case("ISTEXT");
                let value = self.compare(env)?;
                self.close_paren()?;
                let flag = match value {
                    CalcValue::Num(number) if number.is_finite() => !want_text,
                    CalcValue::Num(_) => return None,
                    CalcValue::Text(_) => want_text,
                };
                return Some(CalcValue::Num(if flag { 1.0 } else { 0.0 }));
            }
            if word.eq_ignore_ascii_case("NOT") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return Some(CalcValue::Num(if number == 0.0 { 1.0 } else { 0.0 }));
            }
            if word.eq_ignore_ascii_case("AND") || word.eq_ignore_ascii_case("OR") {
                let any = word.eq_ignore_ascii_case("OR");
                let args = self.logic_args(env)?;
                let flag = if any {
                    args.iter().any(|number| *number != 0.0)
                } else {
                    args.iter().all(|number| *number != 0.0)
                };
                return Some(CalcValue::Num(if flag { 1.0 } else { 0.0 }));
            }
            if word.eq_ignore_ascii_case("XOR") {
                let args = self.logic_args(env)?;
                let odds = args.iter().filter(|number| **number != 0.0).count();
                return Some(CalcValue::Num(if odds % 2 == 1 { 1.0 } else { 0.0 }));
            }
            if word.eq_ignore_ascii_case("BITAND")
                || word.eq_ignore_ascii_case("BITOR")
                || word.eq_ignore_ascii_case("BITXOR")
            {
                let and = word.eq_ignore_ascii_case("BITAND");
                let xor = word.eq_ignore_ascii_case("BITXOR");
                let left = calc_num(self.compare(env)?)?;
                let right = self.comma_number(env)?;
                return bit_excel(left, right, and, xor).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("BITLSHIFT") || word.eq_ignore_ascii_case("BITRSHIFT") {
                let right = word.eq_ignore_ascii_case("BITRSHIFT");
                let number = calc_num(self.compare(env)?)?;
                let shift = self.comma_number(env)?;
                return bit_shift(number, shift, right).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("LARGE") || word.eq_ignore_ascii_case("SMALL") {
                let small = word.eq_ignore_ascii_case("SMALL");
                let mut args = self.arg_list(env)?;
                let Some(rank) = args.pop() else {
                    return None;
                };
                return ranked_excel(&mut args, rank, small).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("COUNTA") || word.eq_ignore_ascii_case("COUNTBLANK") {
                let blanks = word.eq_ignore_ascii_case("COUNTBLANK");
                let (present, blank) = self.tally_args(env)?;
                return Some(CalcValue::Num(if blanks { blank } else { present }));
            }
            if word.eq_ignore_ascii_case("TRIMMEAN") {
                let cells = self.cell_range(env)?;
                self.require_comma()?;
                let percent = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                let mut values = Vec::new();
                for address in cells {
                    if let Some(CalcValue::Num(value)) = self.cell_value(&address, env) {
                        values.push(value);
                    }
                }
                return trimmean_excel(&mut values, percent).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("AVERAGEA")
                || word.eq_ignore_ascii_case("MINA")
                || word.eq_ignore_ascii_case("MAXA")
                || word.eq_ignore_ascii_case("STDEVA")
                || word.eq_ignore_ascii_case("VARA")
            {
                let name = word.to_ascii_uppercase();
                let args = self.a_list(env)?;
                return match name.as_str() {
                    "AVERAGEA" if !args.is_empty() => {
                        let value = args.iter().sum::<f64>() / args.len() as f64;
                        if value.is_finite() {
                            Some(CalcValue::Num(value))
                        } else {
                            None
                        }
                    }
                    "MINA" => Some(CalcValue::Num(
                        args.into_iter().reduce(f64::min).unwrap_or(0.0),
                    )),
                    "MAXA" => Some(CalcValue::Num(
                        args.into_iter().reduce(f64::max).unwrap_or(0.0),
                    )),
                    "STDEVA" => stdev_excel(&args, true).map(CalcValue::Num),
                    "VARA" => var_excel(&args, true).map(CalcValue::Num),
                    _ => None,
                };
            }
            let args = self.arg_list(env)?;
            return match word.to_ascii_uppercase().as_str() {
                "SUM" => Some(CalcValue::Num(args.iter().sum())),
                "PRODUCT" => {
                    let value = if args.is_empty() {
                        0.0
                    } else {
                        args.iter().product()
                    };
                    if value.is_finite() {
                        Some(CalcValue::Num(value))
                    } else {
                        None
                    }
                }
                "MEDIAN" => {
                    if args.is_empty() || args.iter().any(|number| !number.is_finite()) {
                        None
                    } else {
                        let mut sorted = args;
                        sorted.sort_by(|left, right| left.total_cmp(right));
                        let mid = sorted.len() / 2;
                        let value = if sorted.len() % 2 == 1 {
                            sorted[mid]
                        } else {
                            (sorted[mid - 1] + sorted[mid]) / 2.0
                        };
                        if value.is_finite() {
                            Some(CalcValue::Num(value))
                        } else {
                            None
                        }
                    }
                }
                "GCD" => gcd_excel(&args).map(CalcValue::Num),
                "LCM" => lcm_excel(&args).map(CalcValue::Num),
                "AVERAGE" if !args.is_empty() => {
                    Some(CalcValue::Num(args.iter().sum::<f64>() / args.len() as f64))
                }
                "MIN" => args.into_iter().reduce(f64::min).map(CalcValue::Num),
                "MAX" => args.into_iter().reduce(f64::max).map(CalcValue::Num),
                "COUNT" => Some(CalcValue::Num(args.len() as f64)),
                "SUMSQ" => {
                    let value = args.iter().map(|number| number * number).sum::<f64>();
                    if value.is_finite() {
                        Some(CalcValue::Num(value))
                    } else {
                        None
                    }
                }
                "STDEV" | "STDEV.S" => stdev_excel(&args, true).map(CalcValue::Num),
                "STDEVP" | "STDEV.P" => stdev_excel(&args, false).map(CalcValue::Num),
                "VAR" | "VAR.S" => var_excel(&args, true).map(CalcValue::Num),
                "VARP" | "VAR.P" => var_excel(&args, false).map(CalcValue::Num),
                "AVEDEV" => avedev_excel(&args).map(CalcValue::Num),
                "DEVSQ" => devsq_excel(&args).map(CalcValue::Num),
                "GEOMEAN" => geomean_excel(&args).map(CalcValue::Num),
                "HARMEAN" => harmean_excel(&args).map(CalcValue::Num),
                "SKEW" => skew_excel(&args, false).map(CalcValue::Num),
                "SKEW.P" => skew_excel(&args, true).map(CalcValue::Num),
                "KURT" => kurt_excel(&args).map(CalcValue::Num),
                "MULTINOMIAL" => multinomial_excel(&args).map(CalcValue::Num),
                _ => None,
            };
        }
        if self.bytes.get(self.index) == Some(&b'!') {
            return self.foreign_cell(&word, env);
        }
        if !is_cell_address(&word) {
            if let Some(defined) = env.names.get(&word.to_ascii_lowercase()) {
                if defined.cells.len() != 1 {
                    return None;
                }
                let address = named_addresses(defined, env);
                return self.cell_value(&address[0], env);
            }
        }
        self.cell_value(&word, env)
    }

    fn comma_number(&mut self, env: &mut CalcEnv<'_>) -> Option<f64> {
        self.skip();
        if self.bytes.get(self.index) != Some(&b',') {
            return None;
        }
        self.index += 1;
        let number = calc_num(self.compare(env)?)?;
        self.close_paren()?;
        Some(number)
    }

    fn rest_numbers(&mut self, env: &mut CalcEnv<'_>, count: usize) -> Option<Vec<f64>> {
        let mut values = Vec::new();
        for _ in 0..count {
            self.skip();
            if self.bytes.get(self.index) == Some(&b')') {
                self.index += 1;
                return Some(values);
            }
            self.require_comma()?;
            values.push(calc_num(self.compare(env)?)?);
        }
        self.close_paren()?;
        Some(values)
    }

    fn close_paren(&mut self) -> Option<()> {
        self.skip();
        if self.bytes.get(self.index) != Some(&b')') {
            return None;
        }
        self.index += 1;
        Some(())
    }

    fn require_comma(&mut self) -> Option<()> {
        self.skip();
        if self.bytes.get(self.index) != Some(&b',') {
            return None;
        }
        self.index += 1;
        Some(())
    }

    /// A following comma reads one number. `missing` is used when the call ends at `)`.
    fn optional_number(&mut self, env: &mut CalcEnv<'_>, missing: f64) -> Option<f64> {
        self.skip();
        match self.bytes.get(self.index) {
            Some(&b')') => {
                self.index += 1;
                Some(missing)
            }
            Some(&b',') => {
                self.index += 1;
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                Some(number)
            }
            _ => None,
        }
    }

    fn logic_args(&mut self, env: &mut CalcEnv<'_>) -> Option<Vec<f64>> {
        let mut values = Vec::new();
        self.skip();
        if self.bytes.get(self.index) == Some(&b')') {
            return None;
        }
        loop {
            values.push(calc_num(self.compare(env)?)?);
            if values.len() > 255 {
                return None;
            }
            self.skip();
            match self.bytes.get(self.index).copied() {
                Some(b',') => self.index += 1,
                Some(b')') => {
                    self.index += 1;
                    break;
                }
                _ => return None,
            }
        }
        Some(values)
    }

    fn quoted(&mut self) -> Option<String> {
        if self.bytes.get(self.index) != Some(&b'"') {
            return None;
        }
        self.index += 1;
        let mut raw = Vec::new();
        while let Some(byte) = self.bytes.get(self.index).copied() {
            self.index += 1;
            if byte == b'"' {
                if self.bytes.get(self.index) == Some(&b'"') {
                    self.index += 1;
                    raw.push(b'"');
                    continue;
                }
                return String::from_utf8(raw).ok();
            }
            raw.push(byte);
        }
        None
    }

    fn arg_list(&mut self, env: &mut CalcEnv<'_>) -> Option<Vec<f64>> {
        let mut values = Vec::new();
        self.skip();
        if self.bytes.get(self.index) == Some(&b')') {
            self.index += 1;
            return Some(values);
        }
        loop {
            self.skip();
            if self.bytes.get(self.index) == Some(&b')') {
                self.index += 1;
                break;
            }
            let saved = self.index;
            match self.name_arg(env) {
                NameArg::Invalid => return None,
                NameArg::Cells(cells) => {
                    for address in cells {
                        if let Some(CalcValue::Num(value)) = self.cell_value(&address, env) {
                            values.push(value);
                        }
                    }
                }
                NameArg::Absent => {
                    if let Some(start) = self.cell_token() {
                        self.skip();
                        if self.bytes.get(self.index) == Some(&b':') {
                            self.index += 1;
                            self.skip();
                            let Some(end) = self.cell_token() else {
                                return None;
                            };
                            let Some(cells) = cells_in_range(&start, &end) else {
                                return None;
                            };
                            for address in cells {
                                if let Some(CalcValue::Num(value)) = self.cell_value(&address, env)
                                {
                                    values.push(value);
                                }
                            }
                        } else {
                            self.index = saved;
                            values.push(calc_num(self.expr(env)?)?);
                        }
                    } else {
                        self.index = saved;
                        values.push(calc_num(self.expr(env)?)?);
                    }
                }
            }
            self.skip();
            match self.bytes.get(self.index).copied() {
                Some(b',') => self.index += 1,
                Some(b')') => {
                    self.index += 1;
                    break;
                }
                _ => return None,
            }
        }
        if values.len() > 4096 {
            None
        } else {
            Some(values)
        }
    }

    fn a_scalar(&mut self, env: &mut CalcEnv<'_>) -> Option<f64> {
        match self.compare(env)? {
            CalcValue::Num(value) if value.is_finite() => Some(value),
            CalcValue::Num(_) => None,
            CalcValue::Text(_) => Some(0.0),
        }
    }

    fn a_list(&mut self, env: &mut CalcEnv<'_>) -> Option<Vec<f64>> {
        let mut values = Vec::new();
        self.skip();
        if self.bytes.get(self.index) == Some(&b')') {
            self.index += 1;
            return Some(values);
        }
        loop {
            self.skip();
            if self.bytes.get(self.index) == Some(&b')') {
                self.index += 1;
                break;
            }
            let saved = self.index;
            match self.name_arg(env) {
                NameArg::Invalid => return None,
                NameArg::Cells(cells) => {
                    for address in cells {
                        match self.cell_value(&address, env) {
                            Some(CalcValue::Num(value)) => {
                                if !value.is_finite() {
                                    return None;
                                }
                                values.push(value);
                            }
                            Some(CalcValue::Text(_)) => values.push(0.0),
                            None => {}
                        }
                    }
                }
                NameArg::Absent => {
                    if let Some(start) = self.cell_token() {
                        self.skip();
                        if self.bytes.get(self.index) == Some(&b':') {
                            self.index += 1;
                            self.skip();
                            let Some(end) = self.cell_token() else {
                                return None;
                            };
                            let Some(cells) = cells_in_range(&start, &end) else {
                                return None;
                            };
                            for address in cells {
                                match self.cell_value(&address, env) {
                                    Some(CalcValue::Num(value)) => {
                                        if !value.is_finite() {
                                            return None;
                                        }
                                        values.push(value);
                                    }
                                    Some(CalcValue::Text(_)) => values.push(0.0),
                                    None => {}
                                }
                            }
                        } else {
                            self.index = saved;
                            values.push(self.a_scalar(env)?);
                        }
                    } else {
                        self.index = saved;
                        values.push(self.a_scalar(env)?);
                    }
                }
            }
            self.skip();
            match self.bytes.get(self.index).copied() {
                Some(b',') => self.index += 1,
                Some(b')') => {
                    self.index += 1;
                    break;
                }
                _ => return None,
            }
        }
        if values.len() > 4096 {
            None
        } else {
            Some(values)
        }
    }

    fn tally_args(&mut self, env: &mut CalcEnv<'_>) -> Option<(f64, f64)> {
        let mut present = 0.0;
        let mut blank = 0.0;
        let mut seen = 0usize;
        self.skip();
        if self.bytes.get(self.index) == Some(&b')') {
            self.index += 1;
            return Some((0.0, 0.0));
        }
        loop {
            self.skip();
            if self.bytes.get(self.index) == Some(&b')') {
                self.index += 1;
                break;
            }
            let saved = self.index;
            match self.name_arg(env) {
                NameArg::Invalid => return None,
                NameArg::Cells(cells) => {
                    for address in cells {
                        seen += 1;
                        if seen > 4096 {
                            return None;
                        }
                        match self.cell_value(&address, env) {
                            Some(value) => note_presence(&value, &mut present, &mut blank)?,
                            None => blank += 1.0,
                        }
                    }
                }
                NameArg::Absent => {
                    if let Some(start) = self.cell_token() {
                        self.skip();
                        if self.bytes.get(self.index) == Some(&b':') {
                            self.index += 1;
                            self.skip();
                            let Some(end) = self.cell_token() else {
                                return None;
                            };
                            let Some(cells) = cells_in_range(&start, &end) else {
                                return None;
                            };
                            for address in cells {
                                seen += 1;
                                if seen > 4096 {
                                    return None;
                                }
                                match self.cell_value(&address, env) {
                                    Some(value) => note_presence(&value, &mut present, &mut blank)?,
                                    None => blank += 1.0,
                                }
                            }
                        } else {
                            self.index = saved;
                            seen += 1;
                            note_presence(&self.expr(env)?, &mut present, &mut blank)?;
                        }
                    } else {
                        self.index = saved;
                        seen += 1;
                        note_presence(&self.expr(env)?, &mut present, &mut blank)?;
                    }
                }
            }
            if seen > 4096 {
                return None;
            }
            self.skip();
            match self.bytes.get(self.index).copied() {
                Some(b',') => self.index += 1,
                Some(b')') => {
                    self.index += 1;
                    break;
                }
                _ => return None,
            }
        }
        Some((present, blank))
    }

    fn concat_args(&mut self, env: &mut CalcEnv<'_>) -> Option<String> {
        let mut parts = Vec::new();
        self.skip();
        if self.bytes.get(self.index) == Some(&b')') {
            self.index += 1;
            return Some(String::new());
        }
        loop {
            self.skip();
            if self.bytes.get(self.index) == Some(&b')') {
                self.index += 1;
                break;
            }
            let saved = self.index;
            if let Some(start) = self.cell_token() {
                self.skip();
                if self.bytes.get(self.index) == Some(&b':') {
                    self.index += 1;
                    self.skip();
                    let Some(end) = self.cell_token() else {
                        return None;
                    };
                    let Some(cells) = cells_in_range(&start, &end) else {
                        return None;
                    };
                    for address in cells {
                        if let Some(value) = self.cell_value(&address, env) {
                            parts.push(calc_text(&value));
                        }
                    }
                } else {
                    self.index = saved;
                    parts.push(calc_text(&self.expr(env)?));
                }
            } else {
                self.index = saved;
                parts.push(calc_text(&self.expr(env)?));
            }
            self.skip();
            match self.bytes.get(self.index).copied() {
                Some(b',') => self.index += 1,
                Some(b')') => {
                    self.index += 1;
                    break;
                }
                _ => return None,
            }
        }
        if parts.len() > 4096 {
            None
        } else {
            Some(parts.concat())
        }
    }

    fn join_parts(&mut self, env: &mut CalcEnv<'_>) -> Option<Vec<String>> {
        let mut parts = Vec::new();
        loop {
            self.skip();
            if self.bytes.get(self.index) == Some(&b')') {
                self.index += 1;
                break;
            }
            let saved = self.index;
            if let Some(start) = self.cell_token() {
                self.skip();
                if self.bytes.get(self.index) == Some(&b':') {
                    self.index += 1;
                    self.skip();
                    let Some(end) = self.cell_token() else {
                        return None;
                    };
                    let Some(cells) = cells_in_range(&start, &end) else {
                        return None;
                    };
                    for address in cells {
                        match self.cell_value(&address, env) {
                            Some(value) => parts.push(calc_text(&value)),
                            None => parts.push(String::new()),
                        }
                    }
                } else {
                    self.index = saved;
                    parts.push(calc_text(&self.expr(env)?));
                }
            } else {
                self.index = saved;
                parts.push(calc_text(&self.expr(env)?));
            }
            if parts.len() > 4096 {
                return None;
            }
            self.skip();
            match self.bytes.get(self.index).copied() {
                Some(b',') => self.index += 1,
                Some(b')') => {
                    self.index += 1;
                    break;
                }
                _ => return None,
            }
        }
        Some(parts)
    }

    fn sum_if(&mut self, env: &mut CalcEnv<'_>) -> Option<f64> {
        let total: f64 = self.matched_numbers(env)?.iter().sum();
        if total.is_finite() {
            Some(total)
        } else {
            None
        }
    }

    fn matched_numbers(&mut self, env: &mut CalcEnv<'_>) -> Option<Vec<f64>> {
        self.skip();
        let cells = self.cell_range(env)?;
        self.require_comma()?;
        let criteria = self.compare(env)?;
        let (op, target) = compile_criterion(&criteria)?;
        self.skip();
        if self.bytes.get(self.index) != Some(&b')') {
            return None;
        }
        self.index += 1;
        let mut matched = Vec::new();
        for address in cells {
            let Some(CalcValue::Num(number)) = self.cell_value(&address, env) else {
                continue;
            };
            if number.is_finite() && number_matches(number, op, target) {
                matched.push(number);
            }
        }
        Some(matched)
    }

    fn take_name(&mut self, env: &CalcEnv<'_>) -> Option<DefinedRef> {
        let saved = self.index;
        self.skip();
        let Some(word) = self.word() else {
            self.index = saved;
            return None;
        };
        if is_cell_address(&word) {
            self.index = saved;
            return None;
        }
        self.skip();
        if matches!(self.bytes.get(self.index), Some(b'(' | b'!')) {
            self.index = saved;
            return None;
        }
        let defined = env.names.get(&word.to_ascii_lowercase()).cloned();
        if defined.is_none() {
            self.index = saved;
        }
        defined
    }

    fn name_arg(&mut self, env: &CalcEnv<'_>) -> NameArg {
        let saved = self.index;
        let Some(defined) = self.take_name(env) else {
            return NameArg::Absent;
        };
        self.skip();
        match self.bytes.get(self.index).copied() {
            Some(b',' | b')') => NameArg::Cells(named_addresses(&defined, env)),
            _ if defined.cells.len() == 1 => {
                self.index = saved;
                NameArg::Absent
            }
            _ => NameArg::Invalid,
        }
    }

    fn cell_range(&mut self, env: &CalcEnv<'_>) -> Option<Vec<String>> {
        if let Some(defined) = self.take_name(env) {
            let cells = named_addresses(&defined, env);
            if cells.is_empty() || cells.len() > 4096 {
                return None;
            }
            return Some(cells);
        }
        self.skip();
        let start = self.cell_token()?;
        self.skip();
        if self.bytes.get(self.index) != Some(&b':') {
            return None;
        }
        self.index += 1;
        self.skip();
        let end = self.cell_token()?;
        cells_in_range(&start, &end)
    }

    fn finite_range(&mut self, env: &mut CalcEnv<'_>) -> Option<Vec<f64>> {
        let cells = self.cell_range(env)?;
        if cells.len() > 128 {
            return None;
        }
        let mut values = Vec::with_capacity(cells.len());
        for address in cells {
            let Some(CalcValue::Num(number)) = self.cell_value(&address, env) else {
                return None;
            };
            if !number.is_finite() {
                return None;
            }
            values.push(number);
        }
        Some(values)
    }

    fn cell_block(&mut self, env: &CalcEnv<'_>) -> Option<(Vec<String>, u32, u32)> {
        if let Some(defined) = self.take_name(env) {
            let cells = named_addresses(&defined, env);
            if cells.is_empty() {
                return None;
            }
            return Some((cells, defined.rows, defined.cols));
        }
        self.skip();
        let start = self.cell_token()?;
        self.skip();
        if self.bytes.get(self.index) != Some(&b':') {
            return None;
        }
        self.index += 1;
        self.skip();
        let end = self.cell_token()?;
        let (c1, r1) = split_address(&start)?;
        let (c2, r2) = split_address(&end)?;
        let rows = r1.abs_diff(r2) + 1;
        let cols = c1.abs_diff(c2) + 1;
        let cells = cells_in_range(&start, &end)?;
        Some((cells, rows, cols))
    }

    fn rank_call(&mut self, env: &mut CalcEnv<'_>, average: bool) -> Option<f64> {
        let number = calc_num(self.compare(env)?)?;
        self.require_comma()?;
        let cells = self.cell_range(env)?;
        self.skip();
        let ascending = match self.bytes.get(self.index) {
            Some(&b')') => {
                self.index += 1;
                false
            }
            Some(&b',') => {
                self.index += 1;
                let order = calc_num(self.compare(env)?)?;
                if !order.is_finite() {
                    return None;
                }
                self.close_paren()?;
                order != 0.0
            }
            _ => return None,
        };
        let mut values = Vec::new();
        for address in cells {
            if let Some(CalcValue::Num(value)) = self.cell_value(&address, env) {
                if value.is_finite() {
                    values.push(value);
                }
            }
        }
        if average {
            rank_avg_excel(number, &values, ascending)
        } else {
            rank_excel(number, &values, ascending)
        }
    }

    fn percentile_call(&mut self, env: &mut CalcEnv<'_>) -> Option<f64> {
        let cells = self.cell_range(env)?;
        self.require_comma()?;
        let k = calc_num(self.compare(env)?)?;
        self.close_paren()?;
        let mut values = Vec::new();
        for address in cells {
            if let Some(CalcValue::Num(value)) = self.cell_value(&address, env) {
                if value.is_finite() {
                    values.push(value);
                }
            }
        }
        percentile_inc_excel(&mut values, k)
    }

    fn percentile_exc_call(&mut self, env: &mut CalcEnv<'_>) -> Option<f64> {
        let cells = self.cell_range(env)?;
        self.require_comma()?;
        let k = calc_num(self.compare(env)?)?;
        self.close_paren()?;
        let mut values = Vec::new();
        for address in cells {
            if let Some(CalcValue::Num(value)) = self.cell_value(&address, env) {
                if value.is_finite() {
                    values.push(value);
                }
            }
        }
        percentile_exc_excel(&mut values, k)
    }

    fn quartile_call(&mut self, env: &mut CalcEnv<'_>) -> Option<f64> {
        let cells = self.cell_range(env)?;
        self.require_comma()?;
        let quart = calc_num(self.compare(env)?)?;
        if !quart.is_finite() {
            return None;
        }
        self.close_paren()?;
        let quart = quart.trunc();
        if !(0.0..=4.0).contains(&quart) {
            return None;
        }
        let mut values = Vec::new();
        for address in cells {
            if let Some(CalcValue::Num(value)) = self.cell_value(&address, env) {
                if value.is_finite() {
                    values.push(value);
                }
            }
        }
        percentile_inc_excel(&mut values, quart / 4.0)
    }

    fn quartile_exc_call(&mut self, env: &mut CalcEnv<'_>) -> Option<f64> {
        let cells = self.cell_range(env)?;
        self.require_comma()?;
        let quart = calc_num(self.compare(env)?)?;
        if !quart.is_finite() {
            return None;
        }
        self.close_paren()?;
        let quart = quart.trunc();
        if !(1.0..=3.0).contains(&quart) {
            return None;
        }
        let mut values = Vec::new();
        for address in cells {
            if let Some(CalcValue::Num(value)) = self.cell_value(&address, env) {
                if value.is_finite() {
                    values.push(value);
                }
            }
        }
        percentile_exc_excel(&mut values, quart / 4.0)
    }

    fn mode_call(&mut self, env: &mut CalcEnv<'_>) -> Option<f64> {
        let cells = self.cell_range(env)?;
        self.close_paren()?;
        let mut values = Vec::new();
        for address in cells {
            if let Some(CalcValue::Num(value)) = self.cell_value(&address, env) {
                if value.is_finite() {
                    values.push(value);
                }
            }
        }
        mode_excel(&values)
    }

    fn percent_rank_call(&mut self, env: &mut CalcEnv<'_>) -> Option<f64> {
        let cells = self.cell_range(env)?;
        self.require_comma()?;
        let x_value = calc_num(self.compare(env)?)?;
        self.close_paren()?;
        let mut values = Vec::new();
        for address in cells {
            if let Some(CalcValue::Num(value)) = self.cell_value(&address, env) {
                if value.is_finite() {
                    values.push(value);
                }
            }
        }
        percent_rank_inc(&mut values, x_value)
    }

    fn percent_rank_exc_call(&mut self, env: &mut CalcEnv<'_>) -> Option<f64> {
        let cells = self.cell_range(env)?;
        self.require_comma()?;
        let x_value = calc_num(self.compare(env)?)?;
        self.close_paren()?;
        let mut values = Vec::new();
        for address in cells {
            if let Some(CalcValue::Num(value)) = self.cell_value(&address, env) {
                if value.is_finite() {
                    values.push(value);
                }
            }
        }
        percent_rank_exc(&mut values, x_value)
    }

    /// Y range then X range, same length. A pair is kept when both cells are finite numbers.
    fn range_numbers(&mut self, cells: &[String], env: &mut CalcEnv<'_>) -> Vec<f64> {
        let mut values = Vec::new();
        for address in cells {
            if let Some(CalcValue::Num(number)) = self.cell_value(address, env) {
                if number.is_finite() {
                    values.push(number);
                }
            }
        }
        values
    }

    fn paired_ranges(&mut self, env: &mut CalcEnv<'_>) -> Option<Vec<(f64, f64)>> {
        let ys = self.cell_range(env)?;
        self.require_comma()?;
        let xs = self.cell_range(env)?;
        if ys.len() != xs.len() {
            return None;
        }
        self.skip();
        if self.bytes.get(self.index) != Some(&b')') {
            return None;
        }
        self.index += 1;
        let mut pairs = Vec::new();
        for (y_address, x_address) in ys.iter().zip(xs) {
            let Some(CalcValue::Num(y)) = self.cell_value(y_address, env) else {
                continue;
            };
            let Some(CalcValue::Num(x)) = self.cell_value(&x_address, env) else {
                continue;
            };
            if y.is_finite() && x.is_finite() {
                pairs.push((y, x));
            }
        }
        Some(pairs)
    }

    /// One value range, one criteria range, and one criterion. The ranges must match in size.
    fn ifs_values(&mut self, env: &mut CalcEnv<'_>) -> Option<Vec<f64>> {
        let values = self.cell_range(env)?;
        self.require_comma()?;
        let criteria_cells = self.cell_range(env)?;
        if values.len() != criteria_cells.len() {
            return None;
        }
        self.require_comma()?;
        let criteria = self.compare(env)?;
        let (op, target) = compile_criterion(&criteria)?;
        self.skip();
        if self.bytes.get(self.index) != Some(&b')') {
            return None;
        }
        self.index += 1;
        let mut matched = Vec::new();
        for (value_address, criteria_address) in values.iter().zip(criteria_cells) {
            let Some(CalcValue::Num(criterion_number)) = self.cell_value(&criteria_address, env)
            else {
                continue;
            };
            if !criterion_number.is_finite() || !number_matches(criterion_number, op, target) {
                continue;
            }
            let Some(CalcValue::Num(number)) = self.cell_value(value_address, env) else {
                continue;
            };
            if number.is_finite() {
                matched.push(number);
            }
        }
        Some(matched)
    }

    fn sum_product(&mut self, env: &mut CalcEnv<'_>) -> Option<f64> {
        let mut ranges: Vec<Vec<f64>> = Vec::new();
        loop {
            self.skip();
            if self.bytes.get(self.index) == Some(&b')') {
                if ranges.is_empty() {
                    return None;
                }
                self.index += 1;
                break;
            }
            let cells = self.cell_range(env)?;
            if ranges.len() == 8 {
                return None;
            }
            if ranges
                .first()
                .is_some_and(|first| first.len() != cells.len())
            {
                return None;
            }
            let mut values = Vec::with_capacity(cells.len());
            for address in cells {
                let number = match self.cell_value(&address, env) {
                    Some(CalcValue::Num(number)) if number.is_finite() => number,
                    Some(CalcValue::Num(_)) => return None,
                    _ => 0.0,
                };
                values.push(number);
            }
            ranges.push(values);
            self.skip();
            match self.bytes.get(self.index) {
                Some(&b',') => self.index += 1,
                Some(&b')') => {
                    self.index += 1;
                    break;
                }
                _ => return None,
            }
        }
        let width = ranges.first()?.len();
        let mut total = 0.0;
        for index in 0..width {
            let mut product = 1.0;
            for range in &ranges {
                product *= range[index];
            }
            if !product.is_finite() {
                return None;
            }
            total += product;
        }
        if total.is_finite() {
            Some(total)
        } else {
            None
        }
    }

    fn number(&mut self) -> Option<f64> {
        let start = self.index;
        while self
            .bytes
            .get(self.index)
            .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'.')
        {
            self.index += 1;
        }
        std::str::from_utf8(&self.bytes[start..self.index])
            .ok()?
            .parse()
            .ok()
    }

    fn word(&mut self) -> Option<String> {
        let start = self.index;
        while let Some(byte) = self.bytes.get(self.index).copied() {
            if byte.is_ascii_alphanumeric() || byte == b'$' || byte == b'_' {
                self.index += 1;
                continue;
            }
            if byte == b'.'
                && self
                    .bytes
                    .get(self.index + 1)
                    .is_some_and(|next| next.is_ascii_alphanumeric())
            {
                self.index += 1;
                continue;
            }
            break;
        }
        if start == self.index {
            return None;
        }
        Some(String::from_utf8_lossy(&self.bytes[start..self.index]).replace('$', ""))
    }

    fn cell_token(&mut self) -> Option<String> {
        let word = self.word()?;
        if is_cell_address(&word) {
            Some(word)
        } else {
            None
        }
    }

    fn frequency_spill(&mut self, env: &mut CalcEnv<'_>) -> Option<Spill> {
        let data_cells = self.cell_range(env)?;
        if data_cells.len() > 256 {
            return None;
        }
        self.require_comma()?;
        let bin_cells = self.cell_range(env)?;
        if bin_cells.is_empty() || bin_cells.len() > 16 {
            return None;
        }
        self.close_paren()?;
        let mut data = Vec::new();
        for address in data_cells {
            match self.cell_value(&address, env) {
                None => {}
                Some(CalcValue::Num(number)) if number.is_finite() => data.push(number),
                _ => return None,
            }
        }
        let mut bins = Vec::new();
        for address in &bin_cells {
            let Some(CalcValue::Num(number)) = self.cell_value(address, env) else {
                return None;
            };
            if !number.is_finite() {
                return None;
            }
            if bins.last().is_some_and(|prev| number < *prev) {
                return None;
            }
            bins.push(number);
        }
        let mut counts = vec![0.0; bins.len() + 1];
        for value in data {
            let mut placed = false;
            for (index, bin) in bins.iter().enumerate() {
                if value <= *bin {
                    counts[index] += 1.0;
                    placed = true;
                    break;
                }
            }
            if !placed {
                let last = counts.len() - 1;
                counts[last] += 1.0;
            }
        }
        Some(Spill {
            values: counts,
            columns: 1,
            all_or_nothing: false,
        })
    }

    fn mode_mult_spill(&mut self, env: &mut CalcEnv<'_>) -> Option<Spill> {
        let cells = self.cell_range(env)?;
        if cells.len() > 256 {
            return None;
        }
        self.close_paren()?;
        let mut values = Vec::new();
        for address in cells {
            match self.cell_value(&address, env) {
                Some(CalcValue::Num(number)) if number.is_finite() => values.push(number),
                None | Some(CalcValue::Text(_)) => {}
                Some(CalcValue::Num(_)) => return None,
            }
        }
        Some(Spill {
            values: modes_excel(&values)?,
            columns: 1,
            all_or_nothing: false,
        })
    }

    fn line_pairs(&mut self, env: &mut CalcEnv<'_>) -> Option<Vec<(f64, f64)>> {
        let (ys, y_rows, y_cols) = self.cell_block(env)?;
        if ys.len() > 256 || (y_rows != 1 && y_cols != 1) {
            return None;
        }
        self.require_comma()?;
        let (xs, x_rows, x_cols) = self.cell_block(env)?;
        if xs.len() != ys.len() || (x_rows != 1 && x_cols != 1) {
            return None;
        }
        let mut pairs = Vec::new();
        for (y_address, x_address) in ys.iter().zip(xs.iter()) {
            match (
                self.cell_value(y_address, env),
                self.cell_value(x_address, env),
            ) {
                (Some(CalcValue::Num(y)), Some(CalcValue::Num(x)))
                    if y.is_finite() && x.is_finite() =>
                {
                    pairs.push((y, x));
                }
                (None, _)
                | (_, None)
                | (Some(CalcValue::Text(_)), _)
                | (_, Some(CalcValue::Text(_))) => {}
                _ => return None,
            }
        }
        Some(pairs)
    }

    fn linest_spill(&mut self, env: &mut CalcEnv<'_>) -> Option<Spill> {
        let pairs = self.line_pairs(env)?;
        self.skip();
        if self.bytes.get(self.index) == Some(&b')') {
            self.index += 1;
            return Some(Spill {
                values: vec![slope_excel(&pairs)?, intercept_excel(&pairs)?],
                columns: 2,
                all_or_nothing: true,
            });
        }
        self.require_comma()?;
        let constant = calc_num(self.compare(env)?)?;
        if !constant.is_finite() || constant.trunc() == 0.0 {
            return None;
        }
        self.skip();
        if self.bytes.get(self.index) == Some(&b')') {
            self.index += 1;
            return Some(Spill {
                values: vec![slope_excel(&pairs)?, intercept_excel(&pairs)?],
                columns: 2,
                all_or_nothing: true,
            });
        }
        self.require_comma()?;
        let stats = calc_num(self.compare(env)?)?;
        self.close_paren()?;
        if !stats.is_finite() {
            return None;
        }
        if stats.trunc() == 0.0 {
            return Some(Spill {
                values: vec![slope_excel(&pairs)?, intercept_excel(&pairs)?],
                columns: 2,
                all_or_nothing: true,
            });
        }
        Some(Spill {
            values: linest_stats(&pairs)?,
            columns: 2,
            all_or_nothing: true,
        })
    }

    fn trend_spill(&mut self, env: &mut CalcEnv<'_>) -> Option<Spill> {
        let pairs = self.line_pairs(env)?;
        self.require_comma()?;
        let (news, rows, cols) = self.cell_block(env)?;
        if news.is_empty() || news.len() > 16 || (rows != 1 && cols != 1) {
            return None;
        }
        self.close_paren()?;
        let slope = slope_excel(&pairs)?;
        let intercept = intercept_excel(&pairs)?;
        let mut values = Vec::new();
        for address in news {
            let Some(CalcValue::Num(x_value)) = self.cell_value(&address, env) else {
                return None;
            };
            if !x_value.is_finite() {
                return None;
            }
            let y_value = intercept + slope * x_value;
            if !y_value.is_finite() {
                return None;
            }
            values.push(y_value);
        }
        Some(Spill {
            values,
            columns: 1,
            all_or_nothing: true,
        })
    }

    fn quoted_sheet(&mut self) -> Option<String> {
        if self.bytes.get(self.index) != Some(&b'\'') {
            return None;
        }
        self.index += 1;
        let mut raw = Vec::new();
        let mut closed = false;
        while let Some(&byte) = self.bytes.get(self.index) {
            if byte == b'\'' {
                if self.bytes.get(self.index + 1) == Some(&b'\'') {
                    raw.push(b'\'');
                    self.index += 2;
                    continue;
                }
                self.index += 1;
                closed = true;
                break;
            }
            raw.push(byte);
            self.index += 1;
            if raw.len() > 128 {
                return None;
            }
        }
        if !closed {
            return None;
        }
        let name = String::from_utf8(raw).ok()?;
        let count = name.chars().count();
        if count == 0 || count > 31 {
            return None;
        }
        Some(name)
    }

    fn foreign_cell(&mut self, sheet: &str, env: &CalcEnv<'_>) -> Option<CalcValue> {
        self.skip();
        if self.bytes.get(self.index) != Some(&b'!') {
            return None;
        }
        self.index += 1;
        let address = self.cell_token()?;
        self.skip();
        if self.bytes.get(self.index) == Some(&b':') {
            return None;
        }
        let book = env.foreign.get(&sheet.to_ascii_lowercase())?;
        let address = address.to_ascii_uppercase();
        if let Some(number) = book.literals.get(&address) {
            return number.is_finite().then_some(CalcValue::Num(*number));
        }
        book.texts
            .get(&address)
            .map(|text| CalcValue::Text(text.clone()))
    }

    fn cell_value(&self, address: &str, env: &mut CalcEnv<'_>) -> Option<CalcValue> {
        if let Some((sheet, cell)) = address.split_once('!') {
            let book = env.foreign.get(&sheet.to_ascii_lowercase())?;
            let cell = cell.to_ascii_uppercase();
            if let Some(number) = book.literals.get(&cell) {
                return number.is_finite().then_some(CalcValue::Num(*number));
            }
            return book
                .texts
                .get(&cell)
                .map(|text| CalcValue::Text(text.clone()));
        }
        let address = address.to_ascii_uppercase();
        if !env.visiting.insert(address.clone()) {
            return None;
        }
        let value = if let Some(formula) = env.formulas.get(&address) {
            eval_formula(
                formula,
                env.formulas,
                env.literals,
                env.texts,
                env.foreign,
                env.names,
                env.sheet,
                env.date1904,
                env.visiting,
            )
        } else if let Some(number) = env.literals.get(&address) {
            Some(CalcValue::Num(*number))
        } else {
            env.texts
                .get(&address)
                .map(|text| CalcValue::Text(text.clone()))
        };
        env.visiting.remove(&address);
        value
    }
}

fn cells_in_range(start: &str, end: &str) -> Option<Vec<String>> {
    let (c1, r1) = split_address(start)?;
    let (c2, r2) = split_address(end)?;
    let (c1, c2) = (c1.min(c2), c1.max(c2));
    let (r1, r2) = (r1.min(r2), r1.max(r2));
    if (c2 - c1 + 1).saturating_mul(r2 - r1 + 1) > 4096 {
        return None;
    }
    let mut cells = Vec::new();
    for row in r1..=r2 {
        for col in c1..=c2 {
            cells.push(format!("{}{row}", column_name(col)));
        }
    }
    Some(cells)
}

fn split_address(address: &str) -> Option<(u32, u32)> {
    let split = address.find(|ch: char| ch.is_ascii_digit())?;
    let col = column_index(&address[..split])?;
    let row = address[split..].parse::<u32>().ok()?;
    Some((col, row))
}

fn column_index(letters: &str) -> Option<u32> {
    let mut value = 0u32;
    for ch in letters.chars() {
        if !ch.is_ascii_alphabetic() {
            return None;
        }
        value = value
            .checked_mul(26)?
            .checked_add(u32::from(ch.to_ascii_uppercase()) - u32::from(b'A') + 1)?;
    }
    Some(value)
}

fn column_name(mut index: u32) -> String {
    let mut out = Vec::new();
    while index > 0 {
        index -= 1;
        out.push(b'A' + (index % 26) as u8);
        index /= 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

fn cell_element(address: &str, text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.eq_ignore_ascii_case("true") || trimmed.eq_ignore_ascii_case("false") {
        let bit = if trimmed.eq_ignore_ascii_case("true") {
            "1"
        } else {
            "0"
        };
        return format!(r#"<c r="{address}" t="b"><v>{bit}</v></c>"#);
    }
    if is_plain_number(trimmed) {
        return format!(r#"<c r="{address}"><v>{trimmed}</v></c>"#);
    }
    format!(
        r#"<c r="{address}" t="inlineStr"><is><t>{}</t></is></c>"#,
        escape(text)
    )
}

fn is_plain_number(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let rest = if first == '-' {
        let Some(next) = chars.next() else {
            return false;
        };
        if !next.is_ascii_digit() {
            return false;
        }
        chars
    } else if first.is_ascii_digit() {
        chars
    } else {
        return false;
    };
    let mut dot = false;
    for ch in rest {
        if ch == '.' {
            if dot {
                return false;
            }
            dot = true;
        } else if !ch.is_ascii_digit() {
            return false;
        }
    }
    true
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
                    value.push_str(&xml_text(text.as_ref()));
                }
            }
            Ok(Event::GeneralRef(entity)) => {
                if in_v || in_t {
                    value.push_str(entity_text(entity.as_ref()));
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
                current.push_str(&xml_text(text.as_ref()));
            }
            Ok(Event::GeneralRef(entity)) if in_t => {
                current.push_str(entity_text(entity.as_ref()));
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
                current.push_str(&xml_text(text.as_ref()));
            }
            Ok(Event::GeneralRef(entity)) if in_t => {
                current.push_str(entity_text(entity.as_ref()));
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

fn entity_text(raw: &str) -> &'static str {
    match raw {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        _ => "",
    }
}

fn xml_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find(';') else {
            out.push('&');
            rest = after;
            continue;
        };
        let decoded = match &after[..end] {
            "amp" => "&",
            "lt" => "<",
            "gt" => ">",
            "quot" => "\"",
            "apos" => "'",
            _ => "",
        };
        if decoded.is_empty() {
            out.push('&');
            rest = after;
        } else {
            out.push_str(decoded);
            rest = &after[end + 1..];
        }
    }
    out.push_str(rest);
    out
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
    fn set_sheet_cell_rewrites_a_value_and_leaves_a_formula_and_drawing() {
        let drawing = "<drawing>keep-me</drawing>";
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
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1"><v>42</v></c><c r="C1"><f>1+1</f><v>2</v></c></row></sheetData></worksheet>"#,
            ),
            ("xl/drawings/drawing1.xml", drawing),
        ]);
        let err = set_sheet_cell(&bytes, "Budgets", "C1", "9").unwrap_err();
        assert_eq!(err, "viewer-sheet-formula");
        let saved = set_sheet_cell(&bytes, "Budgets", "B1", "7").unwrap();
        let preview = render_office(&saved, false).unwrap();
        let OfficePreview::Sheets(book) = preview else {
            panic!("workbook should be a sheet table");
        };
        let row = &book.sheets[0].rows[0];
        assert_eq!(row[0].text, "Orchid");
        assert_eq!(row[1].text, "7");
        assert_eq!(row[2].text, "2");
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let kept = read_entry(&mut archive, "xl/drawings/drawing1.xml").unwrap();
        assert_eq!(kept, drawing);
        let renamed = set_sheet_cell(&bytes, "Budgets", "A1", "Lily & Rose").unwrap();
        let preview = render_office(&renamed, false).unwrap();
        let OfficePreview::Sheets(book) = preview else {
            panic!("workbook should be a sheet table");
        };
        let mut renamed_zip = ZipArchive::new(Cursor::new(renamed)).unwrap();
        let sheet = read_entry(&mut renamed_zip, "xl/worksheets/sheet1.xml").unwrap();
        let (rows, _) = parse_sheet(&sheet, &[]);
        assert_eq!(rows[0][0].text, "Lily & Rose");
        assert_eq!(book.sheets[0].rows[0][0].text, "Lily & Rose");
        assert_eq!(book.sheets[0].rows[0][1].text, "42");
    }

    #[test]
    fn set_sheet_cell_recalculates_arithmetic_sum_and_average() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>2</v></c><c r="B1"><v>3</v></c><c r="C1"><f>A1+B1</f><v>0</v></c><c r="D1"><f>SUM(A1:A2)</f><v>0</v></c><c r="E1"><f>AVERAGE(A1,B1)</f><v>0</v></c><c r="F1"><f>ROMAN(A1)</f><v>9</v></c></row><row r="2"><c r="A2"><v>4</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let refused = set_sheet_cell(&bytes, "Budgets", "C1", "9").unwrap_err();
        assert_eq!(refused, "viewer-sheet-formula");
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "4").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<c r="C1"><f>A1+B1</f><v>7</v></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SUM(A1:A2)</f><v>8</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>AVERAGE(A1,B1)</f><v>3.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="F1" t="inlineStr"><f>ROMAN(A1)</f><is><t>IV</t></is></c>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_recalculates_min_max_count_and_if() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>2</v></c><c r="B1"><v>8</v></c><c r="C1"><f>MIN(A1:B1)</f><v>0</v></c><c r="D1"><f>MAX(A1,B1)</f><v>0</v></c><c r="E1"><f>COUNT(A1:B1)</f><v>0</v></c><c r="F1"><f>IF(A1>5,B1,A1)</f><v>0</v></c><c r="G1"><f>IF(A1<>2,9,4)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "B1", "3").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>MIN(A1:B1)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MAX(A1,B1)</f><v>3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COUNT(A1:B1)</f><v>2</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>IF(A1>5,B1,A1)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>IF(A1<>2,9,4)</f><v>4</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_rounds_and_joins_text() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1.26</v></c><c r="B1"><f>ROUND(A1,1)</f><v>0</v></c><c r="C1"><f>ABS(-4)</f><v>0</v></c><c r="D1"><f>INT(-1.2)</f><v>0</v></c><c r="E1" t="inlineStr"><is><t>x</t></is></c><c r="F1"><f>A1&amp;E1</f><v>0</v></c><c r="G1"><f>CONCAT("a","b")</f><v>0</v></c><c r="H1"><f>ROMAN(A1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2.26").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>ROUND(A1,1)</f><v>2.3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ABS(-4)</f><v>4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>INT(-1.2)</f><v>-2</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="F1" t="inlineStr"><f>A1&amp;E1</f><is><t>2.26x</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<c r="G1" t="inlineStr"><f>CONCAT("a","b")</f><is><t>ab</t></is></c>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="H1" t="inlineStr"><f>ROMAN(A1)</f><is><t>II</t></is></c>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_measures_and_slices_text() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>LEN("Orchid")</f><v>0</v></c><c r="C1"><f>LEFT("Orchid",2)</f><v>0</v></c><c r="D1"><f>RIGHT("Orchid",3)</f><v>0</v></c><c r="E1"><f>MID("Orchid",2,3)</f><v>0</v></c><c r="F1"><f>UPPER("ab")</f><v>0</v></c><c r="G1"><f>LOWER("AB")</f><v>0</v></c><c r="H1"><f>LEFT("ab",-1)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>LEN("Orchid")</f><v>6</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>LEFT("Orchid",2)</f><is><t>Or</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RIGHT("Orchid",3)</f><is><t>hid</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MID("Orchid",2,3)</f><is><t>rch</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>UPPER("ab")</f><is><t>AB</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOWER("AB")</f><is><t>ab</t></is>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>LEFT("ab",-1)</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_trims_finds_and_repeats_text() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>TRIM("  a   b  ")</f><v>0</v></c><c r="C1"><f>SUBSTITUTE("ababa","a","X")</f><v>0</v></c><c r="D1"><f>SUBSTITUTE("ababa","a","X",2)</f><v>0</v></c><c r="E1"><f>FIND("ch","Orchid")</f><v>0</v></c><c r="F1"><f>SEARCH("CH","Orchid")</f><v>0</v></c><c r="G1"><f>FIND("CH","Orchid")</f><v>7</v></c><c r="H1"><f>REPT("ab",3)</f><v>0</v></c><c r="I1"><f>EXACT("Ab","Ab")</f><v>0</v></c><c r="J1"><f>EXACT("Ab","ab")</f><v>0</v></c><c r="K1"><f>REPT("a",-1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>TRIM("  a   b  ")</f><is><t>a b</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUBSTITUTE("ababa","a","X")</f><is><t>XbXbX</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUBSTITUTE("ababa","a","X",2)</f><is><t>abXba</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FIND("ch","Orchid")</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SEARCH("CH","Orchid")</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FIND("CH","Orchid")</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>REPT("ab",3)</f><is><t>ababab</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EXACT("Ab","Ab")</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EXACT("Ab","ab")</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>REPT("a",-1)</f><v>9</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_and_or_not() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>AND(A1&gt;0,1)</f><v>0</v></c><c r="C1"><f>AND(A1&gt;5,1)</f><v>0</v></c><c r="D1"><f>OR(A1&gt;5,0)</f><v>0</v></c><c r="E1"><f>OR(0,A1)</f><v>0</v></c><c r="F1"><f>NOT(0)</f><v>0</v></c><c r="G1"><f>NOT(A1)</f><v>0</v></c><c r="H1"><f>AND()</f><v>7</v></c><c r="I1"><f>NOT("a")</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>AND(A1&gt;0,1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AND(A1&gt;5,1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>OR(A1&gt;5,0)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>OR(0,A1)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>NOT(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>NOT(A1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>AND()</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>NOT("a")</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_sqrt_power_and_mod() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>SQRT(9)</f><v>0</v></c><c r="C1"><f>POWER(2,3)</f><v>0</v></c><c r="D1"><f>MOD(-3,2)</f><v>0</v></c><c r="E1"><f>MOD(3,-2)</f><v>0</v></c><c r="F1"><f>SQRT(-1)</f><v>7</v></c><c r="G1"><f>POWER(-8,0.5)</f><v>8</v></c><c r="H1"><f>MOD(5,0)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>SQRT(9)</f><v>3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>POWER(2,3)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MOD(-3,2)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MOD(3,-2)</f><v>-1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SQRT(-1)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>POWER(-8,0.5)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MOD(5,0)</f><v>9</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_sign_product_and_quotient() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>SIGN(-4)</f><v>0</v></c><c r="C1"><f>SIGN(0)</f><v>0</v></c><c r="D1"><f>PRODUCT(2,3,4)</f><v>0</v></c><c r="E1"><f>PRODUCT()</f><v>0</v></c><c r="F1"><f>QUOTIENT(5,2)</f><v>0</v></c><c r="G1"><f>QUOTIENT(-5,2)</f><v>0</v></c><c r="H1"><f>QUOTIENT(5,0)</f><v>7</v></c><c r="I1"><f>EVEN(2.1)</f><v>0</v></c><c r="J1"><f>EVEN(-2.1)</f><v>0</v></c><c r="K1"><f>ODD(0)</f><v>0</v></c><c r="L1"><f>PI()</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>SIGN(-4)</f><v>-1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SIGN(0)</f><v>0</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>PRODUCT(2,3,4)</f><v>24</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>PRODUCT()</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>QUOTIENT(5,2)</f><v>2</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>QUOTIENT(-5,2)</f><v>-2</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>QUOTIENT(5,0)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>EVEN(2.1)</f><v>4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>EVEN(-2.1)</f><v>-4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ODD(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>PI()</f><v>3.14159265</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_replaces_a_span() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>REPLACE("Orchid",3,2,"XY")</f><v>0</v></c><c r="C1"><f>REPLACE("ab",3,1,"X")</f><v>0</v></c><c r="D1"><f>REPLACE("abcd",2,10,"Z")</f><v>0</v></c><c r="E1"><f>REPLACE("ab",0,1,"X")</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>REPLACE("Orchid",3,2,"XY")</f><is><t>OrXYid</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>REPLACE("ab",3,1,"X")</f><is><t>abX</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>REPLACE("abcd",2,10,"Z")</f><is><t>aZ</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>REPLACE("ab",0,1,"X")</f><v>7</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_value_t_and_n() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>VALUE(" 2.5 ")</f><v>0</v></c><c r="C1"><f>VALUE("-3")</f><v>0</v></c><c r="D1"><f>VALUE("x")</f><v>7</v></c><c r="E1"><f>T("ab")</f><v>0</v></c><c r="F1"><f>T(4)</f><v>0</v></c><c r="G1"><f>N("ab")</f><v>0</v></c><c r="H1"><f>N(4)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>VALUE(" 2.5 ")</f><v>2.5</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>VALUE("-3")</f><v>-3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>VALUE("x")</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>T("ab")</f><is><t>ab</t></is>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>T(4)</f><is><t></t></is>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>N("ab")</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>N(4)</f><v>4</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_rounds_up_and_down() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>ROUNDUP(1.234,2)</f><v>0</v></c><c r="C1"><f>ROUNDDOWN(1.239,2)</f><v>0</v></c><c r="D1"><f>ROUNDUP(-1.234,2)</f><v>0</v></c><c r="E1"><f>ROUNDDOWN(-1.239,2)</f><v>0</v></c><c r="F1"><f>CEILING.MATH(-1.2)</f><v>0</v></c><c r="G1"><f>FLOOR.MATH(-1.2)</f><v>0</v></c><c r="H1"><f>ROUNDUP(1.2,20)</f><v>7</v></c><c r="I1"><f>CEILING.MATH(1.2,1)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>ROUNDUP(1.234,2)</f><v>1.24</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ROUNDDOWN(1.239,2)</f><v>1.23</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ROUNDUP(-1.234,2)</f><v>-1.24</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ROUNDDOWN(-1.239,2)</f><v>-1.23</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CEILING.MATH(-1.2)</f><v>-1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FLOOR.MATH(-1.2)</f><v>-2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ROUNDUP(1.2,20)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CEILING.MATH(1.2,1)</f><v>2</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_math_step() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>CEILING.MATH(-5.5,2)</f><v>0</v></c><c r="C1"><f>CEILING.MATH(-5.5,2,1)</f><v>0</v></c><c r="D1"><f>FLOOR.MATH(5.5,2)</f><v>0</v></c><c r="E1"><f>FLOOR.MATH(-5.5,1,1)</f><v>0</v></c><c r="F1"><f>CEILING.MATH(5,0)</f><v>0</v></c><c r="G1"><f>FLOOR.MATH(5,-2)</f><v>0</v></c><c r="H1"><f>CEILING.MATH(&quot;ab&quot;,1)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>CEILING.MATH(-5.5,2)</f><v>-4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CEILING.MATH(-5.5,2,1)</f><v>-6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FLOOR.MATH(5.5,2)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FLOOR.MATH(-5.5,1,1)</f><v>-5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CEILING.MATH(5,0)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FLOOR.MATH(5,-2)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CEILING.MATH(&quot;ab&quot;,1)</f><v>7</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_average_a() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><f>AVERAGEA(A1:C1)</f><v>0</v></c><c r="E1"><f>MINA(A1:C1)</f><v>0</v></c><c r="F1"><f>MAXA(A1:C1)</f><v>0</v></c><c r="G1"><f>STDEVA(A1:B1)</f><v>0</v></c><c r="H1"><f>VARA(A1:B1)</f><v>0</v></c><c r="I1"><f>AVERAGEA()</f><v>7</v></c><c r="J1"><f>AVERAGEA(&quot;ab&quot;)</f><v>0</v></c><c r="K1"><f>STDEVA(A1)</f><v>8</v></c><c r="L1"><f>MINA()</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>AVERAGEA(A1:C1)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>MINA(A1:C1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MAXA(A1:C1)</f><v>1</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>STDEVA(A1:B1)</f><v>0.70710678</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>VARA(A1:B1)</f><v>0.5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>AVERAGEA()</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>AVERAGEA(&quot;ab&quot;)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>STDEVA(A1)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MINA()</f><v>0</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_date_serial() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>DATE(1900,1,1)</f><v>0</v></c><c r="C1"><f>DATE(1900,2,28)</f><v>0</v></c><c r="D1"><f>DATE(1900,2,29)</f><v>0</v></c><c r="E1"><f>DATE(1900,3,1)</f><v>0</v></c><c r="F1"><f>DATE(108,1,2)</f><v>0</v></c><c r="G1"><f>DATE(2020,1,1)</f><v>0</v></c><c r="H1"><f>DATE(1900,1,0)</f><v>0</v></c><c r="I1"><f>YEAR(60)</f><v>0</v></c><c r="J1"><f>MONTH(60)</f><v>0</v></c><c r="K1"><f>DAY(60)</f><v>0</v></c><c r="L1"><f>YEAR(43831)</f><v>0</v></c><c r="M1"><f>MONTH(43831)</f><v>0</v></c><c r="N1"><f>DAY(43831)</f><v>0</v></c><c r="O1"><f>YEAR(0)</f><v>0</v></c><c r="P1"><f>MONTH(0)</f><v>0</v></c><c r="Q1"><f>DAY(0)</f><v>0</v></c><c r="R1"><f>DATE(-1,1,1)</f><v>9</v></c><c r="S1"><f>YEAR(-1)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>DATE(1900,1,1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATE(1900,2,28)</f><v>59</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATE(1900,2,29)</f><v>60</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATE(1900,3,1)</f><v>61</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATE(108,1,2)</f><v>39449</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATE(2020,1,1)</f><v>43831</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATE(1900,1,0)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>YEAR(60)</f><v>1900</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MONTH(60)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DAY(60)</f><v>29</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>YEAR(43831)</f><v>2020</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>MONTH(43831)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DAY(43831)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>YEAR(0)</f><v>1900</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MONTH(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DAY(0)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DATE(-1,1,1)</f><v>9</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>YEAR(-1)</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_base_conversion() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>DEC2BIN(5)</f><v>0</v></c><c r="C1"><f>DEC2BIN(5,4)</f><v>0</v></c><c r="D1"><f>DEC2BIN(-1)</f><v>0</v></c><c r="E1"><f>DEC2BIN(512)</f><v>9</v></c><c r="F1"><f>BIN2DEC(&quot;101&quot;)</f><v>0</v></c><c r="G1"><f>BIN2DEC(&quot;1111111111&quot;)</f><v>0</v></c><c r="H1"><f>DEC2HEX(255)</f><v>0</v></c><c r="I1"><f>DEC2HEX(-1)</f><v>0</v></c><c r="J1"><f>HEX2DEC(&quot;ff&quot;)</f><v>0</v></c><c r="K1"><f>HEX2DEC(&quot;FFFFFFFFFF&quot;)</f><v>0</v></c><c r="L1"><f>DEC2OCT(8)</f><v>0</v></c><c r="M1"><f>DEC2OCT(-1)</f><v>0</v></c><c r="N1"><f>OCT2DEC(&quot;10&quot;)</f><v>0</v></c><c r="O1"><f>OCT2DEC(&quot;7777777777&quot;)</f><v>0</v></c><c r="P1"><f>BASE(13,2,8)</f><v>0</v></c><c r="Q1"><f>BASE(255,16)</f><v>0</v></c><c r="R1"><f>DECIMAL(&quot;FF&quot;,16)</f><v>0</v></c><c r="S1"><f>DECIMAL(&quot;101&quot;,2)</f><v>0</v></c><c r="T1"><f>DEC2BIN(-1,4)</f><v>8</v></c><c r="U1"><f>BASE(1,2,0)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<c r="B1" t="inlineStr"><f>DEC2BIN(5)</f><is><t>101</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet
                .contains(r#"<c r="C1" t="inlineStr"><f>DEC2BIN(5,4)</f><is><t>0101</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<c r="D1" t="inlineStr"><f>DEC2BIN(-1)</f><is><t>1111111111</t></is></c>"#
            ),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>DEC2BIN(512)</f><v>9</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>BIN2DEC(&quot;101&quot;)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BIN2DEC(&quot;1111111111&quot;)</f><v>-1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="H1" t="inlineStr"><f>DEC2HEX(255)</f><is><t>FF</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<c r="I1" t="inlineStr"><f>DEC2HEX(-1)</f><is><t>FFFFFFFFFF</t></is></c>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HEX2DEC(&quot;ff&quot;)</f><v>255</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HEX2DEC(&quot;FFFFFFFFFF&quot;)</f><v>-1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="L1" t="inlineStr"><f>DEC2OCT(8)</f><is><t>10</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<c r="M1" t="inlineStr"><f>DEC2OCT(-1)</f><is><t>7777777777</t></is></c>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>OCT2DEC(&quot;10&quot;)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>OCT2DEC(&quot;7777777777&quot;)</f><v>-1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<c r="P1" t="inlineStr"><f>BASE(13,2,8)</f><is><t>00001101</t></is></c>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="Q1" t="inlineStr"><f>BASE(255,16)</f><is><t>FF</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DECIMAL(&quot;FF&quot;,16)</f><v>255</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DECIMAL(&quot;101&quot;,2)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>DEC2BIN(-1,4)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>BASE(1,2,0)</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_bessel() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>BESSELJ(1,0)</f><v>0</v></c><c r="C1"><f>BESSELJ(1,1)</f><v>0</v></c><c r="D1"><f>BESSELI(1,0)</f><v>0</v></c><c r="E1"><f>BESSELI(1,1)</f><v>0</v></c><c r="F1"><f>BESSELJ(0,0)</f><v>0</v></c><c r="G1"><f>BESSELJ(0,1)</f><v>0</v></c><c r="H1"><f>BESSELJ(1,1.9)</f><v>0</v></c><c r="I1"><f>BESSELJ(1,-1)</f><v>9</v></c><c r="J1"><f>BESSELJ(40,0)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>BESSELJ(1,0)</f><v>0.76519769</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BESSELJ(1,1)</f><v>0.44005059</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BESSELI(1,0)</f><v>1.26606588</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BESSELI(1,1)</f><v>0.5651591</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>BESSELJ(0,0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>BESSELJ(0,1)</f><v>0</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>BESSELJ(1,1.9)</f><v>0.44005059</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>BESSELJ(1,-1)</f><v>9</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>BESSELJ(40,0)</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_mdeterm() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c><c r="C1"><v>3</v></c><c r="D1"><f>MDETERM(A1:C3)</f><v>0</v></c><c r="E1"><v>1</v></c><c r="F1"><v>2</v></c><c r="G1"><f>MDETERM(E1:F2)</f><v>0</v></c><c r="H1"><v>5</v></c><c r="I1"><f>MDETERM(H1:H1)</f><v>0</v></c><c r="J1"><v>1</v></c><c r="L1"><f>MDETERM(J1:K2)</f><v>0</v></c><c r="M1" t="inlineStr"><is><t>xy</t></is></c><c r="N1"><v>1</v></c><c r="O1"><f>MDETERM(M1:N2)</f><v>9</v></c><c r="P1"><f>MDETERM(A1:B3)</f><v>8</v></c><c r="Q1"><v>1</v></c><c r="R1"><v>2</v></c><c r="S1"><f>MDETERM(Q1:R2)</f><v>0</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>0</v></c><c r="B2"><v>1</v></c><c r="C2"><v>4</v></c><c r="E2"><v>3</v></c><c r="F2"><v>4</v></c><c r="K2"><v>1</v></c><c r="M2"><v>0</v></c><c r="N2"><v>1</v></c><c r="Q2"><v>2</v></c><c r="R2"><v>4</v></c></row><row r="3"><c r="A3"><v>5</v></c><c r="B3"><v>6</v></c><c r="C3"><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>MDETERM(A1:C3)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MDETERM(E1:F2)</f><v>-2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MDETERM(H1:H1)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MDETERM(J1:K2)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MDETERM(M1:N2)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MDETERM(A1:B3)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MDETERM(Q1:R2)</f><v>0</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_lookup() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>10</v></c><c r="B1"><v>20</v></c><c r="C1"><v>30</v></c><c r="E1"><v>1</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>40</v></c><c r="B2"><v>50</v></c><c r="C2"><v>60</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>Cat</t></is></c></row><row r="4"><c r="A4"><f>INDEX(A1:C2,2,3)</f><v>0</v></c><c r="B4"><f>INDEX(A1:A2,2)</f><v>0</v></c><c r="C4"><f>INDEX(A1:C2,1)</f><v>8</v></c><c r="D4"><f>INDEX(A1:C2,2,0)</f><v>7</v></c><c r="E4"><f>INDEX(A3:A3,1)</f><v>0</v></c><c r="F4"><f>MATCH(40,A1:A2,0)</f><v>0</v></c><c r="G4"><f>MATCH(20,A1:C1,0)</f><v>0</v></c><c r="H4"><f>MATCH(&quot;cat&quot;,A3:A3,0)</f><v>0</v></c><c r="I4"><f>MATCH(40,A1:A2,1)</f><v>6</v></c><c r="J4"><f>MATCH(99,A1:A2,0)</f><v>5</v></c><c r="K4"><f>VLOOKUP(40,A1:C2,3,0)</f><v>0</v></c><c r="L4"><f>VLOOKUP(10,A1:C2,2,0)</f><v>0</v></c><c r="M4"><f>VLOOKUP(40,A1:C2,3)</f><v>4</v></c><c r="N4"><f>HLOOKUP(20,A1:C2,2,0)</f><v>0</v></c><c r="O4"><f>HLOOKUP(30,A1:C2,2,1)</f><v>3</v></c><c r="P4"><f>INDEX(D1:D1,1)</f><v>0</v></c><c r="Q4"><f>MATCH(&quot;1&quot;,E1:E1,0)</f><v>2</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>INDEX(A1:C2,2,3)</f><v>60</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INDEX(A1:A2,2)</f><v>40</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INDEX(A1:C2,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INDEX(A1:C2,2,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<c r="E4" t="inlineStr"><f>INDEX(A3:A3,1)</f><is><t>Cat</t></is></c>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(40,A1:A2,0)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(20,A1:C1,0)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(&quot;cat&quot;,A3:A3,0)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(40,A1:A2,1)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(99,A1:A2,0)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>VLOOKUP(40,A1:C2,3,0)</f><v>60</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>VLOOKUP(10,A1:C2,2,0)</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>VLOOKUP(40,A1:C2,3)</f><v>60</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HLOOKUP(20,A1:C2,2,0)</f><v>50</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HLOOKUP(30,A1:C2,2,1)</f><v>60</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INDEX(D1:D1,1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(&quot;1&quot;,E1:E1,0)</f><v>2</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_lookup_walk() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>10</v></c><c r="B1"><f>MATCH(15,A1:A2,1)</f><v>0</v></c><c r="C1"><f>MATCH(5,A1:A2,1)</f><v>8</v></c><c r="D1"><f>MATCH(30,E1:E2,-1)</f><v>0</v></c><c r="E1"><v>40</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>40</v></c><c r="E2"><v>10</v></c></row><row r="3"><c r="A3" t="inlineStr"><is><t>Cat</t></is></c><c r="B3"><f>MATCH(&quot;c*&quot;,A3:A3,0)</f><v>0</v></c><c r="C3"><f>MATCH(&quot;c?&quot;,A3:A3,0)</f><v>7</v></c><c r="D3"><f>MATCH(&quot;c~*&quot;,A3:A3,0)</f><v>6</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>MATCH(15,A1:A2,1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(5,A1:A2,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(30,E1:E2,-1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(&quot;c*&quot;,A3:A3,0)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(&quot;c?&quot;,A3:A3,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MATCH(&quot;c~*&quot;,A3:A3,0)</f><v>6</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_beta_dist() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>BETA.DIST(0.5,2,3,1)</f><v>0</v></c><c r="C1"><f>BETA.DIST(0.5,2,3,0)</f><v>0</v></c><c r="D1"><f>BETA.DIST(0.3,5,2,1)</f><v>0</v></c><c r="E1"><f>BETA.DIST(0.2,0.5,0.5,1)</f><v>0</v></c><c r="F1"><f>BETA.DIST(0.2,1,1,1)</f><v>0</v></c><c r="G1"><f>BETA.DIST(2,1,1,1)</f><v>9</v></c><c r="H1"><f>BETA.DIST(0.5,2,3)</f><v>8</v></c><c r="I1"><f>BETA.DIST(0.5,0,1,1)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>BETA.DIST(0.5,2,3,1)</f><v>0.6875</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BETA.DIST(0.5,2,3,0)</f><v>1.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BETA.DIST(0.3,5,2,1)</f><v>0.010935</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BETA.DIST(0.2,0.5,0.5,1)</f><v>0.29516724</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BETA.DIST(0.2,1,1,1)</f><v>0.2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BETA.DIST(2,1,1,1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BETA.DIST(0.5,2,3)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BETA.DIST(0.5,0,1,1)</f><v>7</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_t_dist() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>T.DIST(1,1,1)</f><v>0</v></c><c r="C1"><f>T.DIST(0,1,0)</f><v>0</v></c><c r="D1"><f>T.DIST(2,5,1)</f><v>0</v></c><c r="E1"><f>T.DIST(2,5,0)</f><v>0</v></c><c r="F1"><f>T.DIST.RT(2,5)</f><v>0</v></c><c r="G1"><f>T.DIST.2T(2,5)</f><v>0</v></c><c r="H1"><f>TDIST(2,5,1)</f><v>0</v></c><c r="I1"><f>TDIST(2,5,2)</f><v>0</v></c><c r="J1"><f>T.DIST.2T(-1,5)</f><v>9</v></c><c r="K1"><f>TDIST(1,5,3)</f><v>8</v></c><c r="L1"><f>T.DIST(1,0,1)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>T.DIST(1,1,1)</f><v>0.75</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>T.DIST(0,1,0)</f><v>0.31830989</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>T.DIST(2,5,1)</f><v>0.94903026</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>T.DIST(2,5,0)</f><v>0.06509031</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>T.DIST.RT(2,5)</f><v>0.05096974</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>T.DIST.2T(2,5)</f><v>0.10193948</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TDIST(2,5,1)</f><v>0.05096974</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TDIST(2,5,2)</f><v>0.10193948</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>T.DIST.2T(-1,5)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>TDIST(1,5,3)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>T.DIST(1,0,1)</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_f_dist() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>F.DIST(2,5,10,1)</f><v>0</v></c><c r="C1"><f>F.DIST(2,5,10,0)</f><v>0</v></c><c r="D1"><f>F.DIST.RT(2,5,10)</f><v>0</v></c><c r="E1"><f>FDIST(2,5,10)</f><v>0</v></c><c r="F1"><f>F.DIST(-1,5,10,1)</f><v>9</v></c><c r="G1"><f>F.DIST(2,0,10,1)</f><v>8</v></c><c r="H1"><f>FDIST(2,5)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>F.DIST(2,5,10,1)</f><v>0.83580505</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>F.DIST(2,5,10,0)</f><v>0.16200574</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>F.DIST.RT(2,5,10)</f><v>0.16419495</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FDIST(2,5,10)</f><v>0.16419495</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>F.DIST(-1,5,10,1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>F.DIST(2,0,10,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>FDIST(2,5)</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_t_test() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c><c r="C1"><f>T.TEST(A1:A3,B1:B3,2,1)</f><v>0</v></c><c r="D1"><f>T.TEST(A1:A3,B1:B3,1,1)</f><v>0</v></c><c r="E1"><f>TTEST(A1:A3,B1:B3,2,2)</f><v>0</v></c><c r="F1"><f>T.TEST(A1:A3,B1:B3,2,3)</f><v>0</v></c><c r="G1"><f>F.TEST(A1:A3,B1:B3)</f><v>0</v></c><c r="H1"><f>CONFIDENCE.T(0.05,1,10)</f><v>0</v></c><c r="I1"><f>T.TEST(A1:A3,B1:B3,2,4)</f><v>9</v></c><c r="J1"><f>CONFIDENCE.T(0,1,10)</f><v>8</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>2</v></c><c r="B2"><v>3</v></c></row><row r="3"><c r="A3"><v>3</v></c><c r="B3"><v>5</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>T.TEST(A1:A3,B1:B3,2,1)</f><v>0.05719096</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>T.TEST(A1:A3,B1:B3,1,1)</f><v>0.02859548</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TTEST(A1:A3,B1:B3,2,2)</f><v>0.27457663</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>T.TEST(A1:A3,B1:B3,2,3)</f><v>0.28462718</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>F.TEST(A1:A3,B1:B3)</f><v>0.6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONFIDENCE.T(0.05,1,10)</f><v>0.71535691</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>T.TEST(A1:A3,B1:B3,2,4)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONFIDENCE.T(0,1,10)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_annuity() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>-100</v></c><c r="B1"><f>PMT(0.01,10,-1000)</f><v>0</v></c><c r="C1"><f>PMT(0,10,-1000)</f><v>0</v></c><c r="D1"><f>PMT(0.01,10,-1000,0,1)</f><v>0</v></c><c r="E1"><f>FV(0.01,10,-100,-1000)</f><v>0</v></c><c r="F1"><f>PV(0.01,10,-100)</f><v>0</v></c><c r="G1"><f>NPER(0.01,-100,1000)</f><v>0</v></c><c r="H1"><f>NPER(0,-100,1000)</f><v>0</v></c><c r="I1"><f>RATE(12,-100,1000)</f><v>0</v></c><c r="J1"><f>NPV(0.1,100,200)</f><v>0</v></c><c r="K1"><f>IRR(A1:A3)</f><v>0</v></c><c r="L1"><f>RATE(1,1,1)</f><v>9</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>60</v></c></row><row r="3"><c r="A3"><v>60</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>PMT(0.01,10,-1000)</f><v>105.58207655</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PMT(0,10,-1000)</f><v>100</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PMT(0.01,10,-1000,0,1)</f><v>104.53670946</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FV(0.01,10,-100,-1000)</f><v>2150.84337952</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PV(0.01,10,-100)</f><v>947.13045307</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NPER(0.01,-100,1000)</f><v>10.58864446</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NPER(0,-100,1000)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RATE(12,-100,1000)</f><v>0.02922854</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NPV(0.1,100,200)</f><v>256.19834711</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IRR(A1:A3)</f><v>0.13066239</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>RATE(1,1,1)</f><v>9</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_calendar() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>WEEKDAY(1)</f><v>0</v></c><c r="C1"><f>WEEKDAY(1,2)</f><v>0</v></c><c r="D1"><f>WEEKDAY(1,3)</f><v>0</v></c><c r="E1"><f>WEEKDAY(61,1)</f><v>0</v></c><c r="F1"><f>WEEKDAY(1,11)</f><v>9</v></c><c r="G1"><f>EDATE(43861,1)</f><v>0</v></c><c r="H1"><f>EOMONTH(43831,0)</f><v>0</v></c><c r="I1"><f>EOMONTH(43831,1)</f><v>0</v></c><c r="J1"><f>NETWORKDAYS(1,7)</f><v>0</v></c><c r="K1"><f>NETWORKDAYS(1,7,B2:B2)</f><v>0</v></c><c r="L1"><f>WORKDAY(1,5)</f><v>0</v></c><c r="M1"><f>WORKDAY(6,1)</f><v>0</v></c><c r="N1"><f>DATEDIF(43831,44256,&quot;Y&quot;)</f><v>0</v></c><c r="O1"><f>DATEDIF(43831,44256,&quot;M&quot;)</f><v>0</v></c><c r="P1"><f>DATEDIF(43831,44256,&quot;D&quot;)</f><v>0</v></c><c r="Q1"><f>DATEDIF(43831,44256,&quot;YM&quot;)</f><v>0</v></c><c r="R1"><f>DATEDIF(43831,44256,&quot;YD&quot;)</f><v>0</v></c><c r="S1"><f>DATEDIF(43831,44256,&quot;MD&quot;)</f><v>0</v></c></row><row r="2"><c r="B2"><v>2</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>WEEKDAY(1)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>WEEKDAY(1,2)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>WEEKDAY(1,3)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>WEEKDAY(61,1)</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>WEEKDAY(1,11)</f><v>9</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>EDATE(43861,1)</f><v>43890</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EOMONTH(43831,0)</f><v>43861</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EOMONTH(43831,1)</f><v>43890</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NETWORKDAYS(1,7)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NETWORKDAYS(1,7,B2:B2)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>WORKDAY(1,5)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>WORKDAY(6,1)</f><v>8</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>DATEDIF(43831,44256,&quot;Y&quot;)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATEDIF(43831,44256,&quot;M&quot;)</f><v>14</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATEDIF(43831,44256,&quot;D&quot;)</f><v>425</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATEDIF(43831,44256,&quot;YM&quot;)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATEDIF(43831,44256,&quot;YD&quot;)</f><v>59</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATEDIF(43831,44256,&quot;MD&quot;)</f><v>0</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_shared_and_other_sheet() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/><sheet name="Other" sheetId="2" r:id="rId2"/><sheet name="My Sheet" sheetId="3" r:id="rId3"/><sheet name="Bob's" sheetId="4" r:id="rId4"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Target="worksheets/sheet2.xml"/><Relationship Id="rId3" Target="worksheets/sheet3.xml"/><Relationship Id="rId4" Target="worksheets/sheet4.xml"/></Relationships>"#,
            ),
            ("xl/sharedStrings.xml", r#"<sst><si><t>Cat</t></si></sst>"#),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="M1"><f>1+1</f><v>9</v></c><c r="P1"><v>2</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="B2"><f>LEN(A1)</f><v>0</v></c><c r="C2"><f>MATCH(&quot;c*&quot;,A1:A1,0)</f><v>0</v></c><c r="D2"><f>Other!A1</f><v>0</v></c><c r="E2"><f>'My Sheet'!A1</f><v>0</v></c><c r="F2"><f>'Bob''s'!A1</f><v>0</v></c><c r="G2"><f>Other!B1</f><v>0</v></c><c r="H2"><f>Other!C1</f><v>5</v></c><c r="I2"><f>Other!A1:A2</f><v>6</v></c><c r="J2"><f>Missing!A1</f><v>3</v></c><c r="K2"><f>Budgets!M1</f><v>0</v></c><c r="L2"><f>M1</f><v>0</v></c><c r="N2"><f>IRR(A1:P1)</f><v>11</v></c><c r="O2"><f>INDEX(A1:A1,1)</f><v>0</v></c><c r="Q2"><f>AVERAGEA(A1,P1)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet2.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>7</v></c><c r="B1"><f>1+1</f><v>9</v></c><c r="C1"><f>1+1</f></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet3.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>4</v></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet4.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>LEN(A1)</f><v>3</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>MATCH(&quot;c*&quot;,A1:A1,0)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>Other!A1</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>'My Sheet'!A1</f><v>4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>'Bob''s'!A1</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Other!B1</f><v>9</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Other!C1</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Other!A1:A2</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Missing!A1</f><v>3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Budgets!M1</f><v>9</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>M1</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>IRR(A1:P1)</f><v>11</v>"#), "{sheet}");
        assert!(
            sheet.contains(
                r#"<c r="O2" t="inlineStr"><f>INDEX(A1:A1,1)</f><is><t>Cat</t></is></c>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AVERAGEA(A1,P1)</f><v>1</v>"#),
            "{sheet}"
        );
        let other = read_entry(&mut archive, "xl/worksheets/sheet2.xml").unwrap();
        assert!(other.contains(r#"<f>1+1</f><v>9</v>"#), "{other}");
        assert!(other.contains(r#"<c r="C1"><f>1+1</f></c>"#), "{other}");
    }

    #[test]
    fn set_sheet_cell_spills_a_short_result() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c><c r="G1"><v>1</v></c><c r="J1"><v>1</v></c><c r="K1"><v>1</v></c><c r="P1"><v>4</v></c><c r="S1" t="inlineStr"><is><t>Cat</t></is></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>2</v></c><c r="B2"><v>4</v></c><c r="G2"><v>2</v></c><c r="J2"><v>2</v></c><c r="K2"><v>2</v></c><c r="P2"><v>5</v></c></row><row r="3"><c r="A3"><v>3</v></c><c r="B3"><v>5</v></c><c r="G3"><v>2</v></c><c r="J3" t="inlineStr"><is><t>Cat</t></is></c><c r="K3"><v>9</v></c></row><row r="4"><c r="A4"><v>4</v></c><c r="B4"><v>1</v></c><c r="G4"><v>3</v></c><c r="J4"><v>3</v></c><c r="K4"><v>3</v></c></row><row r="5"><c r="G5"><v>3</v></c></row><row r="6"><c r="C6"><f>FREQUENCY(A1:A4,B1:B2)</f><v>9</v></c><c r="E6"><f>FREQUENCY(A1:A4,B1:B2)</f><v>9</v></c><c r="F6"><f>FREQUENCY(A1:A4,B1:B2)</f><v>9</v></c><c r="H6"><f>MODE.MULT(G1:G5)</f><v>0</v></c><c r="I6"><f>MODE.MULT(A1:A4)</f><v>6</v></c><c r="L6"><f>LINEST(J1:J4,K1:K4)</f><v>0</v></c><c r="M6"><v>9</v></c><c r="O6"><f>TREND(J1:J4,K1:K4,P1:P2)</f><v>0</v></c><c r="Q6"><f>TREND(J1:J4,K1:K4,P1:P2)</f><v>3</v></c><c r="R6"><f>FREQUENCY(S1:S1,B1:B1)</f><v>5</v></c><c r="T6"><f>FREQUENCY(A1:A4,B3:B4)</f><v>6</v></c><c r="U6"><f>FREQUENCY(A1:A4,B1:B2)+0</f><v>7</v></c><c r="X6"><f>LINEST(J1:J4,K1:K4)</f><v>4</v></c></row><row r="7"><c r="C7"><v>8</v></c><c r="E7"><v>8</v></c><c r="F7"><f>FOO()</f><v>4</v></c><c r="H7"><v>0</v></c><c r="O7"><v>8</v></c><c r="Q7"><f>FOO()</f><v>6</v></c></row><row r="8"><c r="C8"><v>7</v></c><c r="F8"><v>3</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<c r="C6"><f>FREQUENCY(A1:A4,B1:B2)</f><v>2</v></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="C7"><v>2</v></c>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="C8"><v>0</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="E6"><f>FREQUENCY(A1:A4,B1:B2)</f><v>2</v></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="E7"><v>2</v></c>"#), "{sheet}");
        assert!(!sheet.contains(r#"r="E8""#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="F6"><f>FREQUENCY(A1:A4,B1:B2)</f><v>2</v></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="F7"><f>FOO()</f><v>4</v></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="F8"><v>3</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>MODE.MULT(G1:G5)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="H7"><v>3</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>MODE.MULT(A1:A4)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="L6"><f>LINEST(J1:J4,K1:K4)</f><v>1</v></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="M6"><v>0</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="X6"><f>LINEST(J1:J4,K1:K4)</f><v>4</v></c>"#),
            "{sheet}"
        );
        assert!(!sheet.contains(r#"r="Y6""#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="O6"><f>TREND(J1:J4,K1:K4,P1:P2)</f><v>4</v></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="O7"><v>5</v></c>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="Q6"><f>TREND(J1:J4,K1:K4,P1:P2)</f><v>3</v></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="Q7"><f>FOO()</f><v>6</v></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FREQUENCY(S1:S1,B1:B1)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FREQUENCY(A1:A4,B3:B4)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FREQUENCY(A1:A4,B1:B2)+0</f><v>7</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_interest_and_dated_cash() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>-10000</v></c><c r="B2"><v>39448</v></c><c r="C2"><v>-120000</v></c></row><row r="3"><c r="A3"><v>2750</v></c><c r="B3"><v>39508</v></c><c r="C3"><v>39000</v></c></row><row r="4"><c r="A4"><v>4250</v></c><c r="B4"><v>39751</v></c><c r="C4"><v>30000</v></c></row><row r="5"><c r="A5"><v>3250</v></c><c r="B5"><v>39859</v></c><c r="C5"><v>21000</v></c></row><row r="6"><c r="A6"><v>2750</v></c><c r="B6"><v>39904</v></c><c r="C6"><v>37000</v></c></row><row r="7"><c r="C7"><v>46000</v></c></row><row r="8"><c r="A8"><f>IPMT(0.1/12,1,36,20000)</f><v>0</v></c><c r="B8"><f>PPMT(0.1/12,1,36,20000)</f><v>0</v></c><c r="C8"><f>IPMT(0.1,1,3,8000,0,1)</f><v>0</v></c><c r="D8"><f>CUMIPMT(0.09/12,360,125000,13,24,0)</f><v>0</v></c><c r="E8"><f>CUMPRINC(0.09/12,360,125000,13,24,0)</f><v>0</v></c><c r="F8"><f>IPMT(0.1,0,3,8000)</f><v>9</v></c><c r="G8"><f>XNPV(0.09,A2:A6,B2:B6)</f><v>0</v></c><c r="H8"><f>XIRR(A2:A6,B2:B6)</f><v>0</v></c><c r="I8"><f>MIRR(C2:C7,0.1,0.12)</f><v>0</v></c><c r="J8"><f>XIRR(A2:A2,B2:B2)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>IPMT(0.1/12,1,36,20000)</f><v>-166.66666667</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PPMT(0.1/12,1,36,20000)</f><v>-478.67707721</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IPMT(0.1,1,3,8000,0,1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CUMIPMT(0.09/12,360,125000,13,24,0)</f><v>-11135.23213075</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CUMPRINC(0.09/12,360,125000,13,24,0)</f><v>-934.10712342</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IPMT(0.1,0,3,8000)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XNPV(0.09,A2:A6,B2:B6)</f><v>2086.64760203</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XIRR(A2:A6,B2:B6)</f><v>0.37336253</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MIRR(C2:C7,0.1,0.12)</f><v>0.12609413</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>XIRR(A2:A2,B2:B2)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_text_format() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Cat</t></is></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><f>TEXT(1234.5,&quot;0.00&quot;)</f><v>0</v></c><c r="B2"><f>TEXT(1234.5,&quot;#,##0.00&quot;)</f><v>0</v></c><c r="C2"><f>TEXT(0.25,&quot;0%&quot;)</f><v>0</v></c><c r="D2"><f>TEXT(1234.5,&quot;0.##&quot;)</f><v>0</v></c><c r="E2"><f>TEXT(43831,&quot;yyyy-mm-dd&quot;)</f><v>0</v></c><c r="F2"><f>TEXT(43831,&quot;d/m/yy&quot;)</f><v>0</v></c><c r="G2"><f>TEXT(A1,&quot;@&quot;)</f><v>0</v></c><c r="H2"><f>TEXT(1,&quot;0.00E+00&quot;)</f><v>9</v></c><c r="I2"><f>TEXT(-0.004,&quot;0.00&quot;)</f><v>0</v></c><c r="J2"><f>TEXT(12,&quot;&quot;&quot;id &quot;&quot;0&quot;)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>TEXT(1234.5,&quot;0.00&quot;)</f><is><t>1234.50</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1234.5,&quot;#,##0.00&quot;)</f><is><t>1,234.50</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(0.25,&quot;0%&quot;)</f><is><t>25%</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1234.5,&quot;0.##&quot;)</f><is><t>1234.5</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet
                .contains(r#"<f>TEXT(43831,&quot;yyyy-mm-dd&quot;)</f><is><t>2020-01-01</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(43831,&quot;d/m/yy&quot;)</f><is><t>1/1/20</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(A1,&quot;@&quot;)</f><is><t>Cat</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(1,&quot;0.00E+00&quot;)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(-0.004,&quot;0.00&quot;)</f><is><t>-0.00</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<f>TEXT(12,&quot;&quot;&quot;id &quot;&quot;0&quot;)</f><is><t>id 12</t></is>"#
            ),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_convert_units() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="Z1"><v>0</v></c><c r="A1"><f>CONVERT(1,&quot;ft&quot;,&quot;in&quot;)</f><v>0</v></c><c r="B1"><f>CONVERT(1,&quot;kg&quot;,&quot;g&quot;)</f><v>0</v></c><c r="C1"><f>CONVERT(1,&quot;hr&quot;,&quot;mn&quot;)</f><v>0</v></c><c r="D1"><f>CONVERT(0,&quot;C&quot;,&quot;F&quot;)</f><v>0</v></c><c r="E1"><f>CONVERT(100,&quot;C&quot;,&quot;K&quot;)</f><v>0</v></c><c r="F1"><f>CONVERT(1,&quot;yr&quot;,&quot;day&quot;)</f><v>0</v></c><c r="G1"><f>CONVERT(1,&quot;mi&quot;,&quot;km&quot;)</f><v>0</v></c><c r="H1"><f>CONVERT(1,&quot;m&quot;,&quot;kg&quot;)</f><v>4</v></c><c r="I1"><f>CONVERT(-300,&quot;C&quot;,&quot;K&quot;)</f><v>5</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>CONVERT(1,&quot;ft&quot;,&quot;in&quot;)</f><v>12</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONVERT(1,&quot;kg&quot;,&quot;g&quot;)</f><v>1000</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONVERT(1,&quot;hr&quot;,&quot;mn&quot;)</f><v>60</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONVERT(0,&quot;C&quot;,&quot;F&quot;)</f><v>32</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONVERT(100,&quot;C&quot;,&quot;K&quot;)</f><v>373.15</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONVERT(1,&quot;yr&quot;,&quot;day&quot;)</f><v>365.25</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONVERT(1,&quot;mi&quot;,&quot;km&quot;)</f><v>1.609344</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONVERT(1,&quot;m&quot;,&quot;kg&quot;)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONVERT(-300,&quot;C&quot;,&quot;K&quot;)</f><v>5</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_date1904() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><workbookPr date1904="1"/><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="Z1"><v>0</v></c><c r="A1"><f>DATE(1904,1,1)</f><v>9</v></c><c r="B1"><f>YEAR(0)</f><v>0</v></c><c r="C1"><f>MONTH(0)</f><v>0</v></c><c r="D1"><f>DAY(0)</f><v>0</v></c><c r="E1"><f>WEEKDAY(0)</f><v>0</v></c><c r="F1"><f>WEEKDAY(0,2)</f><v>0</v></c><c r="G1"><f>EDATE(0,1)</f><v>0</v></c><c r="H1"><f>NETWORKDAYS(0,6)</f><v>0</v></c><c r="I1"><f>DATE(1900,1,1)</f><v>8</v></c><c r="J1"><f>TEXT(0,&quot;yyyy-mm-dd&quot;)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>DATE(1904,1,1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>YEAR(0)</f><v>1904</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MONTH(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DAY(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>WEEKDAY(0)</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>WEEKDAY(0,2)</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>EDATE(0,1)</f><v>31</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>NETWORKDAYS(0,6)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DATE(1900,1,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXT(0,&quot;yyyy-mm-dd&quot;)</f><is><t>1904-01-01</t></is>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_linest_stats() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>2</v></c><c r="B1"><v>1</v></c><c r="C1"><f>LINEST(A1:A4,B1:B4,1,1)</f><v>0</v></c><c r="D1"><v>0</v></c><c r="E1"><f>LINEST(A1:A4,B1:B4,0)</f><v>7</v></c><c r="H1"><f>LINEST(A1:A4,B1:B4,1,0)</f><v>0</v></c><c r="I1"><v>0</v></c><c r="K1"><f>LINEST(A1:A2,B1:B2,1,1)</f><v>4</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="A2"><v>3</v></c><c r="B2"><v>2</v></c><c r="C2"><v>0</v></c><c r="D2"><v>0</v></c><c r="H2"><v>9</v></c></row><row r="3"><c r="A3"><v>5</v></c><c r="B3"><v>3</v></c><c r="C3"><v>0</v></c><c r="D3"><v>0</v></c></row><row r="4"><c r="A4"><v>4</v></c><c r="B4"><v>4</v></c><c r="C4"><v>0</v></c><c r="D4"><v>0</v></c></row><row r="5"><c r="C5"><v>0</v></c><c r="D5"><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<c r="C1"><f>LINEST(A1:A4,B1:B4,1,1)</f><v>0.8</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="D1"><v>1.5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="C2"><v>0.42426407</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="D2"><v>1.161895</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="C3"><v>0.64</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="D3"><v>0.9486833</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="C4"><v>3.55555556</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="D4"><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="C5"><v>3.2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="D5"><v>1.8</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>LINEST(A1:A4,B1:B4,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LINEST(A1:A4,B1:B4,1,0)</f><v>0.8</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<c r="I1"><v>1.5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<c r="H2"><v>9</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>LINEST(A1:A2,B1:B2,1,1)</f><v>4</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_defined_names() {
        let bytes = zip_bytes(&[
            (
                "xl/workbook.xml",
                r#"<workbook><sheets><sheet name="Budgets" sheetId="1" r:id="rId1"/><sheet name="Other" sheetId="2" r:id="rId2"/><sheet name="My Sheet" sheetId="3" r:id="rId3"/></sheets><definedNames><definedName name="Sales">Budgets!$A$1:$A$2</definedName><definedName name="Rate">Budgets!$B$1</definedName><definedName name="Live">Budgets!$C$1</definedName><definedName name="Abroad">Other!$A$1:$A$2</definedName><definedName name="OtherTotal">Other!$A$1</definedName><definedName name="Quoted">'My Sheet'!$A$1</definedName><definedName name="Local" localSheetId="0">Budgets!$B$1</definedName><definedName name="Plus">Budgets!$A$1+1</definedName></definedNames></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Target="worksheets/sheet2.xml"/><Relationship Id="rId3" Target="worksheets/sheet3.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>10</v></c><c r="A2"><v>20</v></c><c r="B1"><v>3</v></c><c r="C1"><f>1+1</f><v>8</v></c><c r="Z1"><v>0</v></c></row><row r="2"><c r="D2"><f>SUM(sales)</f><v>0</v></c><c r="E2"><f>Rate*2</f><v>0</v></c><c r="F2"><f>Live</f><v>0</v></c><c r="G2"><f>OtherTotal</f><v>0</v></c><c r="H2"><f>SUM(Abroad)</f><v>0</v></c><c r="I2"><f>Quoted</f><v>0</v></c><c r="J2"><f>Local</f><v>5</v></c><c r="K2"><f>Plus</f><v>4</v></c><c r="L2"><f>SUMIF(Sales,&quot;&gt;15&quot;)</f><v>0</v></c><c r="M2"><f>COUNTA(Sales)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet2.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><f>1+1</f><v>9</v></c></row><row r="2"><c r="A2"><v>4</v></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet3.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>4</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "Z1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>SUM(sales)</f><v>30</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Rate*2</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Live</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>OtherTotal</f><v>9</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SUM(Abroad)</f><v>13</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Quoted</f><v>4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Local</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>Plus</f><v>4</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SUMIF(Sales,&quot;&gt;15&quot;)</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>COUNTA(Sales)</f><v>2</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_takes_a_median() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1"><f>MEDIAN(1,9,3)</f><v>0</v></c><c r="D1"><f>MEDIAN(1,2,3,4)</f><v>0</v></c><c r="E1"><f>MEDIAN(A1:B1)</f><v>0</v></c><c r="F1"><f>MEDIAN()</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>MEDIAN(1,9,3)</f><v>3</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>MEDIAN(1,2,3,4)</f><v>2.5</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>MEDIAN(A1:B1)</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MEDIAN()</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_classifies_number_and_text() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>ISNUMBER(4)</f><v>0</v></c><c r="C1"><f>ISNUMBER("ab")</f><v>0</v></c><c r="D1"><f>ISTEXT("ab")</f><v>0</v></c><c r="E1"><f>ISTEXT(4)</f><v>0</v></c><c r="F1"><f>ISNUMBER(Z9)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>ISNUMBER(4)</f><v>1</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>ISNUMBER("ab")</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ISTEXT("ab")</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ISTEXT(4)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ISNUMBER(Z9)</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_gcd_and_lcm() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>GCD(12,18)</f><v>0</v></c><c r="C1"><f>GCD(12.9,18)</f><v>0</v></c><c r="D1"><f>LCM(4,6)</f><v>0</v></c><c r="E1"><f>LCM(0,5)</f><v>0</v></c><c r="F1"><f>GCD(-2,4)</f><v>7</v></c><c r="G1"><f>GCD()</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>GCD(12,18)</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GCD(12.9,18)</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>LCM(4,6)</f><v>12</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>LCM(0,5)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GCD(-2,4)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GCD()</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_logs_and_exp() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>LN(1)</f><v>0</v></c><c r="C1"><f>LOG10(100)</f><v>0</v></c><c r="D1"><f>LOG(100)</f><v>0</v></c><c r="E1"><f>LOG(8,2)</f><v>0</v></c><c r="F1"><f>EXP(0)</f><v>0</v></c><c r="G1"><f>EXP(1)</f><v>0</v></c><c r="H1"><f>LN(-1)</f><v>7</v></c><c r="I1"><f>LOG(8,1)</f><v>8</v></c><c r="J1"><f>EXP(1000)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>LN(1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>LOG10(100)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>LOG(100)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>LOG(8,2)</f><v>3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>EXP(0)</f><v>1</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>EXP(1)</f><v>2.71828183</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>LN(-1)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>LOG(8,1)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>EXP(1000)</f><v>9</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_factorial() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>FACT(5)</f><v>0</v></c><c r="C1"><f>FACT(0)</f><v>0</v></c><c r="D1"><f>FACT(5.9)</f><v>0</v></c><c r="E1"><f>FACT(-1)</f><v>7</v></c><c r="F1"><f>FACT(171)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>FACT(5)</f><v>120</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>FACT(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>FACT(5.9)</f><v>120</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>FACT(-1)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>FACT(171)</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_trig_in_radians() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>SIN(0)</f><v>0</v></c><c r="C1"><f>COS(0)</f><v>0</v></c><c r="D1"><f>TAN(0)</f><v>0</v></c><c r="E1"><f>COS(PI())</f><v>0</v></c><c r="F1"><f>SIN(RADIANS(90))</f><v>0</v></c><c r="G1"><f>DEGREES(PI())</f><v>0</v></c><c r="H1"><f>RADIANS(180)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>SIN(0)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COS(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>TAN(0)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COS(PI())</f><v>-1</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SIN(RADIANS(90))</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DEGREES(PI())</f><v>180</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RADIANS(180)</f><v>3.14159265</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_inverse_trig() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>ASIN(0)</f><v>0</v></c><c r="C1"><f>ACOS(0)</f><v>0</v></c><c r="D1"><f>ATAN(0)</f><v>0</v></c><c r="E1"><f>ATAN(1)</f><v>0</v></c><c r="F1"><f>ATAN2(1,0)</f><v>0</v></c><c r="G1"><f>ATAN2(0,1)</f><v>0</v></c><c r="H1"><f>ASIN(2)</f><v>7</v></c><c r="I1"><f>ATAN2(0,0)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>ASIN(0)</f><v>0</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>ACOS(0)</f><v>1.57079633</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ATAN(0)</f><v>0</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>ATAN(1)</f><v>0.78539816</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ATAN2(1,0)</f><v>0</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>ATAN2(0,1)</f><v>1.57079633</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ASIN(2)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ATAN2(0,0)</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_hyperbolic() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>SINH(0)</f><v>0</v></c><c r="C1"><f>COSH(0)</f><v>0</v></c><c r="D1"><f>TANH(0)</f><v>0</v></c><c r="E1"><f>SINH(1)</f><v>0</v></c><c r="F1"><f>COSH(1000)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>SINH(0)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COSH(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>TANH(0)</f><v>0</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SINH(1)</f><v>1.17520119</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>COSH(1000)</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_combin() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>COMBIN(5,2)</f><v>0</v></c><c r="C1"><f>COMBIN(5,0)</f><v>0</v></c><c r="D1"><f>COMBIN(5,5)</f><v>0</v></c><c r="E1"><f>COMBIN(5.9,2.2)</f><v>0</v></c><c r="F1"><f>COMBIN(4,5)</f><v>7</v></c><c r="G1"><f>COMBIN(-1,1)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>COMBIN(5,2)</f><v>10</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COMBIN(5,0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COMBIN(5,5)</f><v>1</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>COMBIN(5.9,2.2)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>COMBIN(4,5)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COMBIN(-1,1)</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_permutations() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>PERMUT(5,2)</f><v>0</v></c><c r="C1"><f>PERMUT(5,0)</f><v>0</v></c><c r="D1"><f>PERMUT(5,5)</f><v>0</v></c><c r="E1"><f>PERMUT(4,5)</f><v>7</v></c><c r="F1"><f>PERMUTATIONA(3,2)</f><v>0</v></c><c r="G1"><f>PERMUTATIONA(0,0)</f><v>0</v></c><c r="H1"><f>PERMUTATIONA(0,2)</f><v>0</v></c><c r="I1"><f>PERMUT(-1,1)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>PERMUT(5,2)</f><v>20</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>PERMUT(5,0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>PERMUT(5,5)</f><v>120</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>PERMUT(4,5)</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>PERMUTATIONA(3,2)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERMUTATIONA(0,0)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERMUTATIONA(0,2)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>PERMUT(-1,1)</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_large_and_small() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1"><f>LARGE(1,9,3,1)</f><v>0</v></c><c r="D1"><f>LARGE(1,9,3,2)</f><v>0</v></c><c r="E1"><f>SMALL(1,9,3,1)</f><v>0</v></c><c r="F1"><f>SMALL(1,9,3,2.9)</f><v>0</v></c><c r="G1"><f>LARGE(A1:B1,1)</f><v>0</v></c><c r="H1"><f>LARGE(1,9,0)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>LARGE(1,9,3,1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LARGE(1,9,3,2)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SMALL(1,9,3,1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SMALL(1,9,3,2.9)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LARGE(A1:B1,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>LARGE(1,9,0)</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_truncates() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>TRUNC(1.239)</f><v>0</v></c><c r="C1"><f>TRUNC(1.239,2)</f><v>0</v></c><c r="D1"><f>TRUNC(-1.239,2)</f><v>0</v></c><c r="E1"><f>TRUNC(128,-1)</f><v>0</v></c><c r="F1"><f>TRUNC(1.2,20)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>TRUNC(1.239)</f><v>1</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>TRUNC(1.239,2)</f><v>1.23</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TRUNC(-1.239,2)</f><v>-1.23</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TRUNC(128,-1)</f><v>120</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>TRUNC(1.2,20)</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_even_and_odd() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>ISEVEN(2.9)</f><v>0</v></c><c r="C1"><f>ISODD(2.9)</f><v>0</v></c><c r="D1"><f>ISEVEN(0)</f><v>0</v></c><c r="E1"><f>ISODD(-3)</f><v>0</v></c><c r="F1"><f>ISEVEN(-3.2)</f><v>0</v></c><c r="G1"><f>ISEVEN(1000000000000000)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>ISEVEN(2.9)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ISODD(2.9)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ISEVEN(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ISODD(-3)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ISEVEN(-3.2)</f><v>0</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>ISEVEN(1000000000000000)</f><v>7</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_code_and_char() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>CODE("A")</f><v>0</v></c><c r="C1"><f>CODE("Ab")</f><v>0</v></c><c r="D1"><f>CODE("")</f><v>7</v></c><c r="E1"><f>CHAR(65)</f><v>0</v></c><c r="F1"><f>CHAR(65.9)</f><v>0</v></c><c r="G1"><f>CHAR(8364)</f><v>0</v></c><c r="H1"><f>CHAR(0)</f><v>8</v></c><c r="I1"><f>CODE(A1)</f><v>0</v></c><c r="J1"><f>CHAR(55296)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>CODE("A")</f><v>65</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>CODE("Ab")</f><v>65</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>CODE("")</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>CHAR(65)</f><is><t>A</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHAR(65.9)</f><is><t>A</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains("<f>CHAR(8364)</f><is><t>\u{20AC}</t></is>"),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>CHAR(0)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>CODE(A1)</f><v>50</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>CHAR(55296)</f><v>9</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_counts_text_and_blanks() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="C1" t="inlineStr"><is><t>ab</t></is></c><c r="D1"><f>COUNTA(A1:C1)</f><v>0</v></c><c r="E1"><f>COUNTBLANK(A1:C1)</f><v>0</v></c><c r="F1"><f>COUNTA(1,"ab","")</f><v>0</v></c><c r="G1"><f>COUNTBLANK("")</f><v>0</v></c><c r="H1"><f>COUNTBLANK(4)</f><v>0</v></c><c r="I1"><f>COUNTA()</f><v>0</v></c><c r="J1"><f>COUNTA(Z9)</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>COUNTA(A1:C1)</f><v>2</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>COUNTBLANK(A1:C1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTA(1,"ab","")</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTBLANK("")</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>COUNTBLANK(4)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COUNTA()</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COUNTA(Z9)</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_iferror() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>IFERROR(SQRT(4),0)</f><v>0</v></c><c r="C1"><f>IFERROR(SQRT(-1),9)</f><v>0</v></c><c r="D1"><f>IFERROR(1/0,5)</f><v>0</v></c><c r="E1"><f>IFERROR(SQRT(-1),"no")</f><v>0</v></c><c r="F1"><f>IFERROR(SQRT(-1),SQRT(-1))</f><v>7</v></c><c r="G1"><f>IFERROR(1,2,3)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>IFERROR(SQRT(4),0)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IFERROR(SQRT(-1),9)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IFERROR(1/0,5)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IFERROR(SQRT(-1),"no")</f><is><t>no</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IFERROR(SQRT(-1),SQRT(-1))</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IFERROR(1,2,3)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_clean_and_proper() {
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
                "xl/worksheets/sheet1.xml",
                "<worksheet><sheetData><row r=\"1\"><c r=\"A1\"><v>1</v></c><c r=\"B1\"><f>CLEAN(CHAR(10)&amp;\"ab\")</f><v>0</v></c><c r=\"C1\"><f>CLEAN(\"ab\")</f><v>0</v></c><c r=\"D1\"><f>PROPER(\"ab cd\")</f><v>0</v></c><c r=\"E1\"><f>PROPER(\"a1b\")</f><v>0</v></c><c r=\"F1\"><f>PROPER(\"\u{00C9}RIC\")</f><v>0</v></c></row></sheetData></worksheet>",
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>CLEAN(CHAR(10)&amp;"ab")</f><is><t>ab</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CLEAN("ab")</f><is><t>ab</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PROPER("ab cd")</f><is><t>Ab Cd</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PROPER("a1b")</f><is><t>A1B</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains("<f>PROPER(\"\u{00C9}RIC\")</f><is><t>\u{00C9}ric</t></is>"),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_choose() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>CHOOSE(2,10,20,30)</f><v>0</v></c><c r="C1"><f>CHOOSE(2.9,"a","b")</f><v>0</v></c><c r="D1"><f>CHOOSE(1,SQRT(-1),5)</f><v>7</v></c><c r="E1"><f>CHOOSE(0,1,2)</f><v>8</v></c><c r="F1"><f>CHOOSE(3,1,2)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>CHOOSE(2,10,20,30)</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHOOSE(2.9,"a","b")</f><is><t>b</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHOOSE(1,SQRT(-1),5)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>CHOOSE(0,1,2)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>CHOOSE(3,1,2)</f><v>9</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_switch() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>SWITCH(2,1,10,2,20)</f><v>0</v></c><c r="C1"><f>SWITCH(2,1,10,9)</f><v>0</v></c><c r="D1"><f>SWITCH(3,1,10)</f><v>7</v></c><c r="E1"><f>SWITCH("b","a",1,"b",2)</f><v>0</v></c><c r="F1"><f>SWITCH(1,1,SQRT(-1))</f><v>8</v></c><c r="G1"><f>SWITCH(1,"1",9)</f><v>6</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SWITCH(2,1,10,2,20)</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SWITCH(2,1,10,9)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SWITCH(3,1,10)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SWITCH("b","a",1,"b",2)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SWITCH(1,1,SQRT(-1))</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SWITCH(1,"1",9)</f><v>6</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_xor() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>XOR(1,0)</f><v>0</v></c><c r="C1"><f>XOR(1,0,1)</f><v>0</v></c><c r="D1"><f>XOR(0,0)</f><v>0</v></c><c r="E1"><f>XOR(1,1,1)</f><v>0</v></c><c r="F1"><f>XOR()</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>XOR(1,0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>XOR(1,0,1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>XOR(0,0)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>XOR(1,1,1)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>XOR()</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_text_join() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="C1" t="inlineStr"><is><t>ab</t></is></c><c r="D1"><f>TEXTJOIN(",",1,"a","","b")</f><v>0</v></c><c r="E1"><f>TEXTJOIN(",",0,"a","","b")</f><v>0</v></c><c r="F1"><f>TEXTJOIN(",",1,A1:C1)</f><v>0</v></c><c r="G1"><f>TEXTJOIN(",",0,A1:C1)</f><v>0</v></c><c r="H1"><f>TEXTJOIN(",",1)</f><v>0</v></c><c r="I1"><f>TEXTJOIN("x",0,REPT("a",32767),"b")</f><v>7</v></c><c r="J1"><f>TEXTJOIN(",",1,Z9)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>TEXTJOIN(",",1,"a","","b")</f><is><t>a,b</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXTJOIN(",",0,"a","","b")</f><is><t>a,,b</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXTJOIN(",",1,A1:C1)</f><is><t>2,ab</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXTJOIN(",",0,A1:C1)</f><is><t>2,,ab</t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXTJOIN(",",1)</f><is><t></t></is>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXTJOIN("x",0,REPT("a",32767),"b")</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TEXTJOIN(",",1,Z9)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_ifs() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>IFS(0,1,1,9)</f><v>0</v></c><c r="C1"><f>IFS(0,1)</f><v>7</v></c><c r="D1"><f>IFS(0,SQRT(-1),1,5)</f><v>0</v></c><c r="E1"><f>IFS(1,SQRT(-1),1,5)</f><v>8</v></c><c r="F1"><f>IFS("a",1)</f><v>6</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>IFS(0,1,1,9)</f><v>9</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>IFS(0,1)</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>IFS(0,SQRT(-1),1,5)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>IFS(1,SQRT(-1),1,5)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>IFS("a",1)</f><v>6</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_bitwise() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>BITAND(13,25)</f><v>0</v></c><c r="C1"><f>BITOR(13,25)</f><v>0</v></c><c r="D1"><f>BITXOR(13,25)</f><v>0</v></c><c r="E1"><f>BITAND(1.9,1)</f><v>0</v></c><c r="F1"><f>BITAND(-1,1)</f><v>7</v></c><c r="G1"><f>BITAND(281474976710656,1)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>BITAND(13,25)</f><v>9</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>BITOR(13,25)</f><v>29</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>BITXOR(13,25)</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>BITAND(1.9,1)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>BITAND(-1,1)</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>BITAND(281474976710656,1)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_ceiling_and_floor() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>CEILING(2.5,1)</f><v>0</v></c><c r="C1"><f>CEILING(-2.5,-1)</f><v>0</v></c><c r="D1"><f>CEILING(-2.5,1)</f><v>7</v></c><c r="E1"><f>CEILING(4,0)</f><v>0</v></c><c r="F1"><f>FLOOR(2.5,1)</f><v>0</v></c><c r="G1"><f>FLOOR(-2.5,-1)</f><v>0</v></c><c r="H1"><f>FLOOR(4,0)</f><v>8</v></c><c r="I1"><f>CEILING(2.5)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>CEILING(2.5,1)</f><v>3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CEILING(-2.5,-1)</f><v>-3</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CEILING(-2.5,1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>CEILING(4,0)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>FLOOR(2.5,1)</f><v>2</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>FLOOR(-2.5,-1)</f><v>-2</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>FLOOR(4,0)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>CEILING(2.5)</f><v>9</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_bit_shift() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>BITLSHIFT(5,2)</f><v>0</v></c><c r="C1"><f>BITRSHIFT(20,2)</f><v>0</v></c><c r="D1"><f>BITLSHIFT(5,-1)</f><v>0</v></c><c r="E1"><f>BITRSHIFT(5,-1)</f><v>0</v></c><c r="F1"><f>BITLSHIFT(1,48)</f><v>7</v></c><c r="G1"><f>BITLSHIFT(1,54)</f><v>8</v></c><c r="H1"><f>BITLSHIFT(-1,1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>BITLSHIFT(5,2)</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BITRSHIFT(20,2)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BITLSHIFT(5,-1)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BITRSHIFT(5,-1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BITLSHIFT(1,48)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BITLSHIFT(1,54)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BITLSHIFT(-1,1)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_mround() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>MROUND(10,3)</f><v>0</v></c><c r="C1"><f>MROUND(10,4)</f><v>0</v></c><c r="D1"><f>MROUND(-10,-3)</f><v>0</v></c><c r="E1"><f>MROUND(-10,3)</f><v>7</v></c><c r="F1"><f>MROUND(6,0)</f><v>8</v></c><c r="G1"><f>MROUND(0,0)</f><v>0</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>MROUND(10,3)</f><v>9</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MROUND(10,4)</f><v>12</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>MROUND(-10,-3)</f><v>-9</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>MROUND(-10,3)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MROUND(6,0)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MROUND(0,0)</f><v>0</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_sumif() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1"><v>3</v></c><c r="D1"><f>SUMIF(A1:C1,"&gt;2")</f><v>0</v></c><c r="E1"><f>SUMIF(A1:C1,8)</f><v>0</v></c><c r="F1"><f>SUMIF(A1:C1,"&lt;&gt;8")</f><v>0</v></c><c r="G1"><f>SUMIF(A1:C1,"&lt;0")</f><v>0</v></c><c r="H1"><f>SUMIF(A1:C1,"ab")</f><v>7</v></c><c r="I1"><f>SUMIF(A1:C1,"&gt;2",A1)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SUMIF(A1:C1,"&gt;2")</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIF(A1:C1,8)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIF(A1:C1,"&lt;&gt;8")</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIF(A1:C1,"&lt;0")</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIF(A1:C1,"ab")</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIF(A1:C1,"&gt;2",A1)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_countif() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1"><v>3</v></c><c r="D1"><f>COUNTIF(A1:C1,"&gt;2")</f><v>0</v></c><c r="E1"><f>COUNTIF(A1:C1,8)</f><v>0</v></c><c r="F1"><f>COUNTIF(A1:C1,"&lt;&gt;8")</f><v>0</v></c><c r="G1"><f>COUNTIF(A1:C1,"&lt;0")</f><v>0</v></c><c r="H1"><f>COUNTIF(A1:C1,"ab")</f><v>7</v></c><c r="I1"><f>COUNTIF(A1:C1,"&gt;2",A1)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>COUNTIF(A1:C1,"&gt;2")</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIF(A1:C1,8)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIF(A1:C1,"&lt;&gt;8")</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIF(A1:C1,"&lt;0")</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIF(A1:C1,"ab")</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIF(A1:C1,"&gt;2",A1)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_countifs() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1"><v>3</v></c><c r="D1"><f>COUNTIFS(A1:C1,"&gt;2")</f><v>0</v></c><c r="E1"><f>COUNTIFS(A1:C1,8)</f><v>0</v></c><c r="F1"><f>COUNTIFS(A1:C1,"&lt;&gt;8")</f><v>0</v></c><c r="G1"><f>COUNTIFS(A1:C1,"&lt;0")</f><v>0</v></c><c r="H1"><f>COUNTIFS(A1:C1,"ab")</f><v>7</v></c><c r="I1"><f>COUNTIFS(A1:C1,"&gt;2",B1:B1,1)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>COUNTIFS(A1:C1,"&gt;2")</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIFS(A1:C1,8)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIFS(A1:C1,"&lt;&gt;8")</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIFS(A1:C1,"&lt;0")</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIFS(A1:C1,"ab")</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COUNTIFS(A1:C1,"&gt;2",B1:B1,1)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_slope() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>4</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><v>1</v></c><c r="E1"><v>2</v></c><c r="F1"><v>3</v></c><c r="I1"><v>5</v></c><c r="J1"><v>5</v></c><c r="G1"><f>SLOPE(A1:C1,D1:F1)</f><v>0</v></c><c r="H1"><f>SLOPE(A1:B1,D1:F1)</f><v>7</v></c><c r="K1"><f>SLOPE(A1:B1,I1:J1)</f><v>8</v></c><c r="L1"><f>SLOPE(A1:A1,D1:D1)</f><v>9</v></c><c r="M1"><f>SLOPE(1,2)</f><v>10</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SLOPE(A1:C1,D1:F1)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SLOPE(A1:B1,D1:F1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SLOPE(A1:B1,I1:J1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SLOPE(A1:A1,D1:D1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SLOPE(1,2)</f><v>10</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_intercept() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>4</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><v>1</v></c><c r="E1"><v>2</v></c><c r="F1"><v>3</v></c><c r="I1"><v>5</v></c><c r="J1"><v>5</v></c><c r="G1"><f>INTERCEPT(A1:C1,D1:F1)</f><v>0</v></c><c r="H1"><f>INTERCEPT(A1:B1,D1:F1)</f><v>7</v></c><c r="K1"><f>INTERCEPT(A1:B1,I1:J1)</f><v>8</v></c><c r="L1"><f>INTERCEPT(A1:A1,D1:D1)</f><v>9</v></c><c r="M1"><f>INTERCEPT(1,2)</f><v>10</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>INTERCEPT(A1:C1,D1:F1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INTERCEPT(A1:B1,D1:F1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INTERCEPT(A1:B1,I1:J1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INTERCEPT(A1:A1,D1:D1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>INTERCEPT(1,2)</f><v>10</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_correl() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>4</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><v>1</v></c><c r="E1"><v>2</v></c><c r="F1"><v>3</v></c><c r="I1"><v>5</v></c><c r="J1"><v>5</v></c><c r="N1"><v>3</v></c><c r="O1"><v>3</v></c><c r="P1"><v>1</v></c><c r="Q1"><v>2</v></c><c r="G1"><f>CORREL(A1:C1,D1:F1)</f><v>0</v></c><c r="H1"><f>PEARSON(A1:C1,D1:F1)</f><v>0</v></c><c r="K1"><f>CORREL(A1:B1,D1:F1)</f><v>7</v></c><c r="L1"><f>CORREL(A1:B1,I1:J1)</f><v>8</v></c><c r="M1"><f>CORREL(N1:O1,P1:Q1)</f><v>11</v></c><c r="R1"><f>CORREL(A1:A1,D1:D1)</f><v>9</v></c><c r="S1"><f>CORREL(1,2)</f><v>10</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>CORREL(A1:C1,D1:F1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PEARSON(A1:C1,D1:F1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CORREL(A1:B1,D1:F1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CORREL(A1:B1,I1:J1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CORREL(N1:O1,P1:Q1)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CORREL(A1:A1,D1:D1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>CORREL(1,2)</f><v>10</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_rsq() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>4</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><v>1</v></c><c r="E1"><v>2</v></c><c r="F1"><v>3</v></c><c r="I1"><v>5</v></c><c r="J1"><v>5</v></c><c r="N1"><v>4</v></c><c r="O1"><v>2</v></c><c r="P1"><v>1</v></c><c r="Q1"><v>2</v></c><c r="G1"><f>RSQ(A1:C1,D1:F1)</f><v>0</v></c><c r="H1"><f>RSQ(N1:O1,P1:Q1)</f><v>0</v></c><c r="K1"><f>RSQ(A1:B1,D1:F1)</f><v>7</v></c><c r="L1"><f>RSQ(A1:B1,I1:J1)</f><v>8</v></c><c r="R1"><f>RSQ(A1:A1,D1:D1)</f><v>9</v></c><c r="S1"><f>RSQ(1,2)</f><v>10</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>RSQ(A1:C1,D1:F1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RSQ(N1:O1,P1:Q1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RSQ(A1:B1,D1:F1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RSQ(A1:B1,I1:J1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RSQ(A1:A1,D1:D1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>RSQ(1,2)</f><v>10</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_forecast() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>4</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><v>1</v></c><c r="E1"><v>2</v></c><c r="F1"><v>3</v></c><c r="I1"><v>5</v></c><c r="J1"><v>5</v></c><c r="G1"><f>FORECAST(4,A1:C1,D1:F1)</f><v>0</v></c><c r="H1"><f>FORECAST.LINEAR(0,A1:C1,D1:F1)</f><v>0</v></c><c r="K1"><f>FORECAST(4,A1:B1,D1:F1)</f><v>7</v></c><c r="L1"><f>FORECAST(4,A1:B1,I1:J1)</f><v>8</v></c><c r="M1"><f>FORECAST(4,A1:A1,D1:D1)</f><v>9</v></c><c r="N1"><f>FORECAST("ab",A1:C1,D1:F1)</f><v>10</v></c><c r="O1"><f>FORECAST(4,1,2)</f><v>11</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>FORECAST(4,A1:C1,D1:F1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FORECAST.LINEAR(0,A1:C1,D1:F1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FORECAST(4,A1:B1,D1:F1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FORECAST(4,A1:B1,I1:J1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FORECAST(4,A1:A1,D1:D1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FORECAST("ab",A1:C1,D1:F1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FORECAST(4,1,2)</f><v>11</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_steyx() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>2</v></c><c r="C1"><v>4</v></c><c r="D1"><v>1</v></c><c r="E1"><v>2</v></c><c r="F1"><v>3</v></c><c r="I1"><v>5</v></c><c r="J1"><v>5</v></c><c r="K1"><v>5</v></c><c r="N1"><v>2</v></c><c r="O1"><v>4</v></c><c r="P1"><v>6</v></c><c r="Q1"><v>1</v></c><c r="R1"><v>2</v></c><c r="S1"><v>3</v></c><c r="G1"><f>STEYX(A1:C1,D1:F1)</f><v>0</v></c><c r="H1"><f>STEYX(N1:P1,Q1:S1)</f><v>0</v></c><c r="L1"><f>STEYX(A1:B1,D1:E1)</f><v>7</v></c><c r="M1"><f>STEYX(A1:B1,D1:F1)</f><v>8</v></c><c r="T1"><f>STEYX(A1:C1,I1:K1)</f><v>9</v></c><c r="U1"><f>STEYX(1,2)</f><v>10</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>STEYX(A1:C1,D1:F1)</f><v>0.40824829</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STEYX(N1:P1,Q1:S1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STEYX(A1:B1,D1:E1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STEYX(A1:B1,D1:F1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STEYX(A1:C1,I1:K1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>STEYX(1,2)</f><v>10</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_covariance() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>4</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><v>1</v></c><c r="E1"><v>2</v></c><c r="F1"><v>3</v></c><c r="I1"><v>5</v></c><c r="J1"><v>5</v></c><c r="G1"><f>COVARIANCE.P(A1:C1,D1:F1)</f><v>0</v></c><c r="H1"><f>COVARIANCE.S(A1:C1,D1:F1)</f><v>0</v></c><c r="K1"><f>COVAR(A1:C1,D1:F1)</f><v>0</v></c><c r="L1"><f>COVARIANCE.P(A1:B1,D1:F1)</f><v>7</v></c><c r="M1"><f>COVARIANCE.P(A1:B1,I1:J1)</f><v>9</v></c><c r="N1"><f>COVARIANCE.P(A1:A1,D1:D1)</f><v>8</v></c><c r="O1"><f>COVARIANCE.P(1,2)</f><v>10</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>COVARIANCE.P(A1:C1,D1:F1)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COVARIANCE.S(A1:C1,D1:F1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COVAR(A1:C1,D1:F1)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COVARIANCE.P(A1:B1,D1:F1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COVARIANCE.P(A1:B1,I1:J1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COVARIANCE.P(A1:A1,D1:D1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COVARIANCE.P(1,2)</f><v>10</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_rank() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>20</v></c><c r="C1"><v>20</v></c><c r="D1"><v>30</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><f>RANK(D1,A1:E1)</f><v>0</v></c><c r="G1"><f>RANK.EQ(B1,A1:E1)</f><v>0</v></c><c r="H1"><f>RANK(A1,A1:E1)</f><v>0</v></c><c r="I1"><f>RANK(B1,A1:E1,1)</f><v>0</v></c><c r="J1"><f>RANK(15,A1:E1)</f><v>7</v></c><c r="K1"><f>RANK(B1,A1)</f><v>8</v></c><c r="L1"><f>RANK(B1,A1:E1,1,2)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "10").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>RANK(D1,A1:E1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RANK.EQ(B1,A1:E1)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RANK(A1,A1:E1)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RANK(B1,A1:E1,1)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RANK(15,A1:E1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>RANK(B1,A1)</f><v>8</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>RANK(B1,A1:E1,1,2)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_rank_avg() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>20</v></c><c r="C1"><v>20</v></c><c r="D1"><v>30</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><f>RANK.AVG(B1,A1:E1)</f><v>0</v></c><c r="G1"><f>RANK.AVG(D1,A1:E1)</f><v>0</v></c><c r="H1"><f>RANK.AVG(A1,A1:E1)</f><v>0</v></c><c r="I1"><f>RANK.AVG(B1,A1:E1,1)</f><v>0</v></c><c r="J1"><f>RANK.AVG(15,A1:E1)</f><v>7</v></c><c r="K1"><f>RANK.AVG(B1,A1)</f><v>8</v></c><c r="L1"><f>RANK.AVG(B1,A1:E1,1,2)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "10").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>RANK.AVG(B1,A1:E1)</f><v>2.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RANK.AVG(D1,A1:E1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RANK.AVG(A1,A1:E1)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RANK.AVG(B1,A1:E1,1)</f><v>2.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RANK.AVG(15,A1:E1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RANK.AVG(B1,A1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>RANK.AVG(B1,A1:E1,1,2)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_percentile() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>20</v></c><c r="C1"><v>30</v></c><c r="D1"><v>40</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><f>PERCENTILE(A1:E1,0)</f><v>0</v></c><c r="G1"><f>PERCENTILE(A1:E1,1)</f><v>0</v></c><c r="H1"><f>PERCENTILE.INC(A1:E1,0.25)</f><v>0</v></c><c r="I1"><f>PERCENTILE(A1:E1,0.5)</f><v>0</v></c><c r="J1"><f>PERCENTILE(A1:E1,-0.1)</f><v>7</v></c><c r="K1"><f>PERCENTILE(A1:E1,2)</f><v>8</v></c><c r="L1"><f>PERCENTILE(A1,0)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "10").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>PERCENTILE(A1:E1,0)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE(A1:E1,1)</f><v>40</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE.INC(A1:E1,0.25)</f><v>17.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE(A1:E1,0.5)</f><v>25</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE(A1:E1,-0.1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE(A1:E1,2)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE(A1,0)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_quartile() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>20</v></c><c r="C1"><v>30</v></c><c r="D1"><v>40</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><f>QUARTILE(A1:E1,0)</f><v>0</v></c><c r="G1"><f>QUARTILE(A1:E1,1)</f><v>0</v></c><c r="H1"><f>QUARTILE.INC(A1:E1,1.9)</f><v>0</v></c><c r="I1"><f>QUARTILE(A1:E1,2)</f><v>0</v></c><c r="J1"><f>QUARTILE(A1:E1,3)</f><v>0</v></c><c r="K1"><f>QUARTILE(A1:E1,4)</f><v>0</v></c><c r="L1"><f>QUARTILE(A1:E1,5)</f><v>7</v></c><c r="M1"><f>QUARTILE(A1:E1,-1)</f><v>8</v></c><c r="N1"><f>QUARTILE(A1,0)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "10").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>QUARTILE(A1:E1,0)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE(A1:E1,1)</f><v>17.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE.INC(A1:E1,1.9)</f><v>17.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE(A1:E1,2)</f><v>25</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE(A1:E1,3)</f><v>32.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE(A1:E1,4)</f><v>40</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE(A1:E1,5)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE(A1:E1,-1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE(A1,0)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_mode() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>20</v></c><c r="C1"><v>20</v></c><c r="D1"><v>10</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><v>30</v></c><c r="G1"><v>30</v></c><c r="H1"><v>30</v></c><c r="J1"><f>MODE(A1:H1)</f><v>0</v></c><c r="K1"><f>MODE.SNGL(A1:D1)</f><v>0</v></c><c r="L1"><f>MODE(A1:A1)</f><v>7</v></c><c r="M1"><f>MODE(A1)</f><v>8</v></c><c r="N1"><f>MODE(A1:D1,1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "10").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>MODE(A1:H1)</f><v>30</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>MODE.SNGL(A1:D1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>MODE(A1:A1)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MODE(A1)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>MODE(A1:D1,1)</f><v>9</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_percent_rank() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>20</v></c><c r="C1"><v>30</v></c><c r="D1"><v>40</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><f>PERCENTRANK(A1:E1,A1)</f><v>0</v></c><c r="G1"><f>PERCENTRANK(A1:E1,D1)</f><v>0</v></c><c r="H1"><f>PERCENTRANK.INC(A1:E1,B1)</f><v>0</v></c><c r="I1"><f>PERCENTRANK(A1:E1,25)</f><v>0</v></c><c r="J1"><f>PERCENTRANK(A1:E1,5)</f><v>7</v></c><c r="K1"><f>PERCENTRANK(A1:E1,50)</f><v>8</v></c><c r="L1"><f>PERCENTRANK(A1,10)</f><v>9</v></c><c r="M1"><f>PERCENTRANK(A1:E1,20,3)</f><v>11</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "10").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>PERCENTRANK(A1:E1,A1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK(A1:E1,D1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK.INC(A1:E1,B1)</f><v>0.33333333</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK(A1:E1,25)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK(A1:E1,5)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK(A1:E1,50)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK(A1,10)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK(A1:E1,20,3)</f><v>11</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_percent_rank_exc() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>20</v></c><c r="C1"><v>30</v></c><c r="D1"><v>40</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><f>PERCENTRANK.EXC(A1:E1,A1)</f><v>0</v></c><c r="G1"><f>PERCENTRANK.EXC(A1:E1,D1)</f><v>0</v></c><c r="H1"><f>PERCENTRANK.EXC(A1:E1,B1)</f><v>0</v></c><c r="I1"><f>PERCENTRANK.EXC(A1:E1,25)</f><v>0</v></c><c r="J1"><f>PERCENTRANK.EXC(A1:E1,5)</f><v>7</v></c><c r="K1"><f>PERCENTRANK.EXC(A1:E1,50)</f><v>8</v></c><c r="L1"><f>PERCENTRANK.EXC(A1,10)</f><v>9</v></c><c r="M1"><f>PERCENTRANK.EXC(A1:E1,20,3)</f><v>11</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "10").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>PERCENTRANK.EXC(A1:E1,A1)</f><v>0.2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK.EXC(A1:E1,D1)</f><v>0.8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK.EXC(A1:E1,B1)</f><v>0.4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK.EXC(A1:E1,25)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK.EXC(A1:E1,5)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK.EXC(A1:E1,50)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK.EXC(A1,10)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTRANK.EXC(A1:E1,20,3)</f><v>11</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_standardize() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>40</v></c><c r="C1"><v>1.5</v></c><c r="D1"><v>0</v></c><c r="E1"><f>STANDARDIZE(A1,B1,C1)</f><v>0</v></c><c r="F1"><f>STANDARDIZE(A1,A1,5)</f><v>0</v></c><c r="G1"><f>STANDARDIZE(A1,B1,D1)</f><v>7</v></c><c r="H1"><f>STANDARDIZE(A1,B1,-2)</f><v>8</v></c><c r="I1"><f>STANDARDIZE(A1,B1)</f><v>9</v></c><c r="J1"><f>STANDARDIZE(&quot;ab&quot;,1,2)</f><v>11</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "42").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>STANDARDIZE(A1,B1,C1)</f><v>1.33333333</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STANDARDIZE(A1,A1,5)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STANDARDIZE(A1,B1,D1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STANDARDIZE(A1,B1,-2)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STANDARDIZE(A1,B1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STANDARDIZE(&quot;ab&quot;,1,2)</f><v>11</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_skew() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>1</v></c><c r="C1"><v>4</v></c><c r="D1" t="inlineStr"><is><t>xy</t></is></c><c r="E1"><v>5</v></c><c r="F1"><v>5</v></c><c r="G1"><v>5</v></c><c r="H1"><f>SKEW(A1:D1)</f><v>0</v></c><c r="I1"><f>SKEW.P(A1:C1)</f><v>0</v></c><c r="J1"><f>SKEW(A1:B1)</f><v>5</v></c><c r="K1"><f>SKEW(E1:G1)</f><v>6</v></c><c r="L1"><f>SKEW()</f><v>7</v></c><c r="M1"><f>SKEW(&quot;ab&quot;)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SKEW(A1:D1)</f><v>1.73205081</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SKEW.P(A1:C1)</f><v>0.70710678</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SKEW(A1:B1)</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SKEW(E1:G1)</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SKEW()</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SKEW(&quot;ab&quot;)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_kurt() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>1</v></c><c r="C1"><v>1</v></c><c r="D1"><v>3</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><v>2</v></c><c r="G1"><v>2</v></c><c r="H1"><v>2</v></c><c r="I1"><v>2</v></c><c r="J1"><f>KURT(A1:E1)</f><v>0</v></c><c r="K1"><f>KURT(A1:C1)</f><v>5</v></c><c r="L1"><f>KURT(F1:I1)</f><v>6</v></c><c r="M1"><f>KURT()</f><v>7</v></c><c r="N1"><f>KURT(&quot;ab&quot;)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>KURT(A1:E1)</f><v>4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>KURT(A1:C1)</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>KURT(F1:I1)</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>KURT()</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>KURT(&quot;ab&quot;)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_trimmean() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>20</v></c><c r="C1"><v>30</v></c><c r="D1"><v>100</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><f>TRIMMEAN(A1:E1,0.5)</f><v>0</v></c><c r="G1"><f>TRIMMEAN(A1:E1,0)</f><v>0</v></c><c r="H1"><f>TRIMMEAN(A1:E1,0.2)</f><v>0</v></c><c r="I1"><f>TRIMMEAN(A1:E1,1)</f><v>7</v></c><c r="J1"><f>TRIMMEAN(A1:E1,-0.1)</f><v>8</v></c><c r="K1"><f>TRIMMEAN(A1,0)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "10").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>TRIMMEAN(A1:E1,0.5)</f><v>25</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TRIMMEAN(A1:E1,0)</f><v>40</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TRIMMEAN(A1:E1,0.2)</f><v>40</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TRIMMEAN(A1:E1,1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TRIMMEAN(A1:E1,-0.1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>TRIMMEAN(A1,0)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_fisher() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>FISHER(A1)</f><v>0</v></c><c r="C1"><f>FISHER(0)</f><v>0</v></c><c r="D1"><f>FISHERINV(0)</f><v>0</v></c><c r="E1"><f>FISHERINV(1)</f><v>0</v></c><c r="F1"><f>FISHER(1)</f><v>7</v></c><c r="G1"><f>FISHER(-1)</f><v>8</v></c><c r="H1"><f>FISHER(2)</f><v>9</v></c><c r="I1"><f>FISHER(&quot;ab&quot;)</f><v>11</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "0.5").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>FISHER(A1)</f><v>0.54930614</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>FISHER(0)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>FISHERINV(0)</f><v>0</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>FISHERINV(1)</f><v>0.76159416</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>FISHER(1)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>FISHER(-1)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>FISHER(2)</f><v>9</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>FISHER(&quot;ab&quot;)</f><v>11</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_sqrt_pi() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>SQRTPI(A1)</f><v>0</v></c><c r="C1"><f>SQRTPI(0)</f><v>0</v></c><c r="D1"><f>SQRTPI(-1)</f><v>7</v></c><c r="E1"><f>SQRTPI(&quot;ab&quot;)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SQRTPI(A1)</f><v>1.77245385</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SQRTPI(0)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SQRTPI(-1)</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SQRTPI(&quot;ab&quot;)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_combina() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>COMBINA(A1,3)</f><v>0</v></c><c r="C1"><f>COMBINA(4.9,3.2)</f><v>0</v></c><c r="D1"><f>COMBINA(4,0)</f><v>0</v></c><c r="E1"><f>COMBINA(0,0)</f><v>0</v></c><c r="F1"><f>COMBINA(0,2)</f><v>0</v></c><c r="G1"><f>COMBINA(-1,1)</f><v>7</v></c><c r="H1"><f>COMBINA(4,-1)</f><v>8</v></c><c r="I1"><f>COMBINA(&quot;ab&quot;,1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "4").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>COMBINA(A1,3)</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COMBINA(4.9,3.2)</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>COMBINA(4,0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COMBINA(0,0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COMBINA(0,2)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COMBINA(-1,1)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COMBINA(4,-1)</f><v>8</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>COMBINA(&quot;ab&quot;,1)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_sum_x() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>3</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><v>4</v></c><c r="E1"><v>1</v></c><c r="F1"><v>9</v></c><c r="G1"><f>SUMX2MY2(A1:C1,D1:F1)</f><v>0</v></c><c r="H1"><f>SUMX2PY2(A1:C1,D1:F1)</f><v>0</v></c><c r="I1"><f>SUMXMY2(A1:C1,D1:F1)</f><v>0</v></c><c r="J1"><f>SUMX2MY2(A1:B1,D1:F1)</f><v>7</v></c><c r="K1"><f>SUMX2MY2(1,2)</f><v>8</v></c><c r="L1"><v>1e200</v></c><c r="M1"><v>1</v></c><c r="N1"><f>SUMX2MY2(L1:L1,M1:M1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SUMX2MY2(A1:C1,D1:F1)</f><v>-4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMX2PY2(A1:C1,D1:F1)</f><v>30</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMXMY2(A1:C1,D1:F1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMX2MY2(A1:B1,D1:F1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SUMX2MY2(1,2)</f><v>8</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SUMX2MY2(L1:L1,M1:M1)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_gestep() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>GESTEP(A1,4)</f><v>0</v></c><c r="C1"><f>GESTEP(A1,5)</f><v>0</v></c><c r="D1"><f>GESTEP(A1,6)</f><v>0</v></c><c r="E1"><f>GESTEP(A1)</f><v>0</v></c><c r="F1"><f>GESTEP(-1)</f><v>0</v></c><c r="G1"><f>DELTA(A1,5)</f><v>0</v></c><c r="H1"><f>DELTA(A1,4)</f><v>0</v></c><c r="I1"><f>DELTA(0)</f><v>0</v></c><c r="J1"><f>DELTA(A1)</f><v>0</v></c><c r="K1"><f>GESTEP(&quot;ab&quot;)</f><v>7</v></c><c r="L1"><f>DELTA(A1,&quot;ab&quot;)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "5").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>GESTEP(A1,4)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GESTEP(A1,5)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GESTEP(A1,6)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GESTEP(A1)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GESTEP(-1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DELTA(A1,5)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DELTA(A1,4)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DELTA(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DELTA(A1)</f><v>0</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>GESTEP(&quot;ab&quot;)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>DELTA(A1,&quot;ab&quot;)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_multinomial() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>3</v></c><c r="C1"><v>4</v></c><c r="D1" t="inlineStr"><is><t>xy</t></is></c><c r="E1"><f>MULTINOMIAL(A1:D1)</f><v>0</v></c><c r="F1"><f>MULTINOMIAL(2.9,3.2)</f><v>0</v></c><c r="G1"><f>MULTINOMIAL(0)</f><v>0</v></c><c r="H1"><f>MULTINOMIAL()</f><v>7</v></c><c r="I1"><f>MULTINOMIAL(-1,2)</f><v>8</v></c><c r="J1"><f>MULTINOMIAL(&quot;ab&quot;)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>MULTINOMIAL(A1:D1)</f><v>1260</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MULTINOMIAL(2.9,3.2)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MULTINOMIAL(0)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>MULTINOMIAL()</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>MULTINOMIAL(-1,2)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MULTINOMIAL(&quot;ab&quot;)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_fact_double() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>FACTDOUBLE(A1)</f><v>0</v></c><c r="C1"><f>FACTDOUBLE(7.9)</f><v>0</v></c><c r="D1"><f>FACTDOUBLE(0)</f><v>0</v></c><c r="E1"><f>FACTDOUBLE(1)</f><v>0</v></c><c r="F1"><f>FACTDOUBLE(-1)</f><v>7</v></c><c r="G1"><f>FACTDOUBLE(301)</f><v>8</v></c><c r="H1"><f>FACTDOUBLE(&quot;ab&quot;)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "6").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>FACTDOUBLE(A1)</f><v>48</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FACTDOUBLE(7.9)</f><v>105</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>FACTDOUBLE(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>FACTDOUBLE(1)</f><v>1</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>FACTDOUBLE(-1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FACTDOUBLE(301)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>FACTDOUBLE(&quot;ab&quot;)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_poisson() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>5</v></c><c r="C1"><f>POISSON.DIST(A1,B1,0)</f><v>0</v></c><c r="D1"><f>POISSON(A1,B1,1)</f><v>0</v></c><c r="E1"><f>POISSON.DIST(2.9,5,0)</f><v>0</v></c><c r="F1"><f>POISSON.DIST(0,0,0)</f><v>0</v></c><c r="G1"><f>POISSON.DIST(A1,0,1)</f><v>0</v></c><c r="H1"><f>POISSON.DIST(-1,5,0)</f><v>7</v></c><c r="I1"><f>POISSON.DIST(2,-1,0)</f><v>8</v></c><c r="J1"><f>POISSON.DIST(171,1,0)</f><v>9</v></c><c r="K1"><f>POISSON.DIST(2,700,0)</f><v>11</v></c><c r="L1"><f>POISSON.DIST(2,5)</f><v>12</v></c><c r="M1"><f>POISSON.DIST(&quot;ab&quot;,5,0)</f><v>13</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>POISSON.DIST(A1,B1,0)</f><v>0.08422434</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>POISSON(A1,B1,1)</f><v>0.12465202</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>POISSON.DIST(2.9,5,0)</f><v>0.08422434</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>POISSON.DIST(0,0,0)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>POISSON.DIST(A1,0,1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>POISSON.DIST(-1,5,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>POISSON.DIST(2,-1,0)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>POISSON.DIST(171,1,0)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>POISSON.DIST(2,700,0)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>POISSON.DIST(2,5)</f><v>12</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>POISSON.DIST(&quot;ab&quot;,5,0)</f><v>13</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_binom_dist() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>4</v></c><c r="C1"><v>0.5</v></c><c r="D1"><f>BINOM.DIST(A1,B1,C1,0)</f><v>0</v></c><c r="E1"><f>BINOM.DIST(A1,B1,C1,1)</f><v>0</v></c><c r="F1"><f>BINOM.DIST(2.9,4.9,0.5,0)</f><v>0</v></c><c r="G1"><f>BINOMDIST(0,4,0,0)</f><v>0</v></c><c r="H1"><f>BINOM.DIST(1,4,0,0)</f><v>0</v></c><c r="I1"><f>BINOM.DIST(4,4,1,0)</f><v>0</v></c><c r="J1"><f>BINOM.DIST(3,4,1.1,0)</f><v>7</v></c><c r="K1"><f>BINOM.DIST(5,4,0.5,0)</f><v>8</v></c><c r="L1"><f>BINOM.DIST(2,171,0.5,0)</f><v>9</v></c><c r="M1"><f>BINOM.DIST(2,4,0.5)</f><v>11</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>BINOM.DIST(A1,B1,C1,0)</f><v>0.375</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST(A1,B1,C1,1)</f><v>0.6875</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST(2.9,4.9,0.5,0)</f><v>0.375</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOMDIST(0,4,0,0)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST(1,4,0,0)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST(4,4,1,0)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST(3,4,1.1,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST(5,4,0.5,0)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST(2,171,0.5,0)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST(2,4,0.5)</f><v>11</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_expon_dist() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>2</v></c><c r="C1"><f>EXPON.DIST(A1,B1,0)</f><v>0</v></c><c r="D1"><f>EXPON.DIST(A1,B1,1)</f><v>0</v></c><c r="E1"><f>EXPONDIST(0,2,0)</f><v>0</v></c><c r="F1"><f>EXPON.DIST(0,2,1)</f><v>0</v></c><c r="G1"><f>EXPON.DIST(-1,2,0)</f><v>7</v></c><c r="H1"><f>EXPON.DIST(1,0,0)</f><v>8</v></c><c r="I1"><f>EXPON.DIST(1,-2,1)</f><v>9</v></c><c r="J1"><f>EXPON.DIST(1,2)</f><v>11</v></c><c r="K1"><f>EXPON.DIST(&quot;ab&quot;,2,0)</f><v>12</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "0.5").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>EXPON.DIST(A1,B1,0)</f><v>0.73575888</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EXPON.DIST(A1,B1,1)</f><v>0.63212056</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EXPONDIST(0,2,0)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EXPON.DIST(0,2,1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EXPON.DIST(-1,2,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EXPON.DIST(1,0,0)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EXPON.DIST(1,-2,1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EXPON.DIST(1,2)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>EXPON.DIST(&quot;ab&quot;,2,0)</f><v>12</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_negbinom_dist() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>3</v></c><c r="C1"><f>NEGBINOM.DIST(A1,B1,0.5,0)</f><v>0</v></c><c r="D1"><f>NEGBINOM.DIST(A1,B1,0.5,1)</f><v>0</v></c><c r="E1"><f>NEGBINOMDIST(2,3,0.5)</f><v>0</v></c><c r="F1"><f>NEGBINOM.DIST(2.9,3.2,0.5,0)</f><v>0</v></c><c r="G1"><f>NEGBINOM.DIST(0,1,0.5,0)</f><v>0</v></c><c r="H1"><f>NEGBINOM.DIST(-1,3,0.5,0)</f><v>7</v></c><c r="I1"><f>NEGBINOM.DIST(2,0,0.5,0)</f><v>8</v></c><c r="J1"><f>NEGBINOM.DIST(2,3,0,0)</f><v>9</v></c><c r="K1"><f>NEGBINOM.DIST(2,3,1,1)</f><v>10</v></c><c r="L1"><f>NEGBINOM.DIST(171,1,0.5,0)</f><v>11</v></c><c r="M1"><f>NEGBINOM.DIST(2,3,0.5)</f><v>12</v></c><c r="N1"><f>NEGBINOM.DIST(&quot;ab&quot;,3,0.5,0)</f><v>13</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(A1,B1,0.5,0)</f><v>0.1875</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(A1,B1,0.5,1)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOMDIST(2,3,0.5)</f><v>0.1875</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(2.9,3.2,0.5,0)</f><v>0.1875</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(0,1,0.5,0)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(-1,3,0.5,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(2,0,0.5,0)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(2,3,0,0)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(2,3,1,1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(171,1,0.5,0)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(2,3,0.5)</f><v>12</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NEGBINOM.DIST(&quot;ab&quot;,3,0.5,0)</f><v>13</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_hypgeom_dist() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>5</v></c><c r="C1"><f>HYPGEOM.DIST(A1,B1,4,10,0)</f><v>0</v></c><c r="D1"><f>HYPGEOM.DIST(A1,B1,4,10,1)</f><v>0</v></c><c r="E1"><f>HYPGEOMDIST(2,5,4,10)</f><v>0</v></c><c r="F1"><f>HYPGEOM.DIST(2.9,5.2,4.9,10.8,0)</f><v>0</v></c><c r="G1"><f>HYPGEOM.DIST(0,0,0,0,0)</f><v>0</v></c><c r="H1"><f>HYPGEOM.DIST(-1,5,4,10,0)</f><v>7</v></c><c r="I1"><f>HYPGEOM.DIST(6,5,4,10,0)</f><v>8</v></c><c r="J1"><f>HYPGEOM.DIST(1,11,4,10,0)</f><v>9</v></c><c r="K1"><f>HYPGEOM.DIST(1,5,8,10,0)</f><v>10</v></c><c r="L1"><f>HYPGEOM.DIST(0,171,0,171,0)</f><v>11</v></c><c r="M1"><f>HYPGEOM.DIST(2,5,4,10)</f><v>12</v></c><c r="N1"><f>HYPGEOM.DIST(&quot;ab&quot;,5,4,10,0)</f><v>13</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(A1,B1,4,10,0)</f><v>0.47619048</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(A1,B1,4,10,1)</f><v>0.73809524</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOMDIST(2,5,4,10)</f><v>0.47619048</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(2.9,5.2,4.9,10.8,0)</f><v>0.47619048</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(0,0,0,0,0)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(-1,5,4,10,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(6,5,4,10,0)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(1,11,4,10,0)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(1,5,8,10,0)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(0,171,0,171,0)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(2,5,4,10)</f><v>12</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>HYPGEOM.DIST(&quot;ab&quot;,5,4,10,0)</f><v>13</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_weibull_dist() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>2</v></c><c r="C1"><v>2</v></c><c r="D1"><f>WEIBULL.DIST(A1,B1,C1,0)</f><v>0</v></c><c r="E1"><f>WEIBULL.DIST(A1,B1,C1,1)</f><v>0</v></c><c r="F1"><f>WEIBULL(0.5,1,0.5,0)</f><v>0</v></c><c r="G1"><f>WEIBULL.DIST(0,2,1,0)</f><v>0</v></c><c r="H1"><f>WEIBULL.DIST(0,2,1,1)</f><v>0</v></c><c r="I1"><f>WEIBULL.DIST(0,1,2,0)</f><v>0</v></c><c r="J1"><f>WEIBULL.DIST(0,0.5,1,0)</f><v>7</v></c><c r="K1"><f>WEIBULL.DIST(0,0.5,1,1)</f><v>0</v></c><c r="L1"><f>WEIBULL.DIST(-1,2,1,0)</f><v>8</v></c><c r="M1"><f>WEIBULL.DIST(1,0,1,0)</f><v>9</v></c><c r="N1"><f>WEIBULL.DIST(1,2,0,1)</f><v>10</v></c><c r="O1"><f>WEIBULL.DIST(10000000000000000,20,1,0)</f><v>11</v></c><c r="P1"><f>WEIBULL.DIST(1,2,1)</f><v>12</v></c><c r="Q1"><f>WEIBULL.DIST(&quot;ab&quot;,2,1,0)</f><v>13</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(A1,B1,C1,0)</f><v>0.36787944</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(A1,B1,C1,1)</f><v>0.63212056</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL(0.5,1,0.5,0)</f><v>0.73575888</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(0,2,1,0)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(0,2,1,1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(0,1,2,0)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(0,0.5,1,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(0,0.5,1,1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(-1,2,1,0)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(1,0,1,0)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(1,2,0,1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(10000000000000000,20,1,0)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(1,2,1)</f><v>12</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>WEIBULL.DIST(&quot;ab&quot;,2,1,0)</f><v>13</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_gamma() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>GAMMA(A1)</f><v>0</v></c><c r="C1"><f>GAMMALN(A1)</f><v>0</v></c><c r="D1"><f>GAMMA(0.5)</f><v>0</v></c><c r="E1"><f>GAMMA(-0.5)</f><v>0</v></c><c r="F1"><f>GAMMALN(0.5)</f><v>0</v></c><c r="G1"><f>GAMMA(1)</f><v>0</v></c><c r="H1"><f>GAMMALN(1)</f><v>0</v></c><c r="I1"><f>GAMMA(0)</f><v>7</v></c><c r="J1"><f>GAMMA(-2)</f><v>8</v></c><c r="K1"><f>GAMMA(171)</f><v>9</v></c><c r="L1"><f>GAMMALN(0)</f><v>10</v></c><c r="M1"><f>GAMMALN(-1)</f><v>11</v></c><c r="N1"><f>GAMMA(&quot;ab&quot;)</f><v>12</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "5").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>GAMMA(A1)</f><v>24</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>GAMMALN(A1)</f><v>3.17805383</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA(0.5)</f><v>1.77245385</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA(-0.5)</f><v>-3.5449077</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMALN(0.5)</f><v>0.57236494</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>GAMMA(1)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GAMMALN(1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GAMMA(0)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GAMMA(-2)</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GAMMA(171)</f><v>9</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GAMMALN(0)</f><v>10</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GAMMALN(-1)</f><v>11</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>GAMMA(&quot;ab&quot;)</f><v>12</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_gamma_dist() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>2</v></c><c r="C1"><v>1</v></c><c r="D1"><f>GAMMA.DIST(A1,B1,C1,0)</f><v>0</v></c><c r="E1"><f>GAMMA.DIST(A1,B1,C1,1)</f><v>0</v></c><c r="F1"><f>GAMMADIST(0.5,1,0.5,0)</f><v>0</v></c><c r="G1"><f>GAMMA.DIST(1,0.5,1,0)</f><v>0</v></c><c r="H1"><f>GAMMA.DIST(1,1.5,1,1)</f><v>0</v></c><c r="I1"><f>GAMMA.DIST(0,2,1,0)</f><v>0</v></c><c r="J1"><f>GAMMA.DIST(0,2,1,1)</f><v>0</v></c><c r="K1"><f>GAMMA.DIST(0,1,2,0)</f><v>0</v></c><c r="L1"><f>GAMMA.DIST(0,0.5,1,0)</f><v>7</v></c><c r="M1"><f>GAMMA.DIST(-1,2,1,0)</f><v>8</v></c><c r="N1"><f>GAMMA.DIST(1,0,1,0)</f><v>9</v></c><c r="O1"><f>GAMMA.DIST(1,2,0,1)</f><v>10</v></c><c r="P1"><f>GAMMA.DIST(1,172,1,1)</f><v>11</v></c><c r="Q1"><f>GAMMA.DIST(1000,0.5,1,1)</f><v>12</v></c><c r="R1"><f>GAMMA.DIST(1,2,1)</f><v>13</v></c><c r="S1"><f>GAMMA.DIST(&quot;ab&quot;,2,1,0)</f><v>14</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(A1,B1,C1,0)</f><v>0.36787944</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(A1,B1,C1,1)</f><v>0.26424112</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMADIST(0.5,1,0.5,0)</f><v>0.73575888</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(1,0.5,1,0)</f><v>0.20755375</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(1,1.5,1,1)</f><v>0.4275933</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(0,2,1,0)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(0,2,1,1)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(0,1,2,0)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(0,0.5,1,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(-1,2,1,0)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(1,0,1,0)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(1,2,0,1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(1,172,1,1)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(1000,0.5,1,1)</f><v>12</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(1,2,1)</f><v>13</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.DIST(&quot;ab&quot;,2,1,0)</f><v>14</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_binom_inv() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>0.5</v></c><c r="C1"><f>BINOM.INV(A1,B1,0.5)</f><v>0</v></c><c r="D1"><f>BINOM.INV(A1,B1,0.6875)</f><v>0</v></c><c r="E1"><f>BINOM.INV(A1,B1,0.3125)</f><v>0</v></c><c r="F1"><f>CRITBINOM(4,0.5,0.0625)</f><v>0</v></c><c r="G1"><f>BINOM.INV(4.9,0.5,0.5)</f><v>0</v></c><c r="H1"><f>BINOM.INV(-1,0.5,0.5)</f><v>7</v></c><c r="I1"><f>BINOM.INV(4,0,0.5)</f><v>8</v></c><c r="J1"><f>BINOM.INV(4,1,0.5)</f><v>9</v></c><c r="K1"><f>BINOM.INV(4,0.5,0)</f><v>10</v></c><c r="L1"><f>BINOM.INV(4,0.5,1)</f><v>11</v></c><c r="M1"><f>BINOM.INV(171,0.5,0.5)</f><v>12</v></c><c r="N1"><f>BINOM.INV(4,0.5)</f><v>13</v></c><c r="O1"><f>BINOM.INV(&quot;ab&quot;,0.5,0.5)</f><v>14</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "4").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>BINOM.INV(A1,B1,0.5)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(A1,B1,0.6875)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(A1,B1,0.3125)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CRITBINOM(4,0.5,0.0625)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(4.9,0.5,0.5)</f><v>2</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(-1,0.5,0.5)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(4,0,0.5)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(4,1,0.5)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(4,0.5,0)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(4,0.5,1)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(171,0.5,0.5)</f><v>12</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(4,0.5)</f><v>13</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.INV(&quot;ab&quot;,0.5,0.5)</f><v>14</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_chisq_dist() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>2</v></c><c r="C1"><f>CHISQ.DIST(A1,B1,0)</f><v>0</v></c><c r="D1"><f>CHISQ.DIST(A1,B1,1)</f><v>0</v></c><c r="E1"><f>CHISQ.DIST.RT(A1,B1)</f><v>0</v></c><c r="F1"><f>CHIDIST(1,2)</f><v>0</v></c><c r="G1"><f>CHISQ.DIST(1,2.9,0)</f><v>0</v></c><c r="H1"><f>CHISQ.DIST(1,1,1)</f><v>0</v></c><c r="I1"><f>CHISQ.DIST(-1,2,0)</f><v>7</v></c><c r="J1"><f>CHISQ.DIST(1,0.9,0)</f><v>8</v></c><c r="K1"><f>CHISQ.DIST(1,344,1)</f><v>9</v></c><c r="L1"><f>CHISQ.DIST(1400,2,1)</f><v>10</v></c><c r="M1"><f>CHISQ.DIST(2000,1,1)</f><v>11</v></c><c r="N1"><f>CHISQ.DIST(1,2)</f><v>12</v></c><c r="O1"><f>CHISQ.DIST(&quot;ab&quot;,2,0)</f><v>13</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(A1,B1,0)</f><v>0.30326533</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(A1,B1,1)</f><v>0.39346934</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST.RT(A1,B1)</f><v>0.60653066</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHIDIST(1,2)</f><v>0.60653066</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(1,2.9,0)</f><v>0.30326533</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(1,1,1)</f><v>0.68268949</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(-1,2,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(1,0.9,0)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(1,344,1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(1400,2,1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(2000,1,1)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(1,2)</f><v>12</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.DIST(&quot;ab&quot;,2,0)</f><v>13</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_norms_dist() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>NORM.S.DIST(A1,0)</f><v>0</v></c><c r="C1"><f>NORM.S.DIST(A1,1)</f><v>0</v></c><c r="D1"><f>NORMSDIST(1)</f><v>0</v></c><c r="E1"><f>NORM.S.DIST(0,0)</f><v>0</v></c><c r="F1"><f>NORM.S.DIST(0,1)</f><v>0</v></c><c r="G1"><f>NORM.S.DIST(-1,1)</f><v>0</v></c><c r="H1"><f>NORM.S.DIST(40,0)</f><v>7</v></c><c r="I1"><f>NORM.S.DIST(40,1)</f><v>8</v></c><c r="J1"><f>NORM.S.DIST(1)</f><v>9</v></c><c r="K1"><f>NORM.S.DIST(&quot;ab&quot;,1)</f><v>10</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>NORM.S.DIST(A1,0)</f><v>0.24197072</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.S.DIST(A1,1)</f><v>0.84134475</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORMSDIST(1)</f><v>0.84134475</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.S.DIST(0,0)</f><v>0.39894228</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.S.DIST(0,1)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.S.DIST(-1,1)</f><v>0.15865525</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.S.DIST(40,0)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.S.DIST(40,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.S.DIST(1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.S.DIST(&quot;ab&quot;,1)</f><v>10</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_norm_dist() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>40</v></c><c r="C1"><v>1.5</v></c><c r="D1"><f>NORM.DIST(A1,B1,C1,0)</f><v>0</v></c><c r="E1"><f>NORM.DIST(A1,B1,C1,1)</f><v>0</v></c><c r="F1"><f>NORMDIST(1,0,1,0)</f><v>0</v></c><c r="G1"><f>NORM.DIST(1,0,1,1)</f><v>0</v></c><c r="H1"><f>NORM.DIST(40,0,1,0)</f><v>7</v></c><c r="I1"><f>NORM.DIST(40,0,1,1)</f><v>8</v></c><c r="J1"><f>NORM.DIST(1,0,0,1)</f><v>9</v></c><c r="K1"><f>NORM.DIST(1,0,-1,1)</f><v>10</v></c><c r="L1"><f>NORM.DIST(1,0,1)</f><v>11</v></c><c r="M1"><f>NORM.DIST(&quot;ab&quot;,0,1,0)</f><v>12</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "42").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>NORM.DIST(A1,B1,C1,0)</f><v>0.10934005</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.DIST(A1,B1,C1,1)</f><v>0.90878878</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORMDIST(1,0,1,0)</f><v>0.24197072</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.DIST(1,0,1,1)</f><v>0.84134475</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.DIST(40,0,1,0)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.DIST(40,0,1,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.DIST(1,0,0,1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.DIST(1,0,-1,1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.DIST(1,0,1)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.DIST(&quot;ab&quot;,0,1,0)</f><v>12</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_erf() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>ERF(A1)</f><v>0</v></c><c r="C1"><f>ERF(0,A1)</f><v>0</v></c><c r="D1"><f>ERF(-1,1)</f><v>0</v></c><c r="E1"><f>ERFC(A1)</f><v>0</v></c><c r="F1"><f>GAUSS(A1)</f><v>0</v></c><c r="G1"><f>PHI(0)</f><v>0</v></c><c r="H1"><f>PHI(A1)</f><v>0</v></c><c r="I1"><f>ERF(40)</f><v>7</v></c><c r="J1"><f>ERF(&quot;ab&quot;)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>ERF(A1)</f><v>0.84270079</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ERF(0,A1)</f><v>0.84270079</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ERF(-1,1)</f><v>1.68540159</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ERFC(A1)</f><v>0.15729921</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAUSS(A1)</f><v>0.34134475</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PHI(0)</f><v>0.39894228</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PHI(A1)</f><v>0.24197072</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ERF(40)</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>ERF(&quot;ab&quot;)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_lognorm_dist() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>LOGNORM.DIST(A1,0,1,0)</f><v>0</v></c><c r="C1"><f>LOGNORM.DIST(A1,0,1,1)</f><v>0</v></c><c r="D1"><f>LOGNORMDIST(1,0,1)</f><v>0</v></c><c r="E1"><f>LOGNORM.DIST(1,0,1,0)</f><v>0</v></c><c r="F1"><f>LOGNORM.DIST(0,0,1,1)</f><v>7</v></c><c r="G1"><f>LOGNORM.DIST(-1,0,1,0)</f><v>8</v></c><c r="H1"><f>LOGNORM.DIST(2,0,0,1)</f><v>9</v></c><c r="I1"><f>LOGNORM.DIST(2,0,-1,1)</f><v>10</v></c><c r="J1"><f>LOGNORM.DIST(2,0,1)</f><v>11</v></c><c r="K1"><f>LOGNORM.DIST(&quot;ab&quot;,0,1,0)</f><v>12</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>LOGNORM.DIST(A1,0,1,0)</f><v>0.15687402</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.DIST(A1,0,1,1)</f><v>0.7558914</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORMDIST(1,0,1)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.DIST(1,0,1,0)</f><v>0.39894228</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.DIST(0,0,1,1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.DIST(-1,0,1,0)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.DIST(2,0,0,1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.DIST(2,0,-1,1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.DIST(2,0,1)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.DIST(&quot;ab&quot;,0,1,0)</f><v>12</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_binom_range() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><f>BINOM.DIST.RANGE(4,0.5,A1,2)</f><v>0</v></c><c r="C1"><f>BINOM.DIST.RANGE(4,0.5,2)</f><v>0</v></c><c r="D1"><f>BINOM.DIST.RANGE(4.9,0.5,1.9,2.2)</f><v>0</v></c><c r="E1"><f>BINOM.DIST.RANGE(4,0.5,3,1)</f><v>7</v></c><c r="F1"><f>BINOM.DIST.RANGE(4,0.5,5,5)</f><v>8</v></c><c r="G1"><f>BINOM.DIST.RANGE(4,1.5,1,2)</f><v>9</v></c><c r="H1"><f>BINOM.DIST.RANGE(171,0.5,0,1)</f><v>10</v></c><c r="I1"><f>BINOM.DIST.RANGE(4,0.5)</f><v>11</v></c><c r="J1"><f>BINOM.DIST.RANGE(&quot;ab&quot;,0.5,1,2)</f><v>12</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>BINOM.DIST.RANGE(4,0.5,A1,2)</f><v>0.625</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST.RANGE(4,0.5,2)</f><v>0.375</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST.RANGE(4.9,0.5,1.9,2.2)</f><v>0.625</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST.RANGE(4,0.5,3,1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST.RANGE(4,0.5,5,5)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST.RANGE(4,1.5,1,2)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST.RANGE(171,0.5,0,1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST.RANGE(4,0.5)</f><v>11</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>BINOM.DIST.RANGE(&quot;ab&quot;,0.5,1,2)</f><v>12</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_z_test() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>1</v></c><c r="C1"><v>1</v></c><c r="D1"><v>1</v></c><c r="E1"><f>Z.TEST(A1:D1,0,2)</f><v>0</v></c><c r="F1"><f>Z.TEST(A1:D1,1)</f><v>7</v></c><c r="G1"><f>ZTEST(A2:C2,0)</f><v>0</v></c><c r="H1"><f>Z.TEST(A1,0,1)</f><v>8</v></c><c r="I1"><f>Z.TEST(A1:D1,0,0)</f><v>9</v></c><c r="J1"><f>Z.TEST(A1:D1,&quot;ab&quot;,1)</f><v>10</v></c></row><row r="2"><c r="A2"><v>1</v></c><c r="B2"><v>2</v></c><c r="C2"><v>3</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>Z.TEST(A1:D1,0,2)</f><v>0.15865525</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>Z.TEST(A1:D1,1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ZTEST(A2:C2,0)</f><v>0.000266</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>Z.TEST(A1,0,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>Z.TEST(A1:D1,0,0)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>Z.TEST(A1:D1,&quot;ab&quot;,1)</f><v>10</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_prob() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c><c r="C1"><v>3</v></c><c r="D1"><f>PROB(A1:C1,A2:C2,2)</f><v>0</v></c><c r="E1"><f>PROB(A1:C1,A2:C2,1,2)</f><v>0</v></c><c r="F1"><f>PROB(A1:C1,A2:C2,4)</f><v>0</v></c><c r="G1"><f>PROB(A1:C1,A2:C2,2,1)</f><v>7</v></c><c r="H1"><f>PROB(A1:C1,D2:F2,1)</f><v>8</v></c><c r="I1"><f>PROB(A1,A2,1)</f><v>9</v></c><c r="J1"><f>PROB(A1:C1,A2:C2,&quot;ab&quot;)</f><v>10</v></c></row><row r="2"><c r="A2"><v>0.2</v></c><c r="B2"><v>0</v></c><c r="C2"><v>0.3</v></c><c r="D2"><v>-0.1</v></c><c r="E2"><v>0.2</v></c><c r="F2"><v>0.3</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "B2", "0.5").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>PROB(A1:C1,A2:C2,2)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PROB(A1:C1,A2:C2,1,2)</f><v>0.7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PROB(A1:C1,A2:C2,4)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PROB(A1:C1,A2:C2,2,1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PROB(A1:C1,D2:F2,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>PROB(A1,A2,1)</f><v>9</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>PROB(A1:C1,A2:C2,&quot;ab&quot;)</f><v>10</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_inverse_hyperbolic() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>0.5</v></c><c r="C1"><v>-1</v></c><c r="D1"><f>ASINH(A1)</f><v>9</v></c><c r="E1"><f>ACOSH(A1)</f><v>8</v></c><c r="F1"><f>ATANH(B1)</f><v>7</v></c><c r="G1"><f>ASINH(C1)</f><v>6</v></c><c r="H1"><f>ACOSH(0.5)</f><v>5</v></c><c r="I1"><f>ATANH(1)</f><v>4</v></c><c r="J1"><f>ATANH(-1)</f><v>3</v></c><c r="K1"><f>ATANH(2)</f><v>2</v></c><c r="L1"><f>ASINH(&quot;ab&quot;)</f><v>1</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>ASINH(A1)</f><v>0.88137359</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ACOSH(A1)</f><v>0</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>ATANH(B1)</f><v>0.54930614</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ASINH(C1)</f><v>-0.88137359</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ACOSH(0.5)</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ATANH(1)</f><v>4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ATANH(-1)</f><v>3</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ATANH(2)</f><v>2</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>ASINH(&quot;ab&quot;)</f><v>1</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_reciprocal_trig() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>SEC(A1)</f><v>9</v></c><c r="C1"><f>CSC(A1)</f><v>8</v></c><c r="D1"><f>COT(A1)</f><v>7</v></c><c r="E1"><f>SEC(0)</f><v>6</v></c><c r="F1"><f>CSC(0)</f><v>5</v></c><c r="G1"><f>COT(0)</f><v>4</v></c><c r="H1"><f>SEC(&quot;ab&quot;)</f><v>3</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SEC(A1)</f><v>1.85081572</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CSC(A1)</f><v>1.18839511</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COT(A1)</f><v>0.64209262</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SEC(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>CSC(0)</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COT(0)</f><v>4</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SEC(&quot;ab&quot;)</f><v>3</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_unichar() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>64</v></c><c r="B1"><f>CONCATENATE("a","b")</f><v>0</v></c><c r="C1"><f>UNICHAR(A1)</f><v>0</v></c><c r="D1"><f>UNICODE("AB")</f><v>0</v></c><c r="E1"><f>CONCATENATE(A1,"x")</f><v>0</v></c><c r="F1"><f>UNICHAR(0)</f><v>4</v></c><c r="G1"><f>UNICHAR(55296)</f><v>5</v></c><c r="H1"><f>UNICODE("")</f><v>6</v></c><c r="I1"><f>UNICHAR("ab")</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "65").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(
                r#"<c r="B1" t="inlineStr"><f>CONCATENATE("a","b")</f><is><t>ab</t></is></c>"#
            ),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="C1" t="inlineStr"><f>UNICHAR(A1)</f><is><t>A</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>UNICODE("AB")</f><v>65</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(
                r#"<c r="E1" t="inlineStr"><f>CONCATENATE(A1,"x")</f><is><t>65x</t></is></c>"#
            ),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>UNICHAR(0)</f><v>4</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>UNICHAR(55296)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>UNICODE("")</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>UNICHAR("ab")</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_series_sum() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>0</v></c><c r="C1"><v>3</v></c><c r="D1"><f>SERIESSUM(2,1,1,A1:C1)</f><v>0</v></c><c r="E1"><f>SERIESSUM(2,1,1,A2:C2)</f><v>0</v></c><c r="F1"><f>SERIESSUM(-2,0.5,1,A1:A1)</f><v>7</v></c><c r="G1"><f>SERIESSUM(0,0,1,A1:A1)</f><v>0</v></c><c r="H1"><f>SERIESSUM(2,-1,1,A1:A1)</f><v>0</v></c><c r="I1"><f>SERIESSUM(2,1,1,A1)</f><v>8</v></c><c r="J1"><f>SERIESSUM("ab",1,1,A1:C1)</f><v>9</v></c><c r="K1"><f>SERIESSUM(2,1,1,D2:F2)</f><v>10</v></c></row><row r="2"><c r="A2"><v>1</v></c><c r="C2"><v>3</v></c><c r="D2"><v>1</v></c><c r="E2" t="inlineStr"><is><t>xy</t></is></c><c r="F2"><v>3</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "B1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SERIESSUM(2,1,1,A1:C1)</f><v>34</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SERIESSUM(2,1,1,A2:C2)</f><v>26</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SERIESSUM(-2,0.5,1,A1:A1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SERIESSUM(0,0,1,A1:A1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SERIESSUM(2,-1,1,A1:A1)</f><v>0.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SERIESSUM(2,1,1,A1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SERIESSUM("ab",1,1,A1:C1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SERIESSUM(2,1,1,D2:F2)</f><v>10</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_norm_inv() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0.5</v></c><c r="B1"><f>NORMSINV(A1)</f><v>0</v></c><c r="C1"><f>NORM.S.INV(0.5)</f><v>9</v></c><c r="D1"><f>NORM.INV(A1,10,2)</f><v>0</v></c><c r="E1"><f>NORMINV(0.5,10,2)</f><v>0</v></c><c r="F1"><f>NORMSINV(0.025)</f><v>0</v></c><c r="G1"><f>NORMSINV(0)</f><v>4</v></c><c r="H1"><f>NORMSINV(1)</f><v>5</v></c><c r="I1"><f>NORM.INV(0.5,10,0)</f><v>6</v></c><c r="J1"><f>NORM.INV(0.5,10,-1)</f><v>7</v></c><c r="K1"><f>NORMSINV(&quot;ab&quot;)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "0.975").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>NORMSINV(A1)</f><v>1.95996398</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.S.INV(0.5)</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.INV(A1,10,2)</f><v>13.91992797</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORMINV(0.5,10,2)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORMSINV(0.025)</f><v>-1.95996398</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>NORMSINV(0)</f><v>4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>NORMSINV(1)</f><v>5</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>NORM.INV(0.5,10,0)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORM.INV(0.5,10,-1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>NORMSINV(&quot;ab&quot;)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_lognorm_inv() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>LOGNORM.INV(0.5,A1,1)</f><v>0</v></c><c r="C1"><f>LOGINV(0.5,0,1)</f><v>0</v></c><c r="D1"><f>LOGNORM.INV(0.975,0,1)</f><v>0</v></c><c r="E1"><f>CONFIDENCE(0.05,1,1)</f><v>0</v></c><c r="F1"><f>CONFIDENCE.NORM(0.05,2,4)</f><v>0</v></c><c r="G1"><f>CONFIDENCE(0.05,1,1.9)</f><v>0</v></c><c r="H1"><f>CONFIDENCE(0,1,1)</f><v>4</v></c><c r="I1"><f>CONFIDENCE(1,1,1)</f><v>5</v></c><c r="J1"><f>CONFIDENCE(0.05,0,1)</f><v>6</v></c><c r="K1"><f>CONFIDENCE(0.05,1,0.9)</f><v>7</v></c><c r="L1"><f>LOGNORM.INV(0,0,1)</f><v>8</v></c><c r="M1"><f>LOGNORM.INV(0.5,0,0)</f><v>9</v></c><c r="N1"><f>CONFIDENCE(&quot;ab&quot;,1,1)</f><v>10</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>LOGNORM.INV(0.5,A1,1)</f><v>2.71828183</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGINV(0.5,0,1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.INV(0.975,0,1)</f><v>7.09907138</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONFIDENCE(0.05,1,1)</f><v>1.95996398</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONFIDENCE.NORM(0.05,2,4)</f><v>1.95996398</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONFIDENCE(0.05,1,1.9)</f><v>1.95996398</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONFIDENCE(0,1,1)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONFIDENCE(1,1,1)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONFIDENCE(0.05,0,1)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONFIDENCE(0.05,1,0.9)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.INV(0,0,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>LOGNORM.INV(0.5,0,0)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CONFIDENCE(&quot;ab&quot;,1,1)</f><v>10</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_gamma_inv() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>GAMMA.INV(A1,1,1)</f><v>0</v></c><c r="C1"><f>GAMMAINV(0.5,1,2)</f><v>0</v></c><c r="D1"><f>CHISQ.INV(A1,2)</f><v>0</v></c><c r="E1"><f>CHISQ.INV(0.5,2.9)</f><v>0</v></c><c r="F1"><f>CHISQ.INV.RT(0.5,2)</f><v>0</v></c><c r="G1"><f>CHIINV(0.05,2)</f><v>0</v></c><c r="H1"><f>GAMMA.INV(0,1,1)</f><v>4</v></c><c r="I1"><f>GAMMA.INV(1,1,1)</f><v>5</v></c><c r="J1"><f>GAMMA.INV(0.5,0,1)</f><v>6</v></c><c r="K1"><f>CHISQ.INV(0.5,0.9)</f><v>7</v></c><c r="L1"><f>CHISQ.INV(&quot;ab&quot;,2)</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "0.5").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>GAMMA.INV(A1,1,1)</f><v>0.69314718</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMAINV(0.5,1,2)</f><v>1.38629436</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.INV(A1,2)</f><v>1.38629436</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.INV(0.5,2.9)</f><v>1.38629436</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.INV.RT(0.5,2)</f><v>1.38629436</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHIINV(0.05,2)</f><v>5.99146455</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.INV(0,1,1)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.INV(1,1,1)</f><v>5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>GAMMA.INV(0.5,0,1)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.INV(0.5,0.9)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.INV(&quot;ab&quot;,2)</f><v>8</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_roman() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>3</v></c><c r="B1"><f>ROMAN(A1)</f><v>0</v></c><c r="C1"><f>ROMAN(9)</f><v>0</v></c><c r="D1"><f>ROMAN(1990)</f><v>0</v></c><c r="E1"><f>ROMAN(3.9)</f><v>0</v></c><c r="F1"><f>ROMAN(0)</f><v>5</v></c><c r="G1"><f>ROMAN(4000)</f><v>6</v></c><c r="H1"><f>ROMAN(4,1)</f><v>7</v></c><c r="I1"><f>ROMAN(4,0)</f><v>0</v></c><c r="J1"><f>ARABIC("IV")</f><v>0</v></c><c r="K1"><f>ARABIC("ii")</f><v>0</v></c><c r="L1"><f>ARABIC("MCMXC")</f><v>0</v></c><c r="M1"><f>ARABIC("IIII")</f><v>8</v></c><c r="N1"><f>ARABIC("")</f><v>9</v></c><c r="O1"><f>ROMAN("ab")</f><v>10</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "4").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<c r="B1" t="inlineStr"><f>ROMAN(A1)</f><is><t>IV</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="C1" t="inlineStr"><f>ROMAN(9)</f><is><t>IX</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet
                .contains(r#"<c r="D1" t="inlineStr"><f>ROMAN(1990)</f><is><t>MCMXC</t></is></c>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<c r="E1" t="inlineStr"><f>ROMAN(3.9)</f><is><t>III</t></is></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ROMAN(0)</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ROMAN(4000)</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ROMAN(4,1)</f><v>7</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<c r="I1" t="inlineStr"><f>ROMAN(4,0)</f><is><t>IV</t></is></c>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ARABIC("IV")</f><v>4</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ARABIC("ii")</f><v>2</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>ARABIC("MCMXC")</f><v>1990</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ARABIC("IIII")</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ARABIC("")</f><v>9</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ROMAN("ab")</f><v>10</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_reciprocal_hyper() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>SECH(A1)</f><v>0</v></c><c r="C1"><f>CSCH(A1)</f><v>0</v></c><c r="D1"><f>COTH(A1)</f><v>0</v></c><c r="E1"><f>SECH(0)</f><v>0</v></c><c r="F1"><f>CSCH(0)</f><v>7</v></c><c r="G1"><f>COTH(0)</f><v>8</v></c><c r="H1"><f>SECH(&quot;ab&quot;)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SECH(A1)</f><v>0.64805427</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CSCH(A1)</f><v>0.85091813</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>COTH(A1)</f><v>1.31303529</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SECH(0)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>CSCH(0)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>COTH(0)</f><v>8</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>SECH(&quot;ab&quot;)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_acot() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><f>ACOT(A1)</f><v>0</v></c><c r="C1"><f>ACOT(0)</f><v>0</v></c><c r="D1"><f>ACOT(-1)</f><v>0</v></c><c r="E1"><f>ACOTH(2)</f><v>0</v></c><c r="F1"><f>ACOTH(-2)</f><v>0</v></c><c r="G1"><f>ACOTH(1)</f><v>7</v></c><c r="H1"><f>ACOTH(0.5)</f><v>8</v></c><c r="I1"><f>ACOT(&quot;ab&quot;)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>ACOT(A1)</f><v>0.78539816</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ACOT(0)</f><v>1.57079633</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ACOT(-1)</f><v>2.35619449</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ACOTH(2)</f><v>0.54930614</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>ACOTH(-2)</f><v>-0.54930614</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>ACOTH(1)</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>ACOTH(0.5)</f><v>8</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>ACOT(&quot;ab&quot;)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_chisq_test() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>0</v></c><c r="B1"><v>2</v></c><c r="C1"><v>2</v></c><c r="D1"><v>1</v></c><c r="E1"><f>CHISQ.TEST(A1:B1,C1:D1)</f><v>0</v></c><c r="F1"><f>CHITEST(A1:B1,A1:B1)</f><v>0</v></c><c r="G1"><f>CHISQ.TEST(A2:B3,C2:D3)</f><v>0</v></c><c r="H1"><f>CHISQ.TEST(A1:A1,C1:C1)</f><v>7</v></c><c r="I1"><f>CHISQ.TEST(A1:B1,A2:A3)</f><v>8</v></c><c r="J1"><f>CHISQ.TEST(A1:B1,E2:F2)</f><v>9</v></c><c r="K1"><f>CHISQ.TEST(A1:B1,G2:H2)</f><v>10</v></c><c r="L1"><f>CHISQ.TEST(1,2)</f><v>11</v></c></row><row r="2"><c r="A2"><v>1</v></c><c r="B2"><v>2</v></c><c r="C2"><v>1</v></c><c r="D2"><v>2</v></c><c r="E2"><v>0</v></c><c r="F2"><v>1</v></c><c r="G2" t="inlineStr"><is><t>xy</t></is></c><c r="H2"><v>1</v></c></row><row r="3"><c r="A3"><v>3</v></c><c r="B3"><v>4</v></c><c r="C3"><v>3</v></c><c r="D3"><v>4</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "1").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>CHISQ.TEST(A1:B1,C1:D1)</f><v>0.22067136</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHITEST(A1:B1,A1:B1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.TEST(A2:B3,C2:D3)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.TEST(A1:A1,C1:C1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.TEST(A1:B1,A2:A3)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.TEST(A1:B1,E2:F2)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.TEST(A1:B1,G2:H2)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>CHISQ.TEST(1,2)</f><v>11</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_percentile_exc() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>20</v></c><c r="C1"><v>30</v></c><c r="D1"><v>40</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><f>PERCENTILE.EXC(A1:E1,0.25)</f><v>0</v></c><c r="G1"><f>PERCENTILE.EXC(A1:E1,0.5)</f><v>0</v></c><c r="H1"><f>PERCENTILE.EXC(A1:E1,0.75)</f><v>0</v></c><c r="I1"><f>PERCENTILE.EXC(A1:E1,0.2)</f><v>0</v></c><c r="J1"><f>PERCENTILE.EXC(A1:E1,0.8)</f><v>0</v></c><c r="K1"><f>PERCENTILE.EXC(A1:E1,0)</f><v>7</v></c><c r="L1"><f>PERCENTILE.EXC(A1:E1,1)</f><v>8</v></c><c r="M1"><f>PERCENTILE.EXC(A1:E1,0.1)</f><v>9</v></c><c r="N1"><f>PERCENTILE.EXC(A1,0.5)</f><v>11</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "10").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>PERCENTILE.EXC(A1:E1,0.25)</f><v>12.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE.EXC(A1:E1,0.5)</f><v>25</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE.EXC(A1:E1,0.75)</f><v>37.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE.EXC(A1:E1,0.2)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE.EXC(A1:E1,0.8)</f><v>40</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE.EXC(A1:E1,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE.EXC(A1:E1,1)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE.EXC(A1:E1,0.1)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>PERCENTILE.EXC(A1,0.5)</f><v>11</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_quartile_exc() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>20</v></c><c r="C1"><v>30</v></c><c r="D1"><v>40</v></c><c r="E1" t="inlineStr"><is><t>xy</t></is></c><c r="F1"><f>QUARTILE.EXC(A1:E1,1)</f><v>0</v></c><c r="G1"><f>QUARTILE.EXC(A1:E1,1.9)</f><v>0</v></c><c r="H1"><f>QUARTILE.EXC(A1:E1,2)</f><v>0</v></c><c r="I1"><f>QUARTILE.EXC(A1:E1,3)</f><v>0</v></c><c r="J1"><f>QUARTILE.EXC(A1:E1,0)</f><v>7</v></c><c r="K1"><f>QUARTILE.EXC(A1:E1,4)</f><v>8</v></c><c r="L1"><f>QUARTILE.EXC(A1:E1,5)</f><v>9</v></c><c r="M1"><f>QUARTILE.EXC(A1,1)</f><v>11</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "10").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>QUARTILE.EXC(A1:E1,1)</f><v>12.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE.EXC(A1:E1,1.9)</f><v>12.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE.EXC(A1:E1,2)</f><v>25</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE.EXC(A1:E1,3)</f><v>37.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE.EXC(A1:E1,0)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE.EXC(A1:E1,4)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE.EXC(A1:E1,5)</f><v>9</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>QUARTILE.EXC(A1,1)</f><v>11</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_averageif() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1"><v>3</v></c><c r="D1"><f>AVERAGEIF(A1:C1,"&gt;2")</f><v>0</v></c><c r="E1"><f>AVERAGEIF(A1:C1,8)</f><v>0</v></c><c r="F1"><f>AVERAGEIF(A1:C1,"&lt;0")</f><v>7</v></c><c r="G1"><f>AVERAGEIF(A1:C1,"ab")</f><v>8</v></c><c r="H1"><f>AVERAGEIF(A1:C1,"&gt;2",A1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>AVERAGEIF(A1:C1,"&gt;2")</f><v>5.5</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AVERAGEIF(A1:C1,8)</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AVERAGEIF(A1:C1,"&lt;0")</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AVERAGEIF(A1:C1,"ab")</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AVERAGEIF(A1:C1,"&gt;2",A1)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_sumproduct() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1" t="inlineStr"><is><t>ab</t></is></c><c r="D1"><v>3</v></c><c r="E1"><v>4</v></c><c r="F1"><f>SUMPRODUCT(A1:C1)</f><v>0</v></c><c r="G1"><f>SUMPRODUCT(A1:B1,D1:E1)</f><v>0</v></c><c r="H1"><f>SUMPRODUCT(A1:C1,D1:E1)</f><v>7</v></c><c r="I1"><f>SUMPRODUCT()</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SUMPRODUCT(A1:C1)</f><v>10</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMPRODUCT(A1:B1,D1:E1)</f><v>38</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMPRODUCT(A1:C1,D1:E1)</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>SUMPRODUCT()</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_minifs_and_maxifs() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1"><v>3</v></c><c r="D1"><v>10</v></c><c r="E1"><v>20</v></c><c r="F1"><v>30</v></c><c r="G1"><f>MINIFS(D1:F1,A1:C1,"&gt;2")</f><v>0</v></c><c r="H1"><f>MAXIFS(D1:F1,A1:C1,"&gt;2")</f><v>0</v></c><c r="I1"><f>MINIFS(D1:F1,A1:C1,"&lt;0")</f><v>0</v></c><c r="J1"><f>MINIFS(D1:E1,A1:C1,"&gt;2")</f><v>7</v></c><c r="K1"><f>MINIFS(D1:F1,A1:C1,"ab")</f><v>8</v></c><c r="L1"><f>MINIFS(D1:F1,A1:C1,"&gt;2",A1:C1,1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>MINIFS(D1:F1,A1:C1,"&gt;2")</f><v>20</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MAXIFS(D1:F1,A1:C1,"&gt;2")</f><v>30</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MINIFS(D1:F1,A1:C1,"&lt;0")</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MINIFS(D1:E1,A1:C1,"&gt;2")</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MINIFS(D1:F1,A1:C1,"ab")</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>MINIFS(D1:F1,A1:C1,"&gt;2",A1:C1,1)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_sumifs() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1"><v>3</v></c><c r="D1"><v>10</v></c><c r="E1"><v>20</v></c><c r="F1"><v>30</v></c><c r="G1"><f>SUMIFS(D1:F1,A1:C1,"&gt;2")</f><v>0</v></c><c r="H1"><f>SUMIFS(D1:F1,A1:C1,"&lt;0")</f><v>0</v></c><c r="I1"><f>SUMIFS(D1:E1,A1:C1,"&gt;2")</f><v>7</v></c><c r="J1"><f>SUMIFS(D1:F1,A1:C1,"ab")</f><v>8</v></c><c r="K1"><f>SUMIFS(D1:F1,A1:C1,"&gt;2",A1:C1,1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>SUMIFS(D1:F1,A1:C1,"&gt;2")</f><v>50</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIFS(D1:F1,A1:C1,"&lt;0")</f><v>0</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIFS(D1:E1,A1:C1,"&gt;2")</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIFS(D1:F1,A1:C1,"ab")</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>SUMIFS(D1:F1,A1:C1,"&gt;2",A1:C1,1)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_averageifs() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1"><v>3</v></c><c r="D1"><v>10</v></c><c r="E1"><v>20</v></c><c r="F1"><v>30</v></c><c r="G1"><f>AVERAGEIFS(D1:F1,A1:C1,"&gt;2")</f><v>0</v></c><c r="H1"><f>AVERAGEIFS(D1:F1,A1:C1,"&lt;0")</f><v>4</v></c><c r="I1"><f>AVERAGEIFS(D1:E1,A1:C1,"&gt;2")</f><v>7</v></c><c r="J1"><f>AVERAGEIFS(D1:F1,A1:C1,"ab")</f><v>8</v></c><c r="K1"><f>AVERAGEIFS(D1:F1,A1:C1,"&gt;2",A1:C1,1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>AVERAGEIFS(D1:F1,A1:C1,"&gt;2")</f><v>25</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AVERAGEIFS(D1:F1,A1:C1,"&lt;0")</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AVERAGEIFS(D1:E1,A1:C1,"&gt;2")</f><v>7</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AVERAGEIFS(D1:F1,A1:C1,"ab")</f><v>8</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>AVERAGEIFS(D1:F1,A1:C1,"&gt;2",A1:C1,1)</f><v>9</v>"#),
            "{sheet}"
        );
    }

    #[test]
    fn set_sheet_cell_sumsq() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1"><v>3</v></c><c r="D1" t="inlineStr"><is><t>xy</t></is></c><c r="E1"><v>1e200</v></c><c r="F1"><f>SUMSQ(A1:D1)</f><v>0</v></c><c r="G1"><f>SUMSQ()</f><v>5</v></c><c r="H1"><f>SUMSQ(E1)</f><v>6</v></c><c r="I1"><f>SUMSQ("ab")</f><v>7</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>SUMSQ(A1:D1)</f><v>77</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SUMSQ()</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SUMSQ(E1)</f><v>6</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>SUMSQ("ab")</f><v>7</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_stdev() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>4</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><f>STDEV.S(A1:C1)</f><v>0</v></c><c r="E1"><f>STDEV(A1:B1)</f><v>0</v></c><c r="F1"><f>STDEV.P(A1:B1)</f><v>0</v></c><c r="G1"><f>STDEVP(A1:B1)</f><v>0</v></c><c r="H1"><f>STDEV.S(A1)</f><v>5</v></c><c r="I1"><f>STDEV.P(A1)</f><v>6</v></c><c r="J1"><f>STDEV.S()</f><v>7</v></c><c r="K1"><f>STDEV.S("ab")</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>STDEV.S(A1:C1)</f><v>1.41421356</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STDEV(A1:B1)</f><v>1.41421356</v>"#),
            "{sheet}"
        );
        assert!(
            sheet.contains(r#"<f>STDEV.P(A1:B1)</f><v>1</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>STDEVP(A1:B1)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>STDEV.S(A1)</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>STDEV.P(A1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>STDEV.S()</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>STDEV.S("ab")</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_var() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>4</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><f>VAR.S(A1:C1)</f><v>0</v></c><c r="E1"><f>VAR(A1:B1)</f><v>0</v></c><c r="F1"><f>VAR.P(A1:B1)</f><v>0</v></c><c r="G1"><f>VARP(A1:B1)</f><v>0</v></c><c r="H1"><f>VAR.S(A1)</f><v>5</v></c><c r="I1"><f>VAR.P(A1)</f><v>6</v></c><c r="J1"><f>VAR.S()</f><v>7</v></c><c r="K1"><f>VAR.S("ab")</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>VAR.S(A1:C1)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>VAR(A1:B1)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>VAR.P(A1:B1)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>VARP(A1:B1)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>VAR.S(A1)</f><v>5</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>VAR.P(A1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>VAR.S()</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>VAR.S("ab")</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_avedev() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>4</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><f>AVEDEV(A1:C1)</f><v>0</v></c><c r="E1"><f>AVEDEV(A1)</f><v>9</v></c><c r="F1"><f>AVEDEV()</f><v>7</v></c><c r="G1"><f>AVEDEV("ab")</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>AVEDEV(A1:C1)</f><v>1</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>AVEDEV(A1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>AVEDEV()</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>AVEDEV("ab")</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_devsq() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>4</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><f>DEVSQ(A1:C1)</f><v>0</v></c><c r="E1"><f>DEVSQ(A1)</f><v>9</v></c><c r="F1"><f>DEVSQ()</f><v>7</v></c><c r="G1"><f>DEVSQ("ab")</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(sheet.contains(r#"<f>DEVSQ(A1:C1)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DEVSQ(A1)</f><v>0</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DEVSQ()</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>DEVSQ("ab")</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_geomean() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><v>0</v></c><c r="E1"><v>-3</v></c><c r="F1"><f>GEOMEAN(A1:C1)</f><v>0</v></c><c r="G1"><f>GEOMEAN(A1)</f><v>0</v></c><c r="H1"><f>GEOMEAN(D1)</f><v>5</v></c><c r="I1"><f>GEOMEAN(A1,E1)</f><v>6</v></c><c r="J1"><f>GEOMEAN()</f><v>7</v></c><c r="K1"><f>GEOMEAN("ab")</f><v>8</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>GEOMEAN(A1:C1)</f><v>4</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>GEOMEAN(A1)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GEOMEAN(D1)</f><v>5</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>GEOMEAN(A1,E1)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>GEOMEAN()</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>GEOMEAN("ab")</f><v>8</v>"#), "{sheet}");
    }

    #[test]
    fn set_sheet_cell_harmean() {
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
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>8</v></c><c r="C1" t="inlineStr"><is><t>xy</t></is></c><c r="D1"><v>0</v></c><c r="E1"><v>-3</v></c><c r="L1"><v>1e-320</v></c><c r="F1"><f>HARMEAN(A1:C1)</f><v>0</v></c><c r="G1"><f>HARMEAN(A1)</f><v>0</v></c><c r="H1"><f>HARMEAN(D1)</f><v>5</v></c><c r="I1"><f>HARMEAN(A1,E1)</f><v>6</v></c><c r="J1"><f>HARMEAN()</f><v>7</v></c><c r="K1"><f>HARMEAN("ab")</f><v>8</v></c><c r="M1"><f>HARMEAN(L1)</f><v>9</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let saved = set_sheet_cell(&bytes, "Budgets", "A1", "2").unwrap();
        let mut archive = ZipArchive::new(Cursor::new(saved)).unwrap();
        let sheet = read_entry(&mut archive, "xl/worksheets/sheet1.xml").unwrap();
        assert!(
            sheet.contains(r#"<f>HARMEAN(A1:C1)</f><v>3.2</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>HARMEAN(A1)</f><v>2</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>HARMEAN(D1)</f><v>5</v>"#), "{sheet}");
        assert!(
            sheet.contains(r#"<f>HARMEAN(A1,E1)</f><v>6</v>"#),
            "{sheet}"
        );
        assert!(sheet.contains(r#"<f>HARMEAN()</f><v>7</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>HARMEAN("ab")</f><v>8</v>"#), "{sheet}");
        assert!(sheet.contains(r#"<f>HARMEAN(L1)</f><v>9</v>"#), "{sheet}");
    }

    #[tokio::test]
    async fn edit_cell_writes_the_workbook_and_refuses_a_formula() {
        let drawing = "<drawing>keep-me</drawing>";
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
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1"><v>42</v></c><c r="C1"><f>1+1</f><v>2</v></c></row></sheetData></worksheet>"#,
            ),
            ("xl/drawings/drawing1.xml", drawing),
        ]);
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("book.xlsx");
        std::fs::write(&file, bytes).expect("write");
        let fs_path = orchid_fs::FsPath::from_local(&file).expect("fs path");
        let registry = Arc::new(orchid_fs::FsProviderRegistry::new());
        registry
            .register(Arc::new(orchid_fs::LocalProvider::new()))
            .expect("register local");
        let mut viewer = OfficeViewer::new();
        viewer
            .open(fs_path, Arc::clone(&registry))
            .await
            .expect("open");
        let err = viewer
            .edit_cell(Arc::clone(&registry), "Budgets", "C1", "9")
            .await
            .expect_err("formula");
        assert_eq!(err.to_string(), "viewer-sheet-formula");
        viewer
            .edit_cell(Arc::clone(&registry), "Budgets", "B1", "7")
            .await
            .expect("save");
        let first = viewer.snapshot();
        let second = viewer.snapshot();
        match (&first, &second) {
            (ViewerSnapshot::Sheet(a), ViewerSnapshot::Sheet(b)) => {
                assert!(Arc::ptr_eq(&a.sheets, &b.sheets));
                assert_eq!(a.sheets[0].rows[0][0].text, "Orchid");
                assert_eq!(a.sheets[0].rows[0][1].text, "7");
                assert_eq!(a.sheets[0].rows[0][2].text, "2");
            }
            other => panic!("expected a sheet snapshot, got {other:?}"),
        }
        let on_disk = std::fs::read(&file).expect("reread");
        let preview = render_office(&on_disk, false).expect("preview");
        let OfficePreview::Sheets(book) = preview else {
            panic!("workbook should be a sheet table");
        };
        assert_eq!(book.sheets[0].rows[0][1].text, "7");
        let mut archive = ZipArchive::new(Cursor::new(on_disk)).unwrap();
        let kept = read_entry(&mut archive, "xl/drawings/drawing1.xml").unwrap();
        assert_eq!(kept, drawing);
        assert!(!dir.path().join("book.xlsx.orchid-save").exists());
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
