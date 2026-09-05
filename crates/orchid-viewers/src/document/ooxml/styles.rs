//! Parse `word/styles.xml` for document defaults and named paragraph styles.

use std::collections::HashMap;

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::document::model::{NamedParagraphStyle, RunStyle};
use crate::error::{Result, ViewerError};

/// Default run style from `<w:docDefaults>` plus named paragraph styles.
#[derive(Debug, Clone, Default)]
pub struct StyleDefaults {
    /// Base character style applied when a run omits `<w:rPr>`.
    pub run: RunStyle,
    /// Paragraph styles keyed by `w:styleId`.
    pub paragraph_styles: HashMap<String, NamedParagraphStyle>,
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
    let mut cur_style: Option<NamedParagraphStyle> = None;
    let mut in_style_r_pr = false;
    let mut in_style_p_pr = false;

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
                        let is_paragraph =
                            attr_val(&e, "type").as_deref() == Some("paragraph");
                        let id = attr_val(&e, "styleId").unwrap_or_default();
                        if is_paragraph && !id.is_empty() {
                            cur_style = Some(NamedParagraphStyle {
                                style_id: id,
                                ..Default::default()
                            });
                        } else {
                            cur_style = None;
                        }
                    }
                    "name" if in_style => {
                        if let Some(ref mut s) = cur_style {
                            if let Some(n) = attr_val(&e, "val") {
                                s.name = n;
                            }
                        }
                    }
                    "pPr" if in_style && cur_style.is_some() => in_style_p_pr = true,
                    "outlineLvl" if in_style_p_pr => {
                        if let Some(ref mut s) = cur_style {
                            if let Some(v) = attr_val(&e, "val") {
                                if let Ok(lvl) = v.parse::<u8>() {
                                    s.outline_level = Some(lvl.min(8));
                                }
                            }
                        }
                    }
                    "rPr" if in_style && cur_style.is_some() => in_style_r_pr = true,
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
                    "b" if in_style_r_pr => {
                        if let Some(ref mut s) = cur_style {
                            s.run.bold = true;
                        }
                    }
                    "i" if in_style_r_pr => {
                        if let Some(ref mut s) = cur_style {
                            s.run.italic = true;
                        }
                    }
                    "u" if in_style_r_pr => {
                        if let Some(ref mut s) = cur_style {
                            s.run.underline = true;
                        }
                    }
                    "rFonts" if in_style_r_pr => {
                        if let Some(ref mut s) = cur_style {
                            if let Some(ascii) =
                                attr_val(&e, "ascii").or_else(|| attr_val(&e, "hAnsi"))
                            {
                                s.run.font_family = Some(ascii);
                            }
                        }
                    }
                    "sz" if in_style_r_pr => {
                        if let Some(ref mut s) = cur_style {
                            if let Some(val) = attr_val(&e, "val") {
                                if let Ok(half) = val.parse::<f32>() {
                                    s.run.font_size_pt = Some(half / 2.0);
                                }
                            }
                        }
                    }
                    "color" if in_style_r_pr => {
                        if let Some(ref mut s) = cur_style {
                            if let Some(val) = attr_val(&e, "val") {
                                s.run.color = parse_rgb(&val);
                            }
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
                    "pPr" if in_style_p_pr => in_style_p_pr = false,
                    "style" => {
                        if let Some(s) = cur_style.take() {
                            if s.name.is_empty() {
                                let mut s = s;
                                s.name = s.style_id.clone();
                                defaults.paragraph_styles.insert(s.style_id.clone(), s);
                            } else {
                                defaults.paragraph_styles.insert(s.style_id.clone(), s);
                            }
                        }
                        in_style = false;
                        in_style_r_pr = false;
                        in_style_p_pr = false;
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

    #[test]
    fn parses_doc_defaults_font() {
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
}
