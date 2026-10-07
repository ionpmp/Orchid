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
    let xml = replace_cell_xml(&xml, &address, text)?;
    let xml = recalculate_sheet(&xml);
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

fn recalculate_sheet(xml: &str) -> String {
    let cells = sheet_cells(xml);
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
    for cell in &cells {
        let Some(formula) = &cell.formula else {
            continue;
        };
        if cell.text_cell {
            continue;
        }
        let mut visiting = std::collections::HashSet::new();
        let Some(value) = eval_formula(formula, &formulas, &literals, &texts, &mut visiting) else {
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

fn sheet_cells(xml: &str) -> Vec<SheetCellRef> {
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
                    cells.push(SheetCellRef {
                        address,
                        formula,
                        value,
                        text: if text_cell { inline_text(body) } else { None },
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
    visiting: &'a mut std::collections::HashSet<String>,
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
            if word.eq_ignore_ascii_case("SIN")
                || word.eq_ignore_ascii_case("COS")
                || word.eq_ignore_ascii_case("TAN")
            {
                let kind = word.to_ascii_uppercase();
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                return trig_excel(number, &kind).map(CalcValue::Num);
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
            if word.eq_ignore_ascii_case("COMBIN") {
                let n = calc_num(self.compare(env)?)?;
                let k = self.comma_number(env)?;
                return combin_excel(n, k).map(CalcValue::Num);
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
            if word.eq_ignore_ascii_case("CODE") {
                let text = calc_text(&self.compare(env)?);
                self.close_paren()?;
                return code_excel(&text).map(CalcValue::Num);
            }
            if word.eq_ignore_ascii_case("CHAR") {
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
            if word.eq_ignore_ascii_case("CEILING.MATH") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                if !number.is_finite() {
                    return None;
                }
                return Some(CalcValue::Num(number.ceil()));
            }
            if word.eq_ignore_ascii_case("FLOOR.MATH") {
                let number = calc_num(self.compare(env)?)?;
                self.close_paren()?;
                if !number.is_finite() {
                    return None;
                }
                return Some(CalcValue::Num(number.floor()));
            }
            if word.eq_ignore_ascii_case("CONCAT") {
                return self.concat_args(env).map(CalcValue::Text);
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
                _ => None,
            };
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
                        if let Some(CalcValue::Num(value)) = self.cell_value(&address, env) {
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
        let start = self.cell_token()?;
        self.skip();
        if self.bytes.get(self.index) != Some(&b':') {
            return None;
        }
        self.index += 1;
        self.skip();
        let end = self.cell_token()?;
        self.require_comma()?;
        let criteria = self.compare(env)?;
        let (op, target) = compile_criterion(&criteria)?;
        self.skip();
        if self.bytes.get(self.index) != Some(&b')') {
            return None;
        }
        self.index += 1;
        let cells = cells_in_range(&start, &end)?;
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

    fn cell_range(&mut self) -> Option<Vec<String>> {
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

    fn rank_call(&mut self, env: &mut CalcEnv<'_>, average: bool) -> Option<f64> {
        let number = calc_num(self.compare(env)?)?;
        self.require_comma()?;
        let cells = self.cell_range()?;
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
        let cells = self.cell_range()?;
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

    /// Y range then X range, same length. A pair is kept when both cells are finite numbers.
    fn paired_ranges(&mut self, env: &mut CalcEnv<'_>) -> Option<Vec<(f64, f64)>> {
        let ys = self.cell_range()?;
        self.require_comma()?;
        let xs = self.cell_range()?;
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
        let values = self.cell_range()?;
        self.require_comma()?;
        let criteria_cells = self.cell_range()?;
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
            let start = self.cell_token()?;
            self.skip();
            if self.bytes.get(self.index) != Some(&b':') {
                return None;
            }
            self.index += 1;
            self.skip();
            let end = self.cell_token()?;
            let cells = cells_in_range(&start, &end)?;
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
                    .is_some_and(|next| next.is_ascii_alphabetic())
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

    fn cell_value(&self, address: &str, env: &mut CalcEnv<'_>) -> Option<CalcValue> {
        let address = address.to_ascii_uppercase();
        if !env.visiting.insert(address.clone()) {
            return None;
        }
        let value = if let Some(formula) = env.formulas.get(&address) {
            eval_formula(formula, env.formulas, env.literals, env.texts, env.visiting)
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
        assert!(sheet.contains(r#"<f>ROMAN(A1)</f><v>9</v>"#), "{sheet}");
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
        assert!(sheet.contains(r#"<f>ROMAN(A1)</f><v>9</v>"#), "{sheet}");
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
            sheet.contains(r#"<f>CEILING.MATH(1.2,1)</f><v>8</v>"#),
            "{sheet}"
        );
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
