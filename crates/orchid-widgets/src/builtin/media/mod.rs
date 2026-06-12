//! Media-player widget wrapping the system's now-playing session.
//!
//! On Windows the widget uses [`WindowsMediaProvider`] (SMTC). Other
//! platforms ship with [`NullProvider`], which renders the "no session"
//! state and rejects transport commands with [`MediaError::Unsupported`].
//! Downstream code can inject any other [`MediaProvider`] impl at
//! registration time.

use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::RwLock;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::payloads::MediaPlayerPayload;
use crate::widget::refresh::PeriodicRefresh;
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory};
use orchid_storage::{LifecycleState, WidgetSize};

#[cfg(windows)]
mod windows_provider;

#[cfg(windows)]
pub use windows_provider::WindowsMediaProvider;

/// Stable type id.
pub const TYPE_ID: &str = "media-player";

/// Error type for provider operations.
#[derive(thiserror::Error, Debug)]
#[allow(missing_docs)]
pub enum MediaError {
    #[error("no active media session")]
    NoSession,
    #[error("media control failed: {0}")]
    ControlFailed(String),
    #[error("unsupported on this platform")]
    Unsupported,
}

/// Now-playing snapshot returned by providers.
#[derive(Debug, Clone, Default)]
#[allow(missing_docs)]
pub struct MediaSession {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub source_app: Option<String>,
    pub position: Option<Duration>,
    pub duration: Option<Duration>,
    pub is_playing: bool,
    pub thumbnail_bytes: Option<Vec<u8>>,
}

/// Abstract media provider.
#[async_trait]
pub trait MediaProvider: Send + Sync + std::fmt::Debug {
    /// Current session, if any.
    async fn current(&self) -> Option<MediaSession>;
    /// Start / resume playback.
    async fn play(&self) -> Result<(), MediaError>;
    /// Pause playback.
    async fn pause(&self) -> Result<(), MediaError>;
    /// Next track.
    async fn next(&self) -> Result<(), MediaError>;
    /// Previous track.
    async fn previous(&self) -> Result<(), MediaError>;
    /// Seek to `position`.
    async fn seek_to(&self, position: Duration) -> Result<(), MediaError>;
}

/// Cross-platform stub used when no native integration is wired in.
#[derive(Debug, Default, Clone)]
pub struct NullProvider;

#[async_trait]
impl MediaProvider for NullProvider {
    async fn current(&self) -> Option<MediaSession> {
        None
    }
    async fn play(&self) -> Result<(), MediaError> {
        Err(MediaError::Unsupported)
    }
    async fn pause(&self) -> Result<(), MediaError> {
        Err(MediaError::Unsupported)
    }
    async fn next(&self) -> Result<(), MediaError> {
        Err(MediaError::Unsupported)
    }
    async fn previous(&self) -> Result<(), MediaError> {
        Err(MediaError::Unsupported)
    }
    async fn seek_to(&self, _position: Duration) -> Result<(), MediaError> {
        Err(MediaError::Unsupported)
    }
}

/// Media-player widget.
pub struct MediaPlayerWidget {
    instance_id: Uuid,
    provider: Arc<dyn MediaProvider>,
    session: Arc<RwLock<Option<MediaSession>>>,
    refresh: PeriodicRefresh,
    bus: Arc<orchid_core::EventBus>,
}

/// Live media providers keyed by instance id (for UI transport controls without
/// holding widget locks).
static MEDIA_LIVE: LazyLock<DashMap<Uuid, Arc<dyn MediaProvider>>> = LazyLock::new(DashMap::new);

/// Execute a transport command on the live media session, if any.
///
/// No-op if the instance is not a live media widget.
pub async fn execute_command(instance_id: Uuid, cmd: &'static str) -> Result<(), MediaError> {
    let Some(p) = MEDIA_LIVE.get(&instance_id).map(|e| e.value().clone()) else {
        return Err(MediaError::NoSession);
    };
    match cmd {
        "play" => p.play().await,
        "pause" => p.pause().await,
        "next" => p.next().await,
        "previous" => p.previous().await,
        other => Err(MediaError::ControlFailed(format!("unknown command: {other}"))),
    }
}

impl std::fmt::Debug for MediaPlayerWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaPlayerWidget")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl MediaPlayerWidget {
    /// Construct a media widget.
    pub fn new(
        instance_id: Uuid,
        provider: Arc<dyn MediaProvider>,
        bus: Arc<orchid_core::EventBus>,
    ) -> Self {
        MEDIA_LIVE.insert(instance_id, provider.clone());
        Self {
            instance_id,
            provider,
            session: Arc::new(RwLock::new(None)),
            refresh: PeriodicRefresh::new(Duration::from_millis(500)),
            bus,
        }
    }

    /// Access the provider; used by the UI shell for transport controls.
    pub fn provider(&self) -> Arc<dyn MediaProvider> {
        self.provider.clone()
    }
}

impl Drop for MediaPlayerWidget {
    fn drop(&mut self) {
        MEDIA_LIVE.remove(&self.instance_id);
    }
}

#[async_trait]
impl Widget for MediaPlayerWidget {
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
        let provider = self.provider.clone();
        let slot = self.session.clone();
        let bus = self.bus.clone();
        let instance_id = self.instance_id;
        self.refresh.start(move || {
            let provider = provider.clone();
            let slot = slot.clone();
            let bus = bus.clone();
            async move {
                let session = provider.current().await;
                *slot.write() = session;
                bus.publish(
                    orchid_core::EventSource::Widget(instance_id),
                    WidgetSnapshotUpdated { instance_id },
                );
            }
        });
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
        Ok(())
    }
    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }
    fn snapshot(&self) -> Option<WidgetSnapshot> {
        let session = self.session.read().clone();
        let payload = match session {
            Some(s) => session_to_payload(s),
            None => MediaPlayerPayload::default(),
        };
        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title: "Media".into(),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::MediaPlayer(payload),
        })
    }
    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        Ok(Vec::new())
    }
    fn restore_state(&mut self, _bytes: &[u8]) -> WidgetResult<()> {
        Ok(())
    }
    fn capabilities(&self) -> WidgetCapabilities {
        WidgetCapabilities {
            supports_resize: true,
            min_size: Some(WidgetSize::Medium),
            max_size: None,
            preferred_size: Some(WidgetSize::Medium),
            allows_grouping: true,
            keeps_state_when_unloaded: false,
            has_settings_panel: false,
        }
    }
}

fn session_to_payload(s: MediaSession) -> MediaPlayerPayload {
    let duration = s.duration.unwrap_or_default();
    let position = s.position.unwrap_or_default();
    let fraction = if duration.as_secs() == 0 {
        0.0
    } else {
        (position.as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0)
    };
    let thumbnail_base64 = s
        .thumbnail_bytes
        .as_ref()
        .map(|bytes| base64_encode(bytes));
    MediaPlayerPayload {
        has_session: true,
        title: s.title,
        artist: s.artist.unwrap_or_default(),
        album: s.album.unwrap_or_default(),
        source_app: s.source_app.unwrap_or_default(),
        position_text: format_duration(position),
        duration_text: format_duration(duration),
        progress_fraction: fraction,
        is_playing: s.is_playing,
        thumbnail_base64,
    }
}

fn format_duration(d: Duration) -> String {
    let total = d.as_secs();
    let m = total / 60;
    let s = total % 60;
    format!("{m}:{s:02}")
}

/// Minimal RFC 4648 Base64 encoder for thumbnail payloads. Used only for
/// the UI's `data:image/...;base64,...` URL. A dependency like `base64`
/// would also work; rolling it by hand avoids an extra runtime dep.
fn base64_encode(data: &[u8]) -> String {
    const CHARSET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 3 <= data.len() {
        let b = &data[i..i + 3];
        let v = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(CHARSET[((v >> 18) & 0x3F) as usize] as char);
        out.push(CHARSET[((v >> 12) & 0x3F) as usize] as char);
        out.push(CHARSET[((v >> 6) & 0x3F) as usize] as char);
        out.push(CHARSET[(v & 0x3F) as usize] as char);
        i += 3;
    }
    let rem = data.len() - i;
    if rem == 1 {
        let v = (data[i] as u32) << 16;
        out.push(CHARSET[((v >> 18) & 0x3F) as usize] as char);
        out.push(CHARSET[((v >> 12) & 0x3F) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let v = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
        out.push(CHARSET[((v >> 18) & 0x3F) as usize] as char);
        out.push(CHARSET[((v >> 12) & 0x3F) as usize] as char);
        out.push(CHARSET[((v >> 6) & 0x3F) as usize] as char);
        out.push('=');
    }
    out
}

/// Descriptor using a caller-supplied provider.
#[must_use]
pub fn descriptor_with_provider(provider: Arc<dyn MediaProvider>) -> WidgetDescriptor {
    let factory: WidgetFactory = Arc::new(move |ctx: WidgetContext, _bytes| {
        Ok(Box::new(MediaPlayerWidget::new(
            ctx.instance_id,
            provider.clone(),
            ctx.bus.clone(),
        )) as Box<dyn Widget>)
    });
    base_descriptor(factory)
}

/// Descriptor with the platform default provider.
#[must_use]
pub fn descriptor() -> WidgetDescriptor {
    #[cfg(windows)]
    {
        descriptor_with_provider(Arc::new(WindowsMediaProvider::new()))
    }
    #[cfg(not(windows))]
    {
        descriptor_with_provider(Arc::new(NullProvider))
    }
}

fn base_descriptor(factory: WidgetFactory) -> WidgetDescriptor {
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-media-name",
        description_key: "widget-media-desc",
        icon_name: "media",
        category: WidgetCategory::Media,
        default_size: WidgetSize::Medium,
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

    #[tokio::test]
    async fn null_provider_rejects_controls() {
        let p = NullProvider;
        assert!(matches!(p.play().await.unwrap_err(), MediaError::Unsupported));
        assert!(matches!(p.pause().await.unwrap_err(), MediaError::Unsupported));
        assert!(matches!(p.next().await.unwrap_err(), MediaError::Unsupported));
        assert!(matches!(p.previous().await.unwrap_err(), MediaError::Unsupported));
        assert!(p.current().await.is_none());
    }

    #[test]
    fn base64_roundtrips_small_inputs() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
    }

    #[test]
    fn session_formatting_builds_fraction() {
        let session = MediaSession {
            title: "Song".into(),
            is_playing: true,
            position: Some(Duration::from_secs(30)),
            duration: Some(Duration::from_secs(90)),
            ..Default::default()
        };
        let payload = session_to_payload(session);
        assert!(payload.has_session);
        assert_eq!(payload.position_text, "0:30");
        assert_eq!(payload.duration_text, "1:30");
        assert!((payload.progress_fraction - 1.0 / 3.0).abs() < 1e-3);
    }
}
