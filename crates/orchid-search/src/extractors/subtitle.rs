//! Subtitle and lyric cue extractor.
//!
//! Cue text is indexed. Timestamps, cue numbers, WebVTT notes, and ASS
//! style overrides are not.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable dialogue from subtitle and lyric files.
#[derive(Debug, Default, Clone, Copy)]
pub struct SubtitleExtractor;

#[async_trait]
impl ContentExtractor for SubtitleExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            matches!(
                base.to_ascii_lowercase().as_str(),
                "text/vtt"
                    | "application/x-subrip"
                    | "application/x-srt"
                    | "text/x-ssa"
                    | "application/x-ass"
                    | "text/x-lrc"
            )
        }) || extension.is_some_and(is_subtitle_ext)
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        let text = decode_best_effort(&raw);
        let ext = path.extension().unwrap_or("");
        Ok(subtitle_text(&text, ext))
    }
}

pub(crate) fn is_subtitle_ext(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "srt" | "vtt" | "ass" | "ssa" | "lrc"
    )
}

pub(crate) fn subtitle_text(input: &str, extension: &str) -> String {
    match extension.to_ascii_lowercase().as_str() {
        "ass" | "ssa" => ass_text(input),
        "lrc" => lrc_text(input),
        "vtt" => cue_text(input),
        _ => {
            let head = input.trim_start();
            if head.starts_with("WEBVTT") {
                cue_text(input)
            } else if head.contains("[Events]") || head.contains("Dialogue:") {
                ass_text(input)
            } else if looks_like_lrc(head) {
                lrc_text(input)
            } else {
                cue_text(input)
            }
        }
    }
}

fn cue_text(input: &str) -> String {
    let lines: Vec<&str> = input.lines().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index].trim();
        if line.is_empty() {
            index += 1;
            continue;
        }
        if is_block_header(line) {
            index += 1;
            while index < lines.len() && !lines[index].trim().is_empty() {
                index += 1;
            }
            continue;
        }
        if line.starts_with("WEBVTT") || is_timing(line) {
            index += 1;
            continue;
        }
        if index + 1 < lines.len() && is_timing(lines[index + 1].trim()) {
            index += 1;
            continue;
        }
        push_line(&mut out, &strip_overrides(line));
        index += 1;
    }
    out.trim().to_string()
}

fn ass_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        let line = raw.trim();
        if let Some(rest) = strip_prefix_ignore(line, "Dialogue:") {
            push_line(&mut out, &ass_plain(after_nth_comma(rest, 9)));
            continue;
        }
        if let Some(rest) = strip_prefix_ignore(line, "Title:") {
            push_line(&mut out, rest);
        }
    }
    out.trim().to_string()
}

fn lrc_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        if let Some(line) = lrc_line(raw) {
            push_line(&mut out, &strip_overrides(&line));
        }
    }
    out.trim().to_string()
}

fn looks_like_lrc(head: &str) -> bool {
    head.lines().take(20).any(|line| {
        let line = line.trim();
        line.starts_with("[ti:") || line.starts_with("[ar:") || line.starts_with("[00:")
    })
}

fn is_block_header(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower == "style"
        || lower.starts_with("style ")
        || lower == "region"
        || lower.starts_with("region ")
        || lower == "note"
        || lower.starts_with("note ")
        || lower.starts_with("note\t")
}

fn is_timing(line: &str) -> bool {
    line.contains("-->")
}

fn lrc_line(line: &str) -> Option<String> {
    let mut rest = line.trim();
    if rest.is_empty() {
        return None;
    }
    if !rest.starts_with('[') {
        return Some(rest.to_string());
    }
    while rest.starts_with('[') {
        let end = rest.find(']')?;
        let inner = &rest[1..end];
        if is_lrc_time(inner) {
            rest = rest[end + 1..].trim_start();
            continue;
        }
        let (key, value) = inner.split_once(':')?;
        let key = key.trim().to_ascii_lowercase();
        if matches!(key.as_str(), "ti" | "ar" | "al" | "by" | "au") {
            let value = value.trim();
            if value.is_empty() {
                return None;
            }
            return Some(value.to_string());
        }
        return None;
    }
    let rest = rest.trim();
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

fn is_lrc_time(inner: &str) -> bool {
    !inner.is_empty()
        && inner.contains(':')
        && inner
            .chars()
            .all(|c| c.is_ascii_digit() || c == ':' || c == '.')
}

fn after_nth_comma(value: &str, n: usize) -> &str {
    let mut seen = 0;
    for (index, ch) in value.char_indices() {
        if ch == ',' {
            seen += 1;
            if seen == n {
                return &value[index + ch.len_utf8()..];
            }
        }
    }
    value
}

fn ass_plain(value: &str) -> String {
    strip_overrides(value)
        .replace("\\N", "\n")
        .replace("\\n", "\n")
        .replace("\\h", " ")
}

fn strip_overrides(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '<' {
            for next in chars.by_ref() {
                if next == '>' {
                    break;
                }
            }
            continue;
        }
        if ch == '{' && chars.peek() == Some(&'\\') {
            for next in chars.by_ref() {
                if next == '}' {
                    break;
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn strip_prefix_ignore<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    if line.len() >= prefix.len() && line[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(line[prefix.len()..].trim())
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
    fn srt_and_vtt_keep_cues_and_skip_times() {
        let srt = subtitle_text(
            "1\n\
             00:00:01,000 --> 00:00:04,000\n\
             Hello <i>world</i>\n\
             \n\
             2\n\
             00:00:05,000 --> 00:00:06,000\n\
             Second\n",
            "srt",
        );
        assert!(srt.contains("Hello world"), "{srt}");
        assert!(srt.contains("Second"), "{srt}");
        assert!(!srt.contains("00:00:01"), "{srt}");
        assert!(!srt.contains("<i>"), "{srt}");

        let vtt = subtitle_text(
            "WEBVTT\n\
             \n\
             NOTE\n\
             SECRET note\n\
             \n\
             00:00:01.000 --> 00:00:04.000\n\
             Cue text\n",
            "vtt",
        );
        assert!(vtt.contains("Cue text"), "{vtt}");
        assert!(!vtt.contains("SECRET"), "{vtt}");
        assert!(!vtt.contains("WEBVTT"), "{vtt}");
        assert!(!vtt.contains("00:00:01"), "{vtt}");
    }

    #[test]
    fn ass_keeps_dialogue_and_drops_overrides() {
        let text = subtitle_text(
            "Title: Night\n\
             [Events]\n\
             Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n\
             Dialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,Hello {\\b1}world\n\
             Comment: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,SECRET\n",
            "ass",
        );
        assert!(text.contains("Night"), "{text}");
        assert!(text.contains("Hello world"), "{text}");
        assert!(!text.contains("SECRET"), "{text}");
        assert!(!text.contains("0:00:01"), "{text}");
        assert!(!text.contains("b1"), "{text}");
    }

    #[test]
    fn lrc_keeps_lyrics_and_titles() {
        let text = subtitle_text(
            "[ti:Night Drive]\n\
             [ar:Ada]\n\
             [offset:+100]\n\
             [00:01.00]Hello world\n\
             [00:02.50][00:03.00]Second line\n",
            "lrc",
        );
        assert!(text.contains("Night Drive"), "{text}");
        assert!(text.contains("Ada"), "{text}");
        assert!(text.contains("Hello world"), "{text}");
        assert!(text.contains("Second line"), "{text}");
        assert!(!text.contains("offset"), "{text}");
        assert!(!text.contains("00:01"), "{text}");
        assert!(!text.contains("+100"), "{text}");
    }
}
