//! Persist last media volume / mute across sessions.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaPrefs {
    /// mpv volume 0..150.
    pub volume: f64,
    pub muted: bool,
}

impl Default for MediaPrefs {
    fn default() -> Self {
        Self {
            volume: 100.0,
            muted: false,
        }
    }
}

fn store_path() -> Option<std::path::PathBuf> {
    orchid_storage::OrchidPaths::resolve()
        .ok()
        .map(|p| p.cache_dir.join("media_prefs.json"))
}

fn load_file() -> MediaPrefs {
    let Some(path) = store_path() else {
        return MediaPrefs::default();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return MediaPrefs::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn save_file(prefs: &MediaPrefs) {
    let Some(path) = store_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(prefs) {
        let _ = std::fs::write(path, bytes);
    }
}

static LOCK: Mutex<()> = Mutex::new(());

/// Load last volume / mute (clamped).
#[must_use]
pub fn load() -> MediaPrefs {
    let Ok(_g) = LOCK.lock() else {
        return MediaPrefs::default();
    };
    let mut prefs = load_file();
    prefs.volume = prefs.volume.clamp(0.0, 150.0);
    prefs
}

/// Persist volume / mute.
pub fn store(volume: f64, muted: bool) {
    let Ok(_g) = LOCK.lock() else {
        return;
    };
    save_file(&MediaPrefs {
        volume: volume.clamp(0.0, 150.0),
        muted,
    });
}
