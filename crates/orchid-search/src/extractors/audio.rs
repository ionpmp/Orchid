//! Audio-tag text extractor.
//!
//! ID3 (MP3 and a few related formats) and Vorbis comments (FLAC / Ogg /
//! Opus) contribute title, artist, album, and lyrics to the full-text index.
//! The audio samples themselves are not decoded.

use std::io::Cursor;

use async_trait::async_trait;
use id3::TagLike;

use crate::error::{Result, SearchError};
use crate::extractors::text::MAX_CONTENT_BYTES;
use crate::extractors::ContentExtractor;

/// Index embedded tags from common audio files.
#[derive(Debug, Default, Clone, Copy)]
pub struct AudioTagExtractor;

const ID3_EXTENSIONS: &[&str] = &["mp3", "mp2", "aac", "aiff", "aif", "wav"];
const VORBIS_EXTENSIONS: &[&str] = &["flac", "ogg", "oga", "opus"];

#[async_trait]
impl ContentExtractor for AudioTagExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            base.eq_ignore_ascii_case("audio/mpeg")
                || base.eq_ignore_ascii_case("audio/mp3")
                || base.eq_ignore_ascii_case("audio/flac")
                || base.eq_ignore_ascii_case("audio/ogg")
                || base.eq_ignore_ascii_case("audio/opus")
                || base.eq_ignore_ascii_case("audio/wav")
                || base.eq_ignore_ascii_case("audio/aiff")
        }) || extension.is_some_and(|e| {
            let lower = e.to_ascii_lowercase();
            ID3_EXTENSIONS.contains(&lower.as_str()) || VORBIS_EXTENSIONS.contains(&lower.as_str())
        })
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let extension = path.extension().map(|e| e.to_ascii_lowercase());
        let id3 = extension
            .as_deref()
            .is_some_and(|e| ID3_EXTENSIONS.contains(&e));
        if id3 {
            return extract_id3(provider, path).await;
        }
        extract_vorbis(provider, path).await
    }
}

async fn extract_id3(
    provider: &dyn orchid_fs::FsProvider,
    path: &orchid_fs::FsPath,
) -> Result<String> {
    let path_str = path.to_string();
    let text = if path.is_local() {
        let os_path = path.to_local()?;
        tokio::task::spawn_blocking(move || {
            id3::Tag::read_from_path(&os_path)
                .map(|tag| id3_plain_text(&tag))
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| SearchError::Extraction {
            path: path_str.clone(),
            reason: format!("join: {e}"),
        })?
        .unwrap_or_default()
    } else {
        let bytes = provider.read(path).await?;
        id3::Tag::read_from2(Cursor::new(bytes))
            .map(|tag| id3_plain_text(&tag))
            .unwrap_or_default()
    };
    if text.trim().is_empty() {
        return Err(SearchError::Extraction {
            path: path_str,
            reason: "no audio tags".into(),
        });
    }
    Ok(text)
}

async fn extract_vorbis(
    provider: &dyn orchid_fs::FsProvider,
    path: &orchid_fs::FsPath,
) -> Result<String> {
    let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES.min(1024 * 1024)).await?;
    let extension = path.extension().map(|e| e.to_ascii_lowercase());
    let text = match extension.as_deref() {
        Some("flac") => text_from_flac(&raw),
        _ => text_from_ogg(&raw),
    };
    if text.trim().is_empty() {
        return Err(SearchError::Extraction {
            path: path.to_string(),
            reason: "no audio tags".into(),
        });
    }
    Ok(text)
}

fn id3_plain_text(tag: &id3::Tag) -> String {
    let mut out = String::new();
    push_line(&mut out, tag.title());
    push_line(&mut out, tag.artist());
    push_line(&mut out, tag.album_artist());
    push_line(&mut out, tag.album());
    push_line(&mut out, tag.genre());
    for comment in tag.comments() {
        push_line(&mut out, Some(comment.text.as_str()));
    }
    for lyrics in tag.lyrics() {
        push_line(&mut out, Some(lyrics.text.as_str()));
    }
    for frame in tag.synchronised_lyrics() {
        for line in &frame.content {
            push_line(&mut out, Some(line.1.as_str()));
        }
    }
    out
}

fn text_from_flac(bytes: &[u8]) -> String {
    if bytes.len() < 8 || &bytes[..4] != b"fLaC" {
        return String::new();
    }
    let mut off = 4usize;
    while off + 4 <= bytes.len() {
        let is_last = bytes[off] & 0x80 != 0;
        let block_type = bytes[off] & 0x7f;
        let len = u32::from_be_bytes([0, bytes[off + 1], bytes[off + 2], bytes[off + 3]]) as usize;
        off += 4;
        if off + len > bytes.len() {
            break;
        }
        if block_type == 4 {
            return text_from_vorbis_comment(&bytes[off..off + len]);
        }
        off += len;
        if is_last {
            break;
        }
    }
    String::new()
}

fn text_from_ogg(bytes: &[u8]) -> String {
    let mut i = 0usize;
    let mut out = String::new();
    while i + 27 < bytes.len() {
        if &bytes[i..i + 4] != b"OggS" {
            i += 1;
            continue;
        }
        let nseg = bytes[i + 26] as usize;
        let table = i + 27;
        if table + nseg > bytes.len() {
            break;
        }
        let body_start = table + nseg;
        let body_len: usize = bytes[table..body_start].iter().map(|b| *b as usize).sum();
        let body_end = body_start + body_len;
        if body_end > bytes.len() {
            break;
        }
        let body = &bytes[body_start..body_end];
        let comment = if body.len() > 7 && body[0] == 3 && &body[1..7] == b"vorbis" {
            text_from_vorbis_comment(&body[7..])
        } else if body.len() > 8 && &body[..8] == b"OpusTags" {
            text_from_vorbis_comment(&body[8..])
        } else {
            String::new()
        };
        if !comment.is_empty() {
            push_line(&mut out, Some(comment.as_str()));
        }
        i = body_end;
    }
    out
}

fn text_from_vorbis_comment(data: &[u8]) -> String {
    if data.len() < 8 {
        return String::new();
    }
    let vendor_len = u32::from_le_bytes(data[0..4].try_into().unwrap_or([0; 4])) as usize;
    let mut off = 4usize.saturating_add(vendor_len);
    if off + 4 > data.len() {
        return String::new();
    }
    let count = u32::from_le_bytes(data[off..off + 4].try_into().unwrap_or([0; 4])) as usize;
    off += 4;
    let mut out = String::new();
    for _ in 0..count {
        if off + 4 > data.len() {
            break;
        }
        let len = u32::from_le_bytes(data[off..off + 4].try_into().unwrap_or([0; 4])) as usize;
        off += 4;
        if off + len > data.len() {
            break;
        }
        let entry = String::from_utf8_lossy(&data[off..off + len]);
        off += len;
        let Some((key, value)) = entry.split_once('=') else {
            continue;
        };
        if useful_vorbis_key(key) {
            push_line(&mut out, Some(value.trim()));
        }
    }
    out
}

fn useful_vorbis_key(key: &str) -> bool {
    let key = key.to_ascii_uppercase();
    matches!(
        key.as_str(),
        "TITLE" | "ARTIST" | "ALBUM" | "ALBUMARTIST" | "GENRE" | "COMMENT" | "DESCRIPTION"
    ) || key.contains("LYRIC")
}

fn push_line(out: &mut String, value: Option<&str>) {
    let Some(value) = value.map(str::trim).filter(|s| !s.is_empty()) else {
        return;
    };
    if out.len() >= MAX_CONTENT_BYTES {
        return;
    }
    if !out.is_empty() {
        out.push('\n');
    }
    let room = MAX_CONTENT_BYTES.saturating_sub(out.len());
    let take = value.chars().take(room).collect::<String>();
    out.push_str(&take);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id3_collects_title_artist_and_lyrics() {
        let mut tag = id3::Tag::new();
        tag.set_title("Sunrise");
        tag.set_artist("Ada");
        tag.add_frame(id3::frame::Lyrics {
            lang: "eng".into(),
            description: String::new(),
            text: "line one".into(),
        });
        let text = id3_plain_text(&tag);
        assert!(text.contains("Sunrise"));
        assert!(text.contains("Ada"));
        assert!(text.contains("line one"));
    }

    #[test]
    fn flac_comment_keeps_title_and_lyrics() {
        let vendor = b"orchid";
        let title = b"TITLE=Sunrise";
        let lyrics = b"LYRICS=line one";
        let mut packet = Vec::new();
        packet.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        packet.extend_from_slice(vendor);
        packet.extend_from_slice(&2u32.to_le_bytes());
        for comment in [title.as_slice(), lyrics.as_slice()] {
            packet.extend_from_slice(&(comment.len() as u32).to_le_bytes());
            packet.extend_from_slice(comment);
        }
        let len = packet.len() as u32;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"fLaC");
        bytes.extend_from_slice(&[0x00, 0x00, 0x00, 0x22]);
        bytes.extend_from_slice(&[0u8; 34]);
        bytes.extend_from_slice(&[0x84, (len >> 16) as u8, (len >> 8) as u8, len as u8]);
        bytes.extend_from_slice(&packet);
        let text = text_from_flac(&bytes);
        assert!(text.contains("Sunrise"));
        assert!(text.contains("line one"));
    }
}
