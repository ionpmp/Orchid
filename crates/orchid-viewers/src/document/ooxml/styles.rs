//! Parse `word/styles.xml` for document defaults.

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::document::model::RunStyle;
use crate::error::{Result, ViewerError};

/// Default run style from `<w:docDefaults>`.
#[derive(Debug, Clone, Default)]
pub struct StyleDefaults {
    /// Base character style applied when a run omits `<w:rPr>`.
    pub run: RunStyle,
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

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let local = local_name(e.name().as_ref());
                match local.as_str() {
                    "docDefaults" => in_doc_defaults = true,
                    "rPrDefault" if in_doc_defaults => in_r_pr_default = true,
                    "rPr" if in_r_pr_default => in_r_pr = true,
                    "b" if in_r_pr => defaults.run.bold = true,
                    "i" if in_r_pr => defaults.run.italic = true,
                    "u" if in_r_pr => defaults.run.underline = true,
                    "strike" | "dstrike" if in_r_pr => defaults.run.strikethrough = true,
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
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                match local.as_str() {
                    "docDefaults" => in_doc_defaults = false,
                    "rPrDefault" => in_r_pr_default = false,
                    "rPr" => in_r_pr = false,
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
}
