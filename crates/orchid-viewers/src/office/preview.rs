// Sheet grid and slide-card preview.

pub(crate) struct Preview {
    html: String,
    info: String,
}

pub(crate) struct SheetBook {
    sheets: Vec<SheetPage>,
    info: String,
}

pub(crate) enum OfficePreview {
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

fn split_count(text: &str, cap: u32) -> u32 {
    let Some(number) = plain_preview_number(text) else {
        return 0;
    };
    if number <= 0.0 {
        return 0;
    }
    number.min(f64::from(cap)).trunc() as u32
}

fn pane_splits(event: &quick_xml::events::BytesStart<'_>) -> (u32, u32) {
    let state = attr(event, "state");
    if !state.eq_ignore_ascii_case("frozen") && !state.eq_ignore_ascii_case("frozenSplit") {
        return (0, 0);
    }
    (
        split_count(&attr(event, "ySplit"), 8),
        split_count(&attr(event, "xSplit"), 4),
    )
}

fn frozen_splits(xml: &str) -> (u32, u32) {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut in_view = false;
    let mut saw_view = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "sheetView" && !saw_view {
                    in_view = true;
                    saw_view = true;
                } else if in_view && name == "pane" {
                    return pane_splits(&event);
                }
            }
            Ok(Event::Empty(event)) => {
                let name = local_name(event.name().as_ref());
                if in_view && name == "pane" {
                    return pane_splits(&event);
                }
            }
            Ok(Event::End(event)) => {
                if local_name(event.name().as_ref()) == "sheetView" {
                    in_view = false;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    (0, 0)
}

fn render_sheets<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> std::result::Result<SheetBook, String> {
    let shared = read_entry(archive, "xl/sharedStrings.xml")
        .map(|xml| shared_strings(&xml))
        .unwrap_or_default();
    let formats = read_entry(archive, "xl/styles.xml")
        .map(|xml| preview_formats(&xml))
        .unwrap_or_default();
    let sheets = sheet_entries(archive);
    let mut pages = Vec::new();
    for (name, path) in &sheets {
        let Some(xml) = read_entry(archive, path) else {
            continue;
        };
        let (mut rows, truncated) = parse_sheet(&xml, &shared, &formats);
        drop_hidden_rows(
            &mut rows,
            &sheet_hidden_rows(&xml, &sheet_cells(&xml, &shared)),
        );
        apply_highlights(&mut rows, &conditional_rules(&xml));
        apply_color_scales(&mut rows, &color_scales(&xml));
        apply_data_bars(&mut rows, &data_bars(&xml));
        apply_notes(&mut rows, &load_threaded_notes(archive, path));
        apply_notes(&mut rows, &load_sheet_notes(archive, path));
        apply_sheet_layout(&mut rows, &xml);
        if rows.is_empty() {
            continue;
        }
        let (freeze_rows, freeze_cols) = frozen_splits(&xml);
        pages.push(SheetPage {
            name: name.clone(),
            rows,
            truncated,
            freeze_rows,
            freeze_cols,
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


enum CfOp {
    Gt,
    Lt,
    Eq,
    Ge,
    Le,
    Ne,
}

struct CfExpr {
    ref_col: u32,
    ref_row: u32,
    col_locked: bool,
    row_locked: bool,
    origin_col: u32,
    origin_row: u32,
    op: CfOp,
    threshold: f64,
}

enum CfKind {
    Greater(f64),
    EqualNum(f64),
    EqualText(String),
    Pattern(String),
    Expr(CfExpr),
}

struct CfRule {
    cells: Vec<String>,
    kind: CfKind,
}

fn plain_preview_number(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() || text.len() > 32 {
        return None;
    }
    let mut chars = text.chars();
    let first = chars.next()?;
    let rest = if first == '+' || first == '-' {
        chars.as_str()
    } else if first.is_ascii_digit() {
        text
    } else {
        return None;
    };
    let mut dot = false;
    if rest.is_empty() {
        return None;
    }
    for ch in rest.chars() {
        if ch == '.' {
            if dot {
                return None;
            }
            dot = true;
        } else if !ch.is_ascii_digit() {
            return None;
        }
    }
    text.parse::<f64>().ok().filter(|number| number.is_finite())
}

fn first_quoted(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let start = chars.iter().position(|ch| *ch == '"')?;
    let mut out = String::new();
    let mut index = start + 1;
    while index < chars.len() {
        if chars[index] == '"' {
            if chars.get(index + 1) == Some(&'"') {
                out.push('"');
                index += 2;
                continue;
            }
            return Some(out);
        }
        out.push(chars[index]);
        index += 1;
    }
    None
}

fn sqref_cells(sqref: &str) -> Option<Vec<String>> {
    let mut cells = Vec::new();
    for part in sqref.split_whitespace() {
        let part = part.replace('$', "").to_ascii_uppercase();
        if part.is_empty() {
            continue;
        }
        let extra = if let Some((start, end)) = part.split_once(':') {
            cells_in_range(start, end)?
        } else {
            split_address(&part)?;
            vec![part]
        };
        if cells.len() + extra.len() > 32 {
            return None;
        }
        cells.extend(extra);
    }
    if cells.is_empty() {
        None
    } else {
        Some(cells)
    }
}

fn compile_cf(kind: &str, operator: &str, formula: &str, sqref: &str) -> Option<CfRule> {
    let cells = sqref_cells(sqref)?;
    let kind =
        if kind.eq_ignore_ascii_case("cellIs") && operator.eq_ignore_ascii_case("greaterThan") {
            CfKind::Greater(plain_preview_number(formula)?)
        } else if kind.eq_ignore_ascii_case("cellIs") && operator.eq_ignore_ascii_case("equal") {
            if formula.trim_start().starts_with('"') {
                CfKind::EqualText(first_quoted(formula)?)
            } else {
                CfKind::EqualNum(plain_preview_number(formula)?)
            }
        } else if kind.eq_ignore_ascii_case("containsText") {
            let pattern = first_quoted(formula)?;
            if pattern.chars().count() > 64 {
                return None;
            }
            CfKind::Pattern(pattern)
        } else if kind.eq_ignore_ascii_case("expression") {
            let origin = cells.first()?;
            CfKind::Expr(compile_cf_expr(formula, origin)?)
        } else {
            return None;
        };
    Some(CfRule { cells, kind })
}

fn conditional_rules(xml: &str) -> Vec<CfRule> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut rules = Vec::new();
    let mut sqref = String::new();
    let mut rule_type = String::new();
    let mut operator = String::new();
    let mut formula = String::new();
    let mut in_rule = false;
    let mut in_formula = false;
    let mut saw_formula = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "conditionalFormatting" {
                    sqref = attr(&event, "sqref");
                } else if name == "cfRule" && rules.len() < 8 {
                    in_rule = true;
                    saw_formula = false;
                    formula.clear();
                    rule_type = attr(&event, "type");
                    operator = attr(&event, "operator");
                } else if in_rule && name == "formula" && !saw_formula {
                    in_formula = true;
                    formula.clear();
                }
            }
            Ok(Event::Empty(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "conditionalFormatting" {
                    sqref = attr(&event, "sqref");
                }
            }
            Ok(Event::Text(text)) if in_formula => {
                formula.push_str(&xml_text(text.as_ref()));
            }
            Ok(Event::GeneralRef(entity)) if in_formula => {
                formula.push_str(entity_text(entity.as_ref()));
            }
            Ok(Event::End(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "formula" && in_formula {
                    in_formula = false;
                    saw_formula = true;
                } else if name == "cfRule" && in_rule {
                    in_rule = false;
                    if rules.len() < 8 {
                        if let Some(rule) = compile_cf(&rule_type, &operator, &formula, &sqref) {
                            rules.push(rule);
                        }
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    rules
}

fn compile_cf_expr(formula: &str, origin: &str) -> Option<CfExpr> {
    let (origin_col, origin_row) = split_address(origin)?;
    let text = formula.trim();
    let text = text.strip_prefix('=').unwrap_or(text).trim_start();
    let bytes = text.as_bytes();
    let mut index = 0usize;
    let col_locked = if bytes.get(index) == Some(&b'$') {
        index += 1;
        true
    } else {
        false
    };
    let col_start = index;
    while bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii_alphabetic())
    {
        index += 1;
    }
    if index == col_start {
        return None;
    }
    let ref_col = column_index(&text[col_start..index])?;
    let row_locked = if bytes.get(index) == Some(&b'$') {
        index += 1;
        true
    } else {
        false
    };
    let row_start = index;
    while bytes.get(index).is_some_and(|byte| byte.is_ascii_digit()) {
        index += 1;
    }
    if index == row_start {
        return None;
    }
    let ref_row = text[row_start..index].parse::<u32>().ok()?;
    if ref_col == 0 || ref_row == 0 {
        return None;
    }
    while bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        index += 1;
    }
    let op = if text[index..].starts_with(">=") {
        index += 2;
        CfOp::Ge
    } else if text[index..].starts_with("<=") {
        index += 2;
        CfOp::Le
    } else if text[index..].starts_with("<>") {
        index += 2;
        CfOp::Ne
    } else if text[index..].starts_with('>') {
        index += 1;
        CfOp::Gt
    } else if text[index..].starts_with('<') {
        index += 1;
        CfOp::Lt
    } else if text[index..].starts_with('=') {
        index += 1;
        CfOp::Eq
    } else {
        return None;
    };
    let threshold = plain_preview_number(text[index..].trim())?;
    Some(CfExpr {
        ref_col,
        ref_row,
        col_locked,
        row_locked,
        origin_col,
        origin_row,
        op,
        threshold,
    })
}

fn expr_matches(
    expr: &CfExpr,
    address: &str,
    values: &std::collections::HashMap<String, String>,
) -> bool {
    let Some((col, row)) = split_address(address) else {
        return false;
    };
    let look_col = if expr.col_locked {
        i64::from(expr.ref_col)
    } else {
        i64::from(expr.ref_col) + i64::from(col) - i64::from(expr.origin_col)
    };
    let look_row = if expr.row_locked {
        i64::from(expr.ref_row)
    } else {
        i64::from(expr.ref_row) + i64::from(row) - i64::from(expr.origin_row)
    };
    if look_col < 1 || look_row < 1 || look_col > 16_384 || look_row > 1_048_576 {
        return false;
    }
    let key = format!("{}{look_row}", column_name(look_col as u32));
    let Some(number) = values.get(&key).and_then(|text| plain_preview_number(text)) else {
        return false;
    };
    let gap = (number - expr.threshold).abs();
    match expr.op {
        CfOp::Gt => number > expr.threshold,
        CfOp::Lt => number < expr.threshold,
        CfOp::Eq => gap <= 1e-9,
        CfOp::Ge => number > expr.threshold || gap <= 1e-9,
        CfOp::Le => number < expr.threshold || gap <= 1e-9,
        CfOp::Ne => gap > 1e-9,
    }
}

fn cf_matches(kind: &CfKind, text: &str) -> bool {
    match kind {
        CfKind::Greater(threshold) => {
            plain_preview_number(text).is_some_and(|number| number > *threshold)
        }
        CfKind::EqualNum(threshold) => {
            plain_preview_number(text).is_some_and(|number| (number - *threshold).abs() <= 1e-9)
        }
        CfKind::EqualText(expected) => text.eq_ignore_ascii_case(expected),
        CfKind::Pattern(pattern) => text_pattern(pattern, text),
        CfKind::Expr(_) => false,
    }
}

fn comments_target(rels: &str) -> Option<String> {
    let mut index = 0usize;
    while let Some(at) = rels[index..].find("<Relationship ") {
        let start = index + at;
        let Some(end) = rels[start..].find('>') else {
            break;
        };
        let tag = &rels[start..start + end];
        index = start + end + 1;
        let target = xml_attr(tag, "Target").unwrap_or_default();
        let kind = xml_attr(tag, "Type").unwrap_or_default();
        if !target.is_empty() && kind.to_ascii_lowercase().ends_with("/comments") {
            return Some(target);
        }
    }
    None
}

fn threaded_target(rels: &str) -> Option<String> {
    let mut index = 0usize;
    while let Some(at) = rels[index..].find("<Relationship ") {
        let start = index + at;
        let Some(end) = rels[start..].find('>') else {
            break;
        };
        let tag = &rels[start..start + end];
        index = start + end + 1;
        let target = xml_attr(tag, "Target").unwrap_or_default();
        let kind = xml_attr(tag, "Type").unwrap_or_default();
        if !target.is_empty() && kind.to_ascii_lowercase().ends_with("/threadedcomment") {
            return Some(target);
        }
    }
    None
}

fn load_threaded_notes<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
) -> Vec<(String, String)> {
    let Some(rels_path) = rels_for(path) else {
        return Vec::new();
    };
    let Some(rels) = read_entry(archive, &rels_path) else {
        return Vec::new();
    };
    let Some(target) = threaded_target(&rels) else {
        return Vec::new();
    };
    let Some(xml) = read_entry(archive, &join_target(path, &target)) else {
        return Vec::new();
    };
    threaded_notes(&xml)
}

fn threaded_notes(xml: &str) -> Vec<(String, String)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut notes: Vec<(String, String)> = Vec::new();
    let mut reference = String::new();
    let mut text = String::new();
    let mut in_comment = false;
    let mut in_text = false;
    let mut count = 0u32;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "threadedComment" && count < 32 {
                    in_comment = true;
                    in_text = false;
                    reference = attr(&event, "ref");
                    text.clear();
                } else if in_comment && name == "text" {
                    in_text = true;
                }
            }
            Ok(Event::Text(event)) if in_text => {
                text.push_str(&xml_text(event.as_ref()));
            }
            Ok(Event::GeneralRef(entity)) if in_text => {
                text.push_str(entity_text(entity.as_ref()));
            }
            Ok(Event::End(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "text" {
                    in_text = false;
                } else if name == "threadedComment" && in_comment {
                    in_comment = false;
                    count += 1;
                    let piece: String = text.chars().take(256).collect();
                    let reference = reference.trim().to_ascii_uppercase();
                    if reference.is_empty() || piece.is_empty() {
                        continue;
                    }
                    if let Some((_, existing)) = notes
                        .iter_mut()
                        .find(|(item, _)| item.eq_ignore_ascii_case(&reference))
                    {
                        existing.push('\n');
                        existing.push_str(&piece);
                        *existing = existing.chars().take(256).collect();
                    } else {
                        notes.push((reference, piece));
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    notes
}

fn load_sheet_notes<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    path: &str,
) -> Vec<(String, String)> {
    let Some(rels_path) = rels_for(path) else {
        return Vec::new();
    };
    let Some(rels) = read_entry(archive, &rels_path) else {
        return Vec::new();
    };
    let Some(target) = comments_target(&rels) else {
        return Vec::new();
    };
    let Some(xml) = read_entry(archive, &join_target(path, &target)) else {
        return Vec::new();
    };
    sheet_comment_notes(&xml)
}

fn sheet_comment_notes(xml: &str) -> Vec<(String, String)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut notes = Vec::new();
    let mut reference = String::new();
    let mut text = String::new();
    let mut in_comment = false;
    let mut in_t = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "comment" && notes.len() < 32 {
                    in_comment = true;
                    in_t = false;
                    reference = attr(&event, "ref");
                    text.clear();
                } else if in_comment && name == "t" {
                    in_t = true;
                }
            }
            Ok(Event::Text(event)) if in_t => {
                text.push_str(&xml_text(event.as_ref()));
            }
            Ok(Event::GeneralRef(entity)) if in_t => {
                text.push_str(entity_text(entity.as_ref()));
            }
            Ok(Event::End(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "t" {
                    in_t = false;
                } else if name == "comment" && in_comment {
                    in_comment = false;
                    let note: String = text.chars().take(256).collect();
                    let reference = reference.trim().to_ascii_uppercase();
                    if !reference.is_empty() && !note.is_empty() && notes.len() < 32 {
                        notes.push((reference, note));
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    notes
}

fn apply_notes(rows: &mut [Vec<SheetCell>], notes: &[(String, String)]) {
    for row in rows {
        for cell in row.iter_mut() {
            let address = cell.address.to_ascii_uppercase();
            if let Some((_, note)) = notes
                .iter()
                .find(|(item, _)| item.eq_ignore_ascii_case(&address))
            {
                cell.note.clone_from(note);
            }
        }
    }
}

enum ScaleStop {
    Min,
    Max,
    Num(f64),
    Percentile(f64),
}

struct ColorScale {
    cells: Vec<String>,
    stops: Vec<(ScaleStop, (u8, u8, u8))>,
}

fn parse_rgb(text: &str) -> Option<(u8, u8, u8)> {
    let text = text.trim();
    let hex = if text.len() == 8 {
        &text[2..]
    } else if text.len() == 6 {
        text
    } else {
        return None;
    };
    if !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    Some((
        u8::from_str_radix(&hex[0..2], 16).ok()?,
        u8::from_str_radix(&hex[2..4], 16).ok()?,
        u8::from_str_radix(&hex[4..6], 16).ok()?,
    ))
}

fn scale_stop(kind: &str, value: &str) -> Option<ScaleStop> {
    if kind.eq_ignore_ascii_case("min") {
        Some(ScaleStop::Min)
    } else if kind.eq_ignore_ascii_case("max") {
        Some(ScaleStop::Max)
    } else if kind.eq_ignore_ascii_case("num") {
        Some(ScaleStop::Num(plain_preview_number(value)?))
    } else if kind.eq_ignore_ascii_case("percentile") {
        let number = plain_preview_number(value)?;
        if (0.0..=100.0).contains(&number) {
            Some(ScaleStop::Percentile(number))
        } else {
            None
        }
    } else {
        None
    }
}

fn note_scale_part(
    name: &str,
    event: &quick_xml::events::BytesStart<'_>,
    cfvos: &mut Vec<ScaleStop>,
    colors: &mut Vec<(u8, u8, u8)>,
    rejected: &mut bool,
) {
    if *rejected {
        return;
    }
    if name == "cfvo" {
        if cfvos.len() >= 3 {
            *rejected = true;
            return;
        }
        match scale_stop(&attr(event, "type"), &attr(event, "val")) {
            Some(stop) => cfvos.push(stop),
            None => *rejected = true,
        }
    } else if name == "color" {
        if colors.len() >= 3 {
            *rejected = true;
            return;
        }
        match parse_rgb(&attr(event, "rgb")) {
            Some(color) => colors.push(color),
            None => *rejected = true,
        }
    }
}

fn color_scales(xml: &str) -> Vec<ColorScale> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut scales = Vec::new();
    let mut sqref = String::new();
    let mut in_scale = false;
    let mut rejected = false;
    let mut cfvos = Vec::new();
    let mut colors = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "conditionalFormatting" {
                    sqref = attr(&event, "sqref");
                } else if name == "cfRule"
                    && attr(&event, "type").eq_ignore_ascii_case("colorScale")
                    && scales.len() < 4
                {
                    in_scale = true;
                    rejected = false;
                    cfvos.clear();
                    colors.clear();
                } else if in_scale {
                    note_scale_part(&name, &event, &mut cfvos, &mut colors, &mut rejected);
                }
            }
            Ok(Event::Empty(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "conditionalFormatting" {
                    sqref = attr(&event, "sqref");
                } else if in_scale {
                    note_scale_part(&name, &event, &mut cfvos, &mut colors, &mut rejected);
                }
            }
            Ok(Event::End(event)) => {
                if local_name(event.name().as_ref()) == "cfRule" && in_scale {
                    in_scale = false;
                    if !rejected
                        && cfvos.len() == colors.len()
                        && (cfvos.len() == 2 || cfvos.len() == 3)
                        && scales.len() < 4
                    {
                        let falling = {
                            let mut previous = None;
                            let mut falls = false;
                            for stop in &cfvos {
                                let value = match stop {
                                    ScaleStop::Num(number) | ScaleStop::Percentile(number) => {
                                        Some(*number)
                                    }
                                    ScaleStop::Min | ScaleStop::Max => None,
                                };
                                if let (Some(earlier), Some(later)) = (previous, value) {
                                    if later + 1e-9 < earlier {
                                        falls = true;
                                    }
                                }
                                previous = value;
                            }
                            falls
                        };
                        if !falling {
                            if let Some(cells) = sqref_cells(&sqref) {
                                scales.push(ColorScale {
                                    cells,
                                    stops: cfvos.drain(..).zip(colors.drain(..)).collect(),
                                });
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    scales
}

fn percentile_inc(sorted: &[f64], percent: f64) -> f64 {
    if sorted.len() == 1 {
        return sorted[0];
    }
    let rank = percent / 100.0 * (sorted.len() - 1) as f64;
    let low = rank.floor() as usize;
    let high = rank.ceil() as usize;
    let fraction = rank - low as f64;
    sorted[low] * (1.0 - fraction) + sorted[high] * fraction
}

fn scale_color(value: f64, stops: &[(f64, (u8, u8, u8))]) -> (u8, u8, u8) {
    if value <= stops[0].0 {
        return stops[0].1;
    }
    if value >= stops[stops.len() - 1].0 {
        return stops[stops.len() - 1].1;
    }
    for pair in stops.windows(2) {
        let (low, low_color) = pair[0];
        let (high, high_color) = pair[1];
        if value > high {
            continue;
        }
        if (high - low).abs() < 1e-12 {
            return high_color;
        }
        let portion = (value - low) / (high - low);
        let channel = |from: u8, to: u8| -> u8 {
            (f64::from(from) + (f64::from(to) - f64::from(from)) * portion).round() as u8
        };
        return (
            channel(low_color.0, high_color.0),
            channel(low_color.1, high_color.1),
            channel(low_color.2, high_color.2),
        );
    }
    stops[stops.len() - 1].1
}

fn resolve_stops(
    stops: &[(ScaleStop, (u8, u8, u8))],
    numbers: &[f64],
) -> Option<Vec<(f64, (u8, u8, u8))>> {
    if numbers.is_empty() {
        return None;
    }
    let mut sorted = numbers.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mut resolved = Vec::with_capacity(stops.len());
    for (kind, color) in stops {
        let value = match kind {
            ScaleStop::Min => sorted[0],
            ScaleStop::Max => sorted[sorted.len() - 1],
            ScaleStop::Num(number) => *number,
            ScaleStop::Percentile(percent) => percentile_inc(&sorted, *percent),
        };
        if !value.is_finite() {
            return None;
        }
        resolved.push((value, *color));
    }
    for pair in resolved.windows(2) {
        if pair[1].0 + 1e-9 < pair[0].0 {
            return None;
        }
    }
    Some(resolved)
}

fn apply_color_scales(rows: &mut [Vec<SheetCell>], scales: &[ColorScale]) {
    let mut numbers = std::collections::HashMap::new();
    for row in rows.iter() {
        for cell in row {
            if let Some(number) = plain_preview_number(&cell.text) {
                numbers.insert(cell.address.to_ascii_uppercase(), number);
            }
        }
    }
    for scale in scales {
        let samples: Vec<f64> = scale
            .cells
            .iter()
            .filter_map(|address| numbers.get(&address.to_ascii_uppercase()).copied())
            .collect();
        let Some(stops) = resolve_stops(&scale.stops, &samples) else {
            continue;
        };
        for row in rows.iter_mut() {
            for cell in row.iter_mut() {
                if cell.has_fill {
                    continue;
                }
                let address = cell.address.to_ascii_uppercase();
                if !scale
                    .cells
                    .iter()
                    .any(|item| item.eq_ignore_ascii_case(&address))
                {
                    continue;
                }
                let Some(number) = numbers.get(&address).copied() else {
                    continue;
                };
                let (red, green, blue) = scale_color(number, &stops);
                cell.fill_r = red;
                cell.fill_g = green;
                cell.fill_b = blue;
                cell.has_fill = true;
            }
        }
    }
}

const DATA_BAR_DEFAULT: (u8, u8, u8) = (0x63, 0x8E, 0xC6);

struct DataBar {
    cells: Vec<String>,
    low: ScaleStop,
    high: ScaleStop,
    color: (u8, u8, u8),
}

fn data_bars(xml: &str) -> Vec<DataBar> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut bars = Vec::new();
    let mut sqref = String::new();
    let mut in_bar = false;
    let mut rejected = false;
    let mut stops = Vec::new();
    let mut color = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event) | Event::Empty(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "conditionalFormatting" {
                    sqref = attr(&event, "sqref");
                } else if name == "cfRule"
                    && attr(&event, "type").eq_ignore_ascii_case("dataBar")
                    && bars.len() < 4
                {
                    in_bar = true;
                    rejected = false;
                    stops.clear();
                    color = None;
                } else if in_bar && name == "cfvo" {
                    if stops.len() >= 2 {
                        rejected = true;
                    } else if let Some(stop) =
                        scale_stop(&attr(&event, "type"), &attr(&event, "val"))
                    {
                        stops.push(stop);
                    } else {
                        rejected = true;
                    }
                } else if in_bar && name == "color" {
                    if color.is_some() {
                        rejected = true;
                    } else if let Some(rgb) = parse_rgb(&attr(&event, "rgb")) {
                        color = Some(rgb);
                    } else {
                        rejected = true;
                    }
                }
            }
            Ok(Event::End(event)) => {
                if local_name(event.name().as_ref()) == "cfRule" && in_bar {
                    in_bar = false;
                    if !rejected && stops.len() == 2 && bars.len() < 4 {
                        let falling = match (&stops[0], &stops[1]) {
                            (
                                ScaleStop::Num(earlier) | ScaleStop::Percentile(earlier),
                                ScaleStop::Num(later) | ScaleStop::Percentile(later),
                            ) => *later + 1e-9 < *earlier,
                            _ => false,
                        };
                        if !falling {
                            if let Some(cells) = sqref_cells(&sqref) {
                                let mut taken = stops.drain(..);
                                let low = taken.next().unwrap_or(ScaleStop::Min);
                                let high = taken.next().unwrap_or(ScaleStop::Max);
                                bars.push(DataBar {
                                    cells,
                                    low,
                                    high,
                                    color: color.unwrap_or(DATA_BAR_DEFAULT),
                                });
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    bars
}

fn stop_number(kind: &ScaleStop, sorted: &[f64]) -> Option<f64> {
    let value = match kind {
        ScaleStop::Min => sorted[0],
        ScaleStop::Max => sorted[sorted.len() - 1],
        ScaleStop::Num(number) => *number,
        ScaleStop::Percentile(percent) => percentile_inc(sorted, *percent),
    };
    value.is_finite().then_some(value)
}

fn bar_percent(number: f64, low: f64, high: f64) -> u8 {
    if (high - low).abs() < 1e-12 {
        return 100;
    }
    let portion = ((number - low) / (high - low)).clamp(0.0, 1.0);
    (portion * 100.0).round() as u8
}

fn apply_data_bars(rows: &mut [Vec<SheetCell>], bars: &[DataBar]) {
    let mut numbers = std::collections::HashMap::new();
    for row in rows.iter() {
        for cell in row {
            if let Some(number) = plain_preview_number(&cell.text) {
                numbers.insert(cell.address.to_ascii_uppercase(), number);
            }
        }
    }
    for bar in bars {
        let samples: Vec<f64> = bar
            .cells
            .iter()
            .filter_map(|address| numbers.get(&address.to_ascii_uppercase()).copied())
            .collect();
        if samples.is_empty() {
            continue;
        }
        let mut sorted = samples;
        sorted.sort_by(f64::total_cmp);
        let Some(low) = stop_number(&bar.low, &sorted) else {
            continue;
        };
        let Some(high) = stop_number(&bar.high, &sorted) else {
            continue;
        };
        if high + 1e-9 < low {
            continue;
        }
        for row in rows.iter_mut() {
            for cell in row.iter_mut() {
                if cell.has_bar {
                    continue;
                }
                let address = cell.address.to_ascii_uppercase();
                if !bar
                    .cells
                    .iter()
                    .any(|item| item.eq_ignore_ascii_case(&address))
                {
                    continue;
                }
                let Some(number) = numbers.get(&address).copied() else {
                    continue;
                };
                cell.bar_pct = bar_percent(number, low, high);
                cell.bar_r = bar.color.0;
                cell.bar_g = bar.color.1;
                cell.bar_b = bar.color.2;
                cell.has_bar = true;
            }
        }
    }
}

fn apply_highlights(rows: &mut [Vec<SheetCell>], rules: &[CfRule]) {
    let mut values = std::collections::HashMap::new();
    for row in rows.iter() {
        for cell in row {
            values.insert(cell.address.to_ascii_uppercase(), cell.text.clone());
        }
    }
    for row in rows {
        for cell in row.iter_mut() {
            let address = cell.address.to_ascii_uppercase();
            for rule in rules {
                if !rule
                    .cells
                    .iter()
                    .any(|item| item.eq_ignore_ascii_case(&address))
                {
                    continue;
                }
                let hit = match &rule.kind {
                    CfKind::Expr(expr) => expr_matches(expr, &address, &values),
                    other => cf_matches(other, &cell.text),
                };
                if hit {
                    cell.highlight = true;
                    break;
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum PreviewFmt {
    General,
    Percent,
    Thousands,
    Date,
}

fn code_preview_fmt(code: &str) -> PreviewFmt {
    match code.trim() {
        "0%" | "0.00%" => PreviewFmt::Percent,
        "#,##0" => PreviewFmt::Thousands,
        "yyyy-mm-dd" => PreviewFmt::Date,
        _ => PreviewFmt::General,
    }
}

fn builtin_preview_fmt(id: u32) -> PreviewFmt {
    match id {
        3 => PreviewFmt::Thousands,
        9 | 10 => PreviewFmt::Percent,
        14 => PreviewFmt::Date,
        _ => PreviewFmt::General,
    }
}

fn preview_formats(xml: &str) -> Vec<PreviewFmt> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut custom = std::collections::HashMap::<u32, PreviewFmt>::new();
    let mut formats = Vec::new();
    let mut in_xfs = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event) | Event::Empty(event)) => {
                let name = local_name(event.name().as_ref());
                if name == "numFmt" && custom.len() < 64 {
                    if let Ok(id) = attr(&event, "numFmtId").parse::<u32>() {
                        custom.insert(id, code_preview_fmt(&attr(&event, "formatCode")));
                    }
                } else if name == "cellXfs" {
                    in_xfs = true;
                } else if name == "xf" && in_xfs && formats.len() < 64 {
                    let id = attr(&event, "numFmtId").parse::<u32>().unwrap_or(0);
                    formats.push(
                        custom
                            .get(&id)
                            .copied()
                            .unwrap_or_else(|| builtin_preview_fmt(id)),
                    );
                }
            }
            Ok(Event::End(event)) => {
                if local_name(event.name().as_ref()) == "cellXfs" {
                    in_xfs = false;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    formats
}

fn style_preview_fmt(style: &str, formats: &[PreviewFmt]) -> PreviewFmt {
    style
        .parse::<usize>()
        .ok()
        .and_then(|index| formats.get(index).copied())
        .unwrap_or(PreviewFmt::General)
}


fn drop_hidden_rows(rows: &mut Vec<Vec<SheetCell>>, hidden: &std::collections::HashSet<u32>) {
    rows.retain(|row| {
        row.iter()
            .find(|cell| !cell.address.is_empty())
            .is_none_or(|cell| {
                split_address(&cell.address).is_none_or(|(_, number)| !hidden.contains(&number))
            })
    });
}

fn parse_sheet(
    xml: &str,
    shared: &[String],
    formats: &[PreviewFmt],
) -> (Vec<Vec<SheetCell>>, bool) {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut kind = String::new();
    let mut style = String::new();
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
                    style = attr(&event, "s");
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
                    if let Some(text) =
                        cell_text(&kind, &value, shared, style_preview_fmt(&style, formats))
                    {
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

fn format_thousands(number: f64) -> Option<String> {
    if !number.is_finite() || number.abs() >= 1e15 {
        return None;
    }
    let rounded = number.round();
    if (number - rounded).abs() > 1e-9 {
        return None;
    }
    let negative = rounded < 0.0;
    let digits = format!("{}", rounded.abs() as u64);
    let mut out = String::new();
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index.is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    let mut out: String = out.chars().rev().collect();
    if negative {
        out.insert(0, '-');
    }
    Some(out)
}

fn format_preview_date(number: f64) -> Option<String> {
    let (year, month, day) = excel_parts(number.trunc())?;
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn preview_number(raw: &str, fmt: PreviewFmt) -> String {
    let Ok(number) = raw.parse::<f64>() else {
        return raw.to_string();
    };
    if !number.is_finite() {
        return raw.to_string();
    }
    match fmt {
        PreviewFmt::General => raw.to_string(),
        PreviewFmt::Percent => {
            let shown = format_calc(number * 100.0);
            if shown.is_empty() {
                raw.to_string()
            } else {
                format!("{shown}%")
            }
        }
        PreviewFmt::Thousands => format_thousands(number).unwrap_or_else(|| raw.to_string()),
        PreviewFmt::Date => format_preview_date(number).unwrap_or_else(|| raw.to_string()),
    }
}

fn cell_text(kind: &str, value: &str, shared: &[String], fmt: PreviewFmt) -> Option<String> {
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
    if kind.is_empty() {
        return Some(preview_number(value, fmt));
    }
    Some(value.to_string())
}

fn place(row: &mut Vec<SheetCell>, col: usize, text: String, address: String) {
    if row.len() <= col {
        row.resize(col + 1, SheetCell::default());
    }
    row[col] = SheetCell {
        text,
        address,
        highlight: false,
        fill_r: 0,
        fill_g: 0,
        fill_b: 0,
        has_fill: false,
        bar_pct: 0,
        bar_r: 0,
        bar_g: 0,
        bar_b: 0,
        has_bar: false,
        note: String::new(),
        span: 1,
        covered: false,
        width_px: 72,
    };
}

fn apply_sheet_layout(rows: &mut [Vec<SheetCell>], xml: &str) {
    let widths = column_widths(xml);
    let merges = merge_refs(xml);
    for row in rows.iter_mut() {
        for cell in row.iter_mut() {
            let Some((col, row_num)) = split_address(&cell.address) else {
                cell.span = 1;
                cell.covered = false;
                cell.width_px = 72;
                continue;
            };
            let col = col.saturating_sub(1) as usize;
            if col >= 32 {
                continue;
            }
            if let Some(merge) = merges
                .iter()
                .find(|merge| merge_covers(merge, col, row_num))
            {
                if col == merge.col && row_num == merge.row {
                    cell.covered = false;
                    cell.span = merge.cols;
                    let mut px = 0u32;
                    for offset in 0..merge.cols {
                        let at = col + offset as usize;
                        if at >= 32 {
                            break;
                        }
                        px = px.saturating_add(column_px(&widths, at));
                    }
                    cell.width_px = px.max(72);
                } else {
                    cell.covered = true;
                    cell.span = 1;
                    cell.width_px = 0;
                }
            } else {
                cell.span = 1;
                cell.covered = false;
                cell.width_px = column_px(&widths, col);
            }
        }
    }
}

struct SheetMerge {
    col: usize,
    row: u32,
    cols: u32,
    rows: u32,
}

fn merge_covers(merge: &SheetMerge, col: usize, row: u32) -> bool {
    col >= merge.col
        && row >= merge.row
        && col < merge.col + merge.cols as usize
        && row < merge.row + merge.rows
}

fn column_px(widths: &[f32], col: usize) -> u32 {
    let excel = widths.get(col).copied().unwrap_or(0.0);
    if excel > 0.0 {
        (excel * 8.0).round() as u32
    } else {
        72
    }
}

fn column_widths(xml: &str) -> Vec<f32> {
    let mut widths = vec![0.0; 32];
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event) | Event::Empty(event)) => {
                if local_name(event.name().as_ref()) != "col" {
                    buf.clear();
                    continue;
                }
                let Some(min) = attr(&event, "min").parse::<u32>().ok() else {
                    buf.clear();
                    continue;
                };
                let max = attr(&event, "max").parse::<u32>().unwrap_or(min);
                let Some(width) = attr(&event, "width").parse::<f32>().ok() else {
                    buf.clear();
                    continue;
                };
                if !width.is_finite() || min == 0 {
                    buf.clear();
                    continue;
                }
                let width = width.clamp(1.0, 40.0);
                let start = min.saturating_sub(1).min(32);
                let end = max.min(32);
                for col in start..end {
                    widths[col as usize] = width;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    widths
}

fn merge_refs(xml: &str) -> Vec<SheetMerge> {
    let mut merges = Vec::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    loop {
        if merges.len() >= 16 {
            break;
        }
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(event) | Event::Empty(event)) => {
                if local_name(event.name().as_ref()) == "mergeCell" {
                    if let Some(merge) = parse_merge(&attr(&event, "ref")) {
                        merges.push(merge);
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    merges
}

fn parse_merge(reference: &str) -> Option<SheetMerge> {
    let (start, end) = reference.split_once(':')?;
    let (c1, r1) = split_address(start)?;
    let (c2, r2) = split_address(end)?;
    let cols = c1.abs_diff(c2) + 1;
    let rows = r1.abs_diff(r2) + 1;
    if cols > 8 || rows > 8 || (cols == 1 && rows == 1) {
        return None;
    }
    let col = c1.min(c2).saturating_sub(1) as usize;
    if col >= 32 {
        return None;
    }
    Some(SheetMerge {
        col,
        row: r1.min(r2),
        cols,
        rows,
    })
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

