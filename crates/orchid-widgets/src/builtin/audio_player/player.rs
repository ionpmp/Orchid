//! Playback session wrapping a shared audio-only [`MpvEngine`].

#![allow(missing_docs)]

use std::path::Path;
use std::sync::atomic::Ordering;

use orchid_viewers::{load_cover_art, load_media_tags, EngineMode, FrameBuf, MpvEngine};

/// Owns the libmpv audio session for the library player.
#[derive(Debug)]
pub struct PlayerSession {
    engine: MpvEngine,
}

impl PlayerSession {
    #[must_use]
    pub fn new() -> Self {
        Self {
            engine: MpvEngine::spawn_with_mode(EngineMode::Audio),
        }
    }

    #[must_use]
    pub fn available(&self) -> bool {
        self.engine.shared.available.load(Ordering::Relaxed)
    }

    pub fn load_path(&self, path: &Path) {
        let tags = load_media_tags(path);
        let art = load_cover_art(path);
        self.engine
            .set_cover_and_tags(art, tags.title, tags.artist);
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
    pub fn artist(&self) -> String {
        self.engine.shared.artist.read().clone()
    }

    #[must_use]
    pub fn cover(&self) -> Option<FrameBuf> {
        self.engine.shared.cover.read().clone()
    }

    pub fn pause(&self) {
        if self.is_playing() {
            self.engine.play_pause();
        }
    }

    pub fn cycle_eq(&self) {
        self.engine.cycle_eq();
    }

    #[must_use]
    pub fn eq_label(&self) -> String {
        self.engine.shared.eq_label.read().clone()
    }
}

impl Default for PlayerSession {
    fn default() -> Self {
        Self::new()
    }
}
