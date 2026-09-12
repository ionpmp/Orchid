//! Shared document-edit helpers.

use super::*;
use crate::error::{Result, ViewerError};

pub(crate) fn step_image_aware(doc: &Document, cursor: Cursor, forward: bool) -> Option<Cursor> {
    if is_image_cursor(doc, cursor) {
        if cursor.cell.is_some() {
            return adjacent_in_cell(doc, cursor, forward);
        }
        let bi = cursor.block_idx;
        if forward && bi + 1 < doc.blocks.len() {
            return Some(Cursor::at(bi + 1, 0, 0));
        }
        if !forward && bi > 0 {
            return end_of_block_cursor(doc, bi - 1);
        }
        return None;
    }
    let path = cursor.cell?;
    if path.image_idx.is_some() {
        return None;
    }
    let p = paragraph_ref(doc, cursor)?;
    let at_end = if p.runs.is_empty() {
        true
    } else {
        let last = p.runs.len() - 1;
        cursor.run_idx == last && cursor.byte_offset >= p.runs[last].text.len()
    };
    let at_start = cursor.run_idx == 0 && cursor.byte_offset == 0;
    if forward && at_end {
        return adjacent_in_cell(doc, cursor, true);
    }
    if !forward && at_start {
        return adjacent_in_cell(doc, cursor, false);
    }
    None
}

pub(crate) fn end_of_block_cursor(doc: &Document, block_idx: usize) -> Option<Cursor> {
    match doc.blocks.get(block_idx)? {
        Block::Paragraph(p) => Some(end_of_paragraph_cursor(
            Cursor {
                block_idx,
                cell: None,
                run_idx: 0,
                byte_offset: 0,
            },
            p,
        )),
        Block::Table(_) => Some(Cursor {
            block_idx,
            cell: Some(CellPath::new(0, 0, 0)),
            run_idx: 0,
            byte_offset: 0,
        }),
        Block::Image(_) => Some(Cursor::at(block_idx, 0, 0)),
    }
}

pub(crate) fn end_of_paragraph_cursor(cursor: Cursor, p: &Paragraph) -> Cursor {
    if p.runs.is_empty() {
        return Cursor {
            block_idx: cursor.block_idx,
            cell: cursor.cell,
            run_idx: 0,
            byte_offset: 0,
        };
    }
    let last = p.runs.len() - 1;
    Cursor {
        block_idx: cursor.block_idx,
        cell: cursor.cell,
        run_idx: last,
        byte_offset: p.runs[last].text.len(),
    }
}

pub(crate) fn fallback_cell_caret(doc: &Document, cursor: Cursor, backward: bool) -> Cursor {
    let Some(path) = cursor.cell else {
        return cursor;
    };
    if backward {
        let para_cursor = Cursor {
            block_idx: cursor.block_idx,
            cell: Some(CellPath::new(path.row, path.col, path.para_idx)),
            run_idx: 0,
            byte_offset: 0,
        };
        if let Some(p) = paragraph_ref(doc, para_cursor) {
            return end_of_paragraph_cursor(para_cursor, p);
        }
    }
    Cursor {
        block_idx: cursor.block_idx,
        cell: Some(CellPath::new(path.row, path.col, 0)),
        run_idx: 0,
        byte_offset: 0,
    }
}

pub(crate) fn plain_text_to_blocks_preserving(doc: &Document, text: &str) -> Vec<Block> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = if normalized.is_empty() {
        vec![""]
    } else {
        normalized.split('\n').collect()
    };
    lines
        .into_iter()
        .enumerate()
        .map(|(idx, line)| {
            if let Some(Block::Paragraph(prev)) = doc.blocks.get(idx) {
                let style = prev
                    .runs
                    .first()
                    .map(|r| r.style.clone())
                    .unwrap_or_default();
                Block::Paragraph(Paragraph {
                    runs: vec![Run {
                        text: line.to_string(),
                        style,
                        ..Default::default()
                    }],
                    alignment: prev.alignment,
                    list: prev.list,
                    list_level: prev.list_level,
                    num_id: prev.num_id,
                    page_break_before: prev.page_break_before,
                    keep_next: prev.keep_next,
                    keep_lines: prev.keep_lines,
                    widow_control: prev.widow_control,
                    contextual_spacing: prev.contextual_spacing,
                    bidi: prev.bidi,
                    suppress_auto_hyphens: prev.suppress_auto_hyphens,
                    outline_level: prev.outline_level,
                    style_id: None,
                    space_before_twips: prev.space_before_twips,
                    space_after_twips: prev.space_after_twips,
                    line_spacing: prev.line_spacing,
                    line_spacing_rule: prev.line_spacing_rule,
                    indent_left_twips: prev.indent_left_twips,
                    indent_first_line_twips: prev.indent_first_line_twips,
                    indent_right_twips: prev.indent_right_twips,
                    shade_fill: prev.shade_fill,
                    border_sides: prev.border_sides,
                    section_properties: None,
                    unsupported: prev.unsupported.clone(),
                })
            } else {
                Block::Paragraph(Paragraph {
                    runs: vec![Run {
                        text: line.to_string(),
                        style: RunStyle::default(),
                        ..Default::default()
                    }],
                    ..Default::default()
                })
            }
        })
        .collect()
}

pub(crate) fn first_paragraph(doc: &Document) -> Option<&Paragraph> {
    doc.blocks.iter().find_map(|b| match b {
        Block::Paragraph(p) => Some(p),
        _ => None,
    })
}

pub(crate) fn style_at_cursor(doc: &Document, cursor: Cursor) -> Option<RunStyle> {
    let p = paragraph_ref(doc, cursor)?;
    p.runs
        .get(cursor.run_idx)
        .map(|r| r.style.clone())
        .or_else(|| p.runs.first().map(|r| r.style.clone()))
}

pub(crate) fn effective_style_selection(
    doc: &Document,
    sel: Selection,
    _source_mode: bool,
) -> Selection {
    if !sel.is_collapsed() {
        return sel;
    }
    // Collapsed caret (Source or Preview click) → style the whole paragraph so
    // toolbar B/I/U / align / list remain one-click useful.
    expand_selection_to_paragraph(doc, sel.head)
}

pub(crate) fn expand_selection_to_paragraph(doc: &Document, cursor: Cursor) -> Selection {
    let Some(p) = paragraph_ref(doc, cursor) else {
        return Selection {
            anchor: cursor,
            head: cursor,
        };
    };
    if p.runs.is_empty() {
        let c = Cursor {
            block_idx: cursor.block_idx,
            cell: cursor.cell,
            run_idx: 0,
            byte_offset: 0,
        };
        return Selection { anchor: c, head: c };
    }
    let last = p.runs.len() - 1;
    Selection {
        anchor: Cursor {
            block_idx: cursor.block_idx,
            cell: cursor.cell,
            run_idx: 0,
            byte_offset: 0,
        },
        head: Cursor {
            block_idx: cursor.block_idx,
            cell: cursor.cell,
            run_idx: last,
            byte_offset: p.runs[last].text.len(),
        },
    }
}

/// Body section index owning `caret_block` (0 = first; last = trailing `page_setup`).
pub(crate) fn caret_section_index(doc: &Document, caret_block: usize) -> usize {
    let mut section = 0usize;
    for (bi, block) in doc.blocks.iter().enumerate() {
        if bi >= caret_block {
            break;
        }
        if let Block::Paragraph(p) = block {
            if p.section_properties.is_some() {
                section += 1;
            }
        }
    }
    section
}

pub(crate) enum SectionPageSetupTarget {
    MidBody { end_block_idx: usize },
    Trailing,
}

pub(crate) fn section_page_setup_target(
    doc: &Document,
    section_idx: usize,
) -> SectionPageSetupTarget {
    let mut seen = 0usize;
    for (bi, block) in doc.blocks.iter().enumerate() {
        if let Block::Paragraph(p) = block {
            if p.section_properties.is_some() {
                if seen == section_idx {
                    return SectionPageSetupTarget::MidBody { end_block_idx: bi };
                }
                seen += 1;
            }
        }
    }
    SectionPageSetupTarget::Trailing
}

pub(crate) fn run_style_id_at_cursor(doc: &Document, cursor: Cursor) -> Option<String> {
    let p = paragraph_ref(doc, cursor)?;
    p.runs.get(cursor.run_idx)?.style_id.clone()
}

pub(crate) fn page_setup_for_cursor<'a>(doc: &'a Document, cursor: Cursor) -> &'a PageSetup {
    match section_page_setup_target(doc, caret_section_index(doc, cursor.block_idx)) {
        SectionPageSetupTarget::Trailing => &doc.page_setup,
        SectionPageSetupTarget::MidBody { end_block_idx } => doc
            .blocks
            .get(end_block_idx)
            .and_then(|b| match b {
                Block::Paragraph(p) => p.section_properties.as_ref(),
                _ => None,
            })
            .unwrap_or(&doc.page_setup),
    }
}

/// Widest margins / page size across mid-body + trailing section setups.
pub(crate) fn union_section_page_setup(doc: &Document) -> PageSetup {
    let mut u = doc.page_setup.clone();
    for block in &doc.blocks {
        if let Block::Paragraph(p) = block {
            if let Some(ref s) = p.section_properties {
                u.margin_left_twips = u.margin_left_twips.max(s.margin_left_twips);
                u.margin_right_twips = u.margin_right_twips.max(s.margin_right_twips);
                u.margin_top_twips = u.margin_top_twips.max(s.margin_top_twips);
                u.margin_bottom_twips = u.margin_bottom_twips.max(s.margin_bottom_twips);
                u.width_twips = u.width_twips.max(s.width_twips);
                u.height_twips = u.height_twips.max(s.height_twips);
            }
        }
    }
    u
}

pub(crate) const DEFAULT_FONT_SIZE_PT: f32 = 14.0;
const SPACING_TWIPS_MAX: i32 = 2880;
/// First-line / hanging clamp (±1″).
pub(crate) const INDENT_FIRST_LINE_MIN: i32 = -1440;
pub(crate) const INDENT_FIRST_LINE_MAX: i32 = 1440;
/// Auto line-spacing presets in 240ths of a line (single, 1.15, 1.5, double).
const LINE_SPACING_PRESETS: &[u32] = &[240, 276, 360, 480];
/// Page margin clamp: 0.25″ … 3″.
const MARGIN_TWIPS_MIN: i32 = 360;
const MARGIN_TWIPS_MAX: i32 = 4320;
/// US Letter page size (twips).
pub(crate) const PAGE_LETTER_WIDTH_TWIPS: u32 = 12240;
pub(crate) const PAGE_LETTER_HEIGHT_TWIPS: u32 = 15840;
/// ISO A4 page size (twips).
pub(crate) const PAGE_A4_WIDTH_TWIPS: u32 = 11906;
pub(crate) const PAGE_A4_HEIGHT_TWIPS: u32 = 16838;

pub(crate) fn next_bookmark_name(doc: &Document) -> String {
    let mut n = 1u32;
    loop {
        let name = format!("_OrchidBm{n}");
        if !doc.bookmarks.iter().any(|b| b.name == name) {
            return name;
        }
        n += 1;
    }
}

pub(crate) fn next_comment_id(doc: &Document) -> u32 {
    doc.comments
        .iter()
        .map(|c| c.id)
        .chain(doc.comment_ranges.iter().map(|r| r.id))
        .max()
        .map(|m| m.saturating_add(1))
        .unwrap_or(0)
}

pub(crate) fn comment_id_overlapping(doc: &Document, lo: usize, hi: usize) -> Option<u32> {
    let hi = hi.max(lo);
    let mut best: Option<(u32, usize)> = None; // id, span_len
    for r in &doc.comment_ranges {
        let r0 = r.start_plain.min(r.end_plain);
        let r1 = r.start_plain.max(r.end_plain);
        let overlaps = if r0 == r1 {
            lo <= r0 && r0 <= hi
        } else {
            r0 < hi && r1 > lo
        };
        if !overlaps {
            continue;
        }
        let span = r1.saturating_sub(r0);
        if best.is_none_or(|(_, s)| span < s) {
            best = Some((r.id, span));
        }
    }
    best.map(|(id, _)| id)
}

pub(crate) fn clamp_spacing_twips(v: i32) -> u32 {
    v.clamp(0, SPACING_TWIPS_MAX) as u32
}

pub(crate) fn clamp_margin_twips(v: i32) -> u32 {
    v.clamp(MARGIN_TWIPS_MIN, MARGIN_TWIPS_MAX) as u32
}

pub(crate) fn clamp_header_footer_distance_twips(v: i32) -> u32 {
    v.clamp(0, MARGIN_TWIPS_MAX) as u32
}

pub(crate) fn is_landscape_page(ps: &PageSetup) -> bool {
    ps.width_twips > ps.height_twips
}

pub(crate) fn page_portrait_dims(ps: &PageSetup) -> (u32, u32) {
    if is_landscape_page(ps) {
        (ps.height_twips, ps.width_twips)
    } else {
        (ps.width_twips, ps.height_twips)
    }
}

pub(crate) fn is_a4_page(ps: &PageSetup) -> bool {
    let (w, h) = page_portrait_dims(ps);
    w == PAGE_A4_WIDTH_TWIPS && h == PAGE_A4_HEIGHT_TWIPS
}

pub(crate) fn story_plain_text(paragraphs: &[Paragraph]) -> String {
    paragraphs
        .iter()
        .map(Paragraph::plain_text)
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn paragraphs_from_plain(text: &str) -> Vec<Paragraph> {
    let trimmed = text.trim_end_matches(['\r', '\n']);
    if trimmed.is_empty() {
        return Vec::new();
    }
    trimmed
        .split('\n')
        .map(|line| Paragraph {
            runs: vec![Run {
                text: line.trim_end_matches('\r').to_string(),
                style: RunStyle::default(),
                ..Default::default()
            }],
            ..Default::default()
        })
        .collect()
}

/// Rebuild a story from the plain-text overlay without wiping fields / styles.
///
/// Line `i` merges into existing paragraph `i` when present: `PAGE`/`DATE`/… runs
/// whose cached display text still appears (left-to-right) are kept with their
/// `field` + run props; free text keeps the first non-field run's style shell;
/// paragraph-level properties are preserved. Extra lines become bare paragraphs;
/// empty overlay clears the story.
pub(crate) fn paragraphs_from_plain_preserving(
    existing: &[Paragraph],
    text: &str,
) -> Vec<Paragraph> {
    let trimmed = text.trim_end_matches(['\r', '\n']);
    if trimmed.is_empty() {
        return Vec::new();
    }
    if existing.is_empty() {
        return paragraphs_from_plain(text);
    }
    let lines: Vec<&str> = trimmed
        .split('\n')
        .map(|line| line.trim_end_matches('\r'))
        .collect();
    let mut out = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        if let Some(old) = existing.get(i) {
            out.push(merge_plain_into_paragraph(old, line));
        } else {
            out.push(Paragraph {
                runs: vec![Run {
                    text: (*line).to_string(),
                    style: RunStyle::default(),
                    ..Default::default()
                }],
                ..Default::default()
            });
        }
    }
    out
}

pub(crate) fn merge_plain_into_paragraph(old: &Paragraph, new_plain: &str) -> Paragraph {
    let mut para = old.clone();
    if new_plain == old.plain_text() {
        return para;
    }

    let text_shell = old
        .runs
        .iter()
        .find(|r| r.field.is_none())
        .cloned()
        .unwrap_or_else(|| Run {
            style: old
                .runs
                .first()
                .map(|r| r.style.clone())
                .unwrap_or_default(),
            style_id: old.runs.first().and_then(|r| r.style_id.clone()),
            ..Default::default()
        });

    let field_runs: Vec<&Run> = old.runs.iter().filter(|r| r.field.is_some()).collect();
    if field_runs.is_empty() {
        para.runs = vec![Run {
            text: new_plain.to_string(),
            style: text_shell.style,
            style_id: text_shell.style_id,
            hyperlink: text_shell.hyperlink,
            field: None,
        }];
        return para;
    }

    let mut matches: Vec<(usize, usize, &Run)> = Vec::new();
    let mut search_from = 0;
    for run in field_runs {
        let needle = run.text.as_str();
        if needle.is_empty() {
            continue;
        }
        if let Some(rel) = new_plain[search_from..].find(needle) {
            let start = search_from + rel;
            let end = start + needle.len();
            matches.push((start, end, run));
            search_from = end;
        }
    }

    let mut runs = Vec::new();
    let mut cursor = 0;
    for (start, end, field_run) in matches {
        if start > cursor {
            let chunk = &new_plain[cursor..start];
            if !chunk.is_empty() {
                runs.push(Run {
                    text: chunk.to_string(),
                    style: text_shell.style.clone(),
                    style_id: text_shell.style_id.clone(),
                    hyperlink: text_shell.hyperlink.clone(),
                    field: None,
                });
            }
        }
        runs.push(Run {
            text: new_plain[start..end].to_string(),
            style: field_run.style.clone(),
            style_id: field_run.style_id.clone(),
            hyperlink: field_run.hyperlink.clone(),
            field: field_run.field,
        });
        cursor = end;
    }
    if cursor < new_plain.len() {
        runs.push(Run {
            text: new_plain[cursor..].to_string(),
            style: text_shell.style.clone(),
            style_id: text_shell.style_id.clone(),
            hyperlink: text_shell.hyperlink.clone(),
            field: None,
        });
    }
    if runs.is_empty() {
        runs.push(Run {
            text: new_plain.to_string(),
            style: text_shell.style,
            style_id: text_shell.style_id,
            hyperlink: text_shell.hyperlink,
            field: None,
        });
    }
    para.runs = runs;
    para
}

pub(crate) fn bump_line_spacing(current: u32, delta: i32) -> u32 {
    let effective = if current == 0 { 276 } else { current };
    let idx = LINE_SPACING_PRESETS
        .iter()
        .enumerate()
        .min_by_key(|(_, &p)| (p as i32 - effective as i32).unsigned_abs())
        .map(|(i, _)| i)
        .unwrap_or(1);
    let next = (idx as i32 + delta).clamp(0, LINE_SPACING_PRESETS.len() as i32 - 1) as usize;
    LINE_SPACING_PRESETS[next]
}

const FONT_SIZE_STEPS: &[f32] = &[
    9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 36.0,
];

/// Common Windows-friendly families exposed by the document toolbar.
const FONT_FAMILY_PRESETS: &[&str] = &[
    "Segoe UI",
    "Calibri",
    "Arial",
    "Times New Roman",
    "Consolas",
];

pub(crate) fn next_font_size(current: f32, direction: i32) -> f32 {
    if direction < 0 {
        FONT_SIZE_STEPS
            .iter()
            .rev()
            .find(|&&s| s < current - 0.01)
            .copied()
            .unwrap_or(FONT_SIZE_STEPS[0])
    } else {
        FONT_SIZE_STEPS
            .iter()
            .find(|&&s| s > current + 0.01)
            .copied()
            .unwrap_or(*FONT_SIZE_STEPS.last().unwrap_or(&DEFAULT_FONT_SIZE_PT))
    }
}

pub(crate) fn next_font_family(current: Option<&str>, direction: i32) -> &'static str {
    let n = FONT_FAMILY_PRESETS.len() as i32;
    if n == 0 {
        return "Segoe UI";
    }
    let idx = current
        .and_then(|c| {
            FONT_FAMILY_PRESETS
                .iter()
                .position(|p| p.eq_ignore_ascii_case(c))
        })
        .map(|i| i as i32)
        .unwrap_or(if direction < 0 { 0 } else { -1 });
    let next = if direction < 0 {
        (idx - 1 + n) % n
    } else {
        (idx + 1) % n
    };
    FONT_FAMILY_PRESETS[next as usize]
}

/// Map a toolbar slug (`segoe-ui`) to a preset family name.
pub fn resolve_font_family_slug(slug: &str) -> Option<&'static str> {
    let key = slug.trim().to_ascii_lowercase().replace(' ', "-");
    match key.as_str() {
        "segoe-ui" | "segoe" => Some("Segoe UI"),
        "calibri" => Some("Calibri"),
        "arial" => Some("Arial"),
        "times-new-roman" | "times" | "tnr" => Some("Times New Roman"),
        "consolas" | "mono" => Some("Consolas"),
        _ => FONT_FAMILY_PRESETS
            .iter()
            .copied()
            .find(|p| p.eq_ignore_ascii_case(slug.trim())),
    }
}

pub(crate) fn prev_char_boundary(text: &str, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }
    let mut i = offset.min(text.len()) - 1;
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

pub(crate) fn next_char_boundary(text: &str, offset: usize) -> usize {
    if offset >= text.len() {
        return text.len();
    }
    let mut i = offset + 1;
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

pub(crate) fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Byte offset of the previous word boundary (Windows-style Ctrl+Left).
pub(crate) fn prev_word_boundary(text: &str, offset: usize) -> usize {
    let mut off = offset.min(text.len());
    while off > 0 {
        let prev = prev_char_boundary(text, off);
        let Some(c) = text[prev..off].chars().next() else {
            break;
        };
        if !c.is_whitespace() {
            break;
        }
        off = prev;
    }
    if off == 0 {
        return 0;
    }
    let prev = prev_char_boundary(text, off);
    let Some(c) = text[prev..off].chars().next() else {
        return 0;
    };
    let word = is_word_char(c);
    while off > 0 {
        let prev = prev_char_boundary(text, off);
        let Some(c) = text[prev..off].chars().next() else {
            break;
        };
        if c.is_whitespace() || is_word_char(c) != word {
            break;
        }
        off = prev;
    }
    off
}

/// Inclusive-exclusive byte range of the word (or whitespace run) at `offset`.
pub(crate) fn word_range_at(text: &str, offset: usize) -> (usize, usize) {
    if text.is_empty() {
        return (0, 0);
    }
    let mut probe = offset.min(text.len());
    if probe == text.len() {
        probe = prev_char_boundary(text, probe);
    } else if let Some(c) = text[probe..].chars().next() {
        if c.is_whitespace() && probe > 0 {
            // Prefer the preceding token when the click lands on trailing space.
            let prev = prev_char_boundary(text, probe);
            if let Some(pc) = text[prev..probe].chars().next() {
                if !pc.is_whitespace() {
                    probe = prev;
                }
            }
        }
    }

    let Some(c) = text[probe..].chars().next() else {
        return (text.len(), text.len());
    };
    let class_word = !c.is_whitespace() && is_word_char(c);
    let class_ws = c.is_whitespace();

    let mut start = probe;
    while start > 0 {
        let prev = prev_char_boundary(text, start);
        let Some(pc) = text[prev..start].chars().next() else {
            break;
        };
        let same = if class_ws {
            pc.is_whitespace()
        } else if class_word {
            is_word_char(pc)
        } else {
            !pc.is_whitespace() && !is_word_char(pc)
        };
        if !same {
            break;
        }
        start = prev;
    }

    let mut end = next_char_boundary(text, probe);
    while end < text.len() {
        let Some(nc) = text[end..].chars().next() else {
            break;
        };
        let same = if class_ws {
            nc.is_whitespace()
        } else if class_word {
            is_word_char(nc)
        } else {
            !nc.is_whitespace() && !is_word_char(nc)
        };
        if !same {
            break;
        }
        end = next_char_boundary(text, end);
    }
    (start, end)
}

/// Byte offset of the next word boundary (Windows-style Ctrl+Right).
pub(crate) fn next_word_boundary(text: &str, offset: usize) -> usize {
    let mut off = offset.min(text.len());
    if off >= text.len() {
        return text.len();
    }
    if let Some(c) = text[off..].chars().next() {
        if !c.is_whitespace() {
            let word = is_word_char(c);
            while off < text.len() {
                let Some(c) = text[off..].chars().next() else {
                    break;
                };
                if c.is_whitespace() || is_word_char(c) != word {
                    break;
                }
                off = next_char_boundary(text, off);
            }
        }
    }
    while off < text.len() {
        let Some(c) = text[off..].chars().next() else {
            break;
        };
        if !c.is_whitespace() {
            break;
        }
        off = next_char_boundary(text, off);
    }
    off
}

pub(crate) fn split_runs_at(p: &Paragraph, at: Cursor) -> (Vec<Run>, Vec<Run>) {
    let mut left_runs = Vec::new();
    let mut right_runs = Vec::new();
    if p.runs.is_empty() {
        left_runs.push(Run::default());
    } else {
        for (ri, run) in p.runs.iter().enumerate() {
            if ri < at.run_idx {
                left_runs.push(run.clone());
            } else if ri > at.run_idx {
                right_runs.push(run.clone());
            } else {
                let split = at.byte_offset.min(run.text.len());
                left_runs.push(Run {
                    text: run.text[..split].to_string(),
                    style: run.style.clone(),
                    style_id: run.style_id.clone(),
                    hyperlink: run.hyperlink.clone(),
                    field: None,
                });
                right_runs.push(Run {
                    text: run.text[split..].to_string(),
                    style: run.style.clone(),
                    style_id: run.style_id.clone(),
                    hyperlink: run.hyperlink.clone(),
                    field: None,
                });
            }
        }
    }
    if left_runs.is_empty() {
        left_runs.push(Run::default());
    }
    if right_runs.is_empty() {
        let style = left_runs
            .last()
            .map(|r| r.style.clone())
            .unwrap_or_default();
        let hyperlink = left_runs.last().and_then(|r| r.hyperlink.clone());
        right_runs.push(Run {
            text: String::new(),
            style,
            style_id: left_runs.last().and_then(|r| r.style_id.clone()),
            hyperlink,
            field: None,
        });
    }
    (left_runs, right_runs)
}

pub(crate) fn split_paragraph_blocks(doc: &Document, at: Cursor) -> Result<Vec<Block>> {
    let Block::Paragraph(p) = doc
        .blocks
        .get(at.block_idx)
        .ok_or(ViewerError::EditOutOfBounds)?
    else {
        return Err(ViewerError::EditOutOfBounds);
    };
    let (left_runs, right_runs) = split_runs_at(p, at);
    let left = Paragraph {
        runs: left_runs,
        alignment: p.alignment,
        list: p.list,
        list_level: p.list_level,
        num_id: p.num_id,
        page_break_before: p.page_break_before,
        keep_next: p.keep_next,
        keep_lines: p.keep_lines,
        widow_control: p.widow_control,
        contextual_spacing: p.contextual_spacing,
        bidi: p.bidi,
        suppress_auto_hyphens: p.suppress_auto_hyphens,
        outline_level: p.outline_level,
        style_id: p.style_id.clone(),
        space_before_twips: p.space_before_twips,
        space_after_twips: 0,
        line_spacing: p.line_spacing,
        line_spacing_rule: p.line_spacing_rule,
        indent_left_twips: p.indent_left_twips,
        indent_first_line_twips: p.indent_first_line_twips,
        indent_right_twips: p.indent_right_twips,
        shade_fill: p.shade_fill,
        border_sides: p.border_sides,
        section_properties: None,
        unsupported: p.unsupported.clone(),
    };
    let right = Paragraph {
        runs: right_runs,
        alignment: p.alignment,
        list: ListKind::None,
        list_level: 0,
        num_id: None,
        page_break_before: false,
        keep_next: false,
        keep_lines: false,
        widow_control: false,
        contextual_spacing: false,
        bidi: false,
        suppress_auto_hyphens: false,
        outline_level: None,
        style_id: None,
        space_before_twips: 0,
        space_after_twips: p.space_after_twips,
        line_spacing: p.line_spacing,
        line_spacing_rule: p.line_spacing_rule,
        indent_left_twips: p.indent_left_twips,
        indent_first_line_twips: p.indent_first_line_twips,
        indent_right_twips: p.indent_right_twips,
        shade_fill: p.shade_fill,
        border_sides: p.border_sides,
        section_properties: p.section_properties.clone(),
        unsupported: Vec::new(),
    };
    let mut blocks = doc.blocks.clone();
    blocks[at.block_idx] = Block::Paragraph(left);
    blocks.insert(at.block_idx + 1, Block::Paragraph(right));
    Ok(blocks)
}

pub(crate) fn split_cell_paragraph(doc: &Document, at: Cursor) -> Result<(Vec<Block>, Cursor)> {
    let path = at.cell.ok_or(ViewerError::EditOutOfBounds)?;
    let Block::Table(t) = doc
        .blocks
        .get(at.block_idx)
        .ok_or(ViewerError::EditOutOfBounds)?
    else {
        return Err(ViewerError::EditOutOfBounds);
    };
    let p = t
        .rows
        .get(path.row)
        .ok_or(ViewerError::EditOutOfBounds)?
        .cells
        .get(path.col)
        .ok_or(ViewerError::EditOutOfBounds)?
        .paragraphs
        .get(path.para_idx)
        .ok_or(ViewerError::EditOutOfBounds)?;
    let (left_runs, right_runs) = split_runs_at(p, at);
    let left = Paragraph {
        runs: left_runs,
        alignment: p.alignment,
        list: p.list,
        list_level: p.list_level,
        num_id: p.num_id,
        page_break_before: p.page_break_before,
        keep_next: p.keep_next,
        keep_lines: p.keep_lines,
        widow_control: p.widow_control,
        contextual_spacing: p.contextual_spacing,
        bidi: p.bidi,
        suppress_auto_hyphens: p.suppress_auto_hyphens,
        outline_level: p.outline_level,
        style_id: p.style_id.clone(),
        space_before_twips: p.space_before_twips,
        space_after_twips: 0,
        line_spacing: p.line_spacing,
        line_spacing_rule: p.line_spacing_rule,
        indent_left_twips: p.indent_left_twips,
        indent_first_line_twips: p.indent_first_line_twips,
        indent_right_twips: p.indent_right_twips,
        shade_fill: p.shade_fill,
        border_sides: p.border_sides,
        section_properties: None,
        unsupported: p.unsupported.clone(),
    };
    let right = Paragraph {
        runs: right_runs,
        alignment: p.alignment,
        list: ListKind::None,
        list_level: 0,
        num_id: None,
        page_break_before: false,
        keep_next: false,
        keep_lines: false,
        widow_control: false,
        contextual_spacing: false,
        bidi: false,
        suppress_auto_hyphens: false,
        outline_level: None,
        style_id: None,
        space_before_twips: 0,
        space_after_twips: p.space_after_twips,
        line_spacing: p.line_spacing,
        line_spacing_rule: p.line_spacing_rule,
        indent_left_twips: p.indent_left_twips,
        indent_first_line_twips: p.indent_first_line_twips,
        indent_right_twips: p.indent_right_twips,
        shade_fill: p.shade_fill,
        border_sides: p.border_sides,
        section_properties: p.section_properties.clone(),
        unsupported: Vec::new(),
    };
    let mut blocks = doc.blocks.clone();
    let Block::Table(t) = blocks
        .get_mut(at.block_idx)
        .ok_or(ViewerError::EditOutOfBounds)?
    else {
        return Err(ViewerError::EditOutOfBounds);
    };
    let cell = t
        .rows
        .get_mut(path.row)
        .ok_or(ViewerError::EditOutOfBounds)?
        .cells
        .get_mut(path.col)
        .ok_or(ViewerError::EditOutOfBounds)?;
    cell.paragraphs[path.para_idx] = left;
    cell.paragraphs.insert(path.para_idx + 1, right);
    let caret = Cursor {
        block_idx: at.block_idx,
        cell: Some(CellPath::new(path.row, path.col, path.para_idx + 1)),
        run_idx: 0,
        byte_offset: 0,
    };
    Ok((blocks, caret))
}

pub(crate) fn delete_multi_cell_paragraph(
    doc: &Document,
    start: Cursor,
    end: Cursor,
) -> Result<Vec<Block>> {
    if !start.same_cell(end) {
        return Err(ViewerError::EditOutOfBounds);
    }
    let path = start.cell.ok_or(ViewerError::EditOutOfBounds)?;
    let end_path = end.cell.ok_or(ViewerError::EditOutOfBounds)?;
    let Block::Table(t) = doc
        .blocks
        .get(start.block_idx)
        .ok_or(ViewerError::EditOutOfBounds)?
    else {
        return Err(ViewerError::EditOutOfBounds);
    };
    let paras = &t
        .rows
        .get(path.row)
        .ok_or(ViewerError::EditOutOfBounds)?
        .cells
        .get(path.col)
        .ok_or(ViewerError::EditOutOfBounds)?
        .paragraphs;
    if path.para_idx >= paras.len() || end_path.para_idx >= paras.len() {
        return Err(ViewerError::EditOutOfBounds);
    }
    let start_p = &paras[path.para_idx];
    let end_p = &paras[end_path.para_idx];

    let mut merged_runs = Vec::new();
    for (ri, run) in start_p.runs.iter().enumerate() {
        if ri < start.run_idx {
            merged_runs.push(run.clone());
        } else if ri == start.run_idx {
            merged_runs.push(Run {
                text: run.text[..start.byte_offset.min(run.text.len())].to_string(),
                style: run.style.clone(),
                style_id: run.style_id.clone(),
                hyperlink: run.hyperlink.clone(),
                field: None,
            });
        }
    }
    for (ri, run) in end_p.runs.iter().enumerate() {
        if ri > end.run_idx {
            merged_runs.push(run.clone());
        } else if ri == end.run_idx {
            merged_runs.push(Run {
                text: run.text[end.byte_offset.min(run.text.len())..].to_string(),
                style: run.style.clone(),
                style_id: run.style_id.clone(),
                hyperlink: run.hyperlink.clone(),
                field: None,
            });
        }
    }
    merged_runs.retain(|r| !r.text.is_empty());
    if merged_runs.is_empty() {
        merged_runs.push(Run::default());
    }
    let merged = Paragraph {
        runs: merged_runs,
        alignment: start_p.alignment,
        list: start_p.list,
        list_level: start_p.list_level,
        num_id: start_p.num_id,
        page_break_before: start_p.page_break_before,
        keep_next: start_p.keep_next,
        keep_lines: start_p.keep_lines,
        widow_control: start_p.widow_control,
        contextual_spacing: start_p.contextual_spacing,
        bidi: start_p.bidi,
        suppress_auto_hyphens: start_p.suppress_auto_hyphens,
        outline_level: start_p.outline_level,
        style_id: None,
        space_before_twips: start_p.space_before_twips,
        space_after_twips: start_p.space_after_twips,
        line_spacing: start_p.line_spacing,
        line_spacing_rule: start_p.line_spacing_rule,
        indent_left_twips: start_p.indent_left_twips,
        indent_first_line_twips: start_p.indent_first_line_twips,
        indent_right_twips: start_p.indent_right_twips,
        shade_fill: start_p.shade_fill,
        border_sides: start_p.border_sides,
        section_properties: start_p.section_properties.clone(),
        unsupported: start_p.unsupported.clone(),
    };
    let mut new_paras = Vec::with_capacity(paras.len());
    for (pi, p) in paras.iter().enumerate() {
        if pi < path.para_idx || pi > end_path.para_idx {
            new_paras.push((*p).clone());
        } else if pi == path.para_idx {
            new_paras.push(merged.clone());
        }
    }

    let mut blocks = doc.blocks.clone();
    let Block::Table(t) = blocks
        .get_mut(start.block_idx)
        .ok_or(ViewerError::EditOutOfBounds)?
    else {
        return Err(ViewerError::EditOutOfBounds);
    };
    let cell = t
        .rows
        .get_mut(path.row)
        .ok_or(ViewerError::EditOutOfBounds)?
        .cells
        .get_mut(path.col)
        .ok_or(ViewerError::EditOutOfBounds)?;
    cell.paragraphs = new_paras;
    Ok(blocks)
}

pub(crate) fn delete_multi_paragraph(
    doc: &Document,
    start: Cursor,
    end: Cursor,
) -> Result<Vec<Block>> {
    let Block::Paragraph(start_p) = doc
        .blocks
        .get(start.block_idx)
        .ok_or(ViewerError::EditOutOfBounds)?
    else {
        return Err(ViewerError::EditOutOfBounds);
    };
    let Block::Paragraph(end_p) = doc
        .blocks
        .get(end.block_idx)
        .ok_or(ViewerError::EditOutOfBounds)?
    else {
        return Err(ViewerError::EditOutOfBounds);
    };

    let mut merged_runs = Vec::new();
    for (ri, run) in start_p.runs.iter().enumerate() {
        if ri < start.run_idx {
            merged_runs.push(run.clone());
        } else if ri == start.run_idx {
            merged_runs.push(Run {
                text: run.text[..start.byte_offset.min(run.text.len())].to_string(),
                style: run.style.clone(),
                style_id: run.style_id.clone(),
                hyperlink: run.hyperlink.clone(),
                field: None,
            });
        }
    }
    for (ri, run) in end_p.runs.iter().enumerate() {
        if ri > end.run_idx {
            merged_runs.push(run.clone());
        } else if ri == end.run_idx {
            merged_runs.push(Run {
                text: run.text[end.byte_offset.min(run.text.len())..].to_string(),
                style: run.style.clone(),
                style_id: run.style_id.clone(),
                hyperlink: run.hyperlink.clone(),
                field: None,
            });
        }
    }
    merged_runs.retain(|r| !r.text.is_empty());
    if merged_runs.is_empty() {
        merged_runs.push(Run::default());
    }

    let merged = Paragraph {
        runs: merged_runs,
        alignment: start_p.alignment,
        list: start_p.list,
        list_level: start_p.list_level,
        num_id: start_p.num_id,
        page_break_before: start_p.page_break_before,
        keep_next: start_p.keep_next,
        keep_lines: start_p.keep_lines,
        widow_control: start_p.widow_control,
        contextual_spacing: start_p.contextual_spacing,
        bidi: start_p.bidi,
        suppress_auto_hyphens: start_p.suppress_auto_hyphens,
        outline_level: start_p.outline_level,
        style_id: None,
        space_before_twips: start_p.space_before_twips,
        space_after_twips: start_p.space_after_twips,
        line_spacing: start_p.line_spacing,
        line_spacing_rule: start_p.line_spacing_rule,
        indent_left_twips: start_p.indent_left_twips,
        indent_first_line_twips: start_p.indent_first_line_twips,
        indent_right_twips: start_p.indent_right_twips,
        shade_fill: start_p.shade_fill,
        border_sides: start_p.border_sides,
        section_properties: start_p.section_properties.clone(),
        unsupported: start_p.unsupported.clone(),
    };
    let mut blocks = Vec::with_capacity(doc.blocks.len());
    for (bi, block) in doc.blocks.iter().enumerate() {
        if bi < start.block_idx || bi > end.block_idx {
            blocks.push(block.clone());
        } else if bi == start.block_idx {
            blocks.push(Block::Paragraph(merged.clone()));
        }
    }
    Ok(blocks)
}

/// Non-overlapping UTF-8 byte starts of `needle` inside `haystack`.
pub(crate) fn non_overlapping_match_starts(haystack: &str, needle: &str) -> Vec<usize> {
    if needle.is_empty() || haystack.is_empty() {
        return Vec::new();
    }
    let mut starts = Vec::new();
    let mut pos = 0;
    while pos <= haystack.len() {
        match haystack[pos..].find(needle) {
            Some(rel) => {
                let abs = pos + rel;
                starts.push(abs);
                pos = abs + needle.len();
            }
            None => break,
        }
    }
    starts
}
