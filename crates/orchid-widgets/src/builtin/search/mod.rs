//! Universal search widget.

pub mod aggregator;
pub mod sources;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::{Mutex, RwLock};
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::error::Result as WidgetResult;
use crate::events::WidgetSnapshotUpdated;
use crate::widget::payloads::{SearchCandidateView, UniversalSearchPayload};
use crate::widget::snapshot::{WidgetPayload, WidgetSnapshot, WidgetStatus};
use crate::{Widget, WidgetCapabilities, WidgetCategory, WidgetContext, WidgetDescriptor, WidgetFactory};
use orchid_storage::{LifecycleState, WidgetSize};
use tracing::warn;

pub use aggregator::SearchAggregator;
pub use sources::{ActionTarget, CommandsSource, FilesSource, SearchCandidate, SearchSource, SettingsSource};

/// Live search widget cores keyed by instance id (for UI-side query / activation
/// without holding `Arc<UniversalSearchWidget>`).
static SEARCH_LIVE: LazyLock<DashMap<Uuid, Arc<Inner>>> = LazyLock::new(DashMap::new);

/// Cumulative [`universal_search_push_query`] calls where the instance was not in [`SEARCH_LIVE`].
static SEARCH_LIVE_MISS_COUNT: AtomicU64 = AtomicU64::new(0);
/// Milliseconds since UNIX epoch when we last emitted a rate-limited mismatch warn.
static SEARCH_LIVE_MISS_LAST_WARN_MS: AtomicU64 = AtomicU64::new(0);

const SEARCH_LIVE_MISS_WARN_INTERVAL_MS: u64 = 30_000;
const SEARCH_LIVE_MISS_WARN_EVERY_N: u64 = 100;

fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn record_search_live_miss(instance_id: Uuid) {
    let count = SEARCH_LIVE_MISS_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    let now = now_epoch_ms();
    let last_warn = SEARCH_LIVE_MISS_LAST_WARN_MS.load(Ordering::Relaxed);
    let periodic = count == 1 || count % SEARCH_LIVE_MISS_WARN_EVERY_N == 0;
    let timed = now.saturating_sub(last_warn) >= SEARCH_LIVE_MISS_WARN_INTERVAL_MS;
    if periodic || timed {
        SEARCH_LIVE_MISS_LAST_WARN_MS.store(now, Ordering::Relaxed);
        let suppressed = count.saturating_sub(1) % SEARCH_LIVE_MISS_WARN_EVERY_N;
        warn!(
            target: "orchid_widgets::search",
            %instance_id,
            miss_count = count,
            suppressed_since_last_warn = suppressed,
            "universal_search_push_query: instance not in SEARCH_LIVE (widget closed or not yet activated)"
        );
    }
}

/// Total number of [`universal_search_push_query`] calls for unknown / closed instances.
///
/// Useful when diagnosing UI instance-id mismatches (see `docs/universal-search-issue.md`).
#[must_use]
pub fn universal_search_live_miss_count() -> u64 {
    SEARCH_LIVE_MISS_COUNT.load(Ordering::Relaxed)
}

/// Push a query update into the widget debouncer for `instance_id`.
///
/// Restarts the debouncer when it was stopped (e.g. after close). No-op when
/// the instance is not a live universal-search widget.
pub fn universal_search_push_query(instance_id: Uuid, query: String) {
    if let Some(inner) = SEARCH_LIVE.get(&instance_id) {
        let inner = inner.clone();
        *inner.query.write() = query;
        inner.ensure_debouncer_running();
        inner.notify.notify_one();
    } else {
        record_search_live_miss(instance_id);
    }
}

/// Look up the [`ActionTarget`] for `candidate_id` on `instance_id`.
///
/// Returns `None` if the instance is unknown or the candidate id is stale.
pub fn universal_search_action_target(instance_id: Uuid, candidate_id: &str) -> Option<ActionTarget> {
    SEARCH_LIVE.get(&instance_id).and_then(|inner| {
        inner
            .candidates
            .read()
            .iter()
            .find(|c| c.id == candidate_id)
            .map(|c| c.action_target.clone())
    })
}

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
    /// Background debounce loop; restarted from [`universal_search_push_query`] if it was
    /// stopped (e.g. after `on_sleep`) so typing in the UI still produces hits.
    debouncer_task: Mutex<Option<JoinHandle<()>>>,
}

impl Inner {
    fn ensure_debouncer_running(self: &Arc<Self>) {
        let mut slot = self.debouncer_task.lock();
        if let Some(h) = slot.as_ref() {
            if !h.is_finished() {
                return;
            }
        }
        let inner = Arc::clone(self);
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
        *slot = Some(handle);
    }

    fn stop_debouncer(&self) {
        if let Some(h) = self.debouncer_task.lock().take() {
            h.abort();
        }
    }
}

/// Universal search widget.
pub struct UniversalSearchWidget {
    inner: Arc<Inner>,
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
        let inner = Arc::new(Inner {
            query: RwLock::new(String::new()),
            candidates: RwLock::new(Vec::new()),
            is_searching: RwLock::new(false),
            error: RwLock::new(None),
            notify: Notify::new(),
            aggregator,
            bus,
            instance_id,
            debouncer_task: Mutex::new(None),
        });
        SEARCH_LIVE.insert(instance_id, inner.clone());
        Self { inner }
    }

    /// Update the query. Debounces for [`DEBOUNCE`] before running a real
    /// search.
    pub fn update_query(&self, query: String) {
        *self.inner.query.write() = query;
        self.inner.ensure_debouncer_running();
        self.inner.notify.notify_one();
    }

    fn start_debouncer(&self) {
        self.inner.ensure_debouncer_running();
    }

    fn stop_debouncer(&self) {
        self.inner.stop_debouncer();
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
        } else {
            *self.inner.error.write() = None;
        }
        self.start_debouncer();
        Ok(())
    }
    async fn on_sleep(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
        // Keep the debouncer alive while the widget is visible but idle. Stopping
        // it here caused missed results when the UI pushed queries without a
        // matching `on_activate` (see docs/universal-search-issue.md).
        Ok(())
    }
    async fn on_unload(&mut self, _ctx: &WidgetContext) -> WidgetResult<()> {
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
        SEARCH_LIVE.remove(&self.inner.instance_id);
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

#[cfg(test)]
mod live_tests {
    use std::time::Duration;

    use super::*;
    use async_trait::async_trait;
    use parking_lot::RwLock;
    use orchid_core::{EventBus, EventBusConfig};
    use orchid_storage::StateStore;

    use crate::Widget;
    use crate::widget::snapshot::WidgetPayload;
    use sources::{ActionTarget, SearchCandidate, SearchSource};

    /// Deterministic source so the debouncer path can be asserted without Tantivy.
    struct EchoSource;

    #[async_trait]
    impl SearchSource for EchoSource {
        fn id(&self) -> &'static str {
            "echo"
        }
        fn name_key(&self) -> &'static str {
            "echo"
        }
        fn icon(&self) -> &'static str {
            "x"
        }
        async fn search(&self, query: &str, _limit: usize) -> Vec<SearchCandidate> {
            if query.trim().is_empty() {
                return Vec::new();
            }
            vec![SearchCandidate {
                id: format!("echo:{query}"),
                source_id: "echo",
                title: format!("hit:{query}"),
                subtitle: None,
                icon: "x",
                score: 10,
                action_hint: None,
                action_target: ActionTarget::RunCommand("noop".into()),
            }]
        }
    }

    fn test_ctx(instance_id: Uuid, bus: Arc<orchid_core::EventBus>) -> WidgetContext {
        WidgetContext {
            bus,
            storage: Arc::new(StateStore::open_in_memory("0.0-test").expect("in-memory store")),
            config: Arc::new(RwLock::new(orchid_storage::OrchidConfig::default())),
            instance_id,
            workspace_id: Uuid::new_v4(),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn push_query_after_sleep_still_populates_candidates() {
        let bus = Arc::new(EventBus::new(EventBusConfig::default()));
        let id = Uuid::new_v4();
        let agg = Arc::new(SearchAggregator::new(vec![Arc::new(EchoSource) as Arc<dyn SearchSource>]));
        let mut w = UniversalSearchWidget::new(id, Some(agg), bus.clone());
        let ctx = test_ctx(id, bus);
        w.on_activate(&ctx).await.expect("activate");
        w.on_sleep(&ctx).await.expect("sleep");

        universal_search_push_query(id, "hello".into());
        tokio::time::sleep(Duration::from_millis(300)).await;

        let snap = w.snapshot().expect("snapshot");
        let p = match snap.payload {
            WidgetPayload::UniversalSearch(p) => p,
            _ => panic!("expected UniversalSearch payload"),
        };
        assert_eq!(p.query, "hello");
        assert_eq!(p.candidates.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn push_query_after_debouncer_stopped_still_populates_candidates() {
        let bus = Arc::new(EventBus::new(EventBusConfig::default()));
        let id = Uuid::new_v4();
        let agg = Arc::new(SearchAggregator::new(vec![Arc::new(EchoSource) as Arc<dyn SearchSource>]));
        let mut w = UniversalSearchWidget::new(id, Some(agg), bus.clone());
        let ctx = test_ctx(id, bus);
        w.on_activate(&ctx).await.expect("activate");
        w.stop_debouncer();

        universal_search_push_query(id, "hello".into());
        tokio::time::sleep(Duration::from_millis(300)).await;

        let snap = w.snapshot().expect("snapshot");
        let p = match snap.payload {
            WidgetPayload::UniversalSearch(p) => p,
            _ => panic!("expected UniversalSearch payload"),
        };
        assert_eq!(p.query, "hello");
        assert_eq!(p.candidates.len(), 1);
        assert_eq!(p.candidates[0].title, "hit:hello");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn update_query_triggers_debounced_search() {
        let bus = Arc::new(EventBus::new(EventBusConfig::default()));
        let id = Uuid::new_v4();
        let agg = Arc::new(SearchAggregator::new(vec![Arc::new(EchoSource) as Arc<dyn SearchSource>]));
        let mut w = UniversalSearchWidget::new(id, Some(agg), bus.clone());
        let ctx = test_ctx(id, bus);
        w.on_activate(&ctx).await.expect("activate");

        w.update_query("abc".into());
        tokio::time::sleep(Duration::from_millis(300)).await;

        let snap = w.snapshot().expect("snapshot");
        let p = match snap.payload {
            WidgetPayload::UniversalSearch(p) => p,
            _ => panic!("expected UniversalSearch payload"),
        };
        assert_eq!(p.query, "abc");
        assert_eq!(p.candidates.len(), 1);
        assert_eq!(p.candidates[0].title, "hit:abc");
    }
}
