//! Remember last playback position per local media path.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const MAX_ENTRIES: usize = 200;
/// Ignore resume under this many seconds into the file.
const MIN_RESUME_SECS: f64 = 5.0;
/// Clear resume when within this fraction of the end.
const END_FRACTION: f64 = 0.95;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ResumeFile {
    /// path → (position seconds, unix secs updated)
    entries: HashMap<String, ResumeEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ResumeEntry {
    position_secs: f64,
    updated: u64,
}

fn store_path() -> Option<PathBuf> {
    orchid_storage::OrchidPaths::resolve()
        .ok()
        .map(|p| p.cache_dir.join("media_resume.json"))
}

fn key_for(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn load_file() -> ResumeFile {
    let Some(path) = store_path() else {
        return ResumeFile::default();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return ResumeFile::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn save_file(file: &ResumeFile) {
    let Some(path) = store_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(file) {
        let _ = std::fs::write(path, bytes);
    }
}

static LOCK: Mutex<()> = Mutex::new(());

/// Position to resume from, if a recent enough bookmark exists.
#[must_use]
pub fn take_resume(path: &Path) -> Option<f64> {
    let _g = LOCK.lock().ok()?;
    let key = key_for(path);
    let file = load_file();
    let entry = file.entries.get(&key)?;
    if entry.position_secs < MIN_RESUME_SECS {
        return None;
    }
    Some(entry.position_secs)
}

/// Persist playback position (or clear when near start/end).
pub fn store_resume(path: &Path, position_secs: f64, duration_secs: f64) {
    let Ok(_g) = LOCK.lock() else {
        return;
    };
    let key = key_for(path);
    let mut file = load_file();
    let near_end = duration_secs > 0.0 && position_secs / duration_secs >= END_FRACTION;
    if position_secs < MIN_RESUME_SECS || near_end {
        file.entries.remove(&key);
    } else {
        file.entries.insert(
            key,
            ResumeEntry {
                position_secs,
                updated: now_unix(),
            },
        );
        if file.entries.len() > MAX_ENTRIES {
            evict_oldest(&mut file);
        }
    }
    save_file(&file);
}

fn evict_oldest(file: &mut ResumeFile) {
    let mut pairs: Vec<_> = file
        .entries
        .iter()
        .map(|(k, v)| (k.clone(), v.updated))
        .collect();
    pairs.sort_by_key(|(_, u)| *u);
    let drop_n = file.entries.len().saturating_sub(MAX_ENTRIES);
    for (k, _) in pairs.into_iter().take(drop_n) {
        file.entries.remove(&k);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_stable() {
        let p = Path::new("C:/music/a.mp3");
        assert_eq!(key_for(p), "C:/music/a.mp3");
    }
}
