//! M3U / M3U8 playlist parse helpers for the audio player.

#![allow(missing_docs)]

use std::path::{Path, PathBuf};

/// Parse an M3U playlist body into absolute local file paths.
///
/// Skips blank lines, comments (`#…`), and non-file URLs. Relative entries are
/// resolved against `base` (typically the playlist file's parent directory).
#[must_use]
pub fn parse_m3u(text: &str, base: Option<&Path>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("ftp://")
        {
            continue;
        }
        let path = resolve_entry(line, base);
        let key = path.to_string_lossy().into_owned();
        if seen.insert(key.clone()) {
            out.push(key);
        }
    }
    out
}

fn resolve_entry(entry: &str, base: Option<&Path>) -> PathBuf {
    let p = PathBuf::from(entry);
    let joined = if p.is_absolute() {
        p
    } else if let Some(base) = base {
        base.join(p)
    } else {
        p
    };
    normalize_path(joined)
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn skips_comments_and_urls() {
        let text = "\
#EXTM3U
#EXTINF:-1,Song
C:/music/a.mp3
https://example.com/b.mp3
./rel.flac
";
        let paths = parse_m3u(text, Some(Path::new("D:/playlists")));
        assert_eq!(paths.len(), 2);
        assert_eq!(PathBuf::from(&paths[0]), PathBuf::from("C:/music/a.mp3"));
        assert_eq!(
            PathBuf::from(&paths[1]),
            PathBuf::from("D:/playlists").join("rel.flac")
        );
    }

    #[test]
    fn dedupes_paths() {
        let text = "a.mp3\na.mp3\nb.mp3\n";
        let paths = parse_m3u(text, Some(Path::new("/lib")));
        assert_eq!(paths.len(), 2);
    }
}
