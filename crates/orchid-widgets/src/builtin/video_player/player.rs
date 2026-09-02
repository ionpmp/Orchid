//! Playback session wrapping a shared video [`MpvEngine`].

#![allow(missing_docs)]

use std::path::Path;
use std::sync::atomic::Ordering;

use orchid_viewers::{load_media_tags, EngineMode, FrameBuf, MpvEngine};

/// Owns the libmpv video session for the library player.
#[derive(Debug)]
pub struct PlayerSession {
    engine: MpvEngine,
}

impl PlayerSession {
    #[must_use]
    pub fn new() -> Self {
        Self {
            engine: MpvEngine::spawn_with_mode(EngineMode::Video),
        }
    }

    #[must_use]
    pub fn available(&self) -> bool {
        self.engine.shared.available.load(Ordering::Relaxed)
    }

    pub fn load_path(&self, path: &Path) {
        let tags = load_media_tags(path);
        self.engine
            .set_cover_and_tags(None, tags.title, tags.artist);
        if self.available() {
            self.engine.load(path);
        }
    }

    pub fn play_pause(&self) {
        self.engine.play_pause();
    }

    pub fn stop(&self) {
        self.engine.stop();
    }

    pub fn seek_fraction(&self, frac: f64) {
        let dur = self.engine.shared.duration_ms.load(Ordering::Relaxed) as f64 / 1000.0;
        if dur > 0.0 {
            self.engine.seek_abs(frac.clamp(0.0, 1.0) * dur);
        }
    }

    pub fn set_volume(&self, volume: f64) {
        self.engine.set_volume(volume);
    }

    pub fn mute_toggle(&self) {
        self.engine.mute_toggle();
    }

    #[must_use]
    pub fn take_dirty(&self) -> bool {
        self.engine.take_dirty()
    }

    #[must_use]
    pub fn take_eof(&self) -> bool {
        self.engine.take_eof()
    }

    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.engine.shared.playing.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn position_ms(&self) -> u64 {
        self.engine.shared.position_ms.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        self.engine.shared.duration_ms.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn volume(&self) -> u32 {
        self.engine.shared.volume.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn muted(&self) -> bool {
        self.engine.shared.muted.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn title(&self) -> String {
        self.engine.shared.title.read().clone()
    }

    #[must_use]
    pub fn has_video(&self) -> bool {
        self.engine.shared.has_video.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn frame(&self) -> Option<FrameBuf> {
        self.engine.shared.frame.read().clone()
    }

    pub fn pause(&self) {
        self.engine.pause();
    }

    pub fn volume_delta(&self, delta: f64) {
        self.engine.volume_delta(delta);
    }

    pub fn seek_rel(&self, seconds: f64) {
        self.engine.seek_rel(seconds);
    }

    /// Cycle playback rate through common presets.
    pub fn cycle_speed(&self) {
        const PRESETS: &[f64] = &[0.75, 1.0, 1.25, 1.5, 2.0];
        let cur = self.speed();
        let next = PRESETS
            .iter()
            .copied()
            .find(|p| *p > cur + 0.01)
            .unwrap_or(PRESETS[0]);
        self.engine.set_speed(next);
    }

    pub fn apply_session_prefs(&self, speed: f64) {
        if (speed - 1.0).abs() > 0.01 {
            self.engine.set_speed(speed);
        }
    }

    #[must_use]
    pub fn speed(&self) -> f64 {
        self.engine.shared.speed_x100.load(Ordering::Relaxed) as f64 / 100.0
    }

    #[must_use]
    pub fn speed_label(&self) -> String {
        let s = self.speed();
        if (s - 1.0).abs() < 0.01 {
            String::new()
        } else {
            let rounded = (s * 100.0).round() / 100.0;
            if (rounded * 100.0).round() % 100.0 == 0.0 {
                format!("{rounded:.0}x")
            } else {
                format!("{rounded:.2}x")
            }
        }
    }
}

impl Default for PlayerSession {
    fn default() -> Self {
        Self::new()
    }
}
