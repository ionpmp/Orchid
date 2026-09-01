//! Persist media viewer prefs (volume, mute, hwdec, pitch, last picker folder).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaPrefs {
    /// mpv volume 0..150.
    pub volume: f64,
    pub muted: bool,
    /// 0 = `auto-copy`, 1 = `no`.
    #[serde(default)]
    pub hwdec_mode: u32,
    /// Keep musical pitch when playback speed ≠ 1.
    #[serde(default = "default_pitch_preserve")]
    pub pitch_preserve: bool,
    /// Last directory used by the catalog media file picker.
    #[serde(default)]
    pub last_folder: Option<PathBuf>,
}

fn default_pitch_preserve() -> bool {
    true
}

impl Default for MediaPrefs {
    fn default() -> Self {
        Self {
            volume: 100.0,
            muted: false,
            hwdec_mode: 0,
            pitch_preserve: true,
            last_folder: None,
        }
    }
}

fn store_path() -> Option<PathBuf> {
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

/// Persist volume / mute / hwdec / pitch-preserve (keeps `last_folder`).
pub fn store(volume: f64, muted: bool, hwdec_mode: u32, pitch_preserve: bool) {
    let Ok(_g) = LOCK.lock() else {
        return;
    };
    let mut prefs = load_file();
    prefs.volume = volume.clamp(0.0, 150.0);
    prefs.muted = muted;
    prefs.hwdec_mode = hwdec_mode.min(1);
    prefs.pitch_preserve = pitch_preserve;
    save_file(&prefs);
}

/// Remember the folder shown in the next media file picker.
pub fn store_last_folder(folder: &Path) {
    let Ok(_g) = LOCK.lock() else {
        return;
    };
    let mut prefs = load_file();
    prefs.last_folder = Some(folder.to_path_buf());
    save_file(&prefs);
}
