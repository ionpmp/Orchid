//! Publish Orchid's in-app media session to Windows SMTC (lock screen / media keys).
//!
//! Uses a hidden [`MediaPlayer`] only to obtain
//! [`SystemMediaTransportControls`]. The SMTC consumer widget may see Orchid
//! as the current session while this is enabled.

#![cfg(windows)]

use std::sync::{Mutex, OnceLock};

use orchid_viewers::MediaSnapshot;
use tracing::debug;
use uuid::Uuid;
use windows::core::{Ref, Result as WinResult};
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Media::Playback::MediaPlayer;
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls,
    SystemMediaTransportControlsButton, SystemMediaTransportControlsButtonPressedEventArgs,
    SystemMediaTransportControlsTimelineProperties,
};

static STATE: OnceLock<Mutex<Option<PublisherState>>> = OnceLock::new();

struct PublisherState {
    _player: MediaPlayer,
    smtc: SystemMediaTransportControls,
    active: Option<Uuid>,
    last_title: String,
    last_artist: String,
    last_playing: Option<bool>,
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
    })
}

fn dispatch_command(command: &'static str) {
    let id = {
        let Ok(guard) = state().lock() else {
            return;
        };
        guard.as_ref().and_then(|s| s.active)
    };
    let Some(id) = id else {
        return;
    };
    tokio::spawn(async move {
        if let Err(e) = super::media_command(id, command).await {
            debug!(error = %e, command, "SMTC media_command failed");
        }
    });
}

/// Bind OS media keys to this viewer instance.
pub fn set_active(instance_id: Uuid) {
    let Ok(mut guard) = state().lock() else {
        return;
    };
    let Some(state) = guard.as_mut() else {
        return;
    };
    state.active = Some(instance_id);
    let _ = state.smtc.SetIsEnabled(true);
}

/// Clear SMTC when the active viewer closes or switches away from media.
pub fn clear_active(instance_id: Uuid) {
    let Ok(mut guard) = state().lock() else {
        return;
    };
    let Some(state) = guard.as_mut() else {
        return;
    };
    if state.active == Some(instance_id) {
        state.active = None;
        let _ = state.smtc.SetIsEnabled(false);
        let _ = state.smtc.SetPlaybackStatus(MediaPlaybackStatus::Closed);
        state.last_title.clear();
        state.last_artist.clear();
        state.last_playing = None;
    }
}

/// Push metadata / timeline / status from a media snapshot.
pub fn publish(instance_id: Uuid, snap: &MediaSnapshot) {
    let Ok(mut guard) = state().lock() else {
        return;
    };
    let Some(state) = guard.as_mut() else {
        return;
    };
    if state.active != Some(instance_id) {
        return;
    }
    if !snap.available {
        let _ = state.smtc.SetIsEnabled(false);
        return;
    }
    let _ = state.smtc.SetIsEnabled(true);

    let title = if snap.title.is_empty() {
        snap.path_display.clone()
    } else {
        snap.title.clone()
    };
    let artist = snap.artist.clone();
    if title != state.last_title || artist != state.last_artist {
        if let Ok(updater) = state.smtc.DisplayUpdater() {
            let _ = updater.SetType(MediaPlaybackType::Music);
            if let Ok(props) = updater.MusicProperties() {
                let _ = props.SetTitle(&windows::core::HSTRING::from(title.as_str()));
                let _ = props.SetArtist(&windows::core::HSTRING::from(artist.as_str()));
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

fn ms_to_timespan(ms: u64) -> TimeSpan {
    TimeSpan {
        Duration: (ms as i64).saturating_mul(10_000),
    }
}
