//! Local audio library player (Lark-style core) on shared audio-only libmpv.

#![allow(missing_docs)]

pub mod config;
pub mod library;
pub mod player;
pub mod queue;
pub mod sleep;

use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::RwLock;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::{
    AudioPlayerGroupRow, AudioPlayerPayload, AudioPlayerPlaylistRow, AudioPlayerTrackRow,
};
use crate::widget::refresh::PeriodicRefresh;
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};
use orchid_storage::{LifecycleState, WidgetSize};

pub use config::{AudioPlayerConfig, BrowseTab, PlaylistEntry, RepeatMode};
use library::{LibraryIndex, TrackRow};
use player::PlayerSession;
use queue::PlayQueue;
use sleep::SleepTimer;

/// Stable type id.
pub const TYPE_ID: &str = "audio-player";

static AUDIO_LIVE: LazyLock<DashMap<Uuid, Arc<AudioHandle>>> = LazyLock::new(DashMap::new);

struct AudioHandle {
    instance_id: Uuid,
    config: Arc<RwLock<AudioPlayerConfig>>,
    library: Arc<RwLock<LibraryIndex>>,
    queue: Arc<RwLock<PlayQueue>>,
    player: Arc<PlayerSession>,
    sleep: Arc<RwLock<SleepTimer>>,
    bus: Arc<orchid_core::EventBus>,
}

impl AudioHandle {
    fn publish(&self) {
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    fn sync_queue_to_config(&self) {
        let q = self.queue.read();
        let mut cfg = self.config.write();
        cfg.queue = q.paths.clone();
        cfg.queue_index = q.index as u32;
        cfg.shuffle = q.shuffle;
        cfg.repeat = q.repeat;
        cfg.volume = self.player.volume() as f32;
        cfg.muted = self.player.muted();
    }

    fn load_current(&self) {
        let path = {
            let q = self.queue.read();
            q.current().map(str::to_string)
        };
        if let Some(path) = path {
            self.player.load_path(PathBuf::from(&path).as_path());
            self.player.set_volume(f64::from(self.config.read().volume));
        }
        self.sync_queue_to_config();
        self.publish();
    }
}

/// Snapshot live config for settings (if any).
#[must_use]
pub fn current_config(instance_id: Uuid) -> Option<AudioPlayerConfig> {
    AUDIO_LIVE
        .get(&instance_id)
        .map(|h| h.config.read().clone())
}

/// Rescan library roots.
pub fn rescan(instance_id: Uuid) {
    let Some(h) = AUDIO_LIVE.get(&instance_id) else {
        return;
    };
    let roots = h.config.read().library_roots.clone();
    *h.library.write() = LibraryIndex::scan(&roots);
    h.publish();
}

/// Pick a folder via native dialog and add it as a library root.
pub fn add_library_root(instance_id: Uuid) {
    let Some(h) = AUDIO_LIVE.get(&instance_id) else {
        return;
    };
    let Some(dir) = orchid_viewers::pick_media_folder() else {
        return;
    };
    let s = dir.to_string_lossy().into_owned();
    {
        let mut cfg = h.config.write();
        if !cfg.library_roots.iter().any(|r| r == &s) {
            cfg.library_roots.push(s);
        }
    }
    let roots = h.config.read().library_roots.clone();
    *h.library.write() = LibraryIndex::scan(&roots);
    h.publish();
}

/// Execute a transport / browse command string.
pub fn execute_command(instance_id: Uuid, command: &str) {
    let Some(h) = AUDIO_LIVE.get(&instance_id) else {
        return;
    };
    match command {
        "play-pause" => {
            if h.queue.read().current().is_none() {
                // Nothing queued — play all songs.
                let paths: Vec<String> = h
                    .library
                    .read()
                    .tracks
                    .iter()
                    .map(|t| t.path.to_string_lossy().into_owned())
                    .collect();
                if paths.is_empty() {
                    return;
                }
                {
                    let cfg = h.config.read();
                    h.queue.write().replace(paths, 0);
                    h.queue.write().shuffle = cfg.shuffle;
                    h.queue.write().repeat = cfg.repeat;
                    h.queue.write().rebuild_order();
                }
                h.load_current();
            } else if !h.player.available() {
                // Engine missing — still toggle UI state by reloading.
                h.load_current();
            } else {
                h.player.play_pause();
                h.publish();
            }
        }
        "next" => {
            let next = h.queue.write().next().map(str::to_string);
            if let Some(path) = next {
                h.player.load_path(PathBuf::from(path).as_path());
                h.sync_queue_to_config();
                h.publish();
            }
        }
        "prev" | "previous" => {
            let prev = h.queue.write().previous().map(str::to_string);
            if let Some(path) = prev {
                h.player.load_path(PathBuf::from(path).as_path());
                h.sync_queue_to_config();
                h.publish();
            }
        }
        "stop" => {
            h.player.stop();
            h.publish();
        }
        "mute" => {
            h.player.mute_toggle();
            h.sync_queue_to_config();
            h.publish();
        }
        "shuffle" => {
            let next = !h.queue.read().shuffle;
            h.queue.write().set_shuffle(next);
            h.config.write().shuffle = next;
            h.publish();
        }
        "repeat" => {
            h.queue.write().cycle_repeat();
            h.config.write().repeat = h.queue.read().repeat;
            h.publish();
        }
        "sleep" => {
            h.sleep.write().cycle();
            h.publish();
        }
        "rescan" => {
            drop(h);
            rescan(instance_id);
            return;
        }
        "add-root" => {
            drop(h);
            add_library_root(instance_id);
            return;
        }
        "clear-filter" => {
            h.config.write().browse_filter.clear();
            h.publish();
        }
        "new-playlist" => {
            let mut cfg = h.config.write();
            let n = cfg.playlists.len() + 1;
            let pl = PlaylistEntry::new(format!("Playlist {n}"));
            cfg.active_playlist_id = pl.id.clone();
            cfg.playlists.push(pl);
            cfg.browse_tab = BrowseTab::Playlists;
            drop(cfg);
            h.publish();
        }
        cmd if let Some(raw) = cmd.strip_prefix("seek-frac:") => {
            if let Ok(f) = raw.parse::<f64>() {
                h.player.seek_fraction(f);
                h.publish();
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("volume:") => {
            if let Ok(v) = raw.parse::<f64>() {
                h.player.set_volume(v);
                h.config.write().volume = v as f32;
                h.publish();
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("sleep:") => {
            if let Ok(m) = raw.parse::<u32>() {
                h.sleep.write().set_minutes(m);
                h.publish();
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("tab:") => {
            if let Ok(t) = raw.parse::<u8>() {
                h.config.write().browse_tab = BrowseTab::from_u8(t);
                h.config.write().browse_filter.clear();
                h.publish();
            }
        }
        cmd if let Some(raw) = cmd.strip_prefix("open-group:") => {
            h.config.write().browse_filter = raw.to_string();
            h.publish();
        }
        cmd if let Some(raw) = cmd.strip_prefix("play-track:") => {
            play_track(&h, raw);
        }
        cmd if let Some(raw) = cmd.strip_prefix("toggle-favorite:") => {
            let mut cfg = h.config.write();
            if let Some(i) = cfg.favorites.iter().position(|p| p == raw) {
                cfg.favorites.remove(i);
            } else {
                cfg.favorites.push(raw.to_string());
            }
            drop(cfg);
            h.publish();
        }
        cmd if let Some(raw) = cmd.strip_prefix("select-playlist:") => {
            h.config.write().active_playlist_id = raw.to_string();
            h.config.write().browse_filter.clear();
            h.publish();
        }
        cmd if let Some(raw) = cmd.strip_prefix("add-to-playlist:") => {
            // format: playlistId\x1fpath
            if let Some((pid, path)) = raw.split_once('\u{1f}') {
                let mut cfg = h.config.write();
                if let Some(pl) = cfg.playlists.iter_mut().find(|p| p.id == pid) {
                    if !pl.tracks.iter().any(|t| t == path) {
                        pl.tracks.push(path.to_string());
                    }
                }
                drop(cfg);
                h.publish();
            }
        }
        _ => {}
    }
}

fn play_track(h: &AudioHandle, path: &str) {
    // Build queue from current browse context when possible.
    let cfg = h.config.read().clone();
    let lib = h.library.read();
    let playlist_tracks = cfg
        .playlists
        .iter()
        .find(|p| p.id == cfg.active_playlist_id)
        .map(|p| p.tracks.as_slice());
    let browse = lib.browse_rows(cfg.browse_tab, &cfg.browse_filter, playlist_tracks, &cfg.favorites);
    let paths: Vec<String> = if cfg.browse_tab == BrowseTab::NowPlaying {
        h.queue.read().paths.clone()
    } else if !browse.tracks.is_empty() {
        browse.tracks.iter().map(|t| t.path.clone()).collect()
    } else {
        lib.tracks
            .iter()
            .map(|t| t.path.to_string_lossy().into_owned())
            .collect()
    };
    drop(lib);
    let start = paths.iter().position(|p| p == path).unwrap_or(0);
    {
        let mut q = h.queue.write();
        q.replace(paths, start);
        q.shuffle = cfg.shuffle;
        q.repeat = cfg.repeat;
        if q.shuffle {
            q.rebuild_order();
        }
    }
    h.load_current();
}

fn format_time(ms: u64) -> String {
    let total = ms / 1000;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

fn track_rows_from(
    rows: &[TrackRow],
    current: Option<&str>,
    favorites: &[String],
) -> Vec<AudioPlayerTrackRow> {
    rows.iter()
        .map(|t| AudioPlayerTrackRow {
            path: t.path.clone(),
            title: t.title.clone(),
            artist: t.artist.clone(),
            album: t.album.clone(),
            subtitle: t.subtitle.clone(),
            is_current: current == Some(t.path.as_str()),
            is_favorite: favorites.iter().any(|f| f == &t.path),
        })
        .collect()
}

/// Audio player widget.
pub struct AudioPlayerWidget {
    instance_id: Uuid,
    handle: Arc<AudioHandle>,
    refresh: PeriodicRefresh,
}

impl std::fmt::Debug for AudioPlayerWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioPlayerWidget")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl AudioPlayerWidget {
    /// Construct from persisted config.
    pub fn new(
        instance_id: Uuid,
        mut config: AudioPlayerConfig,
        bus: Arc<orchid_core::EventBus>,
    ) -> Self {
        config.normalize();
        let library = LibraryIndex::scan(&config.library_roots);
        let queue = PlayQueue::from_paths(
            config.queue.clone(),
            config.queue_index as usize,
            config.shuffle,
            config.repeat,
        );
        let player = Arc::new(PlayerSession::new());
        if config.volume > 0.0 {
            player.set_volume(f64::from(config.volume));
        }
        let handle = Arc::new(AudioHandle {
            instance_id,
            config: Arc::new(RwLock::new(config)),
            library: Arc::new(RwLock::new(library)),
            queue: Arc::new(RwLock::new(queue)),
            player,
            sleep: Arc::new(RwLock::new(SleepTimer::default())),
            bus,
        });
        AUDIO_LIVE.insert(instance_id, Arc::clone(&handle));
        Self {
            instance_id,
            handle,
            refresh: PeriodicRefresh::new(Duration::from_millis(200)),
        }
    }

    fn build_payload(&self) -> AudioPlayerPayload {
        let cfg = self.handle.config.read().clone();
        let lib = self.handle.library.read();
        let q = self.handle.queue.read();
        let current = q.current().map(str::to_string);
        let playlist_tracks = cfg
            .playlists
            .iter()
            .find(|p| p.id == cfg.active_playlist_id)
            .map(|p| p.tracks.as_slice());
        let browse = if cfg.browse_tab == BrowseTab::NowPlaying {
            library::BrowseResult {
                groups: Vec::new(),
                tracks: q
                    .paths
                    .iter()
                    .filter_map(|p| {
                        lib.find_by_path(p).map(|t| TrackRow {
                            path: t.path.to_string_lossy().into_owned(),
                            title: t.title.clone(),
                            artist: t.artist.clone(),
                            album: t.album.clone(),
                            subtitle: format!("{} — {}", t.artist, t.album),
                        })
                        .or_else(|| {
                            let stem = PathBuf::from(p)
                                .file_stem()
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_else(|| p.clone());
                            Some(TrackRow {
                                path: p.clone(),
                                title: stem,
                                artist: String::new(),
                                album: String::new(),
                                subtitle: p.clone(),
                            })
                        })
                    })
                    .collect(),
            }
        } else {
            lib.browse_rows(
                cfg.browse_tab,
                &cfg.browse_filter,
                playlist_tracks,
                &cfg.favorites,
            )
        };

        let playlists: Vec<AudioPlayerPlaylistRow> = {
            let mut rows: Vec<_> = cfg
                .playlists
                .iter()
                .map(|p| AudioPlayerPlaylistRow {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    count: p.tracks.len() as u32,
                    is_active: p.id == cfg.active_playlist_id,
                })
                .collect();
            rows.insert(
                0,
                AudioPlayerPlaylistRow {
                    id: String::new(),
                    name: "Favorites".into(),
                    count: cfg.favorites.len() as u32,
                    is_active: cfg.active_playlist_id.is_empty()
                        && cfg.browse_tab == BrowseTab::Playlists,
                },
            );
            rows
        };

        let album = current
            .as_deref()
            .and_then(|p| lib.find_by_path(p))
            .map(|t| t.album.clone())
            .unwrap_or_default();

        let pos = self.handle.player.position_ms();
        let dur = self.handle.player.duration_ms();
        let progress = if dur > 0 {
            (pos as f32 / dur as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let cover = self.handle.player.cover();
        let (has_cover, cover_rgba, cover_width, cover_height) = match cover {
            Some(f) => (true, f.rgba, f.width, f.height),
            None => (false, Arc::new(Vec::new()), 0, 0),
        };

        let roots_label = if cfg.library_roots.is_empty() {
            String::new()
        } else {
            format!("{} folders", cfg.library_roots.len())
        };

        let empty_hint = if cfg.library_roots.is_empty() {
            "audio-player-empty-roots".into()
        } else if lib.tracks.is_empty() {
            "audio-player-empty-library".into()
        } else {
            String::new()
        };

        let title = if self.handle.player.title().is_empty() {
            current
                .as_deref()
                .and_then(|p| lib.find_by_path(p).map(|t| t.title.clone()))
                .unwrap_or_default()
        } else {
            self.handle.player.title()
        };
        let artist = if self.handle.player.artist().is_empty() {
            current
                .as_deref()
                .and_then(|p| lib.find_by_path(p).map(|t| t.artist.clone()))
                .unwrap_or_default()
        } else {
            self.handle.player.artist()
        };

        AudioPlayerPayload {
            engine_available: self.handle.player.available(),
            browse_tab: cfg.browse_tab.as_u8(),
            browse_filter: cfg.browse_filter.clone(),
            browse_filter_label: cfg.browse_filter.clone(),
            groups: browse
                .groups
                .into_iter()
                .map(|g| AudioPlayerGroupRow {
                    key: g.key,
                    label: g.label,
                    count: g.count,
                })
                .collect(),
            tracks: track_rows_from(
                &browse.tracks,
                current.as_deref(),
                &cfg.favorites,
            ),
            playlists,
            queue: track_rows_from(
                &browse.tracks,
                current.as_deref(),
                &cfg.favorites,
            ),
            queue_index: q.index as i32,
            has_track: current.is_some(),
            title,
            artist,
            album,
            is_playing: self.handle.player.is_playing(),
            position_ms: pos,
            duration_ms: dur,
            progress,
            position_label: format_time(pos),
            duration_label: format_time(dur),
            volume: self.handle.player.volume(),
            muted: self.handle.player.muted(),
            shuffle: q.shuffle,
            repeat: q.repeat.as_u8(),
            sleep_label: self.handle.sleep.read().label.clone(),
            library_count: lib.tracks.len() as u32,
            roots_label,
            empty_hint,
            has_cover,
            cover_rgba,
            cover_width,
            cover_height,
        }
    }

    fn start_refresh(&self) {
        let handle = Arc::clone(&self.handle);
        self.refresh.start(move || {
            let handle = Arc::clone(&handle);
            async move {
                let mut dirty = handle.player.take_dirty();
                if handle.player.take_eof() {
                    let next = handle.queue.write().next().map(str::to_string);
                    if let Some(path) = next {
                        handle.player.load_path(PathBuf::from(path).as_path());
                        handle.sync_queue_to_config();
                        dirty = true;
                    }
                }
                if handle.sleep.write().tick() {
                    handle.player.pause();
                    dirty = true;
                }
                if dirty || handle.player.is_playing() || handle.sleep.read().is_active() {
                    handle.publish();
                }
            }
        });
    }
}

#[async_trait]
impl Widget for AudioPlayerWidget {
    fn type_id(&self) -> &'static str {
        TYPE_ID
    }

    fn instance_id(&self) -> Uuid {
        self.instance_id
    }

    async fn on_create(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }

    async fn on_activate(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.start_refresh();
        Ok(())
    }

    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        // Keep playback running in the background; only stop UI polling.
        self.refresh.stop();
        Ok(())
    }

    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.refresh.stop();
        Ok(())
    }

    async fn on_close(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.refresh.stop();
        self.handle.sync_queue_to_config();
        AUDIO_LIVE.remove(&self.instance_id);
        Ok(())
    }

    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }

    fn snapshot(&self) -> Option<WidgetSnapshot> {
        let payload = self.build_payload();
        let title = if payload.title.is_empty() {
            String::new()
        } else {
            payload.title.clone()
        };
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title,
            status: if payload.engine_available {
                WidgetStatus::Ready
            } else {
                WidgetStatus::Stale
            },
            payload: WidgetPayload::AudioPlayer(payload),
        })
    }

    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        self.handle.sync_queue_to_config();
        let cfg = self.handle.config.read().clone();
        state_codec::save_state(&cfg)
    }

    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let mut cfg: AudioPlayerConfig = state_codec::restore_state(bytes).unwrap_or_default();
        cfg.normalize();
        let library = LibraryIndex::scan(&cfg.library_roots);
        let queue = PlayQueue::from_paths(
            cfg.queue.clone(),
            cfg.queue_index as usize,
            cfg.shuffle,
            cfg.repeat,
        );
        *self.handle.library.write() = library;
        *self.handle.queue.write() = queue;
        *self.handle.config.write() = cfg;
        Ok(())
    }

    fn capabilities(&self) -> WidgetCapabilities {
        WidgetCapabilities {
            supports_resize: true,
            min_size: Some(WidgetSize::Medium),
            max_size: None,
            preferred_size: Some(WidgetSize::Large),
            allows_grouping: true,
            keeps_state_when_unloaded: true,
            has_settings_panel: false,
        }
    }
}

/// Descriptor for the audio library player.
#[must_use]
pub fn descriptor() -> WidgetDescriptor {
    let factory: WidgetFactory = Arc::new(|ctx: WidgetContext, bytes| {
        let config = bytes
            .and_then(|b| state_codec::restore_state::<AudioPlayerConfig>(b).ok())
            .unwrap_or_default();
        Ok(Box::new(AudioPlayerWidget::new(
            ctx.instance_id,
            config,
            ctx.bus.clone(),
        )) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-audio-player-name",
        description_key: "widget-audio-player-desc",
        icon_name: "audio-player",
        category: WidgetCategory::Media,
        default_size: WidgetSize::Large,
        min_size: Some(WidgetSize::Medium),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: false,
        factory,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_type_id() {
        let d = descriptor();
        assert_eq!(d.type_id, "audio-player");
        assert!(!d.allows_multiple_instances);
    }

    #[tokio::test]
    async fn snapshot_empty_library() {
        let bus = Arc::new(orchid_core::EventBus::new(
            orchid_core::EventBusConfig::default(),
        ));
        let id = Uuid::new_v4();
        let w = AudioPlayerWidget::new(id, AudioPlayerConfig::default(), bus);
        let snap = w.snapshot().expect("snapshot");
        match snap.payload {
            WidgetPayload::AudioPlayer(p) => {
                assert_eq!(p.library_count, 0);
                assert!(!p.has_track);
            }
            _ => panic!("expected AudioPlayer"),
        }
        AUDIO_LIVE.remove(&id);
    }
}
