//! ID3 (and ID3-compatible) audio tags.

use std::path::Path;

use id3::frame::{SynchronisedLyricsType, TimestampFormat};
use id3::TagLike;

use crate::error::{Result, ViewerError};
use crate::vorbis_lyrics;

/// Extensions that typically carry ID3 tags.
const ID3_EXTENSIONS: &[&str] = &["mp3", "mp2", "aac", "aiff", "wav"];

/// Whether `ext` (lowercase, no dot) is an ID3-capable audio file.
#[must_use]
pub fn is_id3_extension(ext: &str) -> bool {
    ID3_EXTENSIONS.contains(&ext)
}

/// One tag field for the report / editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioTagField {
    /// Label (`Title`, `Artist`, …).
    pub label: String,
    /// Value.
    pub value: String,
}

/// Read ID3v2/v1 tags from a local file.
///
/// # Errors
///
/// I/O or an unreadable tag.
pub fn read_id3_fields(path: &Path) -> Result<Vec<AudioTagField>> {
    let tag = id3::Tag::read_from_path(path).map_err(|e| ViewerError::Metadata(e.to_string()))?;
    let mut out = Vec::new();
    push(&mut out, "Title", tag.title());
    push(&mut out, "Artist", tag.artist());
    push(&mut out, "Album", tag.album());
    push(&mut out, "Album artist", tag.album_artist());
    push(&mut out, "Genre", tag.genre());
    if let Some(y) = tag.year() {
        out.push(AudioTagField {
            label: "Year".into(),
            value: y.to_string(),
        });
    }
    if let Some(t) = tag.track() {
        out.push(AudioTagField {
            label: "Track".into(),
            value: t.to_string(),
        });
    }
    push(
        &mut out,
        "Comment",
        tag.comments().next().map(|c| c.text.as_str()),
    );
    if let Some(ly) = tag.lyrics().next() {
        let preview: String = ly.text.chars().take(120).collect();
        if !preview.trim().is_empty() {
            out.push(AudioTagField {
                label: "Lyrics".into(),
                value: preview,
            });
        }
    } else if tag.synchronised_lyrics().next().is_some() {
        out.push(AudioTagField {
            label: "Lyrics".into(),
            value: "(synchronised)".into(),
        });
    }
    Ok(out)
}

fn push(out: &mut Vec<AudioTagField>, label: &str, value: Option<&str>) {
    if let Some(v) = value.map(str::trim).filter(|s| !s.is_empty()) {
        out.push(AudioTagField {
            label: label.into(),
            value: v.to_string(),
        });
    }
}

/// One timed (or unsynced) lyric line from an embedded tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedLyricLine {
    /// Offset in milliseconds (`0` when unsynchronised).
    pub time_ms: u64,
    /// Lyric text for this cue.
    pub text: String,
}

/// Load embedded lyrics: ID3 `SYLT`/`USLT`, then Vorbis/FLAC comments.
///
/// Returns `None` when the file has no usable lyric data.
#[must_use]
pub fn load_embedded_lyrics(path: &Path) -> Option<Vec<EmbeddedLyricLine>> {
    if let Ok(tag) = id3::Tag::read_from_path(path) {
        if let Some(lines) = sylt_lines(&tag) {
            return Some(lines);
        }
        if let Some(lines) = uslt_lines(&tag) {
            return Some(lines);
        }
    }
    vorbis_lyrics::load_vorbis_lyrics(path)
}

fn sylt_lines(tag: &id3::Tag) -> Option<Vec<EmbeddedLyricLine>> {
    let mut best: Option<Vec<EmbeddedLyricLine>> = None;
    let mut best_rank = 0_u8;
    for frame in tag.synchronised_lyrics() {
        if frame.timestamp_format != TimestampFormat::Ms {
            continue;
        }
        let rank = match frame.content_type {
            SynchronisedLyricsType::Lyrics => 3,
            SynchronisedLyricsType::Transcription => 2,
            SynchronisedLyricsType::Other => 1,
            _ => 0,
        };
        if rank == 0 {
            continue;
        }
        let mut lines = Vec::with_capacity(frame.content.len());
        for (ms, text) in &frame.content {
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            lines.push(EmbeddedLyricLine {
                time_ms: u64::from(*ms),
                text: text.to_string(),
            });
        }
        if lines.is_empty() || rank < best_rank {
            continue;
        }
        best_rank = rank;
        best = Some(lines);
        if rank == 3 {
            break;
        }
    }
    best
}

fn uslt_lines(tag: &id3::Tag) -> Option<Vec<EmbeddedLyricLine>> {
    let text = tag.lyrics().next()?.text.trim();
    plain_text_lines(text)
}

/// Split unsynced lyric text into panel rows (`time_ms = 0`).
#[must_use]
pub(crate) fn plain_text_lines(text: &str) -> Option<Vec<EmbeddedLyricLine>> {
    let mut lines = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        lines.push(EmbeddedLyricLine {
            time_ms: 0,
            text: line.to_string(),
        });
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines)
    }
}

/// Format ID3 tags as a report body.
///
/// # Errors
///
/// See [`read_id3_fields`].
pub fn format_id3_report(path: &Path) -> Result<String> {
    let fields = read_id3_fields(path)?;
    if fields.is_empty() {
        return Ok(String::new());
    }
    let mut body = String::new();
    for f in fields {
        body.push_str(&f.label);
        body.push_str(": ");
        body.push_str(&f.value);
        body.push('\n');
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_id3() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("song.mp3");
        std::fs::write(&path, [0xFF, 0xFB, 0x90, 0x00]).unwrap();
        let mut tag = id3::Tag::new();
        tag.set_title("Hello");
        tag.set_artist("Orchid");
        tag.write_to_path(&path, id3::Version::Id3v23).unwrap();
        let fields = read_id3_fields(&path).unwrap();
        assert!(fields
            .iter()
            .any(|f| f.label == "Title" && f.value == "Hello"));
        assert!(fields
            .iter()
            .any(|f| f.label == "Artist" && f.value == "Orchid"));
    }

    #[test]
    fn loads_uslt_lyrics() {
        use id3::frame::Lyrics;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("song.mp3");
        std::fs::write(&path, [0xFF, 0xFB, 0x90, 0x00]).unwrap();
        let mut tag = id3::Tag::new();
        tag.add_lyrics(Lyrics {
            lang: "eng".into(),
            description: String::new(),
            text: "Line one\nLine two\n".into(),
        });
        tag.write_to_path(&path, id3::Version::Id3v23).unwrap();
        let lines = load_embedded_lyrics(&path).expect("uslt");
        assert_eq!(
            lines,
            vec![
                EmbeddedLyricLine {
                    time_ms: 0,
                    text: "Line one".into()
                },
                EmbeddedLyricLine {
                    time_ms: 0,
                    text: "Line two".into()
                },
            ]
        );
    }

    #[test]
    fn loads_sylt_over_uslt() {
        use id3::frame::{Lyrics, SynchronisedLyrics, SynchronisedLyricsType, TimestampFormat};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("song.mp3");
        std::fs::write(&path, [0xFF, 0xFB, 0x90, 0x00]).unwrap();
        let mut tag = id3::Tag::new();
        tag.add_lyrics(Lyrics {
            lang: "eng".into(),
            description: String::new(),
            text: "plain".into(),
        });
        tag.add_synchronised_lyrics(SynchronisedLyrics {
            lang: "eng".into(),
            timestamp_format: TimestampFormat::Ms,
            content_type: SynchronisedLyricsType::Lyrics,
            description: String::new(),
            content: vec![(1_200, "First".into()), (5_000, "Second".into())],
        });
        tag.write_to_path(&path, id3::Version::Id3v23).unwrap();
        let lines = load_embedded_lyrics(&path).expect("sylt");
        assert_eq!(lines[0].time_ms, 1_200);
        assert_eq!(lines[0].text, "First");
        assert_eq!(lines[1].text, "Second");
    }
}
