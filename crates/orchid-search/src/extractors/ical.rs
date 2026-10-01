//! iCalendar (`.ics`) and vCard (`.vcf`) text extractors.
//!
//! Folded lines are unfolded. Event and contact fields are kept. Binary
//! properties such as `PHOTO` and `ATTACH` are dropped.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract event text from `.ics` files.
#[derive(Debug, Default, Clone, Copy)]
pub struct IcsExtractor;

/// Extract contact text from `.vcf` files.
#[derive(Debug, Default, Clone, Copy)]
pub struct VcfExtractor;

const ICS_FIELDS: &[&str] = &[
    "SUMMARY",
    "DESCRIPTION",
    "LOCATION",
    "COMMENT",
    "CATEGORIES",
    "ORGANIZER",
    "ATTENDEE",
    "CONTACT",
    "URL",
    "NAME",
];

const VCF_FIELDS: &[&str] = &[
    "FN", "N", "NICKNAME", "EMAIL", "TEL", "ORG", "TITLE", "ROLE", "NOTE", "ADR", "URL",
];

#[async_trait]
impl ContentExtractor for IcsExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime_is(mime, &["text/calendar"])
            || extension
                .is_some_and(|e| e.eq_ignore_ascii_case("ics") || e.eq_ignore_ascii_case("ical"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        read_fields(provider, path, ICS_FIELDS).await
    }
}

#[async_trait]
impl ContentExtractor for VcfExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime_is(mime, &["text/vcard", "text/x-vcard"])
            || extension
                .is_some_and(|e| e.eq_ignore_ascii_case("vcf") || e.eq_ignore_ascii_case("vcard"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        read_fields(provider, path, VCF_FIELDS).await
    }
}

async fn read_fields(
    provider: &dyn orchid_fs::FsProvider,
    path: &orchid_fs::FsPath,
    fields: &[&str],
) -> Result<String> {
    let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
    Ok(property_text(&decode_best_effort(&raw), fields))
}

fn mime_is(mime: Option<&str>, names: &[&str]) -> bool {
    mime.is_some_and(|m| {
        let base = m.split(';').next().unwrap_or(m).trim();
        names.iter().any(|n| base.eq_ignore_ascii_case(n))
    })
}

pub(crate) fn property_text(input: &str, fields: &[&str]) -> String {
    let unfolded = unfold(input);
    let mut out = String::new();
    for line in unfolded.lines() {
        let Some((name, value)) = split_property(line) else {
            continue;
        };
        if !fields.iter().any(|f| f.eq_ignore_ascii_case(name)) {
            continue;
        }
        let value = unescape(value).replace(';', " ");
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(value);
        if out.len() >= MAX_CONTENT_BYTES {
            out.truncate(MAX_CONTENT_BYTES);
            break;
        }
    }
    out
}

fn unfold(input: &str) -> String {
    let mut out = String::new();
    for line in input.split(['\n', '\r']) {
        if line.is_empty() {
            continue;
        }
        if (line.starts_with(' ') || line.starts_with('\t')) && !out.is_empty() {
            out.push_str(line.trim_start());
        } else {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(line);
        }
    }
    out
}

fn split_property(line: &str) -> Option<(&str, &str)> {
    let (left, value) = line.split_once(':')?;
    let name = left.split(';').next()?.trim();
    if name.is_empty() {
        return None;
    }
    Some((name, value))
}

fn unescape(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') | Some('N') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(',') => out.push(','),
                Some(';') => out.push(';'),
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ics_unfolds_description_and_skips_attach() {
        let text = property_text(
            "BEGIN:VCALENDAR\r\nSUMMARY:Standup\r\nDESCRIPTION:Line\r\n  two\\, continued\r\nLOCATION:Room A\r\nATTACH:https://example.com/huge.bin\r\nEND:VCALENDAR\r\n",
            ICS_FIELDS,
        );
        assert!(text.contains("Standup"));
        assert!(text.contains("Line"));
        assert!(text.contains("two, continued"));
        assert!(text.contains("Room A"));
        assert!(!text.contains("huge.bin"));
    }

    #[test]
    fn vcf_keeps_contact_fields_and_skips_photo() {
        let text = property_text(
            "BEGIN:VCARD\r\nFN:Ada Lovelace\r\nN:Lovelace;Ada;;;\r\nEMAIL:ada@example.com\r\nNOTE:Analyst\r\nPHOTO;ENCODING=b:NOTINDEXED\r\nEND:VCARD\r\n",
            VCF_FIELDS,
        );
        assert!(text.contains("Ada Lovelace"));
        assert!(text.contains("ada@example.com"));
        assert!(text.contains("Analyst"));
        assert!(text.contains("Lovelace Ada"));
        assert!(!text.contains("NOTINDEXED"));
    }
}
