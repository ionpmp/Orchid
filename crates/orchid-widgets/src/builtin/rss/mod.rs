//! RSS / Atom feed widget.

pub mod config;
pub mod provider;
pub mod types;

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use parking_lot::RwLock;
use tracing::warn;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::config as state_codec;
use crate::widget::payloads::{RssItemView, RssPayload};
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{
    Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory,
};
use orchid_storage::{LifecycleState, WidgetSize};

pub use config::{FeedSource, RssConfig};
pub use provider::RssProvider;
pub use types::{FeedData, FeedItem};

/// Stable type id.
pub const TYPE_ID: &str = "rss";

fn job_key(instance_id: Uuid) -> String {
    format!("rss:{instance_id}")
}

static RSS_LIVE: LazyLock<DashMap<Uuid, Arc<RssHandle>>> = LazyLock::new(DashMap::new);

struct RssHandle {
    instance_id: Uuid,
    config: Arc<RwLock<RssConfig>>,
    data: Arc<RwLock<FeedData>>,
    provider: RssProvider,
    bus: Arc<orchid_core::EventBus>,
    locale: Arc<orchid_i18n::LocaleManager>,
    jobs: Arc<orchid_core::BackgroundJobQueue>,
}

impl RssHandle {
    fn publish(&self) {
        self.bus.publish(
            orchid_core::EventSource::Widget(self.instance_id),
            WidgetSnapshotUpdated {
                instance_id: self.instance_id,
            },
        );
    }

    fn enabled_feeds(&self) -> Vec<FeedSource> {
        self.config
            .read()
            .feeds
            .iter()
            .filter(|f| f.enabled)
            .cloned()
            .collect()
    }

    fn refresh_interval(&self) -> Duration {
        Duration::from_secs(self.config.read().refresh_interval_minutes.max(1) as u64 * 60)
    }

    /// Schedule (or replace) the always-on feed fetch job.
    fn schedule_job(self: &Arc<Self>) {
        let handle = Arc::clone(self);
        let interval = self.refresh_interval();
        self.jobs.schedule(job_key(self.instance_id), interval, move || {
            let handle = Arc::clone(&handle);
            async move {
                let feeds = handle.enabled_feeds();
                let provider = handle.provider.clone();
                let data_slot = handle.data.clone();
                let bus = handle.bus.clone();
                let instance_id = handle.instance_id;
                RssWidget::fetch_once(provider, feeds, data_slot, bus, instance_id).await;
            }
        });
    }

    fn cancel_job(&self) {
        self.jobs.cancel(&job_key(self.instance_id));
    }
}

fn feeds_differ(before: &RssConfig, after: &RssConfig) -> bool {
    if before.feeds.len() != after.feeds.len() {
        return true;
    }
    before.feeds.iter().zip(&after.feeds).any(|(a, b)| {
        a.name != b.name || a.url != b.url || a.enabled != b.enabled
    })
}

/// Snapshot the live RSS config for the settings dialog.
#[must_use]
pub fn current_config(instance_id: Uuid) -> Option<RssConfig> {
    RSS_LIVE
        .get(&instance_id)
        .map(|h| h.config.read().clone())
}

/// Apply a settings-dialog mutation to the live RSS config.
pub fn update_config(instance_id: Uuid, mutate: impl FnOnce(&mut RssConfig)) {
    let Some(h) = RSS_LIVE.get(&instance_id) else {
        return;
    };
    let before = h.config.read().clone();
    {
        let mut cfg = h.config.write();
        mutate(&mut cfg);
        cfg.normalize();
    }
    let after = h.config.read().clone();
    let interval_changed =
        before.refresh_interval_minutes != after.refresh_interval_minutes;
    if feeds_differ(&before, &after) {
        let provider = h.provider.clone();
        let feeds = h.enabled_feeds();
        let data_slot = h.data.clone();
        let bus = h.bus.clone();
        let instance_id = h.instance_id;
        tokio::spawn(async move {
            RssWidget::fetch_once(provider, feeds, data_slot, bus, instance_id).await;
        });
    } else {
        h.publish();
    }
    if interval_changed {
        h.schedule_job();
    }
}

/// RSS widget.
pub struct RssWidget {
    instance_id: Uuid,
    handle: Arc<RssHandle>,
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
        jobs: Arc<orchid_core::BackgroundJobQueue>,
    ) -> Self {
        let handle = Arc::new(RssHandle {
            instance_id,
            config: Arc::new(RwLock::new(cfg)),
            data: Arc::new(RwLock::new(FeedData::default())),
            provider: RssProvider::new(client),
            bus,
            locale,
            jobs,
        });
        RSS_LIVE.insert(instance_id, Arc::clone(&handle));
        Self {
            instance_id,
            handle,
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
        // Always-on fetch: first tick runs immediately via BackgroundJobQueue.
        self.handle.schedule_job();
        Ok(())
    }
    async fn on_activate(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }
    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }
    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }
    async fn on_close(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.handle.cancel_job();
        RSS_LIVE.remove(&self.instance_id);
        Ok(())
    }
    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }
    fn snapshot(&self) -> Option<WidgetSnapshot> {
        let cfg = self.handle.config.read().clone();
        let data = self.handle.data.read().clone();
        let enabled_feed_count = cfg.feeds.iter().filter(|f| f.enabled).count() as u32;
        let failed_feed_count = data.per_feed_errors.len() as u32;
        let is_loading = data.fetched_at.is_none();
        let last_updated_text = match data.fetched_at {
            Some(at) => format_relative(&self.handle.locale, at),
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
                title: if it.title.trim().is_empty() {
                    self.handle.locale.tr("rss-item-untitled")
                } else {
                    it.title.clone()
                },
                source_name: it.source_name.clone(),
                published_text: it
                    .published
                    .map(|t| format_item_relative(&self.handle.locale, now, t))
                    .unwrap_or_default(),
                summary_text: it.summary.clone(),
                link: it.link.clone(),
            })
            .collect::<Vec<_>>();

        Some(WidgetSnapshot {
            instance_id: self.instance_id,
            widget_type: TYPE_ID,
            title: self.handle.locale.tr("widget-rss-name").into(),
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
        state_codec::save_state(&*self.handle.config.read())
    }
    fn restore_state(&mut self, bytes: &[u8]) -> WidgetResult<()> {
        let mut cfg: RssConfig = state_codec::restore_state(bytes)?;
        cfg.normalize();
        *self.handle.config.write() = cfg;
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
            ctx.jobs.clone(),
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
