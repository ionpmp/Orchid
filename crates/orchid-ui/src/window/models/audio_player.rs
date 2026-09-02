//! Slint model for the local audio library player.

use orchid_i18n::{FluentArgs, LocaleManager};
use orchid_widgets::AudioPlayerPayload;
use slint::{Image, ModelRc, SharedString, VecModel};

use crate::slint_generated::{
    AudioPlayerGroupItem, AudioPlayerModel, AudioPlayerPlaylistItem, AudioPlayerRootItem,
    AudioPlayerTrackItem,
};

pub(crate) fn empty_audio_player_model(locale: &LocaleManager) -> AudioPlayerModel {
    fill_labels(
        AudioPlayerModel {
            engine_available: false,
            browse_tab: 0,
            browse_filter: SharedString::new(),
            browse_filter_label: SharedString::new(),
            search_query: SharedString::new(),
            renaming_playlist: false,
            rename_playlist_draft: SharedString::new(),
            active_playlist_id: SharedString::new(),
            groups: ModelRc::new(VecModel::from(Vec::<AudioPlayerGroupItem>::new())),
            tracks: ModelRc::new(VecModel::from(Vec::<AudioPlayerTrackItem>::new())),
            playlists: ModelRc::new(VecModel::from(Vec::<AudioPlayerPlaylistItem>::new())),
            roots: ModelRc::new(VecModel::from(Vec::<AudioPlayerRootItem>::new())),
            has_track: false,
            title: SharedString::new(),
            artist: SharedString::new(),
            album: SharedString::new(),
            is_playing: false,
            progress: 0.0,
            position_label: SharedString::new(),
            duration_label: SharedString::new(),
            volume: 100,
            muted: false,
            shuffle: false,
            repeat: 0,
            sleep_label: SharedString::new(),
            eq_label: SharedString::new(),
            rg_label: SharedString::new(),
            speed_label: SharedString::new(),
            crossfade_label: SharedString::new(),
            lyrics_line: SharedString::new(),
            has_lyrics: false,
            roots_label: SharedString::new(),
            empty_hint: SharedString::new(),
            has_cover: false,
            cover: Image::default(),
            queue_stats_label: SharedString::new(),
            current_track_index: -1,
            has_library_roots: false,
            ..labels_only(locale)
        },
        locale,
    )
}

fn labels_only(locale: &LocaleManager) -> AudioPlayerModel {
    AudioPlayerModel {
        engine_available: false,
        browse_tab: 0,
        browse_filter: SharedString::new(),
        browse_filter_label: SharedString::new(),
        search_query: SharedString::new(),
        renaming_playlist: false,
        rename_playlist_draft: SharedString::new(),
        active_playlist_id: SharedString::new(),
        groups: ModelRc::new(VecModel::from(Vec::<AudioPlayerGroupItem>::new())),
        tracks: ModelRc::new(VecModel::from(Vec::<AudioPlayerTrackItem>::new())),
        playlists: ModelRc::new(VecModel::from(Vec::<AudioPlayerPlaylistItem>::new())),
        roots: ModelRc::new(VecModel::from(Vec::<AudioPlayerRootItem>::new())),
        has_track: false,
        title: SharedString::new(),
        artist: SharedString::new(),
        album: SharedString::new(),
        is_playing: false,
        progress: 0.0,
        position_label: SharedString::new(),
        duration_label: SharedString::new(),
        volume: 100,
        muted: false,
        shuffle: false,
        repeat: 0,
        sleep_label: SharedString::new(),
        eq_label: SharedString::new(),
        rg_label: SharedString::new(),
        speed_label: SharedString::new(),
        crossfade_label: SharedString::new(),
        lyrics_line: SharedString::new(),
        has_lyrics: false,
        roots_label: SharedString::new(),
        empty_hint: SharedString::new(),
        has_cover: false,
        cover: Image::default(),
        queue_stats_label: SharedString::new(),
        current_track_index: -1,
        has_library_roots: false,
        tab_songs: locale.tr("audio-player-tab-songs").into(),
        tab_artists: locale.tr("audio-player-tab-artists").into(),
        tab_albums: locale.tr("audio-player-tab-albums").into(),
        tab_folders: locale.tr("audio-player-tab-folders").into(),
        tab_genres: locale.tr("audio-player-tab-genres").into(),
        tab_playlists: locale.tr("audio-player-tab-playlists").into(),
        tab_now_playing: locale.tr("audio-player-tab-now-playing").into(),
        back_label: locale.tr("audio-player-back").into(),
        add_folder_label: locale.tr("audio-player-add-folder").into(),
        rescan_label: locale.tr("audio-player-rescan").into(),
        new_playlist_label: locale.tr("audio-player-new-playlist").into(),
        no_track_label: locale.tr("audio-player-no-track").into(),
        sleep_off_label: locale.tr("audio-player-sleep-off").into(),
        eq_off_label: locale.tr("audio-player-eq-off").into(),
        rg_off_label: locale.tr("audio-player-rg-off").into(),
        speed_off_label: locale.tr("audio-player-speed-off").into(),
        crossfade_off_label: locale.tr("audio-player-crossfade-off").into(),
        search_placeholder: locale.tr("audio-player-search-placeholder").into(),
        enqueue_label: locale.tr("audio-player-enqueue").into(),
        play_next_label: locale.tr("audio-player-play-next").into(),
        remove_label: locale.tr("audio-player-remove").into(),
        clear_queue_label: locale.tr("audio-player-clear-queue").into(),
        save_queue_as_playlist_label: locale.tr("audio-player-save-queue-as-playlist").into(),
        jump_to_current_label: locale.tr("audio-player-jump-to-current").into(),
        reshuffle_label: locale.tr("audio-player-reshuffle").into(),
        delete_playlist_label: locale.tr("audio-player-delete-playlist").into(),
        rename_playlist_label: locale.tr("audio-player-rename-playlist").into(),
        add_to_playlist_label: locale.tr("audio-player-add-to-playlist").into(),
        remove_from_playlist_label: locale.tr("audio-player-remove-from-playlist").into(),
        move_up_label: locale.tr("audio-player-move-up").into(),
        move_down_label: locale.tr("audio-player-move-down").into(),
        play_group_label: locale.tr("audio-player-play-group").into(),
        remove_root_label: locale.tr("audio-player-remove-root").into(),
        sort_label: sort_label_for(0, locale),
        show_in_fm_label: locale.tr("audio-player-show-in-fm").into(),
        export_m3u_label: locale.tr("audio-player-export-m3u").into(),
        import_m3u_label: locale.tr("audio-player-import-m3u").into(),
        is_current_favorite: false,
        engine_missing_label: locale.tr("audio-player-engine-missing").into(),
    }
}

fn sort_label_for(sort: u8, locale: &LocaleManager) -> SharedString {
    let key = match sort {
        1 => "audio-player-sort-title",
        2 => "audio-player-sort-album",
        3 => "audio-player-sort-year",
        4 => "audio-player-sort-genre",
        _ => "audio-player-sort-artist",
    };
    locale.tr(key).into()
}

fn playlist_name(name: &str, locale: &LocaleManager) -> SharedString {
    if name.starts_with("audio-player-") {
        locale.tr(name).into()
    } else {
        name.into()
    }
}

fn library_stats_label(tracks: u32, folders: u32, locale: &LocaleManager) -> SharedString {
    match (tracks, folders) {
        (0, 0) => SharedString::new(),
        (t, 0) => locale
            .tr_args(
                "audio-player-library-stats-tracks",
                &FluentArgs::new().with("tracks", t.to_string()),
            )
            .into(),
        (0, f) => locale
            .tr_args(
                "audio-player-library-stats-folders",
                &FluentArgs::new().with("folders", f.to_string()),
            )
            .into(),
        (t, f) => locale
            .tr_args(
                "audio-player-library-stats-tracks-folders",
                &FluentArgs::new()
                    .with("tracks", t.to_string())
                    .with("folders", f.to_string()),
            )
            .into(),
    }
}

fn format_queue_duration(ms: u64) -> String {
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

fn crossfade_label(secs: u8, locale: &LocaleManager) -> SharedString {
    if secs == 0 {
        SharedString::new()
    } else {
        locale
            .tr_args(
                "audio-player-crossfade",
                &FluentArgs::new().with("secs", secs.to_string()),
            )
            .into()
    }
}

fn queue_stats_label(count: u32, duration_ms: u64, locale: &LocaleManager) -> SharedString {
    if count == 0 {
        return SharedString::new();
    }
    if duration_ms == 0 {
        locale
            .tr_args(
                "audio-player-queue-stats-tracks",
                &FluentArgs::new().with("tracks", count.to_string()),
            )
            .into()
    } else {
        locale
            .tr_args(
                "audio-player-queue-stats",
                &FluentArgs::new()
                    .with("tracks", count.to_string())
                    .with("duration", format_queue_duration(duration_ms)),
            )
            .into()
    }
}

fn fill_labels(mut m: AudioPlayerModel, locale: &LocaleManager) -> AudioPlayerModel {
    let base = labels_only(locale);
    m.tab_songs = base.tab_songs;
    m.tab_artists = base.tab_artists;
    m.tab_albums = base.tab_albums;
    m.tab_folders = base.tab_folders;
    m.tab_genres = base.tab_genres;
    m.tab_playlists = base.tab_playlists;
    m.tab_now_playing = base.tab_now_playing;
    m.back_label = base.back_label;
    m.add_folder_label = base.add_folder_label;
    m.rescan_label = base.rescan_label;
    m.new_playlist_label = base.new_playlist_label;
    m.no_track_label = base.no_track_label;
    m.sleep_off_label = base.sleep_off_label;
    m.eq_off_label = base.eq_off_label;
    m.rg_off_label = base.rg_off_label;
    m.speed_off_label = base.speed_off_label;
    m.crossfade_off_label = base.crossfade_off_label;
    m.search_placeholder = base.search_placeholder;
    m.enqueue_label = base.enqueue_label;
    m.play_next_label = base.play_next_label;
    m.remove_label = base.remove_label;
    m.clear_queue_label = base.clear_queue_label;
    m.save_queue_as_playlist_label = base.save_queue_as_playlist_label;
    m.jump_to_current_label = base.jump_to_current_label;
    m.reshuffle_label = base.reshuffle_label;
    m.delete_playlist_label = base.delete_playlist_label;
    m.rename_playlist_label = base.rename_playlist_label;
    m.add_to_playlist_label = base.add_to_playlist_label;
    m.remove_from_playlist_label = base.remove_from_playlist_label;
    m.move_up_label = base.move_up_label;
    m.move_down_label = base.move_down_label;
    m.play_group_label = base.play_group_label;
    m.remove_root_label = base.remove_root_label;
    m.show_in_fm_label = base.show_in_fm_label;
    m.export_m3u_label = base.export_m3u_label;
    m.import_m3u_label = base.import_m3u_label;
    m.engine_missing_label = base.engine_missing_label;
    m
}

pub(crate) fn build_audio_player_model(
    p: &AudioPlayerPayload,
    locale: &LocaleManager,
) -> AudioPlayerModel {
    let cover = if p.has_cover && p.cover_width > 0 && p.cover_height > 0 && !p.cover_rgba.is_empty()
    {
        let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
            p.cover_rgba.as_ref(),
            p.cover_width,
            p.cover_height,
        );
        Image::from_rgba8(buf)
    } else {
        Image::default()
    };

    let empty_hint = if p.empty_hint.is_empty() {
        SharedString::new()
    } else {
        locale.tr(&p.empty_hint).into()
    };

    fill_labels(
        AudioPlayerModel {
            engine_available: p.engine_available,
            browse_tab: i32::from(p.browse_tab),
            browse_filter: p.browse_filter.clone().into(),
            browse_filter_label: p.browse_filter_label.clone().into(),
            search_query: p.search_query.clone().into(),
            renaming_playlist: p.renaming_playlist,
            rename_playlist_draft: p.rename_playlist_draft.clone().into(),
            active_playlist_id: p.active_playlist_id.clone().into(),
            groups: ModelRc::new(VecModel::from(
                p.groups
                    .iter()
                    .map(|g| AudioPlayerGroupItem {
                        key: g.key.clone().into(),
                        label: g.label.clone().into(),
                        count: g.count as i32,
                        is_library_root: g.is_library_root,
                    })
                    .collect::<Vec<_>>(),
            )),
            tracks: ModelRc::new(VecModel::from(
                p.tracks
                    .iter()
                    .map(|t| {
                        let cover = if t.has_cover
                            && t.cover_width > 0
                            && t.cover_height > 0
                            && !t.cover_rgba.is_empty()
                        {
                            let buf = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                                t.cover_rgba.as_ref(),
                                t.cover_width,
                                t.cover_height,
                            );
                            Image::from_rgba8(buf)
                        } else {
                            Image::default()
                        };
                        AudioPlayerTrackItem {
                            path: t.path.clone().into(),
                            title: t.title.clone().into(),
                            subtitle: t.subtitle.clone().into(),
                            duration_label: t.duration_label.clone().into(),
                            is_current: t.is_current,
                            is_favorite: t.is_favorite,
                            has_cover: t.has_cover,
                            cover,
                        }
                    })
                    .collect::<Vec<_>>(),
            )),
            playlists: ModelRc::new(VecModel::from(
                p.playlists
                    .iter()
                    .map(|pl| AudioPlayerPlaylistItem {
                        id: pl.id.clone().into(),
                        name: playlist_name(&pl.name, locale),
                        count: pl.count as i32,
                        is_active: pl.is_active,
                    })
                    .collect::<Vec<_>>(),
            )),
            roots: ModelRc::new(VecModel::from(
                p.roots
                    .iter()
                    .map(|r| AudioPlayerRootItem {
                        path: r.path.clone().into(),
                        label: r.label.clone().into(),
                    })
                    .collect::<Vec<_>>(),
            )),
            has_track: p.has_track,
            title: p.title.clone().into(),
            artist: p.artist.clone().into(),
            album: p.album.clone().into(),
            is_playing: p.is_playing,
            progress: p.progress.clamp(0.0, 1.0),
            position_label: p.position_label.clone().into(),
            duration_label: p.duration_label.clone().into(),
            volume: p.volume as i32,
            muted: p.muted,
            shuffle: p.shuffle,
            repeat: i32::from(p.repeat),
            sleep_label: p.sleep_label.clone().into(),
            eq_label: p.eq_label.clone().into(),
            rg_label: p.rg_label.clone().into(),
            speed_label: p.speed_label.clone().into(),
            crossfade_label: crossfade_label(p.crossfade_secs, locale),
            lyrics_line: p.lyrics_line.clone().into(),
            has_lyrics: p.has_lyrics,
            roots_label: library_stats_label(p.library_count, p.library_roots_count, locale),
            empty_hint,
            has_cover: p.has_cover,
            cover,
            queue_stats_label: queue_stats_label(p.queue_count, p.queue_duration_ms, locale),
            current_track_index: p.current_track_index,
            has_library_roots: p.has_library_roots,
            sort_label: sort_label_for(p.library_sort, locale),
            is_current_favorite: p.is_current_favorite,
            ..labels_only(locale)
        },
        locale,
    )
}
