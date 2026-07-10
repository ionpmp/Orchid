//! Concurrent, priority-ordered event bus.
//!
//! See the module-level documentation in [`crate::event`] for an overview.

use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tokio::task::JoinHandle;
use tracing::warn;

use crate::error::{CoreError, Result};
use crate::event::channel::{ChannelInner, ChannelSender, EventReceiver, PushError};
use crate::event::event::{Event, EventEnvelope, EventSource};
use crate::event::priority::HandlerPriority;
use crate::event::subscription::{EventFilter, SubscriptionHandle, SubscriptionId};

/// Policy applied when a subscriber's channel is full.
#[derive(Debug, Default, Clone, Copy)]
pub enum SlowConsumerPolicy {
    /// Evict the oldest pending event and push the new one. The default.
    #[default]
    DropOldest,
    /// Discard the new event.
    DropNewest,
    /// Block for up to `timeout_ms` milliseconds; if still full, drop.
    Block {
        /// Timeout in milliseconds before giving up and dropping the event.
        timeout_ms: u32,
    },
}

/// Tunables that control [`EventBus`] behaviour.
#[derive(Debug, Clone)]
pub struct EventBusConfig {
    /// Buffered capacity for each channel subscriber.
    pub per_subscriber_buffer: usize,
    /// What to do when a channel subscriber falls behind.
    pub slow_consumer_policy: SlowConsumerPolicy,
}

impl Default for EventBusConfig {
    fn default() -> Self {
        Self {
            per_subscriber_buffer: 256,
            slow_consumer_policy: SlowConsumerPolicy::DropOldest,
        }
    }
}

/// Snapshot of bus counters returned by [`EventBus::metrics`].
#[derive(Debug, Clone, Default)]
pub struct EventBusMetrics {
    /// Total events ever accepted by [`EventBus::publish`].
    pub total_published: u64,
    /// Total envelope deliveries dispatched to any handler (sum across subs).
    pub total_delivered: u64,
    /// Total deliveries discarded because of full channels.
    pub total_dropped: u64,
    /// Number of currently-live subscriptions.
    pub active_subscriptions: usize,
}

/// Handler flavour stored inside a subscription entry.
#[allow(clippy::type_complexity)]
enum HandlerKind {
    /// Channel-backed subscriber; events are pushed into a drop-capable queue.
    Channel(ChannelSender),
    /// Async closure — called via `tokio::spawn`.
    Async(Arc<dyn Fn(EventEnvelope) -> futures_core::BoxFuture + Send + Sync>),
    /// Synchronous closure — called inline in the dispatcher.
    Sync(Arc<dyn Fn(&EventEnvelope) + Send + Sync>),
}

mod futures_core {
    //! Minimal boxed-future alias used on the hot path without pulling the
    //! whole `futures` crate into the dependency graph.
    use std::future::Future;
    use std::pin::Pin;

    pub type BoxFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
}

struct SubscriptionEntry {
    priority: HandlerPriority,
    seq: u64,
    filter: EventFilter,
    handler: HandlerKind,
}

/// Shared inner state of the bus. Held by `EventBus` through an `Arc` so the
/// many weak references handed out by `SubscriptionHandle` can outlive it
/// briefly during teardown.
pub(crate) struct EventBusInner {
    config: EventBusConfig,
    subs: DashMap<SubscriptionId, SubscriptionEntry>,
    next_seq: AtomicU64,
    shutdown: AtomicBool,
    in_flight: AtomicUsize,
    // Metrics
    published: AtomicU64,
    delivered: AtomicU64,
    dropped: AtomicU64,
}

impl EventBusInner {
    fn new(config: EventBusConfig) -> Self {
        Self {
            config,
            subs: DashMap::new(),
            next_seq: AtomicU64::new(0),
            shutdown: AtomicBool::new(false),
            in_flight: AtomicUsize::new(0),
            published: AtomicU64::new(0),
            delivered: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
        }
    }

    pub(crate) fn remove_subscription(&self, id: SubscriptionId) {
        self.subs.remove(&id);
    }
}

/// Multi-producer, multi-consumer event bus with priority-ordered delivery.
///
/// `EventBus` is cheap to clone; all clones share the same subscription set.
/// See [`EventBus::publish`], [`EventBus::subscribe_async`], etc.
#[derive(Clone)]
pub struct EventBus {
    inner: Arc<EventBusInner>,
}

impl std::fmt::Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBus")
            .field("active_subscriptions", &self.inner.subs.len())
            .field("is_shutdown", &self.is_shutdown())
            .finish()
    }
}

impl EventBus {
    /// Build a new bus with the given configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_core::{EventBus, EventBusConfig};
    /// let bus = EventBus::new(EventBusConfig::default());
    /// assert!(!bus.is_shutdown());
    /// ```
    #[must_use]
    pub fn new(config: EventBusConfig) -> Self {
        Self {
            inner: Arc::new(EventBusInner::new(config)),
        }
    }

    /// Returns `true` after [`EventBus::shutdown`] has been called.
    #[must_use]
    pub fn is_shutdown(&self) -> bool {
        self.inner.shutdown.load(Ordering::SeqCst)
    }

    /// Live metrics snapshot.
    #[must_use]
    pub fn metrics(&self) -> EventBusMetrics {
        EventBusMetrics {
            total_published: self.inner.published.load(Ordering::Relaxed),
            total_delivered: self.inner.delivered.load(Ordering::Relaxed),
            total_dropped: self.inner.dropped.load(Ordering::Relaxed),
            active_subscriptions: self.inner.subs.len(),
        }
    }

    /// Publish an event. Returns the number of subscribers the envelope
    /// matched. Delivery to async and channel subscribers is not awaited;
    /// see [`EventBus::publish_and_flush`] for a blocking alternative.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_core::{EventBus, EventBusConfig, EventSource};
    /// use orchid_core::event::AppStarted;
    ///
    /// let bus = EventBus::new(EventBusConfig::default());
    /// let _ = bus.publish(EventSource::System, AppStarted { version: "0".into() });
    /// ```
    pub fn publish<E: Event>(&self, source: EventSource, event: E) -> usize {
        if self.inner.shutdown.load(Ordering::SeqCst) {
            return 0;
        }
        let envelope = EventEnvelope::new(source, event);
        let (count, handles) = self.dispatch_internal(&envelope);
        // Detach async handlers -- they complete in the background.
        for h in handles {
            drop(h);
        }
        count
    }

    /// Publish and await every async / channel delivery triggered by this
    /// event.
    ///
    /// For channel subscribers this waits for the envelope to reach the
    /// channel (not for the consumer to drain it). For async subscribers it
    /// waits for the spawned future to complete.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::BusShutdown`] if [`EventBus::shutdown`] was
    /// already called.
    pub async fn publish_and_flush<E: Event>(
        &self,
        source: EventSource,
        event: E,
    ) -> Result<usize> {
        if self.inner.shutdown.load(Ordering::SeqCst) {
            return Err(CoreError::BusShutdown);
        }
        let envelope = EventEnvelope::new(source, event);
        let (count, handles) = self.dispatch_internal(&envelope);
        for h in handles {
            let _ = h.await;
        }
        Ok(count)
    }

    /// Subscribe with a channel. The returned receiver yields every event
    /// passing `filter`, in priority order relative to other subscribers.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::BusShutdown`] if the bus has been shut down.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_core::{EventBus, EventBusConfig, EventFilter, HandlerPriority};
    ///
    /// let bus = EventBus::new(EventBusConfig::default());
    /// let (_handle, _rx) = bus
    ///     .subscribe(EventFilter::any(), HandlerPriority::Normal)
    ///     .unwrap();
    /// ```
    pub fn subscribe(
        &self,
        filter: EventFilter,
        priority: HandlerPriority,
    ) -> Result<(SubscriptionHandle, EventReceiver)> {
        if self.inner.shutdown.load(Ordering::SeqCst) {
            return Err(CoreError::BusShutdown);
        }
        let shared = ChannelInner::new(self.inner.config.per_subscriber_buffer);
        let tx = ChannelSender::new(Arc::clone(&shared));
        let rx = EventReceiver::new(shared);
        let id = SubscriptionId::new();
        let seq = self.inner.next_seq.fetch_add(1, Ordering::Relaxed);
        self.inner.subs.insert(
            id,
            SubscriptionEntry {
                priority,
                seq,
                filter,
                handler: HandlerKind::Channel(tx),
            },
        );
        let handle = SubscriptionHandle {
            id,
            bus: Arc::downgrade(&self.inner),
            leaked: false,
        };
        Ok((handle, rx))
    }

    /// Subscribe with an async closure handler.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::BusShutdown`] if the bus has been shut down.
    pub fn subscribe_async<F, Fut>(
        &self,
        filter: EventFilter,
        priority: HandlerPriority,
        handler: F,
    ) -> Result<SubscriptionHandle>
    where
        F: Fn(EventEnvelope) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        if self.inner.shutdown.load(Ordering::SeqCst) {
            return Err(CoreError::BusShutdown);
        }
        let id = SubscriptionId::new();
        let seq = self.inner.next_seq.fetch_add(1, Ordering::Relaxed);
        let wrapped: Arc<
            dyn Fn(EventEnvelope) -> futures_core::BoxFuture + Send + Sync,
        > = Arc::new(move |env| {
            let fut = handler(env);
            Box::pin(fut) as futures_core::BoxFuture
        });
        self.inner.subs.insert(
            id,
            SubscriptionEntry {
                priority,
                seq,
                filter,
                handler: HandlerKind::Async(wrapped),
            },
        );
        Ok(SubscriptionHandle {
            id,
            bus: Arc::downgrade(&self.inner),
            leaked: false,
        })
    }

    /// Subscribe with a synchronous closure called inline in the dispatcher.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::BusShutdown`] if the bus has been shut down.
    pub fn subscribe_sync<F>(
        &self,
        filter: EventFilter,
        priority: HandlerPriority,
        handler: F,
    ) -> Result<SubscriptionHandle>
    where
        F: Fn(&EventEnvelope) + Send + Sync + 'static,
    {
        if self.inner.shutdown.load(Ordering::SeqCst) {
            return Err(CoreError::BusShutdown);
        }
        let id = SubscriptionId::new();
        let seq = self.inner.next_seq.fetch_add(1, Ordering::Relaxed);
        self.inner.subs.insert(
            id,
            SubscriptionEntry {
                priority,
                seq,
                filter,
                handler: HandlerKind::Sync(Arc::new(handler)),
            },
        );
        Ok(SubscriptionHandle {
            id,
            bus: Arc::downgrade(&self.inner),
            leaked: false,
        })
    }

    /// Remove a subscription by id. Returns `true` if a subscription was
    /// actually removed.
    pub fn unsubscribe(&self, id: SubscriptionId) -> bool {
        self.inner.subs.remove(&id).is_some()
    }

    /// Shut the bus down, rejecting future publishes and waiting for any
    /// already-spawned async handlers to complete.
    ///
    /// # Errors
    ///
    /// Never returns an error in the current implementation; the signature
    /// leaves room for future failure modes (e.g. draining with a timeout).
    pub async fn shutdown(self) -> Result<()> {
        self.inner.shutdown.store(true, Ordering::SeqCst);
        // Poll for in-flight handlers. 10 ms is well below human perception
        // and small enough that shutdown completes promptly once all handlers
        // finish.
        while self.inner.in_flight.load(Ordering::SeqCst) > 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        // Clear subscription set so channel senders are dropped and any
        // receivers observe `None` on their next `recv()`.
        self.inner.subs.clear();
        Ok(())
    }

    // ------------------------------------------------------------------
    // Internal dispatch
    // ------------------------------------------------------------------

    fn dispatch_internal(
        &self,
        envelope: &EventEnvelope,
    ) -> (usize, Vec<JoinHandle<()>>) {
        self.inner.published.fetch_add(1, Ordering::Relaxed);

        // Snapshot matching entries and sort by (priority rank, registration
        // sequence) for deterministic ordering.
        let mut matching: Vec<(HandlerPriority, u64, SubscriptionId)> = self
            .inner
            .subs
            .iter()
            .filter_map(|e| {
                if e.value().filter.matches(envelope) {
                    Some((e.value().priority, e.value().seq, *e.key()))
                } else {
                    None
                }
            })
            .collect();
        matching.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

        let mut handles = Vec::new();
        let mut count = 0_usize;

        for (_, _, id) in matching {
            let Some(entry) = self.inner.subs.get(&id) else {
                continue;
            };
            count += 1;
            match &entry.handler {
                HandlerKind::Sync(f) => {
                    f(envelope);
                    self.inner.delivered.fetch_add(1, Ordering::Relaxed);
                }
                HandlerKind::Async(f) => {
                    let fut = f(envelope.clone());
                    let inner = Arc::clone(&self.inner);
                    inner.in_flight.fetch_add(1, Ordering::SeqCst);
                    let handle = tokio::spawn(async move {
                        fut.await;
                        inner.delivered.fetch_add(1, Ordering::Relaxed);
                        inner.in_flight.fetch_sub(1, Ordering::SeqCst);
                    });
                    handles.push(handle);
                }
                HandlerKind::Channel(tx) => {
                    if self.dispatch_to_channel(tx, envelope) {
                        self.inner.delivered.fetch_add(1, Ordering::Relaxed);
                    } else {
                        self.inner.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }

        (count, handles)
    }

    /// Push `envelope` into a channel applying the configured slow-consumer
    /// policy. Returns `true` when the new envelope is queued.
    fn dispatch_to_channel(&self, tx: &ChannelSender, envelope: &EventEnvelope) -> bool {
        match self.inner.config.slow_consumer_policy {
            SlowConsumerPolicy::DropOldest => match tx.push_drop_oldest(envelope.clone()) {
                Ok(evicted) => {
                    if evicted {
                        self.inner.dropped.fetch_add(1, Ordering::Relaxed);
                        warn!(
                            event_type = envelope.event_type,
                            "slow consumer: channel full, dropped oldest event"
                        );
                    }
                    true
                }
                Err(()) => false,
            },
            SlowConsumerPolicy::DropNewest => match tx.try_push_drop_newest(envelope.clone()) {
                Ok(()) => true,
                Err(PushError::Full) => {
                    warn!(
                        event_type = envelope.event_type,
                        "slow consumer: channel full, dropping new event"
                    );
                    false
                }
                Err(PushError::Closed) => false,
            },
            SlowConsumerPolicy::Block { timeout_ms } => {
                // From a sync dispatch we busy-wait with short sleeps;
                // acceptable for the occasional contended publish.
                match tx.push_block(envelope.clone(), Duration::from_millis(timeout_ms as u64)) {
                    Ok(()) => true,
                    Err(PushError::Closed) => false,
                    Err(PushError::Full) => {
                        warn!(
                            event_type = envelope.event_type,
                            "slow consumer: block timeout exceeded, dropping"
                        );
                        false
                    }
                }
            }
        }
    }
}

