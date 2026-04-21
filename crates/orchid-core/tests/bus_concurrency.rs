//! Concurrent publish / subscribe stress test.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use orchid_core::{EventBus, EventBusConfig, EventFilter, EventSource, HandlerPriority};

#[derive(Debug, Clone)]
struct Beat(#[allow(dead_code)] u32);

impl orchid_core::Event for Beat {
    fn event_type() -> &'static str {
        "test.beat"
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn many_publishers_and_subscribers_deliver_cleanly() {
    // Larger buffer so the slow-consumer policy doesn't kick in and we can
    // reason about delivered counts precisely.
    let bus = Arc::new(EventBus::new(EventBusConfig {
        per_subscriber_buffer: 4096,
        ..EventBusConfig::default()
    }));

    const SUBSCRIBERS: usize = 10;
    const PUBLISHERS: usize = 10;
    const EVENTS_PER_PUBLISHER: usize = 1_000;

    let counters: Vec<Arc<AtomicUsize>> =
        (0..SUBSCRIBERS).map(|_| Arc::new(AtomicUsize::new(0))).collect();

    for counter in &counters {
        let c = Arc::clone(counter);
        bus.subscribe_async(
            EventFilter::of_type("test.beat"),
            HandlerPriority::Normal,
            move |_env| {
                let c = Arc::clone(&c);
                async move {
                    c.fetch_add(1, Ordering::Relaxed);
                }
            },
        )
        .unwrap()
        .leak();
    }

    let mut pubs = Vec::new();
    for _p in 0..PUBLISHERS {
        let bus = Arc::clone(&bus);
        pubs.push(tokio::spawn(async move {
            for i in 0..EVENTS_PER_PUBLISHER {
                bus.publish(EventSource::System, Beat(i as u32));
            }
        }));
    }
    for h in pubs {
        h.await.unwrap();
    }

    // Give the async handlers a moment to drain. We loop with a deadline
    // rather than sleep a fixed time to keep the test fast on hot machines.
    let expected = PUBLISHERS * EVENTS_PER_PUBLISHER;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let min = counters
            .iter()
            .map(|c| c.load(Ordering::Relaxed))
            .min()
            .unwrap();
        if min >= expected {
            break;
        }
        if std::time::Instant::now() >= deadline {
            panic!(
                "did not reach {expected} per subscriber, smallest = {min}"
            );
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    for counter in &counters {
        assert_eq!(counter.load(Ordering::Relaxed), expected);
    }

    let metrics = bus.metrics();
    assert_eq!(metrics.total_published as usize, expected);
    assert_eq!(metrics.active_subscriptions, SUBSCRIBERS);
}
