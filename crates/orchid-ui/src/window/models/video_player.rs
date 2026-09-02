//! Slint model for the local video library player.

use orchid_i18n::LocaleManager;
use orchid_widgets::VideoPlayerPayload;
use slint::{Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};

use crate::slint_generated::{VideoPlayerItem, VideoPlayerModel, VideoPlayerRootItem};

fn slint_image_from_rgba(rgba: &std::sync::Arc<Vec<u8>>, width: u32, height: u32) -> Image {
    if width == 0 || height == 0 || rgba.is_empty() {
        return Image::default();
    }
    let expected = (width as usize).saturating_mul(height as usize).saturating_mul(4);
    if rgba.len() < expected {
        return Image::default();
    }
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
    buf.make_mut_bytes()[..expected].copy_from_slice(&rgba[..expected]);
    Image::from_rgba8(buf)
}

pub(crate) fn empty_video_player_model(locale: &LocaleManager) -> VideoPlayerModel {
    fill_labels(
        VideoPlayerModel {
            engine_available: false,
            browse_tab: 0,
            search_query: SharedString::new(),
            roots: ModelRc::new(VecModel::from(Vec::<VideoPlayerRootItem>::new())),
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
            ..labels_only(locale)
        },
        locale,
    )
}

fn labels_only(_locale: &LocaleManager) -> VideoPlayerModel {
    VideoPlayerModel {
        engine_available: false,
        browse_tab: 0,
        search_query: SharedString::new(),
        roots: ModelRc::new(VecModel::from(Vec::<VideoPlayerRootItem>::new())),
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
        tab_library: SharedString::new(),
        tab_queue: SharedString::new(),
        add_folder_label: SharedString::new(),
        open_file_label: SharedString::new(),
        rescan_label: SharedString::new(),
        no_track_label: SharedString::new(),
        search_placeholder: SharedString::new(),
        enqueue_label: SharedString::new(),
        remove_label: SharedString::new(),
        clear_queue_label: SharedString::new(),
        remove_root_label: SharedString::new(),
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
    m.remove_label = locale.tr("video-player-remove").into();
    m.clear_queue_label = locale.tr("video-player-clear-queue").into();
    m.remove_root_label = locale.tr("video-player-remove-root").into();
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

pub(crate) fn build_video_player_model(
    p: &VideoPlayerPayload,
    locale: &LocaleManager,
) -> VideoPlayerModel {
    let roots: Vec<VideoPlayerRootItem> = p
        .roots
        .iter()
        .map(|r| VideoPlayerRootItem {
            path: r.path.clone().into(),
            label: r.label.clone().into(),
        })
        .collect();
    let items: Vec<VideoPlayerItem> = p
        .items
        .iter()
        .map(|t| VideoPlayerItem {
            path: t.path.clone().into(),
            title: t.title.clone().into(),
            subtitle: t.subtitle.clone().into(),
            duration_label: t.duration_label.clone().into(),
            is_current: t.is_current,
        })
        .collect();
    fill_labels(
        VideoPlayerModel {
            engine_available: p.engine_available,
            browse_tab: i32::from(p.browse_tab),
            search_query: p.search_query.clone().into(),
            roots: ModelRc::new(VecModel::from(roots)),
            items: ModelRc::new(VecModel::from(items)),
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
            ..labels_only(locale)
        },
        locale,
    )
}
