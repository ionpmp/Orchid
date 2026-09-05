//! Vorbis-comment lyrics from FLAC / Ogg (Opus / Vorbis).

use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::audio_tags::{plain_text_lines, EmbeddedLyricLine};

const LYRIC_KEYS: &[&str] = &[
    "LYRICS",
    "UNSYNCEDLYRICS",
    "UNSYNCED LYRICS",
    "SYNCEDLYRICS",
    "SYNCED LYRICS",
];

/// Load lyrics from Vorbis comments (FLAC / Ogg Vorbis / Opus).
#[must_use]
pub fn load_vorbis_lyrics(path: &Path) -> Option<Vec<EmbeddedLyricLine>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())?;
    let text = match ext.as_str() {
        "flac" => read_flac_comment_lyrics(path)?,
        "ogg" | "oga" | "opus" => read_ogg_comment_lyrics(path)?,
        _ => return None,
    };
    plain_text_lines(&text)
}

fn read_flac_comment_lyrics(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic).ok()?;
    if &magic != b"fLaC" {
        return None;
    }
    loop {
        let mut header = [0u8; 4];
        file.read_exact(&mut header).ok()?;
        let is_last = header[0] & 0x80 != 0;
        let block_type = header[0] & 0x7f;
        let len = u32::from_be_bytes([0, header[1], header[2], header[3]]) as usize;
        let mut data = vec![0u8; len];
        file.read_exact(&mut data).ok()?;
        if block_type == 4 {
            return lyrics_from_vorbis_comment_packet(&data);
        }
        if is_last {
            break;
        }
    }
    None
}

fn read_ogg_comment_lyrics(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut buf = Vec::new();
    file.take(1024 * 1024).read_to_end(&mut buf).ok()?;
    scan_ogg_buffer(&buf)
}

fn scan_ogg_buffer(buf: &[u8]) -> Option<String> {
    let mut i = 0usize;
    while i + 27 < buf.len() {
        if &buf[i..i + 4] != b"OggS" {
            i += 1;
            continue;
        }
        let nseg = buf[i + 26] as usize;
        let seg_table_start = i + 27;
        if seg_table_start + nseg > buf.len() {
            break;
        }
        let body_start = seg_table_start + nseg;
        let body_len: usize = buf[seg_table_start..body_start]
            .iter()
            .map(|&b| b as usize)
            .sum();
        let body_end = body_start + body_len;
        if body_end > buf.len() {
            break;
        }
        if let Some(text) = lyrics_from_ogg_packet(&buf[body_start..body_end]) {
            return Some(text);
        }
        i = body_end;
    }
    None
}

fn lyrics_from_ogg_packet(body: &[u8]) -> Option<String> {
    // Vorbis comment packet: 0x03 "vorbis" + comment
    if body.len() > 7 && body[0] == 3 && &body[1..7] == b"vorbis" {
        return lyrics_from_vorbis_comment_packet(&body[7..]);
    }
    // OpusTags
    if body.len() > 8 && &body[..8] == b"OpusTags" {
        return lyrics_from_vorbis_comment_packet(&body[8..]);
    }
    None
}

fn lyrics_from_vorbis_comment_packet(data: &[u8]) -> Option<String> {
    if data.len() < 8 {
        return None;
    }
    let vendor_len = u32::from_le_bytes(data[0..4].try_into().ok()?) as usize;
    let mut off = 4 + vendor_len;
    if off + 4 > data.len() {
        return None;
    }
    let count = u32::from_le_bytes(data[off..off + 4].try_into().ok()?) as usize;
    off += 4;
    let mut best: Option<String> = None;
    let mut best_rank = 0_u8;
    for _ in 0..count {
        if off + 4 > data.len() {
            break;
        }
        let len = u32::from_le_bytes(data[off..off + 4].try_into().ok()?) as usize;
        off += 4;
        if off + len > data.len() {
            break;
        }
        let entry = std::str::from_utf8(&data[off..off + len]).unwrap_or("");
        off += len;
        let Some((key, value)) = entry.split_once('=') else {
            continue;
        };
        let key_u = key.to_ascii_uppercase();
        let rank = lyric_key_rank(&key_u);
        if rank == 0 {
            continue;
        }
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        if rank > best_rank {
            best_rank = rank;
            best = Some(value.to_string());
        }
    }
    best
}

fn lyric_key_rank(key: &str) -> u8 {
    if LYRIC_KEYS.iter().any(|k| *k == key) {
        if key.contains("SYNC") && !key.contains("UNSYNC") {
            3
        } else {
            2
        }
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn reads_flac_lyrics_comment() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("song.flac");
        let mut f = File::create(&path).unwrap();
        f.write_all(b"fLaC").unwrap();
        // STREAMINFO (required type 0), not last, 34 zero bytes.
        f.write_all(&[0x00, 0x00, 0x00, 0x22]).unwrap();
        f.write_all(&[0u8; 34]).unwrap();
        // VORBIS_COMMENT last
        let vendor = b"orchid";
        let comment = b"LYRICS=Line A\nLine B";
        let mut packet = Vec::new();
        packet.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        packet.extend_from_slice(vendor);
        packet.extend_from_slice(&1u32.to_le_bytes());
        packet.extend_from_slice(&(comment.len() as u32).to_le_bytes());
        packet.extend_from_slice(comment);
        let len = packet.len() as u32;
        f.write_all(&[0x84, (len >> 16) as u8, (len >> 8) as u8, len as u8])
            .unwrap();
        f.write_all(&packet).unwrap();
        drop(f);
        let lines = load_vorbis_lyrics(&path).expect("lyrics");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "Line A");
        assert_eq!(lines[1].text, "Line B");
    }
}
