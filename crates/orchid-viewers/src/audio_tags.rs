//! ID3 (and ID3-compatible) audio tags.

use std::path::Path;

use id3::TagLike;

use crate::error::{Result, ViewerError};

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
}
