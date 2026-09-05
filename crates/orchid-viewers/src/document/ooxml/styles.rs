//! Parse `word/styles.xml` for document defaults and named styles.

use std::collections::HashMap;

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::document::model::{
    Alignment, LineSpacingRule, NamedCharacterStyle, NamedParagraphStyle, RunStyle,
    CELL_BORDER_BOTTOM, CELL_BORDER_LEFT, CELL_BORDER_RIGHT, CELL_BORDER_TOP,
};
use crate::error::{Result, ViewerError};

/// Default run style from `<w:docDefaults>` plus named paragraph/character styles.
#[derive(Debug, Clone, Default)]
pub struct StyleDefaults {
    /// Base character style applied when a run omits `<w:rPr>`.
    pub run: RunStyle,
    /// Paragraph styles keyed by `w:styleId`.
    pub paragraph_styles: HashMap<String, NamedParagraphStyle>,
    /// Character styles keyed by `w:styleId`.
    pub character_styles: HashMap<String, NamedCharacterStyle>,
}

/// Parse styles.xml bytes.
///
/// # Errors
///
/// [`ViewerError::DocumentParse`] on malformed XML.
pub fn parse_styles_xml(bytes: &[u8]) -> Result<StyleDefaults> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut defaults = StyleDefaults::default();
    let mut in_doc_defaults = false;
    let mut in_r_pr_default = false;
    let mut in_r_pr = false;
    let mut in_style = false;
    let mut cur_para: Option<NamedParagraphStyle> = None;
    let mut cur_char: Option<NamedCharacterStyle> = None;
    let mut in_style_r_pr = false;
    let mut in_style_p_pr = false;
    let mut in_style_p_bdr = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                match local.as_str() {
                    "docDefaults" => in_doc_defaults = true,
                    "rPrDefault" if in_doc_defaults => in_r_pr_default = true,
                    "rPr" if in_r_pr_default => in_r_pr = true,
                    "style" => {
                        in_style = true;
                        let ty = attr_val(&e, "type").unwrap_or_default();
                        let id = attr_val(&e, "styleId").unwrap_or_default();
                        cur_para = None;
                        cur_char = None;
                        if !id.is_empty() {
                            match ty.as_str() {
                                "paragraph" => {
                                    cur_para = Some(NamedParagraphStyle {
                                        style_id: id,
                                        ..Default::default()
                                    });
                                }
                                "character" => {
                                    cur_char = Some(NamedCharacterStyle {
                                        style_id: id,
                                        ..Default::default()
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                    "name" if in_style => {
                        if let Some(n) = attr_val(&e, "val") {
                            if let Some(ref mut s) = cur_para {
                                s.name = n;
                            } else if let Some(ref mut s) = cur_char {
                                s.name = n;
                            }
                        }
                    }
                    "pPr" if in_style && cur_para.is_some() => in_style_p_pr = true,
                    "outlineLvl" if in_style_p_pr => {
                        if let Some(ref mut s) = cur_para {
                            if let Some(v) = attr_val(&e, "val") {
                                if let Ok(lvl) = v.parse::<u8>() {
                                    s.outline_level = Some(lvl.min(8));
                                }
                            }
                        }
                    }
                    "jc" if in_style_p_pr => {
                        if let Some(ref mut s) = cur_para {
                            if let Some(val) = attr_val(&e, "val") {
                                s.paragraph.alignment = Some(parse_alignment(&val));
                            }
                        }
                    }
                    "spacing" if in_style_p_pr => {
                        if let Some(ref mut s) = cur_para {
                            apply_style_spacing(&e, &mut s.paragraph);
                        }
                    }
                    "ind" if in_style_p_pr => {
                        if let Some(ref mut s) = cur_para {
                            apply_style_indent(&e, &mut s.paragraph);
                        }
                    }
                    "shd" if in_style_p_pr => {
                        if let Some(ref mut s) = cur_para {
                            if let Some(rgb) = attr_val(&e, "fill")
                                .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("auto"))
                                .and_then(|v| parse_rgb(&v))
                            {
                                s.paragraph.shade_fill = Some(rgb);
                            }
                        }
                    }
                    "pBdr" if in_style_p_pr => {
                        in_style_p_bdr = true;
                        if let Some(ref mut s) = cur_para {
                            if s.paragraph.border_sides.is_none() {
                                s.paragraph.border_sides = Some(0);
                            }
                        }
                    }
                    "top" if in_style_p_bdr => {
                        apply_style_border_side(&mut cur_para, CELL_BORDER_TOP, &e);
                    }
                    "left" if in_style_p_bdr => {
                        apply_style_border_side(&mut cur_para, CELL_BORDER_LEFT, &e);
                    }
                    "bottom" if in_style_p_bdr => {
                        apply_style_border_side(&mut cur_para, CELL_BORDER_BOTTOM, &e);
                    }
                    "right" if in_style_p_bdr => {
                        apply_style_border_side(&mut cur_para, CELL_BORDER_RIGHT, &e);
                    }
                    "keepNext" if in_style_p_pr => {
                        if let Some(ref mut s) = cur_para {
                            s.paragraph.keep_next = true;
                        }
                    }
                    "keepLines" if in_style_p_pr => {
                        if let Some(ref mut s) = cur_para {
                            s.paragraph.keep_lines = true;
                        }
                    }
                    "widowControl" if in_style_p_pr => {
                        if let Some(ref mut s) = cur_para {
                            s.paragraph.widow_control = true;
                        }
                    }
                    "contextualSpacing" if in_style_p_pr => {
                        if let Some(ref mut s) = cur_para {
                            s.paragraph.contextual_spacing = true;
                        }
                    }
                    "bidi" if in_style_p_pr => {
                        if let Some(ref mut s) = cur_para {
                            s.paragraph.bidi = true;
                        }
                    }
                    "suppressAutoHyphens" if in_style_p_pr => {
                        if let Some(ref mut s) = cur_para {
                            s.paragraph.suppress_auto_hyphens = true;
                        }
                    }
                    "rPr" if in_style && (cur_para.is_some() || cur_char.is_some()) => {
                        in_style_r_pr = true;
                    }
                    "b" if in_r_pr => defaults.run.bold = true,
                    "i" if in_r_pr => defaults.run.italic = true,
                    "u" if in_r_pr => defaults.run.underline = true,
                    "strike" if in_r_pr => defaults.run.strikethrough = true,
                    "dstrike" if in_r_pr => defaults.run.double_strikethrough = true,
                    "vanish" if in_r_pr => defaults.run.vanish = true,
                    "shadow" if in_r_pr => defaults.run.shadow = true,
                    "emboss" if in_r_pr => defaults.run.emboss = true,
                    "imprint" if in_r_pr => defaults.run.imprint = true,
                    "highlight" if in_r_pr => {
                        let val = attr_val(&e, "val").unwrap_or_default();
                        defaults.run.highlight = !val.is_empty() && val != "none";
                    }
                    "vertAlign" if in_r_pr => {
                        let val = attr_val(&e, "val").unwrap_or_default();
                        match val.as_str() {
                            "superscript" => {
                                defaults.run.superscript = true;
                                defaults.run.subscript = false;
                            }
                            "subscript" => {
                                defaults.run.subscript = true;
                                defaults.run.superscript = false;
                            }
                            _ => {}
                        }
                    }
                    "color" if in_r_pr => {
                        if let Some(val) = attr_val(&e, "val") {
                            defaults.run.color = parse_rgb(&val);
                        }
                    }
                    "rFonts" if in_r_pr => {
                        if let Some(ascii) = attr_val(&e, "ascii").or_else(|| attr_val(&e, "hAnsi"))
                        {
                            defaults.run.font_family = Some(ascii);
                        }
                    }
                    "sz" if in_r_pr => {
                        if let Some(val) = attr_val(&e, "val") {
                            if let Ok(half) = val.parse::<f32>() {
                                defaults.run.font_size_pt = Some(half / 2.0);
                            }
                        }
                    }
                    // Style rPr
                    "b" if in_style_r_pr => apply_style_bool(&mut cur_para, &mut cur_char, |r| {
                        r.bold = true;
                    }),
                    "i" if in_style_r_pr => apply_style_bool(&mut cur_para, &mut cur_char, |r| {
                        r.italic = true;
                    }),
                    "u" if in_style_r_pr => apply_style_bool(&mut cur_para, &mut cur_char, |r| {
                        r.underline = true;
                    }),
                    "rFonts" if in_style_r_pr => {
                        if let Some(ascii) = attr_val(&e, "ascii").or_else(|| attr_val(&e, "hAnsi"))
                        {
                            apply_style_bool(&mut cur_para, &mut cur_char, |r| {
                                r.font_family = Some(ascii.clone());
                            });
                        }
                    }
                    "sz" if in_style_r_pr => {
                        if let Some(val) = attr_val(&e, "val") {
                            if let Ok(half) = val.parse::<f32>() {
                                apply_style_bool(&mut cur_para, &mut cur_char, |r| {
                                    r.font_size_pt = Some(half / 2.0);
                                });
                            }
                        }
                    }
                    "color" if in_style_r_pr => {
                        if let Some(val) = attr_val(&e, "val") {
                            let rgb = parse_rgb(&val);
                            apply_style_bool(&mut cur_para, &mut cur_char, |r| {
                                r.color = rgb;
                            });
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                match local.as_str() {
                    "docDefaults" => in_doc_defaults = false,
                    "rPrDefault" => in_r_pr_default = false,
                    "rPr" if in_r_pr => in_r_pr = false,
                    "rPr" if in_style_r_pr => in_style_r_pr = false,
                    "pBdr" if in_style_p_bdr => in_style_p_bdr = false,
                    "pPr" if in_style_p_pr => {
                        in_style_p_pr = false;
                        in_style_p_bdr = false;
                    }
                    "style" => {
                        if let Some(mut s) = cur_para.take() {
                            if s.name.is_empty() {
                                s.name = s.style_id.clone();
                            }
                            defaults.paragraph_styles.insert(s.style_id.clone(), s);
                        }
                        if let Some(mut s) = cur_char.take() {
                            if s.name.is_empty() {
                                s.name = s.style_id.clone();
                            }
                            defaults.character_styles.insert(s.style_id.clone(), s);
                        }
                        in_style = false;
                        in_style_r_pr = false;
                        in_style_p_pr = false;
                        in_style_p_bdr = false;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(ViewerError::DocumentParse(format!("styles.xml: {e}")));
            }
            _ => {}
        }
        buf.clear();
    }
    let _ = in_style;
    Ok(defaults)
}

fn apply_style_bool(
    cur_para: &mut Option<NamedParagraphStyle>,
    cur_char: &mut Option<NamedCharacterStyle>,
    f: impl FnOnce(&mut RunStyle),
) {
    if let Some(ref mut s) = cur_para {
        f(&mut s.run);
    } else if let Some(ref mut s) = cur_char {
        f(&mut s.run);
    }
}

fn apply_style_spacing(
    e: &quick_xml::events::BytesStart<'_>,
    props: &mut crate::document::model::ParagraphStyleProps,
) {
    if let Some(v) = attr_val(e, "before").and_then(|s| s.parse().ok()) {
        props.space_before_twips = Some(v);
    }
    if let Some(v) = attr_val(e, "after").and_then(|s| s.parse().ok()) {
        props.space_after_twips = Some(v);
    }
    if let Some(rule) = attr_val(e, "lineRule") {
        props.line_spacing_rule = Some(match rule.as_str() {
            "exact" => LineSpacingRule::Exact,
            "atLeast" => LineSpacingRule::AtLeast,
            _ => LineSpacingRule::Auto,
        });
    }
    if let Some(v) = attr_val(e, "line").and_then(|s| s.parse().ok()) {
        props.line_spacing = Some(v);
    }
}

fn apply_style_indent(
    e: &quick_xml::events::BytesStart<'_>,
    props: &mut crate::document::model::ParagraphStyleProps,
) {
    if let Some(v) = attr_val(e, "left")
        .or_else(|| attr_val(e, "start"))
        .and_then(|s| s.parse().ok())
    {
        props.indent_left_twips = Some(v);
    }
    if let Some(v) = attr_val(e, "right")
        .or_else(|| attr_val(e, "end"))
        .and_then(|s| s.parse().ok())
    {
        props.indent_right_twips = Some(v);
    }
    if let Some(v) = attr_val(e, "firstLine").and_then(|s| s.parse::<i32>().ok()) {
        props.indent_first_line_twips = Some(v);
    } else if let Some(v) = attr_val(e, "hanging").and_then(|s| s.parse::<i32>().ok()) {
        props.indent_first_line_twips = Some(-v.abs());
    }
}

fn apply_style_border_side(
    cur_para: &mut Option<NamedParagraphStyle>,
    side_bit: u8,
    e: &quick_xml::events::BytesStart<'_>,
) {
    let val = attr_val(e, "val").unwrap_or_else(|| "single".to_string());
    let visible = !matches!(val.to_ascii_lowercase().as_str(), "nil" | "none" | "");
    if !visible {
        return;
    }
    if let Some(ref mut s) = cur_para {
        let sides = s.paragraph.border_sides.get_or_insert(0);
        *sides |= side_bit;
    }
}

fn parse_alignment(val: &str) -> Alignment {
    match val {
        "center" => Alignment::Center,
        "right" | "end" => Alignment::Right,
        "both" | "distribute" => Alignment::Justify,
        _ => Alignment::Left,
    }
}

fn local_name(name: &str) -> String {
    name.rsplit(':').next().unwrap_or(name).to_string()
}

fn attr_val(e: &quick_xml::events::BytesStart<'_>, key: &str) -> Option<String> {
    for a in e.attributes().flatten() {
        if local_name(a.key.as_ref()) == key {
            return Some(a.value.into_owned());
        }
    }
    None
}

fn parse_rgb(val: &str) -> Option<[u8; 3]> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::model::CELL_BORDER_ALL;

    #[test]
    fn parses_doc_defaults() {
        let xml = br#"<?xml version="1.0"?>
        <w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:docDefaults>
            <w:rPrDefault>
              <w:rPr>
                <w:rFonts w:ascii="Calibri"/>
                <w:sz w:val="22"/>
              </w:rPr>
            </w:rPrDefault>
          </w:docDefaults>
        </w:styles>"#;
        let d = parse_styles_xml(xml).unwrap();
        assert_eq!(d.run.font_family.as_deref(), Some("Calibri"));
        assert_eq!(d.run.font_size_pt, Some(11.0));
    }

    #[test]
    fn parses_heading1_named_style() {
        let xml = br#"<?xml version="1.0"?>
        <w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:style w:type="paragraph" w:styleId="Heading1">
            <w:name w:val="heading 1"/>
            <w:pPr><w:outlineLvl w:val="0"/></w:pPr>
            <w:rPr>
              <w:b/>
              <w:sz w:val="32"/>
            </w:rPr>
          </w:style>
        </w:styles>"#;
        let d = parse_styles_xml(xml).unwrap();
        let h1 = d.paragraph_styles.get("Heading1").expect("Heading1");
        assert_eq!(h1.name, "heading 1");
        assert_eq!(h1.outline_level, Some(0));
        assert!(h1.run.bold);
        assert_eq!(h1.run.font_size_pt, Some(16.0));
    }

    #[test]
    fn parses_paragraph_style_ppr_props() {
        let xml = br#"<?xml version="1.0"?>
        <w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:style w:type="paragraph" w:styleId="Title">
            <w:name w:val="Title"/>
            <w:pPr>
              <w:jc w:val="center"/>
              <w:spacing w:before="240" w:after="120" w:line="480" w:lineRule="auto"/>
              <w:ind w:left="720" w:firstLine="360"/>
              <w:shd w:val="clear" w:fill="FFF2CC"/>
              <w:pBdr>
                <w:top w:val="single" w:sz="4" w:space="1" w:color="auto"/>
                <w:left w:val="single" w:sz="4" w:space="1" w:color="auto"/>
                <w:bottom w:val="single" w:sz="4" w:space="1" w:color="auto"/>
                <w:right w:val="single" w:sz="4" w:space="1" w:color="auto"/>
              </w:pBdr>
              <w:keepNext/>
            </w:pPr>
          </w:style>
        </w:styles>"#;
        let d = parse_styles_xml(xml).unwrap();
        let t = d.paragraph_styles.get("Title").expect("Title");
        assert_eq!(t.paragraph.alignment, Some(Alignment::Center));
        assert_eq!(t.paragraph.space_before_twips, Some(240));
        assert_eq!(t.paragraph.space_after_twips, Some(120));
        assert_eq!(t.paragraph.line_spacing, Some(480));
        assert_eq!(t.paragraph.line_spacing_rule, Some(LineSpacingRule::Auto));
        assert_eq!(t.paragraph.indent_left_twips, Some(720));
        assert_eq!(t.paragraph.indent_first_line_twips, Some(360));
        assert_eq!(t.paragraph.shade_fill, Some([0xFF, 0xF2, 0xCC]));
        assert_eq!(t.paragraph.border_sides, Some(CELL_BORDER_ALL));
        assert!(t.paragraph.keep_next);
    }

    #[test]
    fn parses_character_named_style() {
        let xml = br#"<?xml version="1.0"?>
        <w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:style w:type="character" w:styleId="Strong">
            <w:name w:val="Strong"/>
            <w:rPr>
              <w:b/>
              <w:color w:val="C00000"/>
            </w:rPr>
          </w:style>
        </w:styles>"#;
        let d = parse_styles_xml(xml).unwrap();
        let s = d.character_styles.get("Strong").expect("Strong");
        assert_eq!(s.name, "Strong");
        assert!(s.run.bold);
        assert_eq!(s.run.color, Some([0xC0, 0x00, 0x00]));
        assert!(d.paragraph_styles.is_empty());
    }
}
