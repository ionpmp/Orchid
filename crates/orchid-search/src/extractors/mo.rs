//! GNU gettext binary catalog extractor.
//!
//! Message ids and translations are indexed. The empty-id header
//! (charset and project metadata) is not. Plural forms are separate strings.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::MAX_CONTENT_BYTES;
use crate::extractors::ContentExtractor;

const MO_MAGIC: u32 = 0x9504_12de;
const MO_MAGIC_SWAPPED: u32 = 0xde12_0495;
const MAX_MESSAGES: usize = 100_000;

/// Extract searchable text from compiled gettext catalogs.
#[derive(Debug, Default, Clone, Copy)]
pub struct MoExtractor;

#[async_trait]
impl ContentExtractor for MoExtractor {
    fn can_handle(&self, _mime: Option<&str>, extension: Option<&str>) -> bool {
        extension.is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "mo" | "gmo"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(mo_text(&raw))
    }
}

pub(crate) fn mo_text(raw: &[u8]) -> String {
    let Some((big_endian, count, originals, translations)) = header(raw) else {
        return String::new();
    };
    let encoding = (0..count)
        .find_map(|index| {
            let original = read_string(raw, big_endian, originals, index)?;
            if original.is_empty() {
                read_string(raw, big_endian, translations, index)
            } else {
                None
            }
        })
        .as_deref()
        .map_or(encoding_rs::UTF_8, charset_of);
    let mut out = String::new();
    for index in 0..count {
        let Some(original) = read_string(raw, big_endian, originals, index) else {
            continue;
        };
        if original.is_empty() {
            continue;
        }
        let Some(translation) = read_string(raw, big_endian, translations, index) else {
            continue;
        };
        push_segments(&mut out, &original, encoding);
        push_segments(&mut out, &translation, encoding);
    }
    out.trim().to_string()
}

fn header(raw: &[u8]) -> Option<(bool, usize, usize, usize)> {
    let stored = read_u32(raw, 0, false)?;
    let big_endian = match stored {
        MO_MAGIC => false,
        MO_MAGIC_SWAPPED => true,
        _ => return None,
    };
    let revision = read_u32(raw, 4, big_endian)?;
    if revision > 1 {
        return None;
    }
    let count = read_u32(raw, 8, big_endian)? as usize;
    if count > MAX_MESSAGES {
        return None;
    }
    let originals = read_u32(raw, 12, big_endian)? as usize;
    let translations = read_u32(raw, 16, big_endian)? as usize;
    Some((big_endian, count, originals, translations))
}

fn read_u32(raw: &[u8], at: usize, big_endian: bool) -> Option<u32> {
    let bytes: [u8; 4] = raw.get(at..at + 4)?.try_into().ok()?;
    Some(if big_endian {
        u32::from_be_bytes(bytes)
    } else {
        u32::from_le_bytes(bytes)
    })
}

fn read_string(raw: &[u8], big_endian: bool, table: usize, index: usize) -> Option<Vec<u8>> {
    let at = table.checked_add(index.checked_mul(8)?)?;
    let len = read_u32(raw, at, big_endian)? as usize;
    let offset = read_u32(raw, at + 4, big_endian)? as usize;
    Some(raw.get(offset..offset.checked_add(len)?)?.to_vec())
}

fn charset_of(header: &[u8]) -> &'static encoding_rs::Encoding {
    let text = String::from_utf8_lossy(header);
    for line in text.split(['\n', '\r']) {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if !key.trim().eq_ignore_ascii_case("content-type") {
            continue;
        }
        for part in value.split(';') {
            let Some((name, label)) = part.split_once('=') else {
                continue;
            };
            if !name.trim().eq_ignore_ascii_case("charset") {
                continue;
            }
            let label = label.trim().trim_matches('"');
            if let Some(encoding) = encoding_rs::Encoding::for_label(label.as_bytes()) {
                return encoding;
            }
        }
    }
    encoding_rs::UTF_8
}

fn push_segments(out: &mut String, bytes: &[u8], encoding: &'static encoding_rs::Encoding) {
    for part in bytes.split(|byte| *byte == 0) {
        if part.is_empty() {
            continue;
        }
        let (text, _, _) = encoding.decode(part);
        push_line(out, text.trim());
    }
}

fn push_line(out: &mut String, value: &str) {
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

    fn build_mo(pairs: &[(&[u8], &[u8])]) -> Vec<u8> {
        let count = pairs.len() as u32;
        let originals = 28u32;
        let translations = originals + count * 8;
        let mut blob = vec![0u8; (translations + count * 8) as usize];
        blob[0..4].copy_from_slice(&MO_MAGIC.to_le_bytes());
        blob[8..12].copy_from_slice(&count.to_le_bytes());
        blob[12..16].copy_from_slice(&originals.to_le_bytes());
        blob[16..20].copy_from_slice(&translations.to_le_bytes());
        let mut payload = Vec::new();
        let mut cursor = blob.len() as u32;
        for (index, (original, translation)) in pairs.iter().enumerate() {
            let orig_at = originals as usize + index * 8;
            blob[orig_at..orig_at + 4].copy_from_slice(&(original.len() as u32).to_le_bytes());
            blob[orig_at + 4..orig_at + 8].copy_from_slice(&cursor.to_le_bytes());
            payload.extend_from_slice(original);
            payload.push(0);
            cursor += original.len() as u32 + 1;
            let trans_at = translations as usize + index * 8;
            blob[trans_at..trans_at + 4].copy_from_slice(&(translation.len() as u32).to_le_bytes());
            blob[trans_at + 4..trans_at + 8].copy_from_slice(&cursor.to_le_bytes());
            payload.extend_from_slice(translation);
            payload.push(0);
            cursor += translation.len() as u32 + 1;
        }
        blob.extend(payload);
        blob
    }

    #[test]
    fn indexes_messages_and_skips_the_header() {
        let header = b"Content-Type: text/plain; charset=UTF-8\n";
        let plural = b"one file\0{} files";
        let plural_ru = "один файл\0{} файлов".as_bytes();
        let raw = build_mo(&[
            (b"", header),
            (b"Open", "Открыть".as_bytes()),
            (plural, plural_ru),
        ]);
        let text = mo_text(&raw);
        assert!(text.contains("Open"), "{text}");
        assert!(text.contains("Открыть"), "{text}");
        assert!(text.contains("one file"), "{text}");
        assert!(text.contains("файлов"), "{text}");
        assert!(!text.contains("charset"), "{text}");
        assert!(mo_text(b"not a catalog").is_empty());
    }
}
