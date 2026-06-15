//! Event bus: the central delivery mechanism for Orchid.
//!
//! Three concepts make up this module:
//!
//! * **[`Event`]** — a marker trait implemented by every in-flight message,
//!   carrying a stable string type id for filtering and diagnostics.
//! * **[`EventEnvelope`]** — a type-erased wrapper the bus actually
//!   transports, assigning a uuid, timestamp, and [`EventSource`].
//! * **[`EventBus`]** — a priority-ordered, multi-producer, multi-consumer
//!   dispatcher that routes envelopes to matching subscribers.
//!
//! Subscribers choose one of three styles:
//!
//! | style | API | runs where |
//! |---|---|---|
//! | channel | [`EventBus::subscribe`] | consumer polls `mpsc::Receiver` |
//! | async closure | [`EventBus::subscribe_async`] | spawned on `tokio` |
//! | sync closure | [`EventBus::subscribe_sync`] | inline in the dispatcher |
//!
//! Within the same [`HandlerPriority`] tier subscribers fire in registration
//! order (FIFO).

pub mod bus;
#[allow(clippy::module_inception)]
pub mod event;
pub mod priority;
pub mod subscription;

pub use bus::{EventBus, EventBusConfig, EventBusMetrics, SlowConsumerPolicy};
pub use event::{AppShuttingDown, AppStarted, ConfigUpdated, Event, EventEnvelope, EventSource};
pub use priority::HandlerPriority;
pub use subscription::{EventFilter, SubscriptionHandle, SubscriptionId};

#[cfg(test)]
mod tests {
    //! High-level bus behaviour tests. Lower-level property tests live on
    //! each submodule.
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;
    use crate::event::event::AppStarted;

    fn bus() -> EventBus {
        EventBus::new(EventBusConfig::default())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn subscribe_and_publish_to_matching_filter() {
        let bus = bus();
        let (_handle, mut rx) = bus
            .subscribe(EventFilter::of_type("app.started"), HandlerPriority::Normal)
            .unwrap();
        let matched = bus.publish(
            EventSource::System,
            AppStarted {
                version: "1".into(),
            },
        );
        assert_eq!(matched, 1);

        let env = tokio::time::timeout(Duration::from_millis(200), rx.recv())
            .await
            .expect("should receive")
            .expect("channel open");
        assert_eq!(env.event_type, "app.started");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn filter_mismatch_does_not_deliver() {
        let bus = bus();
        let (_h, mut rx) = bus
            .subscribe(EventFilter::of_type("fs.moved"), HandlerPriority::Normal)
            .unwrap();
        bus.publish(EventSource::System, AppStarted { version: "x".into() });
        let r = tokio::time::timeout(Duration::from_millis(50), rx.recv()).await;
        assert!(r.is_err(), "should time out with no deliveries");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn priority_order_is_respected() {
        let bus = bus();
        let order: Arc<parking_lot::Mutex<Vec<&'static str>>> =
            Arc::new(parking_lot::Mutex::new(Vec::new()));

        let o1 = Arc::clone(&order);
        let _h1 = bus
            .subscribe_sync(EventFilter::any(), HandlerPriority::Audit, move |_| {
                o1.lock().push("audit");
            })
            .unwrap();
        let o2 = Arc::clone(&order);
        let _h2 = bus
            .subscribe_sync(EventFilter::any(), HandlerPriority::Critical, move |_| {
                o2.lock().push("critical");
            })
            .unwrap();
        let o3 = Arc::clone(&order);
        let _h3 = bus
            .subscribe_sync(EventFilter::any(), HandlerPriority::Normal, move |_| {
                o3.lock().push("normal");
            })
            .unwrap();

        bus.publish(EventSource::System, AppStarted { version: "1".into() });
        assert_eq!(order.lock().clone(), vec!["critical", "normal", "audit"]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn same_priority_fires_in_fifo_order() {
        let bus = bus();
        let order: Arc<parking_lot::Mutex<Vec<u8>>> =
            Arc::new(parking_lot::Mutex::new(Vec::new()));
        for i in 0..5_u8 {
            let o = Arc::clone(&order);
            bus.subscribe_sync(EventFilter::any(), HandlerPriority::Normal, move |_| {
                o.lock().push(i);
            })
            .unwrap()
            .leak();
        }
        bus.publish(EventSource::System, AppStarted { version: "1".into() });
        assert_eq!(order.lock().clone(), vec![0, 1, 2, 3, 4]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn dropping_handle_unsubscribes() {
        let bus = bus();
        {
            let counter = Arc::new(AtomicU32::new(0));
            let c = Arc::clone(&counter);
            let _h = bus
                .subscribe_sync(EventFilter::any(), HandlerPriority::Normal, move |_| {
                    c.fetch_add(1, Ordering::Relaxed);
                })
                .unwrap();
            bus.publish(EventSource::System, AppStarted { version: "1".into() });
            assert_eq!(counter.load(Ordering::Relaxed), 1);
        }
        bus.publish(EventSource::System, AppStarted { version: "2".into() });
        // No subscriber should remain; metrics record the publish but nothing
        // is delivered.
        assert_eq!(bus.metrics().active_subscriptions, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn leak_keeps_subscription_alive() {
        let bus = bus();
        let counter = Arc::new(AtomicU32::new(0));
        let c = Arc::clone(&counter);
        {
            let h = bus
                .subscribe_sync(EventFilter::any(), HandlerPriority::Normal, move |_| {
                    c.fetch_add(1, Ordering::Relaxed);
                })
                .unwrap();
            h.leak();
        }
        bus.publish(EventSource::System, AppStarted { version: "x".into() });
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn async_handler_is_awaited_by_publish_and_flush() {
        let bus = bus();
        let counter = Arc::new(AtomicU32::new(0));
        let c = Arc::clone(&counter);
        let _h = bus
            .subscribe_async(EventFilter::any(), HandlerPriority::Normal, move |_env| {
                let c = Arc::clone(&c);
                async move {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                    c.fetch_add(1, Ordering::Relaxed);
                }
            })
            .unwrap();
        bus.publish_and_flush(EventSource::System, AppStarted { version: "1".into() })
            .await
            .unwrap();
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_blocks_further_publish() {
        let bus = bus();
        assert!(!bus.is_shutdown());
        bus.clone().shutdown().await.unwrap();
        // second shutdown-capable clone was consumed; our cloned handle is
        // still valid and reflects the state.
        assert!(bus.is_shutdown());
        assert_eq!(
            bus.publish(EventSource::System, AppStarted { version: "x".into() }),
            0
        );
        let err = bus
            .publish_and_flush(EventSource::System, AppStarted { version: "x".into() })
            .await
            .unwrap_err();
        assert!(matches!(err, crate::CoreError::BusShutdown));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn slow_consumer_drop_oldest_records_drops() {
        let bus = EventBus::new(EventBusConfig {
            per_subscriber_buffer: 2,
            slow_consumer_policy: SlowConsumerPolicy::DropOldest,
        });
        let (_h, _rx) = bus
            .subscribe(EventFilter::any(), HandlerPriority::Normal)
            .unwrap();
        for i in 0..5 {
            bus.publish(
                EventSource::System,
                AppStarted {
                    version: i.to_string(),
                },
            );
        }
        let m = bus.metrics();
        assert!(m.total_dropped > 0, "expected some drops, got {m:?}");
    }
}
