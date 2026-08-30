//! Audio / video viewer backed by libmpv (RGBA blit) with system-player fallback.

mod cover;
mod engine;
mod ffi;
mod resume;
mod sidecars;

use std::any::Any;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;

use crate::error::Result;
use crate::snapshot::{MediaSnapshot, ViewerSnapshot};
use crate::viewer_trait::Viewer;

pub use cover::{discover_cover_sidecars, load_cover_art, load_media_tags};
pub use ffi::mpv_available;
pub use sidecars::discover_sidecar_subs;

/// Native file dialog for picking a local audio/video file.
#[must_use]
pub fn pick_media_file() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter(
            "Media",
            &[
                "mp4", "mkv", "webm", "avi", "mov", "wmv", "m4v", "mpeg", "mpg", "mp3", "wav",
                "flac", "ogg", "aac", "m4a", "wma", "opus", "aiff",
            ],
        )
        .add_filter("All", &["*"])
        .pick_file()
}

use engine::MpvEngine;

/// Extensions treated as audio or video.
pub const MEDIA_FILE_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "webm", "avi", "mov", "wmv", "m4v", "mpeg", "mpg", "mp3", "wav", "flac", "ogg",
    "aac", "m4a", "wma", "opus", "aiff",
];

/// Whether `ext` (lowercase, no dot) is a media file.
#[must_use]
pub fn is_media_file_extension(ext: &str) -> bool {
    MEDIA_FILE_EXTENSIONS.contains(&ext)
}

fn kind_label_for(ext: &str) -> &'static str {
    match ext {
        "mp3" | "wav" | "flac" | "ogg" | "aac" | "m4a" | "wma" | "opus" | "aiff" => "audio",
        _ => "video",
    }
}

/// In-app media viewer (libmpv when available).
#[derive(Debug)]
pub struct MediaViewer {
    path: RwLock<Option<orchid_fs::FsPath>>,
    os_path: RwLock<Option<PathBuf>>,
    kind_label: RwLock<String>,
    engine: MpvEngine,
    /// Playlist index overlay (0-based); set by the widget.
    playlist_index: RwLock<u32>,
    playlist_count: RwLock<u32>,
    playlist_shuffle: RwLock<bool>,
}

impl Default for MediaViewer {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaViewer {
    /// Empty media viewer with a dedicated mpv worker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            path: RwLock::new(None),
            os_path: RwLock::new(None),
            kind_label: RwLock::new(String::new()),
            engine: MpvEngine::spawn(),
            playlist_index: RwLock::new(0),
            playlist_count: RwLock::new(0),
            playlist_shuffle: RwLock::new(false),
        }
    }

    /// Update playlist chrome (1-based display uses index+1).
    pub fn set_playlist_info(&self, index: u32, count: u32, shuffle: bool) {
        *self.playlist_index.write() = index;
        *self.playlist_count.write() = count;
        *self.playlist_shuffle.write() = shuffle;
    }

    /// True when libmpv loaded successfully.
    #[must_use]
    pub fn engine_available(&self) -> bool {
        self.engine.shared.available.load(Ordering::Relaxed)
    }

    /// Whether playback is currently unpaused.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.engine.shared.playing.load(Ordering::Relaxed)
    }

    /// Consume dirty flag from the engine (frame / transport).
    #[must_use]
    pub fn take_dirty(&self) -> bool {
        self.engine.take_dirty()
    }

    /// Toggle play / pause.
    pub fn play_pause(&self) {
        self.engine.play_pause();
    }

    /// Seek relative seconds.
    pub fn seek_rel(&self, seconds: f64) {
        self.engine.seek_rel(seconds);
    }

    /// Seek to absolute position in seconds.
    pub fn seek_abs(&self, seconds: f64) {
        self.engine.seek_abs(seconds);
    }

    /// Seek to a 0..1 progress fraction.
    pub fn seek_fraction(&self, frac: f64) {
        let dur = self.engine.shared.duration_ms.load(Ordering::Relaxed) as f64 / 1000.0;
        if dur > 0.0 {
            self.engine.seek_abs(frac.clamp(0.0, 1.0) * dur);
        }
    }

    /// Set absolute volume 0..150.
    pub fn set_volume(&self, volume: f64) {
        self.engine.set_volume(volume);
    }

    /// Adjust volume by delta.
    pub fn volume_delta(&self, delta: f64) {
        self.engine.volume_delta(delta);
    }

    /// Set playback speed.
    pub fn set_speed(&self, speed: f64) {
        self.engine.set_speed(speed);
    }

    /// Adjust playback speed.
    pub fn speed_delta(&self, delta: f64) {
        self.engine.speed_delta(delta);
    }

    /// Toggle mute.
    pub fn mute_toggle(&self) {
        self.engine.mute_toggle();
    }

    /// Cycle subtitle tracks.
    pub fn cycle_sub(&self) {
        self.engine.cycle_sub();
    }

    /// Toggle subtitle visibility.
    pub fn toggle_sub(&self) {
        self.engine.toggle_sub();
    }

    /// Cycle audio tracks.
    pub fn cycle_audio(&self) {
        self.engine.cycle_audio();
    }

    /// Jump to next chapter.
    pub fn chapter_next(&self) {
        self.engine.chapter_next();
    }

    /// Jump to previous chapter.
    pub fn chapter_prev(&self) {
        self.engine.chapter_prev();
    }

    /// Mark A-B loop start.
    pub fn ab_mark_a(&self) {
        self.engine.ab_mark_a();
    }

    /// Mark A-B loop end.
    pub fn ab_mark_b(&self) {
        self.engine.ab_mark_b();
    }

    /// Clear A-B loop.
    pub fn ab_clear(&self) {
        self.engine.ab_clear();
    }

    /// Open a native file dialog and load an external subtitle.
    pub fn pick_and_add_sub(&self) {
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Subtitles", &["srt", "ass", "ssa", "vtt", "sub"])
            .add_filter("All", &["*"]);
        if let Some(os) = self.os_path.read().clone() {
            if let Some(parent) = os.parent() {
                dialog = dialog.set_directory(parent);
            }
        }
        if let Some(path) = dialog.pick_file() {
            self.engine.add_sub(&path);
        }
    }

    /// Nudge subtitle scale (typical ±0.05).
    pub fn sub_scale_delta(&self, delta: f64) {
        self.engine.sub_scale_delta(delta);
    }

    /// Nudge vertical subtitle position (mpv `sub-pos`, typical ±2).
    pub fn sub_pos_delta(&self, delta: f64) {
        self.engine.sub_pos_delta(delta);
    }

    /// Reset subtitle scale/position defaults.
    pub fn sub_style_reset(&self) {
        self.engine.sub_style_reset();
    }

    /// Cycle simple EQ presets (Flat / Bass / Treble / Vocal).
    pub fn cycle_eq(&self) {
        self.engine.cycle_eq();
    }

    /// Cycle hardware decode: `auto-copy` ↔ software (`no`).
    pub fn cycle_hwdec(&self) {
        self.engine.cycle_hwdec();
    }

    /// Apply a media command string from the UI.
    pub fn apply_command(&self, command: &str) {
        match command {
            "play-pause" | "play" | "pause" => self.play_pause(),
            "seek-back-5" => self.seek_rel(-5.0),
            "seek-fwd-5" => self.seek_rel(5.0),
            "seek-back-10" => self.seek_rel(-10.0),
            "seek-fwd-10" => self.seek_rel(10.0),
            "mute" => self.mute_toggle(),
            "vol-up" => self.volume_delta(5.0),
            "vol-down" => self.volume_delta(-5.0),
            "speed-up" => self.speed_delta(0.1),
            "speed-down" => self.speed_delta(-0.1),
            "speed-reset" => self.set_speed(1.0),
            "cycle-sub" => self.cycle_sub(),
            "toggle-sub" => self.toggle_sub(),
            "sub-add" => self.pick_and_add_sub(),
            "sub-scale-up" => self.sub_scale_delta(0.05),
            "sub-scale-down" => self.sub_scale_delta(-0.05),
            "sub-pos-up" => self.sub_pos_delta(-2.0),
            "sub-pos-down" => self.sub_pos_delta(2.0),
            "sub-style-reset" => self.sub_style_reset(),
            "cycle-eq" => self.cycle_eq(),
            "cycle-hwdec" => self.cycle_hwdec(),
            "cycle-audio" => self.cycle_audio(),
            "chapter-next" => self.chapter_next(),
            "chapter-prev" => self.chapter_prev(),
            "ab-a" => self.ab_mark_a(),
            "ab-b" => self.ab_mark_b(),
            "ab-clear" => self.ab_clear(),
            cmd if let Some(raw) = cmd.strip_prefix("seek-frac:") => {
                if let Ok(f) = raw.parse::<f64>() {
                    self.seek_fraction(f);
                }
            }
            cmd if let Some(raw) = cmd.strip_prefix("volume:") => {
                if let Ok(v) = raw.parse::<f64>() {
                    self.set_volume(v);
                }
            }
            cmd if let Some(raw) = cmd.strip_prefix("speed:") => {
                if let Ok(s) = raw.parse::<f64>() {
                    self.set_speed(s);
                }
            }
            _ => {}
        }
    }
}

#[async_trait]
impl Viewer for MediaViewer {
    fn type_id(&self) -> &'static str {
        "media"
    }

    async fn open(
        &mut self,
        path: orchid_fs::FsPath,
        _registry: Arc<orchid_fs::FsProviderRegistry>,
    ) -> Result<()> {
        let ext = path
            .file_name()
            .and_then(|n| n.rsplit_once('.'))
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        *self.kind_label.write() = kind_label_for(&ext).into();
        let os = path.to_local().ok();
        *self.os_path.write() = os.clone();
        *self.path.write() = Some(path);
        if let Some(os) = os {
            let tags = cover::load_media_tags(&os);
            let art = cover::load_cover_art(&os);
            self.engine
                .set_cover_and_tags(art, tags.title, tags.artist);
            if self.engine_available() {
                self.engine.load(&os);
            }
        }
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        self.engine.stop();
        *self.path.write() = None;
        *self.os_path.write() = None;
        Ok(())
    }

    fn snapshot(&self) -> ViewerSnapshot {
        let path_display = self
            .path
            .read()
            .as_ref()
            .map(|p| p.as_str().to_string())
            .unwrap_or_default();
        let kind_label = self.kind_label.read().clone();
        let shared = &self.engine.shared;
        let available = shared.available.load(Ordering::Relaxed);
        let frame = shared.frame.read().clone();
        let (frame_rgba, frame_width, frame_height) = match frame {
            Some(f) => (f.rgba, f.width, f.height),
            None => (Arc::new(Vec::new()), 0, 0),
        };
        let cover = shared.cover.read().clone();
        let (has_cover, cover_rgba, cover_width, cover_height) = match cover {
            Some(f) => (true, f.rgba, f.width, f.height),
            None => (false, Arc::new(Vec::new()), 0, 0),
        };
        let position_ms = shared.position_ms.load(Ordering::Relaxed);
        let duration_ms = shared.duration_ms.load(Ordering::Relaxed);
        let progress = if duration_ms > 0 {
            (position_ms as f32 / duration_ms as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let error = shared.error.read().clone().unwrap_or_default();
        ViewerSnapshot::Media(MediaSnapshot {
            path_display,
            kind_label,
            info_text: String::new(),
            available,
            playing: shared.playing.load(Ordering::Relaxed),
            position_ms,
            duration_ms,
            progress,
            volume: shared.volume.load(Ordering::Relaxed),
            muted: shared.muted.load(Ordering::Relaxed),
            speed: shared.speed_x100.load(Ordering::Relaxed) as f32 / 100.0,
            has_video: shared.has_video.load(Ordering::Relaxed),
            frame_rgba,
            frame_width,
            frame_height,
            has_cover,
            cover_rgba,
            cover_width,
            cover_height,
            title: shared.title.read().clone(),
            artist: shared.artist.read().clone(),
            playlist_index: *self.playlist_index.read(),
            playlist_count: *self.playlist_count.read(),
            playlist_shuffle: *self.playlist_shuffle.read(),
            sub_label: shared.sub_label.read().clone(),
            sub_visible: shared.sub_visible.load(Ordering::Relaxed),
            audio_label: shared.audio_label.read().clone(),
            chapter_label: shared.chapter_label.read().clone(),
            ab_label: shared.ab_label.read().clone(),
            eq_label: shared.eq_label.read().clone(),
            hwdec_label: shared.hwdec_label.read().clone(),
            error,
        })
    }

    fn current_path(&self) -> Option<&orchid_fs::FsPath> {
        // Stored behind RwLock; trait wants &FsPath — keep None like before
        // and let the widget track path separately.
        None
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
