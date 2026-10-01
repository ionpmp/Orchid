//! BitTorrent metainfo extractor.
//!
//! Display names, comments, announce URLs, and file paths are indexed.
//! The `pieces` hash blob is not.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::MAX_CONTENT_BYTES;
use crate::extractors::ContentExtractor;

/// Extract searchable names and paths from `.torrent` files.
#[derive(Debug, Default, Clone, Copy)]
pub struct TorrentExtractor;

#[async_trait]
impl ContentExtractor for TorrentExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            base.eq_ignore_ascii_case("application/x-bittorrent")
        }) || extension.is_some_and(|ext| ext.eq_ignore_ascii_case("torrent"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(torrent_text(&raw))
    }
}

pub(crate) fn torrent_text(raw: &[u8]) -> String {
    let mut out = String::new();
    let mut pos = 0usize;
    let _ = walk(raw, &mut pos, &mut out, None);
    out.trim().to_string()
}

fn walk(raw: &[u8], pos: &mut usize, out: &mut String, key: Option<&[u8]>) -> Option<()> {
    if key.is_some_and(|name| name == b"pieces") {
        return skip_value(raw, pos);
    }
    match *raw.get(*pos)? {
        b'd' => {
            *pos += 1;
            while raw.get(*pos).is_some_and(|byte| *byte != b'e') {
                let before = *pos;
                let name = parse_string(raw, pos)?;
                walk(raw, pos, out, Some(&name))?;
                if *pos <= before {
                    return None;
                }
            }
            if raw.get(*pos) != Some(&b'e') {
                return None;
            }
            *pos += 1;
            Some(())
        }
        b'l' => {
            *pos += 1;
            while raw.get(*pos).is_some_and(|byte| *byte != b'e') {
                let before = *pos;
                walk(raw, pos, out, key)?;
                if *pos <= before {
                    return None;
                }
            }
            if raw.get(*pos) != Some(&b'e') {
                return None;
            }
            *pos += 1;
            Some(())
        }
        b'i' => skip_int(raw, pos),
        b'0'..=b'9' => {
            let value = parse_string(raw, pos)?;
            if is_indexed_key(key) {
                push_line(out, &String::from_utf8_lossy(&value));
            }
            Some(())
        }
        _ => None,
    }
}

fn skip_value(raw: &[u8], pos: &mut usize) -> Option<()> {
    match *raw.get(*pos)? {
        b'd' | b'l' | b'i' | b'0'..=b'9' => walk(raw, pos, &mut String::new(), None),
        _ => None,
    }
}

fn skip_int(raw: &[u8], pos: &mut usize) -> Option<()> {
    if raw.get(*pos) != Some(&b'i') {
        return None;
    }
    *pos += 1;
    let start = *pos;
    if raw.get(*pos) == Some(&b'-') {
        *pos += 1;
    }
    let digits = *pos;
    while raw.get(*pos).is_some_and(|byte| byte.is_ascii_digit()) {
        *pos += 1;
    }
    if *pos == digits || raw.get(*pos) != Some(&b'e') || *pos == start {
        return None;
    }
    *pos += 1;
    Some(())
}

fn parse_string(raw: &[u8], pos: &mut usize) -> Option<Vec<u8>> {
    let start = *pos;
    while raw.get(*pos).is_some_and(|byte| byte.is_ascii_digit()) {
        *pos += 1;
    }
    if *pos == start || raw.get(*pos) != Some(&b':') {
        return None;
    }
    let len: usize = std::str::from_utf8(&raw[start..*pos]).ok()?.parse().ok()?;
    *pos += 1;
    let end = (*pos).checked_add(len)?;
    let bytes = raw.get(*pos..end)?.to_vec();
    *pos = end;
    Some(bytes)
}

fn is_indexed_key(key: Option<&[u8]>) -> bool {
    let Some(key) = key else {
        return false;
    };
    let key = String::from_utf8_lossy(key);
    matches!(
        key.to_ascii_lowercase().as_str(),
        "name"
            | "name.utf-8"
            | "comment"
            | "comment.utf-8"
            | "path"
            | "path.utf-8"
            | "announce"
            | "announce-list"
            | "publisher"
            | "publisher-url"
            | "publisher.utf-8"
            | "created by"
            | "created by.utf-8"
            | "source"
            | "url-list"
    )
}

fn push_line(out: &mut String, value: &str) {
    let value = value.trim();
    if value.is_empty() || value.contains('\0') || out.len() >= MAX_CONTENT_BYTES {
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

    fn bstr(text: &str) -> String {
        format!("{}:{text}", text.len())
    }

    #[test]
    fn indexes_names_and_skips_piece_hashes() {
        let raw = format!(
            "d8:announce{url}7:comment{comment}4:infod5:filesld6:lengthi1e4:pathl{dir}{file}eee4:name{name}6:pieces{pieces}ee",
            url = bstr("http://tracker.example/announce"),
            comment = bstr("Night drive"),
            dir = bstr("music"),
            file = bstr("song.mp3"),
            name = bstr("orchid.iso"),
            pieces = bstr("SECRETBIN"),
        );
        let text = torrent_text(raw.as_bytes());
        assert!(text.contains("tracker.example"), "{text}");
        assert!(text.contains("Night drive"), "{text}");
        assert!(text.contains("music"), "{text}");
        assert!(text.contains("song.mp3"), "{text}");
        assert!(text.contains("orchid.iso"), "{text}");
        assert!(!text.contains("SECRETBIN"), "{text}");
        assert!(torrent_text(b"not a torrent").is_empty());
    }
}
