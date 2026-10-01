//! Java `.properties` extractor.
//!
//! Keys, values, and comments are indexed. `\uXXXX` escapes are decoded, and
//! a trailing backslash joins the next line.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from Java property files.
#[derive(Debug, Default, Clone, Copy)]
pub struct PropertiesExtractor;

#[async_trait]
impl ContentExtractor for PropertiesExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(|e| e.eq_ignore_ascii_case("properties"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(properties_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn properties_text(input: &str) -> String {
    let mut out = String::new();
    let mut logical = String::new();
    let mut continued = false;
    for raw in input.lines() {
        let line = if continued {
            raw.trim_start()
        } else {
            raw.trim()
        };
        if !continued {
            if line.is_empty() {
                continue;
            }
            if let Some(note) = comment_body(line) {
                push_line(&mut out, &unescape(note));
                continue;
            }
        }
        let (body, cont) = split_continuation(line);
        logical.push_str(body);
        continued = cont;
        if continued {
            continue;
        }
        if let Some((key, value)) = split_prop(&logical) {
            push_line(&mut out, &unescape(&key));
            push_line(&mut out, &unescape(&value));
        }
        logical.clear();
    }
    out.trim().to_string()
}

fn comment_body(line: &str) -> Option<&str> {
    let rest = line.strip_prefix(['#', '!'])?;
    let rest = rest.trim();
    if rest.is_empty() {
        None
    } else {
        Some(rest)
    }
}

fn split_continuation(line: &str) -> (&str, bool) {
    let bytes = line.as_bytes();
    let mut slashes = 0usize;
    let mut index = bytes.len();
    while index > 0 && bytes[index - 1] == b'\\' {
        slashes += 1;
        index -= 1;
    }
    if slashes.is_multiple_of(2) {
        (line, false)
    } else {
        (&line[..index + slashes - 1], true)
    }
}

fn split_prop(line: &str) -> Option<(String, String)> {
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '=' || ch == ':' || ch.is_whitespace() {
            let key = line[..index].trim();
            if key.is_empty() {
                return None;
            }
            let mut rest = line[index + ch.len_utf8()..].trim_start();
            if ch.is_whitespace() {
                if let Some(stripped) = rest.strip_prefix(['=', ':']) {
                    rest = stripped.trim_start();
                }
            }
            return Some((key.to_string(), rest.to_string()));
        }
    }
    None
}

fn unescape(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                if let Some(decoded) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(decoded);
                }
            }
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
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
    fn decodes_unicode_joins_lines_and_keeps_comments() {
        let text = properties_text(
            "# translator note ALPHA\n\
             app.name=Night Drive\n\
             app.path=C:\\\\Temp\n\
             greeting=\\u041F\\u0440\\u0438\\u0432\\u0435\\u0442\n\
             desc=Line one \\\n\
             \x20 line two\n",
        );
        assert!(text.contains("ALPHA"), "{text}");
        assert!(text.contains("app.name"), "{text}");
        assert!(text.contains("Night Drive"), "{text}");
        assert!(text.contains("C:\\Temp"), "{text}");
        assert!(text.contains("Привет"), "{text}");
        assert!(text.contains("Line one"), "{text}");
        assert!(text.contains("line two"), "{text}");
        assert!(!text.contains("\\u041F"), "{text}");
    }
}
