//! Shared OOXML helpers for `word/document.xml`.

#![allow(unused_imports)]

use std::collections::HashMap;
use std::io::Cursor;

use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;

use crate::document::model::{
    Alignment, Block, Bookmark, CellImage, CommentRange, DocField, Document, Hyperlink,
    ImageFormat, InlineImage, LineSpacingRule, ListKind, OpaqueXmlNode, PageSetup, Paragraph, Run,
    RunStyle, SectionBreakType, Table, TableCell, TableRow, VMerge, CELL_BORDER_BOTTOM,
    CELL_BORDER_LEFT, CELL_BORDER_RIGHT, CELL_BORDER_TOP,
};
use crate::document::ooxml::numbering::NumberingDefs;
use crate::document::ooxml::styles::StyleDefaults;
use crate::error::{Result, ViewerError};

use super::Relationships;

/// Resolve a document relationship target to a package part path under `word/`.
#[must_use]
pub fn word_part_path(target: &str) -> String {
    let t = target.replace('\\', "/");
    if t.starts_with("word/") {
        t
    } else if let Some(rest) = t.strip_prefix("./") {
        format!("word/{rest}")
    } else if t.starts_with('/') {
        t.trim_start_matches('/').to_string()
    } else {
        format!("word/{t}")
    }
}

/// EMUs → CSS pixels at 96 DPI (`emu * 96 / 914400`).
pub(super) fn emu_to_css_px(emu: u64) -> u32 {
    ((emu.saturating_mul(96)) / 914_400).max(1) as u32
}

/// Build an [`InlineImage`] from package media (used by container after scan).
#[must_use]
pub fn image_from_part(part_path: &str, bytes: Vec<u8>, r_id: Option<String>) -> InlineImage {
    let ext = part_path.rsplit('.').next().unwrap_or("");
    let format = ImageFormat::from_extension(ext);
    let (width_px, height_px) = image::load_from_memory(&bytes)
        .map(|img| (img.width(), img.height()))
        .unwrap_or((0, 0));
    InlineImage {
        bytes,
        format,
        width_px,
        height_px,
        r_id,
        part_path: Some(part_path.to_string()),
    }
}

pub(super) fn css_px_to_emu(px: u32) -> u64 {
    u64::from(px.max(1)).saturating_mul(914_400) / 96
}

pub(super) fn parse_alignment(val: &str) -> Alignment {
    match val {
        "center" => Alignment::Center,
        "right" | "end" => Alignment::Right,
        "both" | "distribute" => Alignment::Justify,
        _ => Alignment::Left,
    }
}

pub(super) fn alignment_val(a: Alignment) -> &'static str {
    match a {
        Alignment::Left => "left",
        Alignment::Center => "center",
        Alignment::Right => "right",
        Alignment::Justify => "both",
    }
}

pub(super) fn parse_rgb(val: &str) -> Option<[u8; 3]> {
    let v = val.trim();
    if v.len() == 6 {
        let r = u8::from_str_radix(&v[0..2], 16).ok()?;
        let g = u8::from_str_radix(&v[2..4], 16).ok()?;
        let b = u8::from_str_radix(&v[4..6], 16).ok()?;
        Some([r, g, b])
    } else {
        None
    }
}

pub(super) fn local_name(name: &str) -> String {
    name.rsplit(':').next().unwrap_or(name).to_string()
}

pub(super) fn attr_val(e: &BytesStart<'_>, key: &str) -> Option<String> {
    for a in e.attributes().flatten() {
        if local_name(a.key.as_ref()) == key {
            return Some(a.value.into_owned());
        }
    }
    None
}
