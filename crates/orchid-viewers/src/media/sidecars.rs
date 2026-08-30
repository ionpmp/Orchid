//! External subtitle sidecar discovery next to a media file.

use std::path::{Path, PathBuf};

const SUB_EXTS: &[&str] = &["srt", "ass", "ssa", "vtt", "sub"];

/// Same-stem subtitle files in the media file's directory.
#[must_use]
pub fn discover_sidecar_subs(media: &Path) -> Vec<PathBuf> {
    let Some(parent) = media.parent() else {
        return Vec::new();
    };
    let Some(stem) = media.file_stem().and_then(|s| s.to_str()) else {
        return Vec::new();
    };
    let stem_lower = stem.to_ascii_lowercase();
    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some((file_stem, ext)) = name.rsplit_once('.') else {
            continue;
        };
        if !SUB_EXTS.iter().any(|e| ext.eq_ignore_ascii_case(e)) {
            continue;
        }
        let file_stem_lower = file_stem.to_ascii_lowercase();
        // Exact stem or `movie.en.srt` / `movie.forced.srt` style.
        if file_stem_lower == stem_lower
            || file_stem_lower
                .strip_prefix(&stem_lower)
                .is_some_and(|rest| rest.starts_with('.') || rest.starts_with('_'))
        {
            found.push(path);
        }
    }
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn finds_exact_and_lang_sidecars() {
        let dir = tempfile::tempdir().unwrap();
        let media = dir.path().join("movie.mkv");
        fs::write(&media, b"").unwrap();
        fs::write(dir.path().join("movie.srt"), b"").unwrap();
        fs::write(dir.path().join("movie.en.ass"), b"").unwrap();
        fs::write(dir.path().join("other.srt"), b"").unwrap();
        let found = discover_sidecar_subs(&media);
        let names: Vec<_> = found
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        assert!(names.contains(&"movie.srt".into()));
        assert!(names.contains(&"movie.en.ass".into()));
        assert!(!names.iter().any(|n| n == "other.srt"));
    }
}
