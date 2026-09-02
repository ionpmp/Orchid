//! Cover art for audio: embedded ID3 APIC and folder sidecars.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use id3::TagLike;

use super::engine::FrameBuf;

const COVER_NAMES: &[&str] = &[
    "cover.jpg",
    "cover.jpeg",
    "cover.png",
    "folder.jpg",
    "folder.jpeg",
    "folder.png",
    "front.jpg",
    "front.jpeg",
    "front.png",
];

const MAX_COVER_EDGE: u32 = 512;

/// Title / artist for audio chrome (best-effort).
#[derive(Debug, Clone, Default)]
#[allow(missing_docs)]
pub struct MediaTags {
    pub title: String,
    pub artist: String,
}

/// Richer tags for the audio library (scan-time, no cover bytes).
#[derive(Debug, Clone, Default)]
#[allow(missing_docs)]
pub struct TrackMeta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub track: Option<u32>,
    pub year: Option<i32>,
    /// Length from ID3 TLEN when present (milliseconds).
    pub duration_ms: Option<u32>,
}

/// Load display tags from ID3 when available; otherwise use the file stem as title.
#[must_use]
pub fn load_media_tags(path: &Path) -> MediaTags {
    let meta = load_track_meta(path);
    MediaTags {
        title: meta.title,
        artist: meta.artist,
    }
}

/// Load library metadata from ID3 when available; otherwise stem as title.
#[must_use]
pub fn load_track_meta(path: &Path) -> TrackMeta {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string();
    let Ok(tag) = id3::Tag::read_from_path(path) else {
        return TrackMeta {
            title: stem,
            ..TrackMeta::default()
        };
    };
    TrackMeta {
        title: tag
            .title()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or(stem),
        artist: tag
            .artist()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_default(),
        album: tag
            .album()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_default(),
        genre: tag
            .genre()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_default(),
        track: tag.track(),
        year: tag.year(),
        duration_ms: tag.duration(),
    }
}

/// Prefer embedded APIC, then sidecar cover next to the media file.
#[must_use]
pub fn load_cover_art(path: &Path) -> Option<FrameBuf> {
    if let Some(frame) = load_id3_cover(path) {
        return Some(frame);
    }
    for candidate in discover_cover_sidecars(path) {
        if let Some(frame) = decode_cover_file(&candidate) {
            return Some(frame);
        }
    }
    None
}

fn load_id3_cover(path: &Path) -> Option<FrameBuf> {
    let tag = id3::Tag::read_from_path(path).ok()?;
    let pic = tag.pictures().next()?;
    decode_cover_bytes(&pic.data)
}

/// Folder covers and same-stem images (`song.jpg`, `cover.png`, …).
#[must_use]
pub fn discover_cover_sidecars(media: &Path) -> Vec<PathBuf> {
    let Some(parent) = media.parent() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for name in COVER_NAMES {
        let p = parent.join(name);
        if p.is_file() {
            out.push(p);
        }
    }
    if let Some(stem) = media.file_stem().and_then(|s| s.to_str()) {
        for ext in ["jpg", "jpeg", "png", "webp"] {
            let p = parent.join(format!("{stem}.{ext}"));
            if p.is_file() {
                out.push(p);
            }
        }
    }
    out
}

fn decode_cover_file(path: &Path) -> Option<FrameBuf> {
    let bytes = std::fs::read(path).ok()?;
    decode_cover_bytes(&bytes)
}

fn decode_cover_bytes(bytes: &[u8]) -> Option<FrameBuf> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let (nw, nh) = scale_cover(w, h, MAX_COVER_EDGE);
    let rgba = if nw != w || nh != h {
        image::imageops::resize(&rgba, nw, nh, image::imageops::FilterType::Triangle)
    } else {
        rgba
    };
    Some(FrameBuf {
        rgba: Arc::new(rgba.into_raw()),
        width: nw,
        height: nh,
    })
}

fn scale_cover(w: u32, h: u32, max_edge: u32) -> (u32, u32) {
    let edge = w.max(h);
    if edge <= max_edge {
        return (w, h);
    }
    let scale = f64::from(max_edge) / f64::from(edge);
    (
        ((f64::from(w) * scale).round() as u32).max(1),
        ((f64::from(h) * scale).round() as u32).max(1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn discovers_folder_and_stem_covers() {
        let dir = tempfile::tempdir().unwrap();
        let media = dir.path().join("track.mp3");
        fs::write(&media, b"").unwrap();
        fs::write(dir.path().join("cover.jpg"), b"").unwrap();
        fs::write(dir.path().join("track.png"), b"").unwrap();
        fs::write(dir.path().join("other.jpg"), b"").unwrap();
        let found = discover_cover_sidecars(&media);
        let names: Vec<_> = found
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        assert!(names.iter().any(|n| n == "cover.jpg"));
        assert!(names.iter().any(|n| n == "track.png"));
        assert!(!names.iter().any(|n| n == "other.jpg"));
    }

    #[test]
    fn scale_keeps_small() {
        assert_eq!(scale_cover(200, 200, 512), (200, 200));
    }

    #[test]
    fn scale_shrinks_large() {
        let (w, h) = scale_cover(1000, 500, 512);
        assert_eq!(w, 512);
        assert_eq!(h, 256);
    }
}
