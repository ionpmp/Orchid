//! Local video library player on shared video libmpv (RGBA blit).

#![allow(missing_docs)]

pub mod config;
pub mod library;
pub mod player;
pub mod queue;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    VideoPlayerGroupRow, VideoPlayerItemRow, VideoPlayerPayload, VideoPlayerRootRow,
};
use crate::widget::refresh::PeriodicRefresh;
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};
use orchid_storage::{LifecycleState, WidgetSize};

pub use config::{BrowseTab, RepeatMode, VideoPlayerConfig};
use library::{LibraryIndex, VideoRow};
use player::PlayerSession;
use queue::PlayQueue;

/// Stable type id.
pub const TYPE_ID: &str = "video-player";

static VIDEO_LIVE: LazyLock<DashMap<Uuid, Arc<VideoHandle>>> = LazyLock::new(DashMap::new);

struct VideoHandle {
    instance_id: Uuid,
    config: Arc<RwLock<VideoPlayerConfig>>,
    library: Arc<RwLock<LibraryIndex>>,
    queue: Arc<RwLock<PlayQueue>>,
    player: Arc<PlayerSession>,
    bus: Arc<orchid_core::EventBus>,
    scanning: AtomicBool,
    scan_gen: AtomicU64,
    /// Bumped when Queue should auto-scroll to the current track.
    scroll_gen: AtomicU64,
    last_scroll: parking_lot::Mutex<(u8, Option<String>)>,
    /// Last published `position_ms / 1000` so a playing tick without a new
    /// frame / property change does not republish the whole library snapshot.
    last_pos_sec: AtomicU64,
}

impl VideoHandle {
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
        cfg.speed_x100 = (self.player.speed() * 100.0).round() as u32;
    }

    /// Advance [`Self::scroll_gen`] when the Queue tab should re-centre on the current track.
    fn note_queue_scroll(&self, tab: BrowseTab, path: Option<&str>) -> u64 {
        let tab_u = tab.as_u8();
        let mut last = self.last_scroll.lock();
        let changed = last.0 != tab_u || last.1.as_deref() != path;
        if changed {
            last.0 = tab_u;
            last.1 = path.map(str::to_string);
            if tab == BrowseTab::Queue {
                self.scroll_gen.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.scroll_gen.load(Ordering::Relaxed)
    }

    fn load_current(&self) {
        let path = {
            let q = self.queue.read();
            q.current().map(str::to_string)
        };
        if let Some(path) = path {
            self.load_path(&path);
        }
        self.sync_queue_to_config();
        self.publish();
    }

    fn load_path(&self, path: &str) {
        let p = PathBuf::from(path);
        self.player.load_path(p.as_path());
        self.player.set_volume(f64::from(self.config.read().volume));
        pause_rivals();
        #[cfg(windows)]
        {
            crate::builtin::viewer::smtc_publisher::set_active_video(self.instance_id);
            self.push_smtc();
        }
    }

    fn restore_current_paused(&self) {
        let path = {
            let q = self.queue.read();
            q.current().map(str::to_string)
        };
        let Some(path) = path else {
            return;
        };
        self.load_path(&path);
        self.player.pause();
    }

    #[cfg(windows)]
    fn push_smtc(&self) {
        use crate::builtin::viewer::smtc_publisher::{publish_now_playing, NowPlaying};
        let title = self.player.title();
        let title = if title.is_empty() {
            self.queue
                .read()
                .current()
                .map(|p| {
                    std::path::Path::new(p)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(p)
                        .to_string()
                })
                .unwrap_or_default()
        } else {
            title
        };
        publish_now_playing(
            self.instance_id,
            &NowPlaying {
                available: self.player.available(),
                title,
                artist: String::new(),
                playing: self.player.is_playing(),
                position_ms: self.player.position_ms(),
                duration_ms: self.player.duration_ms(),
                has_cover: false,
                cover_rgba: Arc::new(Vec::new()),
                cover_width: 0,
                cover_height: 0,
            },
        );
    }
}

fn kick_scan(handle: Arc<VideoHandle>) {
    let gen = handle.scan_gen.fetch_add(1, Ordering::AcqRel) + 1;
    handle.scanning.store(true, Ordering::Release);
    handle.publish();
    let roots = handle.config.read().library_roots.clone();
    tokio::task::spawn_blocking(move || {
        let index = LibraryIndex::scan(&roots);
        if handle.scan_gen.load(Ordering::Acquire) != gen {
            return;
        }
        *handle.library.write() = index;
        handle.scanning.store(false, Ordering::Release);
        handle.publish();
    });
}

/// Pause every live Video Player instance.
pub fn pause_all() {
    for h in VIDEO_LIVE.iter() {
        if h.player.is_playing() {
            h.player.pause();
            h.publish();
        }
    }
}

fn pause_rivals() {
    crate::builtin::audio_player::pause_all();
    tokio::spawn(async {
        crate::builtin::viewer::pause_all_media().await;
    });
}

/// Snapshot live config (if any).
#[must_use]
pub fn current_config(instance_id: Uuid) -> Option<VideoPlayerConfig> {
    VIDEO_LIVE
        .get(&instance_id)
        .map(|h| h.config.read().clone())
}

/// Rescan library roots.
pub fn rescan(instance_id: Uuid) {
    let Some(h) = VIDEO_LIVE.get(&instance_id) else {
        return;
    };
    kick_scan(Arc::clone(h.value()));
}

/// Pick a folder via native dialog and add it as a library root.
pub fn add_library_root(instance_id: Uuid) {
    let Some(dir) = orchid_viewers::pick_media_folder() else {
        return;
    };
    add_library_root_path(instance_id, &dir.to_string_lossy());
}

/// Add `root` as a library folder (deduped) and rescan.
pub fn add_library_root_path(instance_id: Uuid, root: &str) {
    let Some(h) = VIDEO_LIVE.get(&instance_id) else {
        return;
    };
    let s = root.trim();
    if s.is_empty() {
        return;
    }
    {
        let mut cfg = h.config.write();
        if !cfg.library_roots.iter().any(|r| r == s) {
            cfg.library_roots.push(s.to_string());
        } else {
            return;
        }
    }
    kick_scan(Arc::clone(h.value()));
}

/// Remove a library root folder (by exact path). Triggers a rescan.
pub fn remove_library_root(instance_id: Uuid, root: &str) {
    let Some(h) = VIDEO_LIVE.get(&instance_id) else {
        return;
    };
    {
        let mut cfg = h.config.write();
        let before = cfg.library_roots.len();
        cfg.library_roots.retain(|r| r != root);
        if cfg.library_roots.len() == before {
            return;
        }
    }
    kick_scan(Arc::clone(h.value()));
}

/// Ingest Explorer / OS drop paths: folders become library roots, video files enqueue.
///
/// Returns `true` if at least one path was handled.
pub fn ingest_os_paths(instance_id: Uuid, paths: &[String]) -> bool {
    let Some(h) = VIDEO_LIVE.get(&instance_id) else {
        return false;
    };
    let mut added_root = false;
    let mut enqueued = false;
    for raw in paths {
        let path = std::path::Path::new(raw);
        if path.is_dir() {
            let s = path.to_string_lossy();
            let mut cfg = h.config.write();
            if !cfg.library_roots.iter().any(|r| r == s.as_ref()) {
                cfg.library_roots.push(s.into_owned());
                added_root = true;
            }
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if !library::is_video_extension(&ext.to_ascii_lowercase()) {
            continue;
        }
        if h.queue.write().enqueue_end(raw) {
            enqueued = true;
        }
    }
    if added_root {
        kick_scan(Arc::clone(h.value()));
    }
    if enqueued {
        h.sync_queue_to_config();
        h.publish();
    }
    added_root || enqueued
}

/// Replace the queue with `paths` and start playback at index 0.
pub fn play_paths(instance_id: Uuid, paths: Vec<String>) {
    let Some(h) = VIDEO_LIVE.get(&instance_id) else {
        return;
    };
    if paths.is_empty() {
        return;
    }
    let cfg = h.config.read().clone();
    {
        let mut q = h.queue.write();
        q.replace(paths, 0);
        q.shuffle = cfg.shuffle;
        q.repeat = cfg.repeat;
        if q.shuffle {
            q.rebuild_order();
        }
    }
    h.load_current();
}

/// Append `paths` to the queue (deduped). Does not start playback.
pub fn enqueue_paths(instance_id: Uuid, paths: &[String]) {
    let Some(h) = VIDEO_LIVE.get(&instance_id) else {
        return;
    };
    let mut any = false;
    {
        let mut q = h.queue.write();
        for p in paths {
            if q.enqueue_end(p) {
                any = true;
            }
        }
    }
    if any {
        h.sync_queue_to_config();
        h.publish();
    }
}

/// Path of the currently playing / loaded video, if any.
#[must_use]
pub fn current_track_path(instance_id: Uuid) -> Option<String> {
    VIDEO_LIVE
        .get(&instance_id)?
        .queue
        .read()
        .current()
        .map(str::to_string)
}

/// Dispatch a UI command string.
pub fn execute_command(instance_id: Uuid, command: &str) {
    let Some(h) = VIDEO_LIVE.get(&instance_id) else {
        return;
    };
    match command {
        "play-pause" => {
            let resuming = !h.player.is_playing();
            if h.queue.read().current().is_none() {
                let paths: Vec<String> = h
                    .library
                    .read()
                    .videos
                    .iter()
                    .map(|t| t.path.to_string_lossy().into_owned())
                    .collect();
                if paths.is_empty() {
                    return;
                }
                {
                    let cfg = h.config.read();
                    let mut q = h.queue.write();
                    q.replace(paths, 0);
                    q.shuffle = cfg.shuffle;
                    q.repeat = cfg.repeat;
                    q.rebuild_order();
                }
                h.load_current();
            } else if !h.player.available() {
                h.load_current();
            } else {
                if resuming {
                    pause_rivals();
                    #[cfg(windows)]
                    {
                        crate::builtin::viewer::smtc_publisher::set_active_video(h.instance_id);
                        h.push_smtc();
                    }
                }
                h.player.play_pause();
                h.publish();
            }
        }
        "next" => {
            let next = h.queue.write().next().map(str::to_string);
            if let Some(path) = next {
                h.load_path(&path);
                h.sync_queue_to_config();
                h.publish();
            }
        }
        "prev" => {
            let prev = h.queue.write().previous().map(str::to_string);
            if let Some(path) = prev {
                h.load_path(&path);
                h.sync_queue_to_config();
                h.publish();
            }
        }
        "stop" => {
            h.player.stop();
            h.publish();
        }
        "seek-back-5" => {
            h.player.seek_rel(-5.0);
            h.publish();
        }
        "seek-fwd-5" => {
            h.player.seek_rel(5.0);
            h.publish();
        }
        "seek-back-10" => {
            h.player.seek_rel(-10.0);
            h.publish();
        }
        "seek-fwd-10" => {
            h.player.seek_rel(10.0);
            h.publish();
        }
        "vol-up" => {
            h.player.volume_delta(5.0);
            h.sync_queue_to_config();
            h.publish();
        }
        "vol-down" => {
            h.player.volume_delta(-5.0);
            h.sync_queue_to_config();
            h.publish();
        }
        "mute" => {
            h.player.mute_toggle();
            h.sync_queue_to_config();
            h.publish();
        }
        "shuffle" => {
            let on = {
                let mut q = h.queue.write();
                let next = !q.shuffle;
                q.set_shuffle(next);
                next
            };
            h.config.write().shuffle = on;
            h.sync_queue_to_config();
            h.publish();
        }
        "reshuffle" => {
            if !h.queue.write().reshuffle_remaining() {
                return;
            }
            h.config.write().shuffle = true;
            h.sync_queue_to_config();
            h.publish();
        }
        "repeat" => {
            h.queue.write().cycle_repeat();
            h.config.write().repeat = h.queue.read().repeat;
            h.sync_queue_to_config();
            h.publish();
        }
        "speed" => {
            h.player.cycle_speed();
            h.sync_queue_to_config();
            h.publish();
        }
        "add-folder" => {
            drop(h);
            add_library_root(instance_id);
        }
        "rescan" => {
            kick_scan(Arc::clone(h.value()));
        }
        "open-file" => {
            let Some(path) = orchid_viewers::pick_video_file() else {
                return;
            };
            let s = path.to_string_lossy().into_owned();
            {
                let cfg = h.config.read().clone();
                let mut q = h.queue.write();
                q.replace(vec![s], 0);
                q.shuffle = cfg.shuffle;
                q.repeat = cfg.repeat;
            }
            h.load_current();
        }
        "clear-queue" => {
            h.queue.write().replace(Vec::new(), 0);
            h.player.stop();
            h.sync_queue_to_config();
            h.publish();
        }
        "clear-search" => {
            h.config.write().search_query.clear();
            h.publish();
        }
        "clear-filter" => {
            h.config.write().browse_filter.clear();
            h.publish();
        }
        "play-search" => {
            play_search(&h);
        }
        "enqueue-search" => {
            enqueue_search(&h);
        }
        "queue-first" => {
            if h.queue.write().jump_first() {
                h.load_current();
            }
        }
        "queue-last" => {
            if h.queue.write().jump_last() {
                h.load_current();
            }
        }
        "scroll-to-current" => {
            h.scroll_gen.fetch_add(1, Ordering::Relaxed);
            h.publish();
        }
        "tab:0" => {
            h.config.write().browse_tab = BrowseTab::Library;
            h.publish();
        }
        "tab:1" => {
            h.config.write().browse_tab = BrowseTab::Queue;
            h.publish();
        }
        cmd if cmd.starts_with("search:") => {
            h.config.write().search_query = cmd["search:".len()..].to_string();
            h.publish();
        }
        cmd if cmd.starts_with("open-group:") => {
            h.config.write().browse_filter = cmd["open-group:".len()..].to_string();
            h.publish();
        }
        cmd if cmd.starts_with("play-group:") => {
            play_group(&h, &cmd["play-group:".len()..]);
        }
        cmd if cmd.starts_with("enqueue-group:") => {
            enqueue_group(&h, &cmd["enqueue-group:".len()..]);
        }
        cmd if cmd.starts_with("play:") => {
            let path = &cmd["play:".len()..];
            if path.is_empty() {
                return;
            }
            {
                let cfg = h.config.read().clone();
                let mut q = h.queue.write();
                if let Some(i) = q.paths.iter().position(|p| p == path) {
                    q.index = i;
                } else {
                    q.replace(vec![path.to_string()], 0);
                }
                q.shuffle = cfg.shuffle;
                q.repeat = cfg.repeat;
                if q.shuffle {
                    q.rebuild_order();
                }
            }
            h.load_current();
        }
        cmd if cmd.starts_with("queue-play:") => {
            let path = &cmd["queue-play:".len()..];
            if h.queue.write().play_at(path) {
                h.load_current();
            }
        }
        cmd if cmd.starts_with("enqueue:") => {
            let path = &cmd["enqueue:".len()..];
            if path.is_empty() {
                return;
            }
            if h.queue.write().enqueue_end(path) {
                h.sync_queue_to_config();
                h.publish();
            }
        }
        cmd if cmd.starts_with("play-next:") => {
            let path = &cmd["play-next:".len()..];
            if path.is_empty() {
                return;
            }
            if h.queue.write().enqueue_next(path) {
                h.sync_queue_to_config();
                h.publish();
            }
        }
        cmd if cmd.starts_with("queue-up:") => {
            let path = &cmd["queue-up:".len()..];
            if h.queue.write().move_up(path) {
                h.sync_queue_to_config();
                h.publish();
            }
        }
        cmd if cmd.starts_with("queue-down:") => {
            let path = &cmd["queue-down:".len()..];
            if h.queue.write().move_down(path) {
                h.sync_queue_to_config();
                h.publish();
            }
        }
        cmd if cmd.starts_with("queue-move-to:") => {
            let raw = &cmd["queue-move-to:".len()..];
            if let Some((idx_s, path)) = raw.split_once('|') {
                if let Ok(idx) = idx_s.parse::<usize>() {
                    if h.queue.write().move_to(path, idx) {
                        h.sync_queue_to_config();
                        h.publish();
                    }
                }
            }
        }
        cmd if cmd.starts_with("remove-queue:") => {
            let path = &cmd["remove-queue:".len()..];
            let changed = h.queue.write().remove_path(path);
            h.sync_queue_to_config();
            if changed {
                if h.queue.read().current().is_some() {
                    h.load_current();
                } else {
                    h.player.stop();
                    h.publish();
                }
            } else {
                h.publish();
            }
        }
        cmd if cmd.starts_with("remove-root:") => {
            let root = &cmd["remove-root:".len()..];
            drop(h);
            remove_library_root(instance_id, root);
        }
        cmd if cmd.starts_with("seek-frac:") => {
            if let Ok(frac) = cmd["seek-frac:".len()..].parse::<f64>() {
                h.player.seek_fraction(frac);
                h.publish();
            }
        }
        _ => {}
    }
}

fn play_group(h: &VideoHandle, group_key: &str) {
    let paths = h.library.read().paths_in_folder(group_key);
    if paths.is_empty() {
        return;
    }
    let cfg = h.config.read().clone();
    {
        let mut q = h.queue.write();
        q.replace(paths, 0);
        q.shuffle = cfg.shuffle;
        q.repeat = cfg.repeat;
        if q.shuffle {
            q.rebuild_order();
        }
    }
    {
        let mut c = h.config.write();
        c.browse_filter = group_key.to_string();
    }
    h.load_current();
}

fn enqueue_group(h: &VideoHandle, group_key: &str) {
    let paths = h.library.read().paths_in_folder(group_key);
    if paths.is_empty() {
        return;
    }
    let mut any = false;
    {
        let mut q = h.queue.write();
        for path in paths {
            if q.enqueue_end(&path) {
                any = true;
            }
        }
    }
    if any {
        h.sync_queue_to_config();
        h.publish();
    }
}

fn play_search(h: &VideoHandle) {
    let search = h.config.read().search_query.clone();
    let paths = h.library.read().paths_matching(&search);
    if paths.is_empty() {
        return;
    }
    let cfg = h.config.read().clone();
    {
        let mut q = h.queue.write();
        q.replace(paths, 0);
        q.shuffle = cfg.shuffle;
        q.repeat = cfg.repeat;
        if q.shuffle {
            q.rebuild_order();
        }
    }
    h.load_current();
}

fn enqueue_search(h: &VideoHandle) {
    let search = h.config.read().search_query.clone();
    let paths = h.library.read().paths_matching(&search);
    if paths.is_empty() {
        return;
    }
    let mut any = false;
    {
        let mut q = h.queue.write();
        for path in paths {
            if q.enqueue_end(&path) {
                any = true;
            }
        }
    }
    if any {
        h.sync_queue_to_config();
        h.publish();
    }
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

fn video_row_from_path(lib: &LibraryIndex, path: &str) -> VideoRow {
    lib.find_by_path(path)
        .map(library::video_row)
        .unwrap_or_else(|| {
            let stem = PathBuf::from(path)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string());
            VideoRow {
                path: path.to_string(),
                title: stem,
                subtitle: path.to_string(),
                duration_label: String::new(),
            }
        })
}

fn item_rows(rows: &[VideoRow], current: Option<&str>) -> Vec<VideoPlayerItemRow> {
    rows.iter()
        .map(|r| VideoPlayerItemRow {
            path: r.path.clone(),
            title: r.title.clone(),
            subtitle: r.subtitle.clone(),
            duration_label: r.duration_label.clone(),
            is_current: current == Some(r.path.as_str()),
        })
        .collect()
}

fn browse_filter_label(filter: &str) -> String {
    if filter.is_empty() {
        return String::new();
    }
    PathBuf::from(filter)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| filter.to_string())
}

/// Videos from the current row onward, plus remaining ms of the playing track.
fn queue_remaining(q: &PlayQueue, position_ms: u64, duration_ms: u64) -> (u32, u64) {
    let paths = q.display_paths();
    let Some(cur) = q.current() else {
        return (0, 0);
    };
    let Some(idx) = paths.iter().position(|p| p == cur) else {
        return (0, 0);
    };
    let count = (paths.len() - idx) as u32;
    let rem_current = duration_ms.saturating_sub(position_ms);
    // Library has no per-file durations yet; only count remaining of the current clip.
    (count, rem_current)
}

/// Video player widget.
pub struct VideoPlayerWidget {
    instance_id: Uuid,
    handle: Arc<VideoHandle>,
    refresh: PeriodicRefresh,
}

impl std::fmt::Debug for VideoPlayerWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoPlayerWidget")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl VideoPlayerWidget {
    /// Construct from persisted config.
    pub fn new(
        instance_id: Uuid,
        mut config: VideoPlayerConfig,
        bus: Arc<orchid_core::EventBus>,
    ) -> Self {
        config.normalize();
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
        if config.muted != player.muted() {
            player.mute_toggle();
        }
        player.apply_session_prefs(f64::from(config.speed_x100) / 100.0);
        let handle = Arc::new(VideoHandle {
            instance_id,
            config: Arc::new(RwLock::new(config)),
            library: Arc::new(RwLock::new(LibraryIndex::default())),
            queue: Arc::new(RwLock::new(queue)),
            player,
            bus,
            scanning: AtomicBool::new(false),
            scan_gen: AtomicU64::new(0),
            scroll_gen: AtomicU64::new(0),
            last_scroll: parking_lot::Mutex::new((0, None)),
            last_pos_sec: AtomicU64::new(u64::MAX),
        });
        VIDEO_LIVE.insert(instance_id, Arc::clone(&handle));
        handle.restore_current_paused();
        kick_scan(Arc::clone(&handle));
        Self {
            instance_id,
            handle,
            refresh: PeriodicRefresh::new(Duration::from_millis(100)),
        }
    }

    fn build_payload(&self) -> VideoPlayerPayload {
        let cfg = self.handle.config.read().clone();
        let lib = self.handle.library.read();
        let q = self.handle.queue.read();
        let current = q.current().map(str::to_string);
        let display = q.display_paths();

        let (browse_groups, browse_items) = if cfg.browse_tab == BrowseTab::Queue {
            let search = cfg.search_query.trim().to_lowercase();
            let items = display
                .iter()
                .map(|p| video_row_from_path(&lib, p))
                .filter(|row| {
                    if search.is_empty() {
                        return true;
                    }
                    row.title.to_lowercase().contains(&search)
                        || row.path.to_lowercase().contains(&search)
                })
                .collect::<Vec<_>>();
            (Vec::new(), items)
        } else {
            let browse = lib.browse_rows(&cfg.search_query, &cfg.browse_filter, &cfg.library_roots);
            (browse.groups, browse.items)
        };

        let queue_rows: Vec<VideoRow> = display
            .iter()
            .map(|p| video_row_from_path(&lib, p))
            .collect();

        let pos = self.handle.player.position_ms();
        let dur = self.handle.player.duration_ms();
        let progress = if dur > 0 {
            (pos as f32 / dur as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let frame = self.handle.player.frame();
        let (has_video, frame_rgba, frame_width, frame_height) = match frame {
            Some(f) if self.handle.player.has_video() => (true, f.rgba, f.width, f.height),
            Some(f) => (false, f.rgba, f.width, f.height),
            None => (false, Arc::new(Vec::new()), 0, 0),
        };

        let roots: Vec<VideoPlayerRootRow> = cfg
            .library_roots
            .iter()
            .map(|path| {
                let label = PathBuf::from(path)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| path.clone());
                VideoPlayerRootRow {
                    path: path.clone(),
                    label,
                }
            })
            .collect();
        let has_library_roots = !roots.is_empty();

        let empty_hint = if cfg.library_roots.is_empty() && cfg.browse_tab == BrowseTab::Library {
            "video-player-empty-roots".into()
        } else if self.handle.scanning.load(Ordering::Acquire) {
            "video-player-scanning".into()
        } else if cfg.browse_tab == BrowseTab::Queue && q.paths.is_empty() {
            "video-player-empty-queue".into()
        } else if lib.videos.is_empty() && cfg.browse_tab == BrowseTab::Library {
            "video-player-empty-library".into()
        } else if (!cfg.search_query.trim().is_empty() || !cfg.browse_filter.is_empty())
            && browse_groups.is_empty()
            && browse_items.is_empty()
        {
            if cfg.search_query.trim().is_empty() {
                "video-player-empty-folder".into()
            } else {
                "video-player-empty-search".into()
            }
        } else {
            String::new()
        };

        let title = if self.handle.player.title().is_empty() {
            current
                .as_deref()
                .and_then(|p| lib.find_by_path(p).map(|t| t.title.clone()))
                .or_else(|| {
                    current.as_ref().map(|p| {
                        PathBuf::from(p)
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| p.clone())
                    })
                })
                .unwrap_or_default()
        } else {
            self.handle.player.title()
        };

        let current_track_index = current
            .as_deref()
            .and_then(|cur| display.iter().position(|p| p == cur).map(|i| i as i32))
            .unwrap_or(-1);
        let scroll_gen = self
            .handle
            .note_queue_scroll(cfg.browse_tab, current.as_deref());
        let (queue_remaining_count, queue_remaining_ms) = queue_remaining(&q, pos, dur);
        let library_folders_count = {
            let mut folders = std::collections::BTreeSet::new();
            for v in &lib.videos {
                folders.insert(v.folder.as_str());
            }
            folders.len() as u32
        };

        VideoPlayerPayload {
            engine_available: self.handle.player.available(),
            browse_tab: cfg.browse_tab.as_u8(),
            browse_filter: cfg.browse_filter.clone(),
            browse_filter_label: browse_filter_label(&cfg.browse_filter),
            search_query: cfg.search_query,
            roots,
            groups: browse_groups
                .into_iter()
                .map(|g| VideoPlayerGroupRow {
                    key: g.key,
                    label: g.label,
                    count: g.count,
                    is_library_root: g.is_library_root,
                })
                .collect(),
            items: item_rows(&browse_items, current.as_deref()),
            queue: item_rows(&queue_rows, current.as_deref()),
            queue_index: q.index as i32,
            queue_count: q.paths.len() as u32,
            current_track_index,
            scroll_gen,
            queue_remaining_count,
            queue_remaining_ms,
            queue_duration_ms: 0,
            has_track: current.is_some(),
            title,
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
            speed_label: self.handle.player.speed_label(),
            library_count: lib.videos.len() as u32,
            library_folders_count,
            has_library_roots,
            empty_hint,
            has_video,
            frame_rgba,
            frame_width,
            frame_height,
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
                        handle.load_path(&path);
                        handle.sync_queue_to_config();
                        dirty = true;
                    } else {
                        handle.player.pause();
                        dirty = true;
                    }
                }
                let pos_sec = handle.player.position_ms() / 1000;
                let last_sec = handle.last_pos_sec.swap(pos_sec, Ordering::Relaxed);
                if dirty || pos_sec != last_sec {
                    #[cfg(windows)]
                    if dirty {
                        handle.push_smtc();
                    }
                    handle.publish();
                }
            }
        });
    }
}

#[async_trait]
impl Widget for VideoPlayerWidget {
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
        #[cfg(windows)]
        crate::builtin::viewer::smtc_publisher::clear_active(self.instance_id);
        VIDEO_LIVE.remove(&self.instance_id);
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
            payload: WidgetPayload::VideoPlayer(payload),
        })
    }

    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        self.handle.sync_queue_to_config();
        let cfg = self.handle.config.read().clone();
        state_codec::save_state(&cfg)
    }

    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let mut cfg: VideoPlayerConfig = state_codec::restore_state(bytes).unwrap_or_default();
        cfg.normalize();
        let queue = PlayQueue::from_paths(
            cfg.queue.clone(),
            cfg.queue_index as usize,
            cfg.shuffle,
            cfg.repeat,
        );
        *self.handle.library.write() = LibraryIndex::default();
        *self.handle.queue.write() = queue;
        self.handle
            .player
            .apply_session_prefs(f64::from(cfg.speed_x100) / 100.0);
        *self.handle.config.write() = cfg;
        self.handle.restore_current_paused();
        kick_scan(Arc::clone(&self.handle));
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

/// Descriptor for the video library player.
#[must_use]
pub fn descriptor() -> WidgetDescriptor {
    let factory: WidgetFactory = Arc::new(|ctx: WidgetContext, bytes| {
        let config = bytes
            .and_then(|b| state_codec::restore_state::<VideoPlayerConfig>(b).ok())
            .unwrap_or_default();
        Ok(Box::new(VideoPlayerWidget::new(
            ctx.instance_id,
            config,
            ctx.bus.clone(),
        )) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-video-player-name",
        description_key: "widget-video-player-desc",
        icon_name: "video-player",
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
        assert_eq!(d.type_id, "video-player");
        assert!(!d.allows_multiple_instances);
    }

    #[tokio::test]
    async fn snapshot_empty_library() {
        let bus = Arc::new(orchid_core::EventBus::new(
            orchid_core::EventBusConfig::default(),
        ));
        let id = Uuid::new_v4();
        let w = VideoPlayerWidget::new(id, VideoPlayerConfig::default(), bus);
        let snap = w.snapshot().expect("snapshot");
        match snap.payload {
            WidgetPayload::VideoPlayer(p) => {
                assert_eq!(p.library_count, 0);
                assert!(!p.has_track);
            }
            _ => panic!("expected VideoPlayer"),
        }
        VIDEO_LIVE.remove(&id);
    }
}
