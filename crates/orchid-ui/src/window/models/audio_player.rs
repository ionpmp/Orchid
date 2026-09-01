//! Slint model for the local audio library player.

use orchid_i18n::LocaleManager;
use orchid_widgets::AudioPlayerPayload;
use slint::{Image, ModelRc, SharedString, VecModel};

use crate::slint_generated::{
    AudioPlayerGroupItem, AudioPlayerModel, AudioPlayerPlaylistItem, AudioPlayerTrackItem,
};

pub(crate) fn empty_audio_player_model(locale: &LocaleManager) -> AudioPlayerModel {
    fill_labels(
        AudioPlayerModel {
            engine_available: false,
            browse_tab: 0,
            browse_filter: SharedString::new(),
            search_query: SharedString::new(),
            groups: ModelRc::new(VecModel::from(Vec::<AudioPlayerGroupItem>::new())),
            tracks: ModelRc::new(VecModel::from(Vec::<AudioPlayerTrackItem>::new())),
            playlists: ModelRc::new(VecModel::from(Vec::<AudioPlayerPlaylistItem>::new())),
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
            speed_label: SharedString::new(),
            lyrics_line: SharedString::new(),
            has_lyrics: false,
            roots_label: SharedString::new(),
            empty_hint: SharedString::new(),
            has_cover: false,
            cover: Image::default(),
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
        search_query: SharedString::new(),
        groups: ModelRc::new(VecModel::from(Vec::<AudioPlayerGroupItem>::new())),
        tracks: ModelRc::new(VecModel::from(Vec::<AudioPlayerTrackItem>::new())),
        playlists: ModelRc::new(VecModel::from(Vec::<AudioPlayerPlaylistItem>::new())),
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
        speed_label: SharedString::new(),
        lyrics_line: SharedString::new(),
        has_lyrics: false,
        roots_label: SharedString::new(),
        empty_hint: SharedString::new(),
        has_cover: false,
        cover: Image::default(),
        tab_songs: locale.tr("audio-player-tab-songs").into(),
        tab_artists: locale.tr("audio-player-tab-artists").into(),
        tab_albums: locale.tr("audio-player-tab-albums").into(),
        tab_folders: locale.tr("audio-player-tab-folders").into(),
        tab_playlists: locale.tr("audio-player-tab-playlists").into(),
        tab_now_playing: locale.tr("audio-player-tab-now-playing").into(),
        back_label: locale.tr("audio-player-back").into(),
        add_folder_label: locale.tr("audio-player-add-folder").into(),
        new_playlist_label: locale.tr("audio-player-new-playlist").into(),
        no_track_label: locale.tr("audio-player-no-track").into(),
        sleep_off_label: locale.tr("audio-player-sleep-off").into(),
        eq_off_label: locale.tr("audio-player-eq-off").into(),
        speed_off_label: locale.tr("audio-player-speed-off").into(),
        search_placeholder: locale.tr("audio-player-search-placeholder").into(),
        engine_missing_label: locale.tr("audio-player-engine-missing").into(),
    }
}

fn fill_labels(mut m: AudioPlayerModel, locale: &LocaleManager) -> AudioPlayerModel {
    let base = labels_only(locale);
    m.tab_songs = base.tab_songs;
    m.tab_artists = base.tab_artists;
    m.tab_albums = base.tab_albums;
    m.tab_folders = base.tab_folders;
    m.tab_playlists = base.tab_playlists;
    m.tab_now_playing = base.tab_now_playing;
    m.back_label = base.back_label;
    m.add_folder_label = base.add_folder_label;
    m.new_playlist_label = base.new_playlist_label;
    m.no_track_label = base.no_track_label;
    m.sleep_off_label = base.sleep_off_label;
    m.eq_off_label = base.eq_off_label;
    m.speed_off_label = base.speed_off_label;
    m.search_placeholder = base.search_placeholder;
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
            search_query: p.search_query.clone().into(),
            groups: ModelRc::new(VecModel::from(
                p.groups
                    .iter()
                    .map(|g| AudioPlayerGroupItem {
                        key: g.key.clone().into(),
                        label: g.label.clone().into(),
                        count: g.count as i32,
                    })
                    .collect::<Vec<_>>(),
            )),
            tracks: ModelRc::new(VecModel::from(
                p.tracks
                    .iter()
                    .map(|t| AudioPlayerTrackItem {
                        path: t.path.clone().into(),
                        title: t.title.clone().into(),
                        subtitle: t.subtitle.clone().into(),
                        is_current: t.is_current,
                        is_favorite: t.is_favorite,
                    })
                    .collect::<Vec<_>>(),
            )),
            playlists: ModelRc::new(VecModel::from(
                p.playlists
                    .iter()
                    .map(|pl| AudioPlayerPlaylistItem {
                        id: pl.id.clone().into(),
                        name: pl.name.clone().into(),
                        count: pl.count as i32,
                        is_active: pl.is_active,
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
            speed_label: p.speed_label.clone().into(),
            lyrics_line: p.lyrics_line.clone().into(),
            has_lyrics: p.has_lyrics,
            roots_label: if p.library_count > 0 && !p.roots_label.is_empty() {
                format!("{} tracks · {}", p.library_count, p.roots_label).into()
            } else if p.library_count > 0 {
                format!("{} tracks", p.library_count).into()
            } else {
                p.roots_label.clone().into()
            },
            empty_hint,
            has_cover: p.has_cover,
            cover,
            ..labels_only(locale)
        },
        locale,
    )
}
