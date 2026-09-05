//! Parse `word/styles.xml` for document defaults and named styles.

use std::collections::HashMap;

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::document::model::{NamedCharacterStyle, NamedParagraphStyle, RunStyle};
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
                        if let Some(ascii) =
                            attr_val(&e, "ascii").or_else(|| attr_val(&e, "hAnsi"))
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
                    "pPr" if in_style_p_pr => in_style_p_pr = false,
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
