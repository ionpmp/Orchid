//! RFC 822 / `.eml` text extractor.
//!
//! Subject, From, To, and text or HTML body parts are indexed. Attachments
//! are skipped. Quoted-printable, base64, and encoded-words (`=?utf-8?B?…?=`)
//! are decoded.

use async_trait::async_trait;
use base64::Engine;

use crate::error::Result;
use crate::extractors::epub_odf::html_text;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from email messages.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmlExtractor;

#[async_trait]
impl ContentExtractor for EmlExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            base.eq_ignore_ascii_case("message/rfc822")
        }) || extension.is_some_and(|e| e.eq_ignore_ascii_case("eml"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(eml_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn eml_text(input: &str) -> String {
    let (headers, body) = split_message(input);
    let mut out = String::new();
    for name in ["Subject", "From", "To", "Cc"] {
        if let Some(value) = header_value(&headers, name) {
            push_line(&mut out, &decode_encoded_words(&value));
        }
    }
    let content_type = header_value(&headers, "Content-Type").unwrap_or_default();
    let encoding = header_value(&headers, "Content-Transfer-Encoding").unwrap_or_default();
    if let Some(boundary) = boundary_of(&content_type) {
        for part in split_parts(body, &boundary) {
            push_part(&mut out, part);
        }
    } else {
        push_decoded_body(&mut out, &content_type, &encoding, body);
    }
    out.trim().to_string()
}

fn push_part(out: &mut String, part: &str) {
    let (headers, body) = split_message(part);
    let content_type = header_value(&headers, "Content-Type").unwrap_or_default();
    let base = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if !(base.starts_with("text/") || base.is_empty()) {
        return;
    }
    let encoding = header_value(&headers, "Content-Transfer-Encoding").unwrap_or_default();
    if let Some(boundary) = boundary_of(&content_type) {
        for nested in split_parts(body, &boundary) {
            push_part(out, nested);
        }
        return;
    }
    push_decoded_body(out, &content_type, &encoding, body);
}

fn push_decoded_body(out: &mut String, content_type: &str, encoding: &str, body: &str) {
    let decoded = decode_transfer(encoding, body);
    let text = if content_type.to_ascii_lowercase().contains("text/html") {
        html_text(&decoded)
    } else {
        decoded.trim().to_string()
    };
    push_line(out, &text);
}

fn split_message(input: &str) -> (String, &str) {
    let body_at = input
        .find("\r\n\r\n")
        .map(|i| i + 4)
        .or_else(|| input.find("\n\n").map(|i| i + 2));
    match body_at {
        Some(at) => (unfold_headers(&input[..at]), &input[at..]),
        None => (unfold_headers(input), ""),
    }
}

fn unfold_headers(input: &str) -> String {
    let head = input
        .split_once("\r\n\r\n")
        .or_else(|| input.split_once("\n\n"))
        .map(|(h, _)| h)
        .unwrap_or(input);
    let mut out = String::new();
    for line in head.split(['\n', '\r']) {
        if line.is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            out.push(' ');
            out.push_str(line.trim());
        } else {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(line);
        }
    }
    out
}

fn header_value(headers: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}:");
    for line in headers.lines() {
        if line.len() >= prefix.len() && line[..prefix.len()].eq_ignore_ascii_case(&prefix) {
            return Some(line[prefix.len()..].trim().to_string());
        }
    }
    None
}

fn boundary_of(content_type: &str) -> Option<String> {
    let lower = content_type.to_ascii_lowercase();
    let idx = lower.find("boundary=")?;
    let rest = content_type[idx + "boundary=".len()..].trim();
    let boundary = if let Some(stripped) = rest.strip_prefix('"') {
        stripped.split('"').next().unwrap_or("").to_string()
    } else {
        rest.split(';').next().unwrap_or("").trim().to_string()
    };
    if boundary.is_empty() {
        None
    } else {
        Some(boundary)
    }
}

fn split_parts<'a>(body: &'a str, boundary: &str) -> Vec<&'a str> {
    let marker = format!("--{boundary}");
    let mut parts = Vec::new();
    for chunk in body.split(&marker) {
        let chunk = chunk.trim_start_matches(['\r', '\n']);
        if chunk.is_empty() || chunk.starts_with("--") {
            continue;
        }
        parts.push(chunk);
    }
    parts
}

fn decode_transfer(encoding: &str, body: &str) -> String {
    let enc = encoding.trim().to_ascii_lowercase();
    if enc == "quoted-printable" {
        decode_quoted_printable(body)
    } else if enc == "base64" {
        let cleaned: String = body.chars().filter(|c| !c.is_whitespace()).collect();
        base64::engine::general_purpose::STANDARD
            .decode(cleaned)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_else(|_| body.to_string())
    } else {
        body.to_string()
    }
}

fn decode_quoted_printable(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' {
            if i + 1 < bytes.len() && (bytes[i + 1] == b'\n' || bytes[i + 1] == b'\r') {
                i += 2;
                if i <= bytes.len()
                    && bytes.get(i - 1) == Some(&b'\r')
                    && bytes.get(i) == Some(&b'\n')
                {
                    i += 1;
                }
                continue;
            }
            if i + 2 < bytes.len() {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                if let Ok(value) = u8::from_str_radix(hex, 16) {
                    out.push(value);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn decode_encoded_words(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' && bytes.get(i + 1) == Some(&b'?') {
            if let Some((text, next)) = decode_one_word(&value[i..]) {
                out.push_str(&text);
                i += next;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn decode_one_word(input: &str) -> Option<(String, usize)> {
    let rest = input.strip_prefix("=?")?;
    let (charset, rest) = rest.split_once('?')?;
    let (encoding, rest) = rest.split_once('?')?;
    let (data, rest) = rest.split_once("?=")?;
    let encoding_rs = encoding_rs::Encoding::for_label(charset.as_bytes())?;
    let bytes = if encoding.eq_ignore_ascii_case("B") {
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .ok()?
    } else if encoding.eq_ignore_ascii_case("Q") {
        decode_quoted_printable(&data.replace('_', " ")).into_bytes()
    } else {
        return None;
    };
    let text = encoding_rs.decode(&bytes).0.into_owned();
    Some((text, input.len() - rest.len()))
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
    fn plain_message_keeps_headers_and_body() {
        let text = eml_text(
            "Subject: Hello\r\nFrom: Ada <ada@example.com>\r\nTo: Bob <bob@example.com>\r\n\r\nChapter one\r\n",
        );
        assert!(text.contains("Hello"));
        assert!(text.contains("ada@example.com"));
        assert!(text.contains("Chapter one"));
    }

    #[test]
    fn skips_attachment_and_decodes_plain_part() {
        let raw = "\
Subject: =?utf-8?B?0J/RgNC40LLQtdGC?=\r\n\
Content-Type: multipart/mixed; boundary=\"bnd\"\r\n\
\r\n\
--bnd\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
Content-Transfer-Encoding: quoted-printable\r\n\
\r\n\
Line =\r\n\
two\r\n\
--bnd\r\n\
Content-Type: application/pdf\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
T1RBVFRBQ0g=\r\n\
--bnd--\r\n";
        let text = eml_text(raw);
        assert!(text.contains("Привет"));
        assert!(text.contains("Line two"));
        assert!(!text.contains("OTATTACH"));
        assert!(!text.contains("T1RBVFRBQ0g"));
    }

    #[test]
    fn html_part_strips_tags() {
        let raw = "\
Subject: Note\r\n\
Content-Type: text/html; charset=utf-8\r\n\
\r\n\
<html><body><p>Visible</p><script>secret()</script></body></html>\r\n";
        let text = eml_text(raw);
        assert!(text.contains("Visible"));
        assert!(!text.contains("secret"));
        assert!(!text.contains("<p>"));
    }
}
