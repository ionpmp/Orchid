//! Universal search widget.

pub mod aggregator;
pub mod sources;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::RwLock;
use tokio::sync::Notify;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::payloads::{SearchCandidateView, UniversalSearchPayload};
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory};
use orchid_storage::{LifecycleState, WidgetSize};

pub use aggregator::SearchAggregator;
pub use sources::{ActionTarget, CommandsSource, FilesSource, SearchCandidate, SearchSource, SettingsSource};

/// Stable type id.
pub const TYPE_ID: &str = "universal-search";

/// Debounce window before a pending query is actually executed.
const DEBOUNCE: Duration = Duration::from_millis(150);

struct Inner {
    query: RwLock<String>,
    candidates: RwLock<Vec<SearchCandidate>>,
    is_searching: RwLock<bool>,
    error: RwLock<Option<String>>,
    notify: Notify,
    aggregator: Option<Arc<SearchAggregator>>,
    bus: Arc<orchid_core::EventBus>,
    instance_id: Uuid,
}

/// Universal search widget.
pub struct UniversalSearchWidget {
    inner: Arc<Inner>,
    // Debouncer task handle. Aborted on close.
    task: parking_lot::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl std::fmt::Debug for UniversalSearchWidget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UniversalSearchWidget")
            .field("instance_id", &self.inner.instance_id)
            .finish_non_exhaustive()
    }
}

impl UniversalSearchWidget {
    /// Construct a search widget with a pre-built aggregator.
    ///
    /// `aggregator` is optional: when `None` the widget renders the empty
    /// state and reports `error = "no sources"` on activation. This keeps
    /// the descriptor usable even before the UI layer wires up real
    /// sources.
    pub fn new(
        instance_id: Uuid,
        aggregator: Option<Arc<SearchAggregator>>,
        bus: Arc<orchid_core::EventBus>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                query: RwLock::new(String::new()),
                candidates: RwLock::new(Vec::new()),
                is_searching: RwLock::new(false),
                error: RwLock::new(None),
                notify: Notify::new(),
                aggregator,
                bus,
                instance_id,
            }),
            task: parking_lot::Mutex::new(None),
        }
    }

    /// Update the query. Debounces for [`DEBOUNCE`] before running a real
    /// search.
    pub fn update_query(&self, query: String) {
        *self.inner.query.write() = query;
        self.inner.notify.notify_one();
    }

    fn start_debouncer(&self) {
        let inner = self.inner.clone();
        let handle = tokio::spawn(async move {
            loop {
                inner.notify.notified().await;
                // Coalesce rapid changes over a short window.
                tokio::time::sleep(DEBOUNCE).await;
                let query = inner.query.read().clone();
                if query.trim().is_empty() {
                    *inner.candidates.write() = Vec::new();
                    *inner.is_searching.write() = false;
                    inner.bus.publish(
                        orchid_core::EventSource::Widget(inner.instance_id),
                        WidgetSnapshotUpdated { instance_id: inner.instance_id },
                    );
                    continue;
                }
                *inner.is_searching.write() = true;
                inner.bus.publish(
                    orchid_core::EventSource::Widget(inner.instance_id),
                    WidgetSnapshotUpdated { instance_id: inner.instance_id },
                );

                let candidates = match inner.aggregator.as_ref() {
                    Some(agg) => agg.query(&query, 10).await,
                    None => Vec::new(),
                };
                *inner.candidates.write() = candidates;
                *inner.is_searching.write() = false;
                inner.bus.publish(
                    orchid_core::EventSource::Widget(inner.instance_id),
                    WidgetSnapshotUpdated { instance_id: inner.instance_id },
                );
            }
        });
        *self.task.lock() = Some(handle);
    }

    fn stop_debouncer(&self) {
        if let Some(h) = self.task.lock().take() {
            h.abort();
        }
    }

    /// Clone a candidate's action target for external dispatch.
    #[must_use]
    pub fn action_target_for(&self, candidate_id: &str) -> Option<ActionTarget> {
        self.inner
            .candidates
            .read()
            .iter()
            .find(|c| c.id == candidate_id)
            .map(|c| c.action_target.clone())
    }
}

#[async_trait]
impl Widget for UniversalSearchWidget {
    fn type_id(&self) -> &'static str {
        TYPE_ID
    }
    fn instance_id(&self) -> Uuid {
        self.inner.instance_id
    }
    async fn on_create(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        Ok(())
    }
    async fn on_activate(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        if self.inner.aggregator.is_none() {
            *self.inner.error.write() = Some("Search sources not yet configured".into());
        }
        self.start_debouncer();
        Ok(())
    }
    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.stop_debouncer();
        Ok(())
    }
    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.stop_debouncer();
        Ok(())
    }
    async fn on_close(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        self.stop_debouncer();
        Ok(())
    }
    async fn on_resize(&mut self, _ctx: &WidgetContext, _size: WidgetSize) -> WidgetResult<()> {
        Ok(())
    }
    fn snapshot(&self) -> Option<WidgetSnapshot> {
        let query = self.inner.query.read().clone();
        let candidates = self.inner.candidates.read().clone();
        let is_searching = *self.inner.is_searching.read();
        let error = self.inner.error.read().clone();
        let views = candidates
            .iter()
            .map(|c| SearchCandidateView {
                id: c.id.clone(),
                source_name: c.source_id.to_string(),
                source_icon: c.icon,
                title: c.title.clone(),
                subtitle: c.subtitle.clone(),
                shortcut_hint: c.action_hint.clone(),
            })
            .collect();
        Some(WidgetSnapshot {
            instance_id: self.inner.instance_id,
            widget_type: TYPE_ID,
            title: "Universal Search".into(),
            status: WidgetStatus::Ready,
            payload: WidgetPayload::UniversalSearch(UniversalSearchPayload {
                query,
                candidates: views,
                is_searching,
                error,
            }),
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
            preferred_size: Some(WidgetSize::Large),
            allows_grouping: false,
            keeps_state_when_unloaded: false,
            has_settings_panel: false,
        }
    }
}

impl Drop for UniversalSearchWidget {
    fn drop(&mut self) {
        self.stop_debouncer();
    }
}

/// Descriptor for the universal-search widget. The supplied `aggregator`
/// (wrapped in `Arc`) is captured by the factory closure, so every instance
/// shares it.
#[must_use]
pub fn descriptor(aggregator: Arc<SearchAggregator>) -> WidgetDescriptor {
    let agg = aggregator;
    let factory: WidgetFactory = Arc::new(move |ctx: WidgetContext, _bytes| {
        Ok(Box::new(UniversalSearchWidget::new(
            ctx.instance_id,
            Some(agg.clone()),
            ctx.bus.clone(),
        )) as Box<dyn Widget>)
    });
    base_descriptor(factory)
}

/// Descriptor without an aggregator — renders the empty state only. Used
/// during bootstrap before the search index is wired up.
#[must_use]
pub fn descriptor_stub() -> WidgetDescriptor {
    let factory: WidgetFactory = Arc::new(|ctx: WidgetContext, _bytes| {
        Ok(Box::new(UniversalSearchWidget::new(
            ctx.instance_id,
            None,
            ctx.bus.clone(),
        )) as Box<dyn Widget>)
    });
    base_descriptor(factory)
}

fn base_descriptor(factory: WidgetFactory) -> WidgetDescriptor {
    WidgetDescriptor {
        type_id: TYPE_ID,
        display_name_key: "widget-search-name",
        description_key: "widget-search-desc",
        icon_name: "search",
        category: WidgetCategory::Productivity,
        default_size: WidgetSize::Large,
        min_size: Some(WidgetSize::Medium),
        max_size: None,
        default_lifecycle: LifecycleState::Active,
        allows_multiple_instances: false,
        factory,
    }
}
