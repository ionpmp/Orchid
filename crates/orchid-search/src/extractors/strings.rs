//! Apple `.strings` and `.stringsdict` extractor.
//!
//! Keys, translations, and block comments are indexed. A binary plist is
//! skipped.

use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from Apple string tables.
#[derive(Debug, Default, Clone, Copy)]
pub struct StringsExtractor;

#[async_trait]
impl ContentExtractor for StringsExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension
            .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "strings" | "stringsdict"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        let text = decode_best_effort(&raw);
        let ext = path.extension().unwrap_or("");
        Ok(strings_text(&text, ext))
    }
}

pub(crate) fn strings_text(input: &str, extension: &str) -> String {
    if extension.eq_ignore_ascii_case("stringsdict") || looks_like_plist(input) {
        stringsdict_text(input)
    } else {
        strings_table(input)
    }
}

fn looks_like_plist(input: &str) -> bool {
    let probe: String = input.chars().take(800).collect();
    probe.to_ascii_lowercase().contains("<plist")
}

fn strings_table(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index].is_whitespace() {
            index += 1;
            continue;
        }
        if chars[index] == '/' && chars.get(index + 1) == Some(&'/') {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if chars[index] == '/' && chars.get(index + 1) == Some(&'*') {
            index += 2;
            let start = index;
            while index + 1 < chars.len() && !(chars[index] == '*' && chars[index + 1] == '/') {
                index += 1;
            }
            let note: String = chars[start..index.min(chars.len())].iter().collect();
            push_line(&mut out, note.trim());
            index = (index + 2).min(chars.len());
            continue;
        }
        if chars[index] == '"' {
            let key = read_quoted(&chars, &mut index);
            skip_ws(&chars, &mut index);
            if chars.get(index) == Some(&'=') {
                index += 1;
            }
            skip_ws(&chars, &mut index);
            let value = read_quoted(&chars, &mut index);
            while index < chars.len() && chars[index] != ';' && chars[index] != '\n' {
                index += 1;
            }
            if chars.get(index) == Some(&';') {
                index += 1;
            }
            push_line(&mut out, &key);
            push_line(&mut out, &value);
            continue;
        }
        index += 1;
    }
    out.trim().to_string()
}

fn read_quoted(chars: &[char], index: &mut usize) -> String {
    if chars.get(*index) != Some(&'"') {
        return String::new();
    }
    *index += 1;
    let mut out = String::new();
    while *index < chars.len() {
        let ch = chars[*index];
        *index += 1;
        if ch == '"' {
            break;
        }
        if ch == '\\' {
            let Some(next) = chars.get(*index).copied() else {
                break;
            };
            *index += 1;
            match next {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                'u' | 'U' => {
                    let width = if next == 'u' { 4 } else { 8 };
                    let hex: String = chars.iter().skip(*index).take(width).collect();
                    if hex.len() == width {
                        *index += width;
                        if let Some(decoded) =
                            u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)
                        {
                            out.push(decoded);
                        }
                    }
                }
                other => out.push(other),
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn skip_ws(chars: &[char], index: &mut usize) {
    while chars.get(*index).is_some_and(|c| c.is_whitespace()) {
        *index += 1;
    }
}

fn stringsdict_text(xml: &str) -> String {
    if xml.starts_with("bplist") {
        return String::new();
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut capture: Option<String> = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name().as_ref());
                if matches!(local.as_str(), "key" | "string") {
                    capture = Some(String::new());
                }
            }
            Ok(Event::Text(t)) => {
                if let Some(current) = capture.as_mut() {
                    current.push_str(t.as_ref());
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if let Some(current) = capture.as_mut() {
                    current.push(decode_ref(r.as_ref()));
                }
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name().as_ref());
                if capture.is_some() && matches!(local.as_str(), "key" | "string") {
                    if let Some(text) = capture.take() {
                        push_line(&mut out, text.trim());
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
        if out.len() >= MAX_CONTENT_BYTES {
            out.truncate(MAX_CONTENT_BYTES);
            break;
        }
    }
    out.trim().to_string()
}

fn decode_ref(name: &str) -> char {
    match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        _ => ' ',
    }
}

fn local_name(name: &str) -> String {
    name.rsplit(':').next().unwrap_or(name).to_string()
}

fn push_line(out: &mut String, value: &str) {
    let value = value.trim();
    if value.is_empty() || out.len() >= MAX_CONTENT_BYTES {
        return;
    }
    if !out.is_empty() {
        out.push('\n');
    }
    let room = MAX_CONTENT_BYTES.saturating_sub(out.len());
    out.push_str(&value.chars().take(room).collect::<String>());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_keeps_comments_keys_and_unescapes() {
        let text = strings_text(
            "/* Menu title */\n\
             \"Hello\" = \"\u{041F}\u{0440}\u{0438}\u{0432}\u{0435}\u{0442}\";\n\
             \"Path\" = \"C:\\\\Temp\";\n\
             // ignored line comment SECRET\n\
             \"Line\" = \"one\\ntwo\";\n",
            "strings",
        );
        assert!(text.contains("Menu title"), "{text}");
        assert!(text.contains("Hello"), "{text}");
        assert!(text.contains("Привет"), "{text}");
        assert!(text.contains("C:\\Temp"), "{text}");
        assert!(text.contains("one"), "{text}");
        assert!(text.contains("two"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
    }

    #[test]
    fn stringsdict_keeps_keys_and_strings() {
        let text = strings_text(
            r#"<plist><dict>
              <key>items</key>
              <dict>
                <key>one</key>
                <string>One item</string>
              </dict>
            </dict></plist>"#,
            "stringsdict",
        );
        assert!(text.contains("items"), "{text}");
        assert!(text.contains("one"), "{text}");
        assert!(text.contains("One item"), "{text}");
        assert!(!text.contains("plist"), "{text}");
        assert!(!text.contains("dict"), "{text}");
    }
}
