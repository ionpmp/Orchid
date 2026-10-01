//! Rich Text Format plain-text extractor.
//!
//! Control words are dropped. Paragraphs, tabs, hex bytes (`\'hh`), and
//! Unicode (`\uN`) are kept. Font tables, pictures, and `\*` destinations
//! are skipped so they do not pollute the index.

use async_trait::async_trait;

use crate::error::{Result, SearchError};
use crate::extractors::text::MAX_CONTENT_BYTES;
use crate::extractors::ContentExtractor;

/// Extract plain text from `.rtf` files.
#[derive(Debug, Default, Clone, Copy)]
pub struct RtfExtractor;

const SKIP_DESTINATIONS: &[&str] = &[
    "fonttbl",
    "colortbl",
    "stylesheet",
    "pict",
    "object",
    "objdata",
    "header",
    "headerl",
    "headerr",
    "headerf",
    "footer",
    "footerl",
    "footerr",
    "footerf",
    "footnote",
    "fldinst",
    "datastore",
    "themedata",
    "latentstyles",
    "listtable",
    "listoverridetable",
    "revtbl",
    "rsidtbl",
    "xmlnstbl",
    "pgdsctbl",
    "wgrffmtfilter",
    "generator",
    "nonshppict",
    "shppict",
    "blipuid",
    "filetbl",
    "colorschememapping",
];

#[async_trait]
impl ContentExtractor for RtfExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| m == "application/rtf" || m == "text/rtf")
            || extension.is_some_and(|e| e.eq_ignore_ascii_case("rtf"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        let text = rtf_to_text(&String::from_utf8_lossy(&raw));
        if text.trim().is_empty() {
            return Err(SearchError::Extraction {
                path: path.to_string(),
                reason: "no document text".into(),
            });
        }
        Ok(text)
    }
}

fn rtf_to_text(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut out = String::new();
    let mut skipping: Vec<bool> = Vec::new();
    let mut uc_skip = 1usize;
    let mut pending_skip = 0usize;

    while i < chars.len() {
        if pending_skip > 0 {
            i = skip_fallback(&chars, i);
            pending_skip -= 1;
            continue;
        }
        match chars[i] {
            '{' => {
                let skip =
                    skipping.last().copied().unwrap_or(false) || ignorable_group(&chars, i + 1);
                skipping.push(skip);
                i += 1;
            }
            '}' => {
                skipping.pop();
                i += 1;
            }
            '\\' => {
                i += 1;
                if i >= chars.len() {
                    break;
                }
                let next = chars[i];
                if next == '*' {
                    if let Some(flag) = skipping.last_mut() {
                        *flag = true;
                    }
                    i += 1;
                    continue;
                }
                if matches!(next, '\\' | '{' | '}') {
                    if !is_skipping(&skipping) {
                        out.push(next);
                    }
                    i += 1;
                    continue;
                }
                if next == '\'' {
                    if let Some((byte, next_i)) = hex_byte(&chars, i) {
                        if !is_skipping(&skipping) {
                            out.push(char::from(byte));
                        }
                        i = next_i;
                        continue;
                    }
                }
                if !next.is_ascii_alphabetic() {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < chars.len() && chars[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let number = take_number(&chars, &mut i);
                if i < chars.len() && chars[i] == ' ' {
                    i += 1;
                }
                if word == "uc" {
                    uc_skip = number.unwrap_or(1).max(0) as usize;
                    continue;
                }
                if word == "u" {
                    if let Some(n) = number {
                        let code = if n < 0 { n + 65536 } else { n };
                        if !is_skipping(&skipping) {
                            if let Some(ch) = char::from_u32(code as u32) {
                                out.push(ch);
                            }
                        }
                        pending_skip = uc_skip;
                    }
                    continue;
                }
                if is_destination(&word) {
                    if let Some(flag) = skipping.last_mut() {
                        *flag = true;
                    }
                    continue;
                }
                if !is_skipping(&skipping) {
                    match word.as_str() {
                        "par" | "line" | "row" => out.push('\n'),
                        "tab" | "cell" => out.push('\t'),
                        "emdash" => out.push('\u{2014}'),
                        "endash" => out.push('\u{2013}'),
                        "bullet" => out.push('\u{2022}'),
                        "lquote" => out.push('\u{2018}'),
                        "rquote" => out.push('\u{2019}'),
                        "ldblquote" => out.push('\u{201c}'),
                        "rdblquote" => out.push('\u{201d}'),
                        _ => {}
                    }
                }
            }
            '\r' | '\n' => i += 1,
            ch => {
                if !is_skipping(&skipping) {
                    out.push(ch);
                }
                i += 1;
            }
        }
    }
    out.trim().to_string()
}

fn is_skipping(stack: &[bool]) -> bool {
    stack.last().copied().unwrap_or(false)
}

fn is_destination(word: &str) -> bool {
    SKIP_DESTINATIONS.contains(&word)
}

fn ignorable_group(chars: &[char], mut i: usize) -> bool {
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    if i >= chars.len() || chars[i] != '\\' {
        return false;
    }
    i += 1;
    if i < chars.len() && chars[i] == '*' {
        return true;
    }
    let start = i;
    while i < chars.len() && chars[i].is_ascii_alphabetic() {
        i += 1;
    }
    let word: String = chars[start..i].iter().collect();
    is_destination(&word)
}

fn hex_byte(chars: &[char], slash_i: usize) -> Option<(u8, usize)> {
    if slash_i + 2 >= chars.len() {
        return None;
    }
    let hex: String = chars[slash_i + 1..slash_i + 3].iter().collect();
    let byte = u8::from_str_radix(&hex, 16).ok()?;
    Some((byte, slash_i + 3))
}

fn take_number(chars: &[char], i: &mut usize) -> Option<i32> {
    if *i >= chars.len() {
        return None;
    }
    let start = *i;
    if chars[*i] == '-' {
        *i += 1;
    }
    let digits = *i;
    while *i < chars.len() && chars[*i].is_ascii_digit() {
        *i += 1;
    }
    if *i == digits {
        *i = start;
        return None;
    }
    chars[start..*i].iter().collect::<String>().parse().ok()
}

fn skip_fallback(chars: &[char], i: usize) -> usize {
    if i >= chars.len() {
        return i;
    }
    if chars[i] == '\\' && i + 1 < chars.len() && chars[i + 1] == '\'' && i + 3 < chars.len() {
        return i + 4;
    }
    i + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_body_and_drops_font_table() {
        let text = rtf_to_text(r"{\rtf1\ansi{\fonttbl{\f0 Calibri;}}Hello \par World\'21}");
        assert!(text.contains("Hello"));
        assert!(text.contains("World!"));
        assert!(!text.contains("Calibri"));
        assert!(text.find("Hello").unwrap() < text.find("World").unwrap());
    }

    #[test]
    fn unicode_and_star_groups() {
        let text = rtf_to_text(r"{\rtf1{\*\generator MsWord}\u1040? tail}");
        assert!(text.contains('А'));
        assert!(text.contains("tail"));
        assert!(!text.contains("MsWord"));
    }

    #[test]
    fn escaped_backslash_and_tab() {
        let text = rtf_to_text(r"{\rtf1 a\tab b\\c}");
        assert!(text.contains("a\tb\\c"));
    }
}
