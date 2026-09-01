//! Persist media viewer prefs across sessions.

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
    /// Subtitle color/outline preset index.
    #[serde(default)]
    pub sub_style_index: u32,
    /// ReplayGain mode index (0=off, 1=track, 2=album).
    #[serde(default)]
    pub replaygain_index: u32,
    /// Whether the folder playlist side panel starts open.
    #[serde(default = "default_playlist_panel_open")]
    pub playlist_panel_open: bool,
}

fn default_pitch_preserve() -> bool {
    true
}

fn default_playlist_panel_open() -> bool {
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
            sub_style_index: 0,
            replaygain_index: 0,
            playlist_panel_open: true,
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

fn update(f: impl FnOnce(&mut MediaPrefs)) {
    let Ok(_g) = LOCK.lock() else {
        return;
    };
    let mut prefs = load_file();
    f(&mut prefs);
    save_file(&prefs);
}

/// Load prefs (volume clamped).
#[must_use]
pub fn load() -> MediaPrefs {
    let Ok(_g) = LOCK.lock() else {
        return MediaPrefs::default();
    };
    let mut prefs = load_file();
    prefs.volume = prefs.volume.clamp(0.0, 150.0);
    prefs
}

/// Persist volume / mute / hwdec / pitch-preserve.
pub fn store(volume: f64, muted: bool, hwdec_mode: u32, pitch_preserve: bool) {
    update(|prefs| {
        prefs.volume = volume.clamp(0.0, 150.0);
        prefs.muted = muted;
        prefs.hwdec_mode = hwdec_mode.min(1);
        prefs.pitch_preserve = pitch_preserve;
    });
}

/// Persist subtitle style + ReplayGain indices.
pub fn store_look(sub_style_index: u32, replaygain_index: u32) {
    update(|prefs| {
        prefs.sub_style_index = sub_style_index;
        prefs.replaygain_index = replaygain_index;
    });
}

/// Remember the folder shown in the next media file picker.
pub fn store_last_folder(folder: &Path) {
    update(|prefs| {
        prefs.last_folder = Some(folder.to_path_buf());
    });
}

/// Persist playlist side-panel open state.
pub fn store_playlist_panel_open(open: bool) {
    update(|prefs| {
        prefs.playlist_panel_open = open;
    });
}
