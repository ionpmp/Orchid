//! Integration: workspace switch emits the right `WorkspaceSwitched` event.

mod common;

use std::sync::Arc;
use std::time::Duration;

use orchid_core::{Event, EventBus, EventBusConfig, EventFilter, HandlerPriority};
use orchid_storage::StateStore;
use orchid_widgets::{WorkspaceManager, WorkspaceSwitched};
use parking_lot::Mutex;

#[tokio::test(flavor = "multi_thread")]
async fn workspace_switch_emits_event_with_ids() {
    let bus = Arc::new(EventBus::new(EventBusConfig::default()));
    let storage = Arc::new(StateStore::open_in_memory("test").unwrap());
    let mgr = WorkspaceManager::new(bus.clone(), storage);

    let captured: Arc<Mutex<Vec<WorkspaceSwitched>>> = Arc::new(Mutex::new(Vec::new()));
    let captured_clone = captured.clone();
    let _sub = bus
        .subscribe_sync(
            EventFilter::of_type(WorkspaceSwitched::event_type()),
            HandlerPriority::Normal,
            move |env| {
                if let Some(ev) = env.downcast::<WorkspaceSwitched>() {
                    captured_clone.lock().push(ev.clone());
                }
            },
        )
        .unwrap();

    let a = mgr.create("A".into()).await.unwrap();
    let b = mgr.create("B".into()).await.unwrap();
    mgr.switch_to(b).await.unwrap();

    tokio::time::sleep(Duration::from_millis(40)).await;
    let events = captured.lock().clone();
    assert!(
        events.iter().any(|e| e.from == Some(a) && e.to == b),
        "missing workspace-switched event (got {events:?})"
    );
}
