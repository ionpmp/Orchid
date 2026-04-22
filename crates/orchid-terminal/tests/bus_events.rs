//! Open a session, observe the bus events, close it.

#![cfg(windows)]

use std::sync::Arc;
use std::time::Duration;

use orchid_terminal::{BackendSpec, PtySize, SessionManager};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_lifecycle_events_fire() {
    let bus = Arc::new(orchid_core::EventBus::new(orchid_core::EventBusConfig::default()));
    let storage = Arc::new(orchid_storage::StateStore::open_in_memory("0").unwrap());

    let (_opened_handle, mut opened_rx) = bus
        .subscribe(
            orchid_core::EventFilter::of_type("terminal.opened"),
            orchid_core::HandlerPriority::Normal,
        )
        .unwrap();
    let (_closed_handle, mut closed_rx) = bus
        .subscribe(
            orchid_core::EventFilter::of_type("terminal.closed"),
            orchid_core::HandlerPriority::Normal,
        )
        .unwrap();

    let manager = SessionManager::new(Arc::clone(&bus), storage);
    let id = manager
        .open(
            BackendSpec::cmd().with_initial_command("exit"),
            PtySize::default_80x24(),
        )
        .await
        .unwrap();

    // Observe `terminal.opened`.
    let opened = tokio::time::timeout(Duration::from_secs(3), opened_rx.recv())
        .await
        .expect("no terminal.opened event")
        .unwrap();
    assert_eq!(opened.event_type, "terminal.opened");

    // Observe `terminal.closed` (emitted by the reader task on child exit
    // *or* by our explicit close below).
    let closed_future = tokio::time::timeout(Duration::from_secs(10), closed_rx.recv());
    let close_future = manager.close(id);
    let (closed_result, close_result) = tokio::join!(closed_future, close_future);
    close_result.unwrap();
    let closed = closed_result.unwrap().unwrap();
    assert_eq!(closed.event_type, "terminal.closed");
}
