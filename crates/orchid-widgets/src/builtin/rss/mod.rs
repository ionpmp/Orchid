//! RSS / Atom feed widget.

pub mod config;
pub mod provider;
pub mod types;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use parking_lot::RwLock;
use tracing::warn;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::{RssItemView, RssPayload};
use crate::widget::refresh::PeriodicRefresh;
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory};
use orchid_storage::{LifecycleState, WidgetSize};

pub use config::{FeedSource, RssConfig};
pub use provider::RssProvider;
pub use types::{FeedData, FeedItem};

/// Stable type id.
pub const TYPE_ID: &str = "rss";

/// RSS widget.
pub struct RssWidget {
    instance_id: Uuid,
    config: RwLock<RssConfig>,
    provider: RssProvider,
    data: Arc<RwLock<FeedData>>,
    refresh: PeriodicRefresh,
    bus: Arc<orchid_core::EventBus>,
    locale: Arc<orchid_i18n::LocaleManager>,
}

impl std::fmt::Debug for RssWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RssWidget")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl RssWidget {
    /// Construct an RSS widget.
    pub fn new(
        instance_id: Uuid,
        cfg: RssConfig,
        client: reqwest::Client,
        bus: Arc<orchid_core::EventBus>,
        locale: Arc<orchid_i18n::LocaleManager>,
    ) -> Self {
        let interval = Duration::from_secs(cfg.refresh_interval_minutes.max(1) as u64 * 60);
        Self {
            instance_id,
            config: RwLock::new(cfg),
            provider: RssProvider::new(client),
            data: Arc::new(RwLock::new(FeedData::default())),
            refresh: PeriodicRefresh::new(interval),
            bus,
            locale,
        }
    }

    /// Fetch once and update shared state; publishes a snapshot-updated event.
    async fn fetch_once(
        provider: RssProvider,
        feeds: Vec<FeedSource>,
        data_slot: Arc<RwLock<FeedData>>,
        bus: Arc<orchid_core::EventBus>,
        instance_id: Uuid,
    ) {
        let fetched = provider.fetch_all(&feeds).await;
        if !fetched.per_feed_errors.is_empty() {
            warn!(
                %instance_id,
                failed = fetched.per_feed_errors.len(),
                "rss fetch had feed errors"
            );
        }
        *data_slot.write() = fetched;
        bus.publish(
            orchid_core::EventSource::Widget(instance_id),
            WidgetSnapshotUpdated { instance_id },
        );
    }
}

#[async_trait]
impl Widget for RssWidget {
    fn type_id(&self) -> &'static str {
        TYPE_ID
    }
    fn instance_id(&self) -> Uuid {
        self.instance_id
    }
    async fn on_create(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        let feeds = self
            .config
            .read()
            .feeds
            .iter()
            .filter(|f| f.enabled)
            .cloned()
            .collect::<Vec<_>>();
        let provider = self.provider.clone();
        let data_slot = self.data.clone();
        let bus = self.bus.clone();
        let instance_id = self.instance_id;
        Self::fetch_once(provider, feeds, data_slot, bus, instance_id).await;
        Ok(())
    }
    async fn on_activate(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        let feeds = self
            .config
            .read()
            .feeds
            .iter()
            .filter(|f| f.enabled)
            .cloned()
            .collect::<Vec<_>>();
        let provider = self.provider.clone();
        let data_slot = self.data.clone();
        let bus = self.bus.clone();
        let instance_id = self.instance_id;
        self.refresh.start(move || {
            let feeds = feeds.clone();
            let provider = provider.clone();
            let data_slot = data_slot.clone();
            let bus = bus.clone();
            async move {
                Self::fetch_once(provider, feeds, data_slot, bus, instance_id).await;
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
        let cfg = self.config.read().clone();
        let data = self.data.read().clone();
        let enabled_feed_count = cfg.feeds.iter().filter(|f| f.enabled).count() as u32;
        let failed_feed_count = data.per_feed_errors.len() as u32;
        let is_loading = data.fetched_at.is_none();
        let last_updated_text = match data.fetched_at {
            Some(at) => format_relative(&self.locale, at),
            None => String::new(),
        };

        let now = Utc::now();
        let limit = cfg.max_items_displayed as usize;
        let items = data
            .items
            .iter()
            .take(limit.max(1))
            .map(|it| RssItemView {
                id: it.id.clone(),
                title: it.title.clone(),
                source_name: it.source_name.clone(),
                published_text: it
                    .published
                    .map(|t| format_item_relative(&self.locale, now, t))
                    .unwrap_or_default(),
                summary_text: it.summary.clone(),
                link: it.link.clone(),
            })
            .collect::<Vec<_>>();

        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title: self.locale.tr("widget-rss-name").into(),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::RssFeed(RssPayload {
                items,
                last_updated_text,
                is_loading,
                enabled_feed_count,
                failed_feed_count,
            }),
        })
    }
    fn save_state(&self) -> WidgetResult<Vec<u8>> {
        state_codec::save_state(&*self.config.read())
    }
    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let mut cfg: RssConfig = state_codec::restore_state(bytes)?;
        cfg.normalize();
        *self.config.write() = cfg;
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
            has_settings_panel: true,
        }
    }
}

fn format_relative(locale: &orchid_i18n::LocaleManager, at: chrono::DateTime<Utc>) -> String {
    let secs = (Utc::now() - at).num_seconds().max(0);
    if secs < 60 {
        locale.tr("weather-updated-just-now")
    } else if secs < 3600 {
        locale.tr_args(
            "weather-updated-minutes",
            &orchid_i18n::FluentArgs::new().with("m", (secs / 60).to_string()),
        )
    } else if secs < 86400 {
        locale.tr_args(
            "weather-updated-hours",
            &orchid_i18n::FluentArgs::new().with("h", (secs / 3600).to_string()),
        )
    } else {
        locale.tr_args(
            "weather-updated-days",
            &orchid_i18n::FluentArgs::new().with("d", (secs / 86400).to_string()),
        )
    }
}

fn format_item_relative(
    locale: &orchid_i18n::LocaleManager,
    now: chrono::DateTime<Utc>,
    at: chrono::DateTime<Utc>,
) -> String {
    let secs = (now - at).num_seconds().max(0);
    if secs < 60 {
        locale.tr("relative-just-now")
    } else if secs < 3600 {
        locale.tr_args(
            "relative-minutes",
            &orchid_i18n::FluentArgs::new().with("m", (secs / 60).to_string()),
        )
    } else if secs < 86400 {
        locale.tr_args(
            "relative-hours",
            &orchid_i18n::FluentArgs::new().with("h", (secs / 3600).to_string()),
        )
    } else {
        locale.tr_args(
            "relative-days",
            &orchid_i18n::FluentArgs::new().with("d", (secs / 86400).to_string()),
        )
    }
}

/// Descriptor ready to register on a widget registry.
#[must_use]
pub fn descriptor(http_client: reqwest::Client) -> WidgetDescriptor {
    let factory: WidgetFactory = Arc::new(move |ctx: WidgetContext, state_bytes| {
        let mut cfg = match state_bytes {
            Some(bytes) => state_codec::restore_state::<RssConfig>(bytes).unwrap_or_default(),
            None => RssConfig::default(),
        };
        cfg.normalize();
        Ok(Box::new(RssWidget::new(
            ctx.instance_id,
            cfg,
            http_client.clone(),
            ctx.bus.clone(),
            ctx.locale.clone(),
        )) as Box<dyn Widget>)
    });
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-rss-name",
        description_key: "widget-rss-desc",
        icon_name: "rss",
        category: WidgetCategory::Information,
        default_size: WidgetSize::Large,
        min_size: Some(WidgetSize::Medium),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: true,
        factory,
    }
}

/// Open a feed item's link in the system browser.
///
/// # Errors
///
/// Propagates [`opener`] errors.
pub fn open_link(link: &str) -> std::io::Result<()> {
    opener::open(link).map_err(|e| match e {
        opener::OpenError::Io(io) => io,
        other => std::io::Error::other(other.to_string()),
    })
}
