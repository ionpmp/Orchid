//! Playlist text extractor.
//!
//! M3U / M3U8 titles (`#EXTINF`) and entry paths are indexed. PLS `Title`
//! and `File` values are indexed. Durations and playlist directives are not.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable titles and paths from audio playlists.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlaylistExtractor;

#[async_trait]
impl ContentExtractor for PlaylistExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            matches!(
                base.to_ascii_lowercase().as_str(),
                "audio/x-mpegurl"
                    | "audio/mpegurl"
                    | "application/vnd.apple.mpegurl"
                    | "application/mpegurl"
                    | "application/x-scpls"
                    | "audio/x-scpls"
            )
        }) || extension
            .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "m3u" | "m3u8" | "pls"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        let text = decode_best_effort(&raw);
        let ext = path.extension().unwrap_or("");
        Ok(playlist_text(&text, ext))
    }
}

pub(crate) fn playlist_text(input: &str, extension: &str) -> String {
    if extension.eq_ignore_ascii_case("pls") || looks_like_pls(input) {
        pls_text(input)
    } else {
        m3u_text(input)
    }
}

fn looks_like_pls(input: &str) -> bool {
    input.lines().take(30).any(|line| {
        let line = line.trim();
        line.eq_ignore_ascii_case("[playlist]")
            || split_pls(line).is_some_and(|(key, _)| key.eq_ignore_ascii_case("file1"))
    })
}

fn m3u_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = strip_ignore_ascii(line, "#EXTINF:") {
            if let Some((_, title)) = rest.split_once(',') {
                push_line(&mut out, title);
            }
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        push_line(&mut out, line);
    }
    out.trim().to_string()
}

fn pls_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let Some((key, value)) = split_pls(raw.trim()) else {
            continue;
        };
        if is_file_or_title(key) {
            push_line(&mut out, value);
        }
    }
    out.trim().to_string()
}

fn split_pls(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    let value = value.trim();
    if key.is_empty() || value.is_empty() {
        None
    } else {
        Some((key, value))
    }
}

fn is_file_or_title(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("file")
        .or_else(|| lower.strip_prefix("title"));
    rest.is_some_and(|num| !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()))
}

fn strip_ignore_ascii<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    if line.len() >= prefix.len() && line[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&line[prefix.len()..])
    } else {
        None
    }
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
    fn m3u_keeps_titles_and_entries_and_skips_directives() {
        let text = playlist_text(
            "#EXTM3U\n\
             #EXTINF:-1,Night Drive\n\
             C:/music/night.mp3\n\
             #EXT-X-VERSION:3\n\
             https://radio.example/stream\n",
            "m3u",
        );
        assert!(text.contains("Night Drive"), "{text}");
        assert!(text.contains("C:/music/night.mp3"), "{text}");
        assert!(text.contains("https://radio.example/stream"), "{text}");
        assert!(!text.contains("EXT-X-VERSION"), "{text}");
        assert!(!text.contains("#EXTM3U"), "{text}");
    }

    #[test]
    fn pls_keeps_titles_and_files_and_skips_lengths() {
        let text = playlist_text(
            "[playlist]\n\
             File1=album/track.flac\n\
             Title1=Blue Hour\n\
             Length1=200\n\
             NumberOfEntries=1\n",
            "pls",
        );
        assert!(text.contains("album/track.flac"), "{text}");
        assert!(text.contains("Blue Hour"), "{text}");
        assert!(!text.contains("200"), "{text}");
        assert!(!text.contains("NumberOfEntries"), "{text}");
    }
}
