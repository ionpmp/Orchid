//! Windows System Media Transport Controls (SMTC) integration.

use std::sync::OnceLock;
use std::time::Duration;

use async_trait::async_trait;
use windows::core::HSTRING;
use windows::Foundation::TimeSpan;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession,
    GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};

use super::{MediaError, MediaProvider, MediaSession};

static MANAGER: OnceLock<Result<GlobalSystemMediaTransportControlsSessionManager, String>> =
    OnceLock::new();

/// Reads the OS now-playing session via SMTC.
#[derive(Debug, Default, Clone)]
pub struct WindowsMediaProvider;

impl WindowsMediaProvider {
    /// Construct a provider. The session manager is opened lazily on first use.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

fn manager() -> Result<&'static GlobalSystemMediaTransportControlsSessionManager, MediaError> {
    MANAGER
        .get_or_init(|| {
            GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
                .and_then(|op| op.get())
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| MediaError::ControlFailed(e.clone()))
}

fn timespan_to_duration(span: TimeSpan) -> Duration {
    let ticks = span.Duration.max(0) as u64;
    Duration::from_nanos(ticks * 100)
}

fn duration_to_timespan(d: Duration) -> i64 {
    (d.as_nanos() / 100) as i64
}

fn hstring_opt(s: HSTRING) -> Option<String> {
    let text = s.to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn session_open(
    session: &GlobalSystemMediaTransportControlsSession,
) -> Result<(), MediaError> {
    let info = session
        .GetPlaybackInfo()
        .map_err(|e| MediaError::ControlFailed(e.to_string()))?;
    let status = info
        .PlaybackStatus()
        .map_err(|e| MediaError::ControlFailed(e.to_string()))?;
    if status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Closed {
        return Err(MediaError::NoSession);
    }
    Ok(())
}

fn session_snapshot() -> Option<MediaSession> {
    let manager = manager().ok()?;
    let session = manager.GetCurrentSession().ok()?;
    session_open(&session).ok()?;

    let props = session
        .TryGetMediaPropertiesAsync()
        .ok()?
        .get()
        .ok()?;
    let timeline = session.GetTimelineProperties().ok()?;
    let info = session.GetPlaybackInfo().ok()?;
    let status = info.PlaybackStatus().ok()?;

    let title = props.Title().ok()?.to_string();
    if title.is_empty() {
        return None;
    }

    let is_playing =
        status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing;
    let position = timespan_to_duration(timeline.Position().ok()?);
    let duration = timespan_to_duration(timeline.EndTime().ok()?);
    let source_app = session
        .SourceAppUserModelId()
        .ok()
        .and_then(hstring_opt);

    Some(MediaSession {
        title,
        artist: props.Artist().ok().and_then(hstring_opt),
        album: props.AlbumTitle().ok().and_then(hstring_opt),
        source_app,
        position: Some(position),
        duration: Some(duration),
        is_playing,
        thumbnail_bytes: None,
    })
}

fn with_current_session<F>(f: F) -> Result<(), MediaError>
where
    F: FnOnce(&GlobalSystemMediaTransportControlsSession) -> Result<(), MediaError>,
{
    let manager = manager()?;
    let session = manager
        .GetCurrentSession()
        .map_err(|e| MediaError::ControlFailed(e.to_string()))?;
    session_open(&session)?;
    f(&session)
}

fn run_bool_async(op: windows::Foundation::IAsyncOperation<bool>) -> Result<(), MediaError> {
    let ok = op
        .get()
        .map_err(|e| MediaError::ControlFailed(e.to_string()))?;
    if ok {
        Ok(())
    } else {
        Err(MediaError::ControlFailed("transport command rejected".into()))
    }
}

#[async_trait]
impl MediaProvider for WindowsMediaProvider {
    async fn current(&self) -> Option<MediaSession> {
        tokio::task::spawn_blocking(session_snapshot)
            .await
            .ok()
            .flatten()
    }

    async fn play(&self) -> Result<(), MediaError> {
        tokio::task::spawn_blocking(|| {
            with_current_session(|session| {
                let op = session
                    .TryPlayAsync()
                    .map_err(|e| MediaError::ControlFailed(e.to_string()))?;
                run_bool_async(op)
            })
        })
        .await
        .map_err(|e| MediaError::ControlFailed(e.to_string()))?
    }

    async fn pause(&self) -> Result<(), MediaError> {
        tokio::task::spawn_blocking(|| {
            with_current_session(|session| {
                let op = session
                    .TryPauseAsync()
                    .map_err(|e| MediaError::ControlFailed(e.to_string()))?;
                run_bool_async(op)
            })
        })
        .await
        .map_err(|e| MediaError::ControlFailed(e.to_string()))?
    }

    async fn next(&self) -> Result<(), MediaError> {
        tokio::task::spawn_blocking(|| {
            with_current_session(|session| {
                let op = session
                    .TrySkipNextAsync()
                    .map_err(|e| MediaError::ControlFailed(e.to_string()))?;
                run_bool_async(op)
            })
        })
        .await
        .map_err(|e| MediaError::ControlFailed(e.to_string()))?
    }

    async fn previous(&self) -> Result<(), MediaError> {
        tokio::task::spawn_blocking(|| {
            with_current_session(|session| {
                let op = session
                    .TrySkipPreviousAsync()
                    .map_err(|e| MediaError::ControlFailed(e.to_string()))?;
                run_bool_async(op)
            })
        })
        .await
        .map_err(|e| MediaError::ControlFailed(e.to_string()))?
    }

    async fn seek_to(&self, position: Duration) -> Result<(), MediaError> {
        let target = duration_to_timespan(position);
        tokio::task::spawn_blocking(move || {
            with_current_session(|session| {
                let op = session
                    .TryChangePlaybackPositionAsync(target)
                    .map_err(|e| MediaError::ControlFailed(e.to_string()))?;
                run_bool_async(op)
            })
        })
        .await
        .map_err(|e| MediaError::ControlFailed(e.to_string()))?
    }
}
