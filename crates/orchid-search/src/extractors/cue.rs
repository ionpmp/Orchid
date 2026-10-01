//! CUE sheet extractor.
//!
//! Album and track titles, performers, songwriters, and file names are
//! indexed. `INDEX` timestamps and `TRACK` headers are not.

use async_trait::async_trait;

use crate::error::Result;
use crate::extractors::text::{decode_best_effort, MAX_CONTENT_BYTES};
use crate::extractors::ContentExtractor;

/// Extract searchable text from `.cue` sheets.
#[derive(Debug, Default, Clone, Copy)]
pub struct CueExtractor;

#[async_trait]
impl ContentExtractor for CueExtractor {
    fn can_handle(&self, mime: Option<&str>, extension: Option<&str>) -> bool {
        mime.is_some_and(|m| {
            let base = m.split(';').next().unwrap_or(m).trim();
            base.eq_ignore_ascii_case("application/x-cue")
                || base.eq_ignore_ascii_case("audio/x-cue")
        }) || extension.is_some_and(|e| e.eq_ignore_ascii_case("cue"))
    }

    async fn extract(
        &self,
        provider: &dyn orchid_fs::FsProvider,
        path: &orchid_fs::FsPath,
    ) -> Result<String> {
        let raw = orchid_fs::read_prefix(provider, path, MAX_CONTENT_BYTES).await?;
        Ok(cue_text(&decode_best_effort(&raw)))
    }
}

pub(crate) fn cue_text(input: &str) -> String {
    let mut out = String::new();
    for raw in input.lines() {
        if let Some(value) = cue_line(raw) {
            push_line(&mut out, &value);
        }
    }
    out.trim().to_string()
}

fn cue_line(line: &str) -> Option<String> {
    let line = line.trim();
    let (key, rest) = line.split_once(char::is_whitespace)?;
    if rest.trim().is_empty() {
        return None;
    }
    let value = match key.to_ascii_uppercase().as_str() {
        "TITLE" | "PERFORMER" | "SONGWRITER" | "CATALOG" => unquote(rest),
        "FILE" | "CDTEXTFILE" => file_name(rest),
        "REM" => rem_value(rest),
        _ => return None,
    };
    let value = value.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn rem_value(rest: &str) -> String {
    let rest = rest.trim();
    if let Some((key, value)) = rest.split_once(char::is_whitespace) {
        let tagged = key
            .chars()
            .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit());
        if tagged && !value.trim().is_empty() {
            return unquote(value);
        }
    }
    unquote(rest)
}

fn file_name(rest: &str) -> String {
    let name = unquote(rest);
    const TYPES: &[&str] = &["WAVE", "MP3", "AIFF", "FLAC", "BINARY", "MOTOROLA"];
    if let Some((file, kind)) = name.rsplit_once(char::is_whitespace) {
        if TYPES.iter().any(|t| kind.eq_ignore_ascii_case(t)) {
            return file.trim().to_string();
        }
    }
    name
}

fn unquote(rest: &str) -> String {
    let rest = rest.trim();
    if !rest.starts_with('"') {
        return rest.to_string();
    }
    let mut out = String::new();
    let mut chars = rest[1..].chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
            continue;
        }
        if ch == '"' {
            if chars.peek() == Some(&'"') {
                chars.next();
                out.push('"');
                continue;
            }
            break;
        }
        out.push(ch);
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
    fn indexes_titles_and_skips_indexes() {
        let text = cue_text(
            "REM GENRE Rock\n\
             PERFORMER \"Ada Lovelace\"\n\
             TITLE \"Album Name\"\n\
             FILE \"album.wav\" WAVE\n\
               TRACK 01 AUDIO\n\
                 TITLE \"First Song\"\n\
                 SONGWRITER \"Grace\"\n\
                 INDEX 01 00:00:00\n\
               TRACK 02 AUDIO\n\
                 TITLE \"Second \\\"Quoted\\\"\"\n\
                 INDEX 01 03:21:00\n",
        );
        assert!(text.contains("Rock"), "{text}");
        assert!(text.contains("Ada Lovelace"), "{text}");
        assert!(text.contains("Album Name"), "{text}");
        assert!(text.contains("album.wav"), "{text}");
        assert!(text.contains("First Song"), "{text}");
        assert!(text.contains("Grace"), "{text}");
        assert!(text.contains("Second \"Quoted\""), "{text}");
        assert!(!text.contains("00:00:00"), "{text}");
        assert!(!text.contains("03:21:00"), "{text}");
        assert!(!text.contains("AUDIO"), "{text}");
        assert!(!text.contains("WAVE"), "{text}");
    }
}
