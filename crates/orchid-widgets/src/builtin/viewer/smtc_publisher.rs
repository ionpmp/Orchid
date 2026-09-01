//! Publish Orchid's in-app media session to Windows SMTC (lock screen / media keys).
//!
//! Used by both the Media Viewer and the Audio Player. A single OS session is
//! shared; the last widget that calls [`set_active`] owns media keys.

#![cfg(windows)]

use std::sync::Arc;
use std::sync::{Mutex, OnceLock};

use tracing::debug;
use uuid::Uuid;
use windows::core::{Ref, Result as WinResult, HSTRING};
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Media::Playback::MediaPlayer;
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls,
    SystemMediaTransportControlsButton, SystemMediaTransportControlsButtonPressedEventArgs,
    SystemMediaTransportControlsTimelineProperties,
};
use windows::Storage::Streams::{
    DataWriter, InMemoryRandomAccessStream, RandomAccessStreamReference,
};

static STATE: OnceLock<Mutex<Option<PublisherState>>> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveKind {
    Viewer,
    AudioPlayer,
}

struct PublisherState {
    _player: MediaPlayer,
    smtc: SystemMediaTransportControls,
    active: Option<(ActiveKind, Uuid)>,
    last_title: String,
    last_artist: String,
    last_playing: Option<bool>,
    last_cover_sig: u64,
}

/// Snapshot fields needed to drive SMTC (viewer or audio player).
#[derive(Debug, Clone)]
pub struct NowPlaying {
    pub available: bool,
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub has_cover: bool,
    pub cover_rgba: Arc<Vec<u8>>,
    pub cover_width: u32,
    pub cover_height: u32,
}

impl NowPlaying {
    /// Build from a media viewer snapshot.
    #[must_use]
    pub fn from_media(snap: &orchid_viewers::MediaSnapshot) -> Self {
        let title = if snap.title.is_empty() {
            snap.path_display.clone()
        } else {
            snap.title.clone()
        };
        Self {
            available: snap.available,
            title,
            artist: snap.artist.clone(),
            playing: snap.playing,
            position_ms: snap.position_ms,
            duration_ms: snap.duration_ms,
            has_cover: snap.has_cover,
            cover_rgba: Arc::clone(&snap.cover_rgba),
            cover_width: snap.cover_width,
            cover_height: snap.cover_height,
        }
    }
}

fn state() -> &'static Mutex<Option<PublisherState>> {
    STATE.get_or_init(|| Mutex::new(init_publisher().ok()))
}

fn init_publisher() -> WinResult<PublisherState> {
    let player = MediaPlayer::new()?;
    if let Ok(cmd) = player.CommandManager() {
        let _ = cmd.SetIsEnabled(false);
    }
    let smtc = player.SystemMediaTransportControls()?;
    smtc.SetIsEnabled(false)?;
    smtc.SetIsPlayEnabled(true)?;
    smtc.SetIsPauseEnabled(true)?;
    smtc.SetIsNextEnabled(true)?;
    smtc.SetIsPreviousEnabled(true)?;

    let handler = TypedEventHandler::<
        SystemMediaTransportControls,
        SystemMediaTransportControlsButtonPressedEventArgs,
    >::new(
        move |_sender, args: Ref<'_, SystemMediaTransportControlsButtonPressedEventArgs>| {
            let Ok(args) = args.ok() else {
                return Ok(());
            };
            let Ok(button) = args.Button() else {
                return Ok(());
            };
            let cmd = match button {
                SystemMediaTransportControlsButton::Play
                | SystemMediaTransportControlsButton::Pause => Some("play-pause"),
                SystemMediaTransportControlsButton::Next => Some("next"),
                SystemMediaTransportControlsButton::Previous => Some("prev"),
                _ => None,
            };
            if let Some(cmd) = cmd {
                dispatch_command(cmd);
            }
            Ok(())
        },
    );
    smtc.ButtonPressed(&handler)?;

    Ok(PublisherState {
        _player: player,
        smtc,
        active: None,
        last_title: String::new(),
        last_artist: String::new(),
        last_playing: None,
        last_cover_sig: 0,
    })
}

fn dispatch_command(command: &'static str) {
    let active = {
        let Ok(guard) = state().lock() else {
            return;
        };
        guard.as_ref().and_then(|s| s.active)
    };
    let Some((kind, id)) = active else {
        return;
    };
    match kind {
        ActiveKind::Viewer => {
            tokio::spawn(async move {
                if let Err(e) = super::media_command(id, command).await {
                    debug!(error = %e, command, "SMTC viewer media_command failed");
                }
            });
        }
        ActiveKind::AudioPlayer => {
            crate::builtin::audio_player::execute_command(id, command);
        }
    }
}

fn set_active_kind(kind: ActiveKind, instance_id: Uuid) {
    let Ok(mut guard) = state().lock() else {
        return;
    };
    let Some(state) = guard.as_mut() else {
        return;
    };
    state.active = Some((kind, instance_id));
    let _ = state.smtc.SetIsEnabled(true);
}

/// Bind OS media keys to this media viewer instance.
pub fn set_active(instance_id: Uuid) {
    set_active_kind(ActiveKind::Viewer, instance_id);
}

/// Bind OS media keys to this audio-player instance.
pub fn set_active_audio(instance_id: Uuid) {
    set_active_kind(ActiveKind::AudioPlayer, instance_id);
}

/// Clear SMTC when the active widget closes or switches away.
pub fn clear_active(instance_id: Uuid) {
    let Ok(mut guard) = state().lock() else {
        return;
    };
    let Some(state) = guard.as_mut() else {
        return;
    };
    if state.active.is_some_and(|(_, id)| id == instance_id) {
        state.active = None;
        let _ = state.smtc.SetIsEnabled(false);
        let _ = state.smtc.SetPlaybackStatus(MediaPlaybackStatus::Closed);
        state.last_title.clear();
        state.last_artist.clear();
        state.last_playing = None;
        state.last_cover_sig = 0;
    }
}

/// Push metadata / timeline / status for the active instance.
pub fn publish_now_playing(instance_id: Uuid, snap: &NowPlaying) {
    let Ok(mut guard) = state().lock() else {
        return;
    };
    let Some(state) = guard.as_mut() else {
        return;
    };
    if !state.active.is_some_and(|(_, id)| id == instance_id) {
        return;
    }
    if !snap.available {
        let _ = state.smtc.SetIsEnabled(false);
        return;
    }
    let _ = state.smtc.SetIsEnabled(true);

    let title = snap.title.clone();
    let artist = snap.artist.clone();
    let cover_sig = cover_signature(snap);
    let meta_changed = title != state.last_title || artist != state.last_artist;
    let cover_changed = cover_sig != state.last_cover_sig;
    if meta_changed || cover_changed {
        if let Ok(updater) = state.smtc.DisplayUpdater() {
            let _ = updater.SetType(MediaPlaybackType::Music);
            if let Ok(props) = updater.MusicProperties() {
                let _ = props.SetTitle(&HSTRING::from(title.as_str()));
                let _ = props.SetArtist(&HSTRING::from(artist.as_str()));
            }
            if cover_changed {
                if snap.has_cover {
                    if let Some(stream_ref) = jpeg_stream_ref_from_rgba(
                        &snap.cover_rgba,
                        snap.cover_width,
                        snap.cover_height,
                    ) {
                        let _ = updater.SetThumbnail(&stream_ref);
                    }
                }
                state.last_cover_sig = cover_sig;
            }
            let _ = updater.Update();
        }
        state.last_title = title;
        state.last_artist = artist;
    }

    let playing = snap.playing;
    if state.last_playing != Some(playing) {
        let status = if playing {
            MediaPlaybackStatus::Playing
        } else {
            MediaPlaybackStatus::Paused
        };
        let _ = state.smtc.SetPlaybackStatus(status);
        state.last_playing = Some(playing);
    }

    if let Ok(timeline) = SystemMediaTransportControlsTimelineProperties::new() {
        let _ = timeline.SetStartTime(TimeSpan { Duration: 0 });
        let _ = timeline.SetPosition(ms_to_timespan(snap.position_ms));
        let _ = timeline.SetEndTime(ms_to_timespan(snap.duration_ms));
        let _ = timeline.SetMinSeekTime(TimeSpan { Duration: 0 });
        let _ = timeline.SetMaxSeekTime(ms_to_timespan(snap.duration_ms));
        let _ = state.smtc.UpdateTimelineProperties(&timeline);
    }
}

/// Push from a media viewer snapshot (compatibility wrapper).
pub fn publish(instance_id: Uuid, snap: &orchid_viewers::MediaSnapshot) {
    publish_now_playing(instance_id, &NowPlaying::from_media(snap));
}

fn ms_to_timespan(ms: u64) -> TimeSpan {
    TimeSpan {
        Duration: (ms as i64).saturating_mul(10_000),
    }
}

fn cover_signature(snap: &NowPlaying) -> u64 {
    if !snap.has_cover || snap.cover_rgba.is_empty() {
        return 0;
    }
    let mut h = (snap.cover_width as u64)
        .wrapping_mul(0x9E37_79B9)
        .wrapping_add(snap.cover_height as u64);
    h ^= snap.cover_rgba.len() as u64;
    for (i, b) in snap.cover_rgba.iter().take(64).enumerate() {
        h = h
            .wrapping_mul(31)
            .wrapping_add(u64::from(*b))
            .wrapping_add(i as u64);
    }
    h
}

fn jpeg_stream_ref_from_rgba(
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Option<RandomAccessStreamReference> {
    if width == 0 || height == 0 || rgba.len() < (width as usize) * (height as usize) * 4 {
        return None;
    }
    let img = image::RgbaImage::from_raw(width, height, rgba.to_vec())?;
    let mut jpeg = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut jpeg);
    let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, 85)
        .encode(rgb.as_raw(), width, height, image::ExtendedColorType::Rgb8)
        .ok()?;
    if jpeg.is_empty() {
        return None;
    }
    let stream = InMemoryRandomAccessStream::new().ok()?;
    let writer = DataWriter::CreateDataWriter(&stream).ok()?;
    writer.WriteBytes(&jpeg).ok()?;
    writer.StoreAsync().ok()?.join().ok()?;
    writer.FlushAsync().ok()?.join().ok()?;
    drop(writer);
    stream.Seek(0).ok()?;
    RandomAccessStreamReference::CreateFromStream(&stream).ok()
}
