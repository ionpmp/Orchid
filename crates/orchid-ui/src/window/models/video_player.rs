//! Slint model for the local video library player.

use std::cell::RefCell;
use std::sync::Arc;

use orchid_i18n::{FluentArgs, LocaleManager};
use orchid_widgets::VideoPlayerPayload;
use slint::{Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};

use super::sync_eq_rows;

use crate::slint_generated::{
    VideoPlayerGroupItem, VideoPlayerItem, VideoPlayerModel, VideoPlayerRootItem,
};

thread_local! {
    /// Reuse the last uploaded frame when the payload still points at the
    /// same `Arc` (progress ticks and workspace rebuilds).
    static FRAME_CACHE: RefCell<Option<(usize, u32, u32, Image)>> = const { RefCell::new(None) };
}

fn slint_image_from_rgba(rgba: &Arc<Vec<u8>>, width: u32, height: u32) -> Image {
    if width == 0 || height == 0 || rgba.is_empty() {
        FRAME_CACHE.with(|c| *c.borrow_mut() = None);
        return Image::default();
    }
    let expected = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    if rgba.len() < expected {
        FRAME_CACHE.with(|c| *c.borrow_mut() = None);
        return Image::default();
    }
    let ptr = Arc::as_ptr(rgba) as *const u8 as usize;
    if let Some(img) = FRAME_CACHE.with(|c| {
        c.borrow().as_ref().and_then(|(p, w, h, img)| {
            (*p == ptr && *w == width && *h == height).then(|| img.clone())
        })
    }) {
        return img;
    }
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
    buf.make_mut_bytes()[..expected].copy_from_slice(&rgba[..expected]);
    let img = Image::from_rgba8(buf);
    FRAME_CACHE.with(|c| *c.borrow_mut() = Some((ptr, width, height, img.clone())));
    img
}

fn format_queue_duration(ms: u64) -> String {
    let total_secs = ms / 1000;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if hours > 0 {
        format!("{hours}:{mins:02}:{secs:02}")
    } else {
        format!("{mins}:{secs:02}")
    }
}

fn queue_stats_label(
    count: u32,
    duration_ms: u64,
    remaining_count: u32,
    remaining_ms: u64,
    locale: &LocaleManager,
) -> SharedString {
    if remaining_count > 0 {
        if remaining_ms == 0 {
            return locale
                .tr_args(
                    "video-player-queue-remaining-tracks",
                    &FluentArgs::new().with("tracks", remaining_count.to_string()),
                )
                .into();
        }
        return locale
            .tr_args(
                "video-player-queue-remaining",
                &FluentArgs::new()
                    .with("tracks", remaining_count.to_string())
                    .with("duration", format_queue_duration(remaining_ms)),
            )
            .into();
    }
    if count == 0 {
        return SharedString::new();
    }
    if duration_ms == 0 {
        locale
            .tr_args(
                "video-player-queue-stats-tracks",
                &FluentArgs::new().with("tracks", count.to_string()),
            )
            .into()
    } else {
        locale
            .tr_args(
                "video-player-queue-stats",
                &FluentArgs::new()
                    .with("tracks", count.to_string())
                    .with("duration", format_queue_duration(duration_ms)),
            )
            .into()
    }
}

pub(crate) fn empty_video_player_model(locale: &LocaleManager) -> VideoPlayerModel {
    fill_labels(
        VideoPlayerModel {
            engine_available: false,
            browse_tab: 0,
            browse_filter: SharedString::new(),
            browse_filter_label: SharedString::new(),
            search_query: SharedString::new(),
            roots: ModelRc::new(VecModel::from(Vec::<VideoPlayerRootItem>::new())),
            groups: ModelRc::new(VecModel::from(Vec::<VideoPlayerGroupItem>::new())),
            items: ModelRc::new(VecModel::from(Vec::<VideoPlayerItem>::new())),
            has_track: false,
            title: SharedString::new(),
            is_playing: false,
            progress: 0.0,
            position_label: SharedString::new(),
            duration_label: SharedString::new(),
            volume: 100,
            muted: false,
            shuffle: false,
            repeat: 0,
            speed_label: SharedString::new(),
            empty_hint: SharedString::new(),
            has_library_roots: false,
            has_video: false,
            frame: Image::default(),
            queue_count: 0,
            library_count: 0,
            queue_stats_label: SharedString::new(),
            current_track_index: -1,
            scroll_gen: 0,
            ..labels_only(locale)
        },
        locale,
    )
}

fn labels_only(_locale: &LocaleManager) -> VideoPlayerModel {
    VideoPlayerModel {
        engine_available: false,
        browse_tab: 0,
        browse_filter: SharedString::new(),
        browse_filter_label: SharedString::new(),
        search_query: SharedString::new(),
        roots: ModelRc::new(VecModel::from(Vec::<VideoPlayerRootItem>::new())),
        groups: ModelRc::new(VecModel::from(Vec::<VideoPlayerGroupItem>::new())),
        items: ModelRc::new(VecModel::from(Vec::<VideoPlayerItem>::new())),
        has_track: false,
        title: SharedString::new(),
        is_playing: false,
        progress: 0.0,
        position_label: SharedString::new(),
        duration_label: SharedString::new(),
        volume: 100,
        muted: false,
        shuffle: false,
        repeat: 0,
        speed_label: SharedString::new(),
        empty_hint: SharedString::new(),
        has_library_roots: false,
        has_video: false,
        frame: Image::default(),
        queue_count: 0,
        library_count: 0,
        queue_stats_label: SharedString::new(),
        current_track_index: -1,
        scroll_gen: 0,
        tab_library: SharedString::new(),
        tab_queue: SharedString::new(),
        add_folder_label: SharedString::new(),
        open_file_label: SharedString::new(),
        rescan_label: SharedString::new(),
        no_track_label: SharedString::new(),
        search_placeholder: SharedString::new(),
        enqueue_label: SharedString::new(),
        play_next_label: SharedString::new(),
        remove_label: SharedString::new(),
        clear_queue_label: SharedString::new(),
        remove_root_label: SharedString::new(),
        back_label: SharedString::new(),
        play_group_label: SharedString::new(),
        jump_to_current_label: SharedString::new(),
        reshuffle_label: SharedString::new(),
        move_up_label: SharedString::new(),
        move_down_label: SharedString::new(),
        engine_missing_label: SharedString::new(),
    }
}

fn fill_labels(mut m: VideoPlayerModel, locale: &LocaleManager) -> VideoPlayerModel {
    m.tab_library = locale.tr("video-player-tab-library").into();
    m.tab_queue = locale.tr("video-player-tab-queue").into();
    m.add_folder_label = locale.tr("video-player-add-folder").into();
    m.open_file_label = locale.tr("video-player-open-file").into();
    m.rescan_label = locale.tr("video-player-rescan").into();
    m.no_track_label = locale.tr("video-player-no-track").into();
    m.search_placeholder = locale.tr("video-player-search-placeholder").into();
    m.enqueue_label = locale.tr("video-player-enqueue").into();
    m.play_next_label = locale.tr("video-player-play-next").into();
    m.remove_label = locale.tr("video-player-remove").into();
    m.clear_queue_label = locale.tr("video-player-clear-queue").into();
    m.remove_root_label = locale.tr("video-player-remove-root").into();
    m.back_label = locale.tr("video-player-back").into();
    m.play_group_label = locale.tr("video-player-play-group").into();
    m.jump_to_current_label = locale.tr("video-player-jump-to-current").into();
    m.reshuffle_label = locale.tr("video-player-reshuffle").into();
    m.move_up_label = locale.tr("video-player-move-up").into();
    m.move_down_label = locale.tr("video-player-move-down").into();
    m.engine_missing_label = locale.tr("video-player-engine-missing").into();
    m
}

fn resolve_hint(key: &str, locale: &LocaleManager) -> SharedString {
    if key.is_empty() {
        SharedString::new()
    } else {
        locale.tr(key).into()
    }
}

fn map_roots(p: &VideoPlayerPayload) -> Vec<VideoPlayerRootItem> {
    p.roots
        .iter()
        .map(|r| VideoPlayerRootItem {
            path: r.path.clone().into(),
            label: r.label.clone().into(),
        })
        .collect()
}

fn map_groups(p: &VideoPlayerPayload) -> Vec<VideoPlayerGroupItem> {
    p.groups
        .iter()
        .map(|g| VideoPlayerGroupItem {
            key: g.key.clone().into(),
            label: g.label.clone().into(),
            count: g.count as i32,
            is_library_root: g.is_library_root,
        })
        .collect()
}

fn map_items(p: &VideoPlayerPayload) -> Vec<VideoPlayerItem> {
    p.items
        .iter()
        .map(|t| VideoPlayerItem {
            path: t.path.clone().into(),
            title: t.title.clone().into(),
            subtitle: t.subtitle.clone().into(),
            duration_label: t.duration_label.clone().into(),
            is_current: t.is_current,
        })
        .collect()
}

pub(crate) fn build_video_player_model(
    p: &VideoPlayerPayload,
    locale: &LocaleManager,
) -> VideoPlayerModel {
    fill_labels(
        VideoPlayerModel {
            engine_available: p.engine_available,
            browse_tab: i32::from(p.browse_tab),
            browse_filter: p.browse_filter.clone().into(),
            browse_filter_label: p.browse_filter_label.clone().into(),
            search_query: p.search_query.clone().into(),
            roots: ModelRc::new(VecModel::from(map_roots(p))),
            groups: ModelRc::new(VecModel::from(map_groups(p))),
            items: ModelRc::new(VecModel::from(map_items(p))),
            has_track: p.has_track,
            title: p.title.clone().into(),
            is_playing: p.is_playing,
            progress: p.progress,
            position_label: p.position_label.clone().into(),
            duration_label: p.duration_label.clone().into(),
            volume: p.volume as i32,
            muted: p.muted,
            shuffle: p.shuffle,
            repeat: i32::from(p.repeat),
            speed_label: p.speed_label.clone().into(),
            empty_hint: resolve_hint(&p.empty_hint, locale),
            has_library_roots: p.has_library_roots,
            has_video: p.has_video,
            frame: slint_image_from_rgba(&p.frame_rgba, p.frame_width, p.frame_height),
            queue_count: p.queue_count as i32,
            library_count: p.library_count as i32,
            queue_stats_label: queue_stats_label(
                p.queue_count,
                p.queue_duration_ms,
                p.queue_remaining_count,
                p.queue_remaining_ms,
                locale,
            ),
            current_track_index: p.current_track_index,
            scroll_gen: p.scroll_gen.min(i32::MAX as u64) as i32,
            ..labels_only(locale)
        },
        locale,
    )
}

/// Update an existing [`VideoPlayerModel`] in place, keeping nested `ModelRc`s.
pub(crate) fn patch_video_player_model(
    model: &mut VideoPlayerModel,
    p: &VideoPlayerPayload,
    locale: &LocaleManager,
) {
    sync_eq_rows(&model.roots, map_roots(p));
    sync_eq_rows(&model.groups, map_groups(p));
    sync_eq_rows(&model.items, map_items(p));
    model.engine_available = p.engine_available;
    model.browse_tab = i32::from(p.browse_tab);
    model.browse_filter = p.browse_filter.clone().into();
    model.browse_filter_label = p.browse_filter_label.clone().into();
    model.search_query = p.search_query.clone().into();
    model.has_track = p.has_track;
    model.title = p.title.clone().into();
    model.is_playing = p.is_playing;
    model.progress = p.progress;
    model.position_label = p.position_label.clone().into();
    model.duration_label = p.duration_label.clone().into();
    model.volume = p.volume as i32;
    model.muted = p.muted;
    model.shuffle = p.shuffle;
    model.repeat = i32::from(p.repeat);
    model.speed_label = p.speed_label.clone().into();
    model.empty_hint = resolve_hint(&p.empty_hint, locale);
    model.has_library_roots = p.has_library_roots;
    model.has_video = p.has_video;
    model.frame = slint_image_from_rgba(&p.frame_rgba, p.frame_width, p.frame_height);
    model.queue_count = p.queue_count as i32;
    model.library_count = p.library_count as i32;
    model.queue_stats_label = queue_stats_label(
        p.queue_count,
        p.queue_duration_ms,
        p.queue_remaining_count,
        p.queue_remaining_ms,
        locale,
    );
    model.current_track_index = p.current_track_index;
    model.scroll_gen = p.scroll_gen.min(i32::MAX as u64) as i32;
    let _ = locale;
}
