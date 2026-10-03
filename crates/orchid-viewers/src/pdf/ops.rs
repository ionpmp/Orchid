//! Document-level PDF operations that run off the raster worker:
//! outline extraction, full-document search, highlight export, sticky notes,
//! and AcroForm fill-in.

use std::path::{Path, PathBuf};

use pdfium_render::prelude::*;

use crate::error::{Result, ViewerError};
use crate::image::export::unique_export_dest;
use crate::snapshot::PdfOutlineItem;

use super::bindings::with_pdfium;
use super::layer::PtsRect;

/// One search hit: 1-based page plus PDF-space rects for the match.
#[derive(Debug, Clone)]
pub struct FindHit {
    /// 1-based page index.
    pub page: u32,
    /// Segment bounds in PDF points.
    pub rects: Vec<PtsRect>,
}

/// Walk bookmarks into a flat outline (depth via `parent()`).
///
/// # Errors
///
/// Pdfium bind / load failures.
pub fn extract_outline(bytes: &[u8]) -> Result<Vec<PdfOutlineItem>> {
    with_pdfium(|pdfium| {
        let document =
            pdfium
                .load_pdf_from_byte_slice(bytes, None)
                .map_err(|e| ViewerError::PdfRender {
                    page: 1,
                    reason: format!("load document: {e}"),
                })?;
        let mut items = Vec::new();
        for bm in document.bookmarks().iter() {
            let title = bm.title().unwrap_or_default();
            let page = bm
                .destination()
                .and_then(|d| d.page_index().ok())
                .map(|idx| (idx.max(0) as u32).saturating_add(1))
                .unwrap_or(0);
            let mut depth = 0u32;
            let mut parent = bm.parent();
            while let Some(p) = parent {
                depth = depth.saturating_add(1);
                if depth > 64 {
                    break;
                }
                parent = p.parent();
            }
            items.push(PdfOutlineItem { title, page, depth });
        }
        Ok(items)
    })
}

/// Search every page for `query`. Empty query yields no hits.
///
/// # Errors
///
/// Pdfium bind / load failures.
pub fn search_document(bytes: &[u8], query: &str, match_case: bool) -> Result<Vec<FindHit>> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    with_pdfium(|pdfium| {
        let document =
            pdfium
                .load_pdf_from_byte_slice(bytes, None)
                .map_err(|e| ViewerError::PdfRender {
                    page: 1,
                    reason: format!("load document: {e}"),
                })?;
        let count = i32::from(document.pages().len()).max(0) as u32;
        let options = PdfSearchOptions::new().match_case(match_case);
        let mut hits = Vec::new();
        for i in 0..count {
            let page_1 = i + 1;
            let pdf_page = match document.pages().get(i as i32) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let Ok(text) = pdf_page.text() else {
                continue;
            };
            let Ok(search) = text.search(query, &options) else {
                continue;
            };
            while let Some(segments) = search.find_next() {
                let mut rects = Vec::new();
                for seg in segments.iter() {
                    let b = seg.bounds();
                    let rect = PtsRect::from_pdf_values(
                        b.bottom().value,
                        b.left().value,
                        b.top().value,
                        b.right().value,
                    );
                    if rect.width() > 0.0 && rect.height() > 0.0 {
                        rects.push(rect);
                    }
                }
                if !rects.is_empty() {
                    hits.push(FindHit {
                        page: page_1,
                        rects,
                    });
                }
            }
        }
        Ok(hits)
    })
}

/// Write highlight annotations over `rects` to `dest`.
///
/// # Errors
///
/// Empty rects, Pdfium failures, or I/O.
pub fn save_highlight(
    bytes: &[u8],
    page: u32,
    rects: &[PtsRect],
    dest: &Path,
) -> Result<std::path::PathBuf> {
    if rects.is_empty() {
        return Err(ViewerError::PdfHighlightEmpty);
    }
    let dest = dest.to_path_buf();
    with_pdfium(|pdfium| {
        let document =
            pdfium
                .load_pdf_from_byte_slice(bytes, None)
                .map_err(|e| ViewerError::PdfRender {
                    page,
                    reason: format!("load document: {e}"),
                })?;
        let count = i32::from(document.pages().len()).max(0) as u32;
        if count == 0 {
            return Err(ViewerError::PdfEmpty);
        }
        let current = page.clamp(1, count);
        let mut pdf_page = document
            .pages()
            .get(current.saturating_sub(1) as i32)
            .map_err(|e| ViewerError::PdfRender {
                page: current,
                reason: format!("open page: {e}"),
            })?;
        let color = PdfColor::new(255, 230, 0, 80);
        for rect in rects {
            let pdf_rect = PdfRect::new_from_values(rect.bottom, rect.left, rect.top, rect.right);
            let mut ann = pdf_page
                .annotations_mut()
                .create_highlight_annotation()
                .map_err(|e| ViewerError::PdfRender {
                    page: current,
                    reason: format!("highlight: {e}"),
                })?;
            ann.set_position(pdf_rect.left(), pdf_rect.bottom())
                .map_err(|e| ViewerError::PdfRender {
                    page: current,
                    reason: format!("highlight position: {e}"),
                })?;
            ann.set_stroke_color(color)
                .map_err(|e| ViewerError::PdfRender {
                    page: current,
                    reason: format!("highlight stroke: {e}"),
                })?;
            ann.set_fill_color(color)
                .map_err(|e| ViewerError::PdfRender {
                    page: current,
                    reason: format!("highlight fill: {e}"),
                })?;
            ann.set_width(pdf_rect.width())
                .map_err(|e| ViewerError::PdfRender {
                    page: current,
                    reason: format!("highlight width: {e}"),
                })?;
            ann.set_height(pdf_rect.height())
                .map_err(|e| ViewerError::PdfRender {
                    page: current,
                    reason: format!("highlight height: {e}"),
                })?;
            ann.attachment_points_mut()
                .create_attachment_point_at_end(PdfQuadPoints::from_rect(&pdf_rect))
                .map_err(|e| ViewerError::PdfRender {
                    page: current,
                    reason: format!("highlight quad: {e}"),
                })?;
        }
        document
            .save_to_file(&dest)
            .map_err(|e| ViewerError::PdfRender {
                page: current,
                reason: format!("save highlight: {e}"),
            })?;
        Ok(dest)
    })
}

/// Sibling `*-hl.pdf` next to `src_path` (fallback when the open file is not writable).
///
/// # Errors
///
/// Same as [`save_highlight`].
pub fn save_highlight_sibling(
    bytes: &[u8],
    page: u32,
    rects: &[PtsRect],
    src_path: &Path,
) -> Result<std::path::PathBuf> {
    let dest = unique_export_dest(src_path, "hl", "pdf");
    save_highlight(bytes, page, rects, &dest)
}

/// Write a sticky text annotation at the first rect into `dest`.
///
/// # Errors
///
/// Empty text / rects, Pdfium failures, or I/O.
pub fn save_text_comment(
    bytes: &[u8],
    page: u32,
    rects: &[PtsRect],
    text: &str,
    dest: &Path,
) -> Result<std::path::PathBuf> {
    let text = text.trim();
    if text.is_empty() || rects.is_empty() {
        return Err(ViewerError::PdfCommentEmpty);
    }
    let dest = dest.to_path_buf();
    let note = text.to_string();
    let anchor = rects[0];
    with_pdfium(|pdfium| {
        let document =
            pdfium
                .load_pdf_from_byte_slice(bytes, None)
                .map_err(|e| ViewerError::PdfRender {
                    page,
                    reason: format!("load document: {e}"),
                })?;
        let count = i32::from(document.pages().len()).max(0) as u32;
        if count == 0 {
            return Err(ViewerError::PdfEmpty);
        }
        let current = page.clamp(1, count);
        let mut pdf_page = document
            .pages()
            .get(current.saturating_sub(1) as i32)
            .map_err(|e| ViewerError::PdfRender {
                page: current,
                reason: format!("open page: {e}"),
            })?;
        let pdf_rect =
            PdfRect::new_from_values(anchor.bottom, anchor.left, anchor.top, anchor.right);
        let mut ann = pdf_page
            .annotations_mut()
            .create_text_annotation(&note)
            .map_err(|e| ViewerError::PdfRender {
                page: current,
                reason: format!("comment: {e}"),
            })?;
        ann.set_position(pdf_rect.left(), pdf_rect.top())
            .map_err(|e| ViewerError::PdfRender {
                page: current,
                reason: format!("comment position: {e}"),
            })?;
        ann.set_width(PdfPoints::new(18.0))
            .map_err(|e| ViewerError::PdfRender {
                page: current,
                reason: format!("comment width: {e}"),
            })?;
        ann.set_height(PdfPoints::new(18.0))
            .map_err(|e| ViewerError::PdfRender {
                page: current,
                reason: format!("comment height: {e}"),
            })?;
        document
            .save_to_file(&dest)
            .map_err(|e| ViewerError::PdfRender {
                page: current,
                reason: format!("save comment: {e}"),
            })?;
        Ok(dest)
    })
}

/// Sibling `*-note.pdf` next to `src_path` when the open file is not writable.
///
/// # Errors
///
/// Same as [`save_text_comment`].
pub fn save_text_comment_sibling(
    bytes: &[u8],
    page: u32,
    rects: &[PtsRect],
    text: &str,
    src_path: &Path,
) -> Result<std::path::PathBuf> {
    let dest = unique_export_dest(src_path, "note", "pdf");
    save_text_comment(bytes, page, rects, text, &dest)
}

/// One AcroForm field the viewer can show or fill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormField {
    /// Partial field name (`/T`).
    pub name: String,
    /// `text`, `checkbox`, `radio`, `combo`, `list`, `button`, `signature`, or `unknown`.
    pub kind: String,
    /// Current value. Checkboxes and radios are `true` or `false`.
    pub value: String,
}

/// List AcroForm widgets. Radio and checkbox groups collapse to one row per name,
/// preferring a checked control.
///
/// # Errors
///
/// Pdfium bind / load failures.
pub fn list_form_fields(bytes: &[u8]) -> Result<Vec<FormField>> {
    with_pdfium(|pdfium| {
        let document =
            pdfium
                .load_pdf_from_byte_slice(bytes, None)
                .map_err(|e| ViewerError::PdfRender {
                    page: 1,
                    reason: format!("load document: {e}"),
                })?;
        let mut fields = Vec::new();
        let page_count = i32::from(document.pages().len()).max(0);
        for index in 0..page_count {
            let page = document
                .pages()
                .get(index)
                .map_err(|e| ViewerError::PdfRender {
                    page: (index.max(0) as u32).saturating_add(1),
                    reason: format!("open page: {e}"),
                })?;
            for annotation in page.annotations().iter() {
                let Some(field) = annotation.as_form_field() else {
                    continue;
                };
                let name = field.name().unwrap_or_default();
                if name.is_empty() {
                    continue;
                }
                let (kind, value, prefer) = field_row(field);
                upsert_field(&mut fields, FormField { name, kind, value }, prefer);
            }
        }
        Ok(fields)
    })
}

fn field_row(field: &PdfFormField<'_>) -> (String, String, bool) {
    match field.field_type() {
        PdfFormFieldType::Text => (
            "text".to_string(),
            field
                .as_text_field()
                .and_then(|text| text.value())
                .unwrap_or_default(),
            true,
        ),
        PdfFormFieldType::Checkbox => {
            let on = field
                .as_checkbox_field()
                .and_then(|box_| box_.is_checked().ok())
                .unwrap_or(false);
            ("checkbox".to_string(), bool_value(on), on)
        }
        PdfFormFieldType::RadioButton => {
            let radio = field.as_radio_button_field();
            let on = radio
                .and_then(|radio| radio.is_checked().ok())
                .unwrap_or(false);
            let value = if on {
                radio
                    .and_then(|radio| radio.group_value())
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| bool_value(true))
            } else {
                bool_value(false)
            };
            ("radio".to_string(), value, on)
        }
        PdfFormFieldType::ComboBox => (
            "combo".to_string(),
            field
                .as_combo_box_field()
                .and_then(|combo| combo.value())
                .unwrap_or_default(),
            true,
        ),
        PdfFormFieldType::ListBox => (
            "list".to_string(),
            field
                .as_list_box_field()
                .and_then(|list| list.value())
                .unwrap_or_default(),
            true,
        ),
        PdfFormFieldType::PushButton => ("button".to_string(), String::new(), false),
        PdfFormFieldType::Signature => ("signature".to_string(), String::new(), false),
        PdfFormFieldType::Unknown => ("unknown".to_string(), String::new(), false),
    }
}

fn bool_value(on: bool) -> String {
    if on {
        "true".to_string()
    } else {
        "false".to_string()
    }
}

fn upsert_field(fields: &mut Vec<FormField>, field: FormField, prefer: bool) {
    if let Some(existing) = fields.iter_mut().find(|row| row.name == field.name) {
        if prefer {
            existing.kind = field.kind;
            existing.value = field.value;
        }
        return;
    }
    fields.push(field);
}

/// Write `value` into the AcroForm field `name` and save `dest`.
///
/// Text fields take the string as-is. Checkboxes treat `true`, `yes`, `on`,
/// and `1` as checked. Radio buttons select the widget whose export value
/// matches `value`. Combo boxes and list boxes select the option whose
/// displayed label matches `value`. Signatures are left unchanged.
///
/// # Errors
///
/// Missing / read-only / unsupported fields, or Pdfium / I/O failures.
pub fn save_form_value(bytes: &[u8], name: &str, value: &str, dest: &Path) -> Result<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ViewerError::PdfFormEmpty);
    }
    let dest = dest.to_path_buf();
    let name = name.to_string();
    let value = value.to_string();
    if let Some(patched) = choice_fill_bytes(bytes, &name, &value)? {
        std::fs::write(&dest, patched).map_err(|e| ViewerError::PdfRender {
            page: 1,
            reason: format!("save form: {e}"),
        })?;
        return Ok(dest);
    }
    with_pdfium(|pdfium| {
        let mut document =
            pdfium
                .load_pdf_from_byte_slice(bytes, None)
                .map_err(|e| ViewerError::PdfRender {
                    page: 1,
                    reason: format!("load document: {e}"),
                })?;
        apply_form_value(&mut document, &name, &value)?;
        document
            .save_to_file(&dest)
            .map_err(|e| ViewerError::PdfRender {
                page: 1,
                reason: format!("save form: {e}"),
            })?;
        Ok(dest)
    })
}

/// Sibling `*-form.pdf` when the open file is not writable.
///
/// # Errors
///
/// Same as [`save_form_value`].
pub fn save_form_value_sibling(
    bytes: &[u8],
    name: &str,
    value: &str,
    src_path: &Path,
) -> Result<PathBuf> {
    let dest = unique_export_dest(src_path, "form", "pdf");
    save_form_value(bytes, name, value, &dest)
}

/// `Some` when `name` is a combo or list and the bytes were rewritten.
/// `None` when the field is a text, checkbox, or radio control.
fn choice_fill_bytes(bytes: &[u8], name: &str, value: &str) -> Result<Option<Vec<u8>>> {
    let selection = with_pdfium(|pdfium| choice_option(pdfium, bytes, name, value))?;
    let Some((index, label)) = selection else {
        return Ok(None);
    };
    patch_choice_field(bytes, name, &label, index).map(Some)
}

fn choice_option(
    pdfium: &Pdfium,
    bytes: &[u8],
    name: &str,
    value: &str,
) -> Result<Option<(usize, String)>> {
    let document =
        pdfium
            .load_pdf_from_byte_slice(bytes, None)
            .map_err(|e| ViewerError::PdfRender {
                page: 1,
                reason: format!("load document: {e}"),
            })?;
    let page_count = i32::from(document.pages().len()).max(0);
    let mut saw_choice = false;
    for index in 0..page_count {
        let page = document
            .pages()
            .get(index)
            .map_err(|e| ViewerError::PdfRender {
                page: (index.max(0) as u32).saturating_add(1),
                reason: format!("open page: {e}"),
            })?;
        for annotation in page.annotations().iter() {
            let Some(field) = annotation.as_form_field() else {
                continue;
            };
            if field.name().as_deref() != Some(name) {
                continue;
            }
            if field.is_read_only() {
                return Err(ViewerError::PdfFormReadOnly);
            }
            let options = if let Some(combo) = field.as_combo_box_field() {
                combo.options()
            } else if let Some(list) = field.as_list_box_field() {
                list.options()
            } else {
                continue;
            };
            saw_choice = true;
            let want = value.trim();
            for option in options.iter() {
                let Some(label) = option.label() else {
                    continue;
                };
                if label == want {
                    return Ok(Some((option.index() as usize, label.clone())));
                }
            }
        }
    }
    if saw_choice {
        Err(ViewerError::PdfFormUnsupported)
    } else {
        Ok(None)
    }
}

fn patch_choice_field(bytes: &[u8], name: &str, label: &str, index: usize) -> Result<Vec<u8>> {
    let spaced = format!("/T ({name})");
    let tight = format!("/T({name})");
    let Some(at) =
        find_bytes(bytes, spaced.as_bytes()).or_else(|| find_bytes(bytes, tight.as_bytes()))
    else {
        return Err(ViewerError::PdfFormUnsupported);
    };
    let Some((obj_num, object)) = object_containing(bytes, at) else {
        return Err(ViewerError::PdfFormUnsupported);
    };
    if !object.contains("/FT /Ch") && !object.contains("/FT/Ch") {
        return Err(ViewerError::PdfFormUnsupported);
    }
    let updated = rewrite_choice_object(&object, label, index);
    append_pdf_object(bytes, obj_num, &updated).ok_or(ViewerError::PdfFormUnsupported)
}

fn rewrite_choice_object(object: &str, label: &str, index: usize) -> String {
    let escaped = pdf_literal(label);
    let with_value = replace_pdf_literal(object, "/V", &escaped)
        .unwrap_or_else(|| insert_before_dict_end(object, &format!("/V ({escaped}) ")));
    replace_pdf_index(&with_value, index)
        .unwrap_or_else(|| insert_before_dict_end(&with_value, &format!("/I [{index}] ")))
}

fn pdf_literal(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

fn replace_pdf_literal(object: &str, key: &str, escaped: &str) -> Option<String> {
    let start = find_pdf_key(object, key)?;
    let after = &object[start + key.len()..];
    let ws = after
        .chars()
        .take_while(|c| c.is_ascii_whitespace())
        .count();
    if !after[ws..].starts_with('(') {
        return None;
    }
    let value_at = start + key.len() + ws + 1;
    let byte_end = pdf_literal_end(&object[value_at..])?;
    let mut out = String::new();
    out.push_str(&object[..value_at]);
    out.push_str(escaped);
    out.push_str(&object[value_at + byte_end..]);
    Some(out)
}

/// `/V` or `/I` followed by a delimiter, so `/View` does not match `/V`.
fn find_pdf_key(object: &str, key: &str) -> Option<usize> {
    let bytes = object.as_bytes();
    let needle = key.as_bytes();
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if bytes[i..].starts_with(needle) {
            let next = bytes.get(i + needle.len()).copied();
            if next.is_none_or(|b| {
                matches!(
                    b,
                    b' ' | b'\n' | b'\r' | b'\t' | b'(' | b'[' | b'/' | b'>' | b'<'
                )
            }) {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn pdf_literal_end(tail: &str) -> Option<usize> {
    let bytes = tail.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b')' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn replace_pdf_index(object: &str, index: usize) -> Option<String> {
    let start = find_pdf_key(object, "/I")?;
    let after = &object[start + 2..];
    let ws = after
        .chars()
        .take_while(|c| c.is_ascii_whitespace())
        .count();
    if !after[ws..].starts_with('[') {
        return None;
    }
    let bracket = start + 2 + ws;
    let end = object[bracket..].find(']')?;
    let mut out = String::new();
    out.push_str(&object[..start]);
    out.push_str(&format!("/I [{index}]"));
    out.push_str(&object[bracket + end + 1..]);
    Some(out)
}

fn insert_before_dict_end(object: &str, piece: &str) -> String {
    if let Some(at) = object.rfind(">>") {
        let mut out = String::new();
        out.push_str(&object[..at]);
        out.push_str(piece);
        out.push_str(&object[at..]);
        out
    } else {
        object.to_string()
    }
}

fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len())
        .position(|window| window == needle)
}

fn object_containing(bytes: &[u8], at: usize) -> Option<(u32, String)> {
    let head = &bytes[..at];
    let marker = b" obj";
    let rel = head
        .windows(marker.len())
        .rposition(|window| window == marker)?;
    let line_start = head[..rel]
        .iter()
        .rposition(|b| *b == b'\n' || *b == b'\r')
        .map(|pos| pos + 1)
        .unwrap_or(0);
    let header = std::str::from_utf8(&head[line_start..rel]).ok()?.trim();
    let mut parts = header.split_whitespace();
    let num: u32 = parts.next()?.parse().ok()?;
    let _gen: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let end_rel = bytes[at..]
        .windows(6)
        .position(|window| window == b"endobj")?;
    let end = at + end_rel + 6;
    let object = std::str::from_utf8(&bytes[line_start..end])
        .ok()?
        .to_string();
    Some((num, object))
}

fn append_pdf_object(bytes: &[u8], obj_num: u32, object: &str) -> Option<Vec<u8>> {
    let key = b"startxref";
    let startxref_at = rfind_bytes(bytes, key)?;
    let mut line = startxref_at + key.len();
    while line < bytes.len() && matches!(bytes[line], b'\r' | b'\n' | b' ' | b'\t') {
        line += 1;
    }
    let num_start = line;
    while line < bytes.len() && bytes[line].is_ascii_digit() {
        line += 1;
    }
    let prev: usize = std::str::from_utf8(&bytes[num_start..line])
        .ok()?
        .parse()
        .ok()?;
    let trailer_at = rfind_bytes(&bytes[..startxref_at], b"trailer")?;
    let trailer = &bytes[trailer_at..startxref_at];
    let root_at = find_bytes(trailer, b"/Root ")?;
    let root_tail = &trailer[root_at + 6..];
    let root_end = root_tail
        .windows(2)
        .position(|window| window == b">>")
        .unwrap_or(root_tail.len());
    let root_text = std::str::from_utf8(&root_tail[..root_end]).ok()?.trim();
    let root_ref = root_text.split('/').next()?.trim();
    if root_ref.is_empty() {
        return None;
    }
    let parsed_size = find_bytes(trailer, b"/Size ")
        .and_then(|at| std::str::from_utf8(&trailer[at + 6..]).ok())
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|token| token.parse::<u32>().ok());
    let size = parsed_size
        .unwrap_or(obj_num.saturating_add(1))
        .max(obj_num.saturating_add(1));
    let mut out = Vec::with_capacity(bytes.len() + object.len() + 128);
    out.extend_from_slice(bytes);
    if !out.ends_with(b"\n") {
        out.push(b'\n');
    }
    let offset = out.len();
    let body = if object.starts_with(&format!("{obj_num} ")) {
        object.to_string()
    } else {
        format!("{obj_num} 0 obj\n{object}\nendobj\n")
    };
    let body = if body.ends_with('\n') {
        body
    } else {
        format!("{body}\n")
    };
    out.extend_from_slice(body.as_bytes());
    let xref = out.len();
    let xref_text = format!(
        "xref\n{obj_num} 1\n{offset:010} 00000 n \ntrailer\n<< /Size {size} /Root {root_ref} /Prev {prev} >>\nstartxref\n{xref}\n%%EOF\n"
    );
    out.extend_from_slice(xref_text.as_bytes());
    Some(out)
}

fn rfind_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len())
        .rposition(|window| window == needle)
}

fn apply_form_value(document: &mut PdfDocument<'_>, name: &str, value: &str) -> Result<()> {
    let page_count = i32::from(document.pages().len()).max(0);
    let mut found = false;
    let mut wrote = false;
    for index in 0..page_count {
        let page = document
            .pages()
            .get(index)
            .map_err(|e| ViewerError::PdfRender {
                page: (index.max(0) as u32).saturating_add(1),
                reason: format!("open page: {e}"),
            })?;
        let count = page.annotations().len();
        for ai in 0..count {
            let mut annotation =
                page.annotations()
                    .get(ai)
                    .map_err(|e| ViewerError::PdfRender {
                        page: (index.max(0) as u32).saturating_add(1),
                        reason: format!("annotation: {e}"),
                    })?;
            let Some(field) = annotation.as_form_field_mut() else {
                continue;
            };
            if field.name().as_deref() != Some(name) {
                continue;
            }
            found = true;
            if field.is_read_only() {
                return Err(ViewerError::PdfFormReadOnly);
            }
            match field.field_type() {
                PdfFormFieldType::Text => {
                    field
                        .as_text_field_mut()
                        .ok_or(ViewerError::PdfFormUnsupported)?
                        .set_value(value)
                        .map_err(|e| ViewerError::PdfRender {
                            page: 1,
                            reason: format!("set text: {e}"),
                        })?;
                    wrote = true;
                }
                PdfFormFieldType::Checkbox => {
                    field
                        .as_checkbox_field_mut()
                        .ok_or(ViewerError::PdfFormUnsupported)?
                        .set_checked(is_checked_value(value))
                        .map_err(|e| ViewerError::PdfRender {
                            page: 1,
                            reason: format!("set checkbox: {e}"),
                        })?;
                    wrote = true;
                }
                PdfFormFieldType::RadioButton => {
                    if radio_matches(field, value) {
                        field
                            .as_radio_button_field_mut()
                            .ok_or(ViewerError::PdfFormUnsupported)?
                            .set_checked()
                            .map_err(|e| ViewerError::PdfRender {
                                page: 1,
                                reason: format!("set radio: {e}"),
                            })?;
                        wrote = true;
                    }
                }
                PdfFormFieldType::ComboBox | PdfFormFieldType::ListBox => {}
                PdfFormFieldType::PushButton
                | PdfFormFieldType::Signature
                | PdfFormFieldType::Unknown => {}
            }
        }
    }
    if !found {
        return Err(ViewerError::PdfFormMissing);
    }
    if !wrote {
        return Err(ViewerError::PdfFormUnsupported);
    }
    Ok(())
}

fn is_checked_value(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "true" | "yes" | "on" | "1"
    )
}

fn radio_matches(field: &PdfFormField<'_>, value: &str) -> bool {
    let want = export_token(value);
    if want.is_empty() {
        return false;
    }
    let appearance = field
        .appearance_stream()
        .is_some_and(|stream| export_token(&stream) == want);
    if appearance {
        return true;
    }
    field
        .as_radio_button_field()
        .and_then(|radio| radio.group_value())
        .is_some_and(|group| export_token(&group) == want)
}

fn export_token(value: &str) -> String {
    value.trim().trim_start_matches('/').to_ascii_lowercase()
}

/// Split `Name=value` into a field name and a value.
///
/// # Errors
///
/// [`ViewerError::PdfFormEmpty`] when the name or `=` is missing.
pub fn split_form_assignment(raw: &str) -> Result<(&str, &str)> {
    let raw = raw.trim();
    let Some((name, value)) = raw.split_once('=') else {
        return Err(ViewerError::PdfFormEmpty);
    };
    let name = name.trim();
    if name.is_empty() {
        return Err(ViewerError::PdfFormEmpty);
    }
    Ok((name, value.trim()))
}

pub(crate) fn form_summary(fields: &[FormField]) -> String {
    const MAX_FIELDS: usize = 6;
    let mut parts = Vec::new();
    for field in fields.iter().take(MAX_FIELDS) {
        parts.push(format!("{}={}", field.name, field.value));
    }
    if fields.len() > MAX_FIELDS {
        parts.push("…".to_string());
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::layer::extract_layer;
    use crate::pdf::render::MINIMAL_PDF;

    #[test]
    fn outline_and_search_on_minimal_pdf() {
        let layer = extract_layer(MINIMAL_PDF, 1, 100, 100)
            .expect("pdfium should open minimal PDF when available");
        assert!(
            layer.page_w_pts > 0.0,
            "minimal page MediaBox width should be positive"
        );
        let outline = extract_outline(MINIMAL_PDF).expect("outline");
        assert!(outline.is_empty(), "minimal PDF has no bookmarks");
        let hits = search_document(MINIMAL_PDF, "no-such-text", false).expect("search");
        assert!(hits.is_empty(), "minimal PDF has no text to match");
    }

    #[test]
    fn fills_text_field_and_checkbox() {
        let bytes = acro_form_pdf();
        let fields = list_form_fields(&bytes).expect("list");
        assert!(
            fields
                .iter()
                .any(|field| field.name == "Name" && field.kind == "text" && field.value == "Ada"),
            "text field should start as Ada, got {fields:?}"
        );
        assert!(
            fields.iter().any(|field| field.name == "Agree"
                && field.kind == "checkbox"
                && field.value == "false"),
            "checkbox should start unchecked, got {fields:?}"
        );
        let dest = std::env::temp_dir().join("orchid-acro-fill.pdf");
        save_form_value(&bytes, "Name", "Orchid", &dest).expect("fill text");
        let saved = std::fs::read(&dest).expect("read filled");
        let fields = list_form_fields(&saved).expect("list filled");
        assert!(
            fields
                .iter()
                .any(|field| field.name == "Name" && field.value == "Orchid"),
            "filled text should read back, got {fields:?}"
        );
        save_form_value(&saved, "Agree", "yes", &dest).expect("fill checkbox");
        let saved = std::fs::read(&dest).expect("read checkbox");
        let fields = list_form_fields(&saved).expect("list checkbox");
        assert!(
            fields
                .iter()
                .any(|field| field.name == "Agree" && field.value == "true"),
            "checkbox should be checked, got {fields:?}"
        );
        let missing = save_form_value(&saved, "Missing", "x", &dest);
        assert!(matches!(missing, Err(ViewerError::PdfFormMissing)));
        save_form_value(&saved, "City", "Lima", &dest).expect("fill combo");
        let saved = std::fs::read(&dest).expect("read combo");
        let fields = list_form_fields(&saved).expect("list combo");
        assert!(
            fields.iter().any(|field| field.name == "City"
                && field.kind == "combo"
                && field.value == "Lima"),
            "combo should select Lima, got {fields:?}"
        );
        save_form_value(&saved, "Color", "Blue", &dest).expect("fill list");
        let saved = std::fs::read(&dest).expect("read list");
        let fields = list_form_fields(&saved).expect("list box");
        assert!(
            fields.iter().any(|field| field.name == "Color"
                && field.kind == "list"
                && field.value == "Blue"),
            "list should select Blue, got {fields:?}"
        );
        let unknown = save_form_value(&saved, "City", "Nowhere", &dest);
        assert!(matches!(unknown, Err(ViewerError::PdfFormUnsupported)));
        let _ = std::fs::remove_file(&dest);
    }

    /// One-page PDF with a text field `Name=Ada` and an unchecked checkbox `Agree`.
    fn acro_form_pdf() -> Vec<u8> {
        let objects = [
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R 5 0 R 6 0 R 7 0 R] >> >>\nendobj\n",
            "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n",
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Annots [4 0 R 5 0 R 6 0 R 7 0 R] >>\nendobj\n",
            "4 0 obj\n<< /Type /Annot /Subtype /Widget /FT /Tx /T (Name) /V (Ada) /Rect [20 20 200 40] /F 4 >>\nendobj\n",
            "5 0 obj\n<< /Type /Annot /Subtype /Widget /FT /Btn /T (Agree) /V /Off /AS /Off /Rect [20 50 40 70] /F 4 >>\nendobj\n",
            "6 0 obj\n<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 131072 /T (City) /V (Paris) /Opt [(Paris) (Lima)] /Rect [20 80 200 100] /F 4 >>\nendobj\n",
            "7 0 obj\n<< /Type /Annot /Subtype /Widget /FT /Ch /T (Color) /V (Red) /I [0] /Opt [(Red) (Blue)] /Rect [20 110 200 160] /F 4 >>\nendobj\n",
        ];
        let mut body = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        for object in objects {
            offsets.push(body.len());
            body.push_str(object);
        }
        let xref = body.len();
        body.push_str(&format!("xref\n0 {}\n", objects.len() + 1));
        body.push_str("0000000000 65535 f \n");
        for offset in offsets {
            body.push_str(&format!("{offset:010} 00000 n \n"));
        }
        body.push_str(&format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        ));
        body.into_bytes()
    }
}
