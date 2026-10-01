//! Windows Registry (`.reg`) text extractor.
//!
//! Key paths and string values are indexed. `dword` and `hex` values are
//! not. UTF-16 exports (the usual Windows Unicode `.reg`) are decoded.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from registry exports.
#[derive(Debug, Default, Clone, Copy)]
pub struct RegExtractor;

#[async_trait]
impl ContentExtractor for RegExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(|e| e.eq_ignore_ascii_case("reg"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(reg_text(&decode_reg(&raw)))
    }
}

pub(crate) fn decode_reg(bytes: &[u8]) -> String {
    if let Some(text) = decode_utf16_bom(bytes) {
        return text;
    }
    decode_best_effort(bytes)
}

pub(crate) fn reg_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if let Some(key) = key_path(line) {
            push_line(&mut out, key);
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        if is_binary_value(value) {
            continue;
        }
        let Some(text) = unquote(value) else {
            continue;
        };
        if let Some(name) = unquote(name.trim()) {
            push_line(&mut out, &name);
        }
        push_line(&mut out, &text);
    }
    out.trim().to_string()
}

fn key_path(line: &str) -> Option<&str> {
    let line = line.strip_prefix('[')?;
    let end = line.find(']')?;
    let key = line[..end].trim();
    if key.is_empty() {
        None
    } else {
        Some(key)
    }
}

fn is_binary_value(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("dword:") || lower.starts_with("hex:") || lower.starts_with("hex(")
}

fn unquote(token: &str) -> Option<String> {
    let token = token.trim();
    if !token.starts_with('"') {
        return None;
    }
    let mut out = String::new();
    let mut chars = token[1..].chars();
    while let Some(ch) = chars.next() {
        if ch == '"' {
            return Some(out);
        }
        if ch == '\\' {
            match chars.next() {
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => {}
            }
            continue;
        }
        out.push(ch);
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn decode_utf16_bom(bytes: &[u8]) -> Option<String> {
    let (rest, le) = if bytes.starts_with(&[0xFF, 0xFE]) {
        (&bytes[2..], true)
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        (&bytes[2..], false)
    } else {
        return None;
    };
    let units: Vec<u16> = rest
        .chunks_exact(2)
        .map(|pair| {
            if le {
                u16::from_le_bytes([pair[0], pair[1]])
            } else {
                u16::from_be_bytes([pair[0], pair[1]])
            }
        })
        .collect();
    Some(String::from_utf16_lossy(&units))
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
    fn keeps_keys_and_strings_and_skips_binary() {
        let text = reg_text(
            "Windows Registry Editor Version 5.00\n\
             \n\
             [HKEY_CURRENT_USER\\Software\\Night]\n\
             \"Title\"=\"Night Drive\"\n\
             \"Path\"=\"C:\\\\Temp\"\n\
             \"Count\"=dword:0000000a\n\
             \"Data\"=hex:01,02,03\n\
             @=\"Default Label\"\n",
        );
        assert!(
            text.contains("HKEY_CURRENT_USER\\Software\\Night"),
            "{text}"
        );
        assert!(text.contains("Title"), "{text}");
        assert!(text.contains("Night Drive"), "{text}");
        assert!(text.contains("C:\\Temp"), "{text}");
        assert!(text.contains("Default Label"), "{text}");
        assert!(!text.contains("0000000a"), "{text}");
        assert!(!text.contains("01,02,03"), "{text}");
        assert!(!text.contains("Registry Editor"), "{text}");
    }

    #[test]
    fn decodes_utf16_le_export() {
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "\"Title\"=\"Night\"\n".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let text = reg_text(&decode_reg(&bytes));
        assert!(text.contains("Title"), "{text}");
        assert!(text.contains("Night"), "{text}");
    }
}
