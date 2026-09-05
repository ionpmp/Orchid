//! Sidecar `.lrc` lyrics for the audio player.

#![allow(missing_docs)]

use std::path::Path;

/// One timed lyric line.
#[derive(Debug, Clone, Default)]
pub struct LyricLine {
    pub time_ms: u64,
    pub text: String,
}

/// Parsed LRC document (may be empty).
#[derive(Debug, Clone, Default)]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
}

impl Lyrics {
    /// Load `stem.lrc` next to `media`, or return empty.
    #[must_use]
    pub fn load_for(media: &Path) -> Self {
        let Some(lrc) = sidecar_lrc(media) else {
            return Self::default();
        };
        let Ok(body) = std::fs::read_to_string(&lrc) else {
            return Self::default();
        };
        Self::parse(&body)
    }

    /// Parse enhanced/simple LRC timestamps (`[mm:ss.xx]` / `[mm:ss]`).
    #[must_use]
    pub fn parse(body: &str) -> Self {
        let mut lines = Vec::new();
        for raw in body.lines() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            // Multiple tags per line: [00:12.00][00:15.00]text
            let mut rest = line;
            let mut stamps = Vec::new();
            while rest.starts_with('[') {
                let Some(end) = rest.find(']') else {
                    break;
                };
                let tag = &rest[1..end];
                rest = &rest[end + 1..];
                if let Some(ms) = parse_lrc_time(tag) {
                    stamps.push(ms);
                }
            }
            let text = rest.trim().to_string();
            if text.is_empty() && stamps.is_empty() {
                continue;
            }
            for ms in stamps {
                lines.push(LyricLine {
                    time_ms: ms,
                    text: text.clone(),
                });
            }
        }
        lines.sort_by_key(|l| l.time_ms);
        Self { lines }
    }

    /// Active line at `position_ms` (last line whose time ≤ position).
    #[must_use]
    pub fn line_at(&self, position_ms: u64) -> String {
        if self.lines.is_empty() {
            return String::new();
        }
        if !self.is_synced() {
            return self.lines[0].text.clone();
        }
        let mut cur = String::new();
        for line in &self.lines {
            if line.time_ms > position_ms {
                break;
            }
            cur = line.text.clone();
        }
        cur
    }

    /// Active line index at `position_ms` (−1 if none / unsynced).
    #[must_use]
    pub fn active_index(&self, position_ms: u64) -> i32 {
        if !self.is_synced() {
            return -1;
        }
        let mut idx = -1_i32;
        for (i, line) in self.lines.iter().enumerate() {
            if line.time_ms > position_ms {
                break;
            }
            idx = i as i32;
        }
        idx
    }

    /// True when at least one line has a non-zero timestamp.
    #[must_use]
    pub fn is_synced(&self) -> bool {
        self.lines.iter().any(|l| l.time_ms > 0)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

fn sidecar_lrc(media: &Path) -> Option<std::path::PathBuf> {
    let stem = media.file_stem()?.to_str()?;
    let parent = media.parent()?;
    let candidate = parent.join(format!("{stem}.lrc"));
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

fn parse_lrc_time(tag: &str) -> Option<u64> {
    // Skip metadata tags like [ar:Artist]
    if tag.contains(':') && tag.chars().next()?.is_ascii_alphabetic() {
        return None;
    }
    let (mm, rest) = tag.split_once(':')?;
    let minutes: u64 = mm.parse().ok()?;
    // ss or ss.xx / ss:xx
    let (secs_s, frac_s) = if let Some((s, f)) = rest.split_once('.') {
        (s, f)
    } else if let Some((s, f)) = rest.split_once(':') {
        (s, f)
    } else {
        (rest, "0")
    };
    let secs: u64 = secs_s.parse().ok()?;
    let frac = frac_s.trim_end_matches(|c: char| !c.is_ascii_digit());
    let millis = match frac.len() {
        0 => 0,
        1 => frac.parse::<u64>().ok()? * 100,
        2 => frac.parse::<u64>().ok()? * 10,
        _ => frac[..3].parse::<u64>().ok()?,
    };
    Some(minutes * 60_000 + secs * 1000 + millis)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_basic_lrc() {
        let l = Lyrics::parse(
            "[00:12.00]First line\n[00:15.50]Second\n[ar:Meta]\n[01:00]Third\n",
        );
        assert_eq!(l.lines.len(), 3);
        assert_eq!(l.line_at(0), "");
        assert_eq!(l.line_at(12_000), "First line");
        assert_eq!(l.line_at(15_500), "Second");
        assert_eq!(l.line_at(60_000), "Third");
    }

    #[test]
    fn loads_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let media = dir.path().join("song.mp3");
        fs::write(&media, b"").unwrap();
        fs::write(dir.path().join("song.lrc"), "[00:01.00]Hello\n").unwrap();
        let l = Lyrics::load_for(&media);
        assert_eq!(l.line_at(1500), "Hello");
    }
}
