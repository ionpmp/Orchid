//! Integration: create / transition / close an instance and verify lifecycle
//! events fire on the bus.

mod common;

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use orchid_core::{
    Event, EventBus, EventBusConfig, EventFilter, EventSource, HandlerPriority,
};
use orchid_storage::{LifecycleState, StateStore};
use orchid_widgets::{
    CreateWidgetRequest, WidgetLifecycleChanged, WidgetManager, WidgetManagerOptions,
    WidgetRegistry,
};
use parking_lot::{Mutex, RwLock};
use uuid::Uuid;

use common::{register_dummy, DummyCounters};

fn make_manager() -> (WidgetManager, Arc<WidgetRegistry>, Arc<DummyCounters>, Arc<EventBus>) {
    let registry = Arc::new(WidgetRegistry::new());
    let counters = register_dummy(&registry);
    let bus = Arc::new(EventBus::new(EventBusConfig::default()));
    let storage = Arc::new(StateStore::open_in_memory("test").unwrap());
    let config = Arc::new(RwLock::new(orchid_storage::OrchidConfig::default()));
    let manager = WidgetManager::new(
        registry.clone(),
        bus.clone(),
        storage,
        config,
        WidgetManagerOptions::default(),
    );
    (manager, registry, counters, bus)
}

#[tokio::test(flavor = "multi_thread")]
async fn lifecycle_transitions_emit_events_and_invoke_callbacks() {
    let (manager, _registry, counters, bus) = make_manager();

    let captured: Arc<Mutex<Vec<WidgetLifecycleChanged>>> = Arc::new(Mutex::new(Vec::new()));
    let captured_clone = captured.clone();
    let _sub = bus
        .subscribe_sync(
            EventFilter::of_type(WidgetLifecycleChanged::event_type()),
            HandlerPriority::Normal,
            move |env| {
                if let Some(payload) = env.downcast::<WidgetLifecycleChanged>() {
                    captured_clone.lock().push(payload.clone());
                }
            },
        )
        .expect("subscribe");

    let ws = Uuid::new_v4();
    let id = manager
        .create(CreateWidgetRequest {
            type_id: "test.dummy".into(),
            workspace_id: ws,
            position: None,
            size: None,
            initial_lifecycle: None,
            config_bytes: None,
        })
        .await
        .expect("create");

    // on_create + on_activate fired exactly once
    assert_eq!(counters.on_create.load(Ordering::SeqCst), 1);
    assert_eq!(counters.on_activate.load(Ordering::SeqCst), 1);

    // Active → Sleeping → Active.
    manager
        .change_lifecycle(id, LifecycleState::Sleeping)
        .await
        .unwrap();
    manager
        .change_lifecycle(id, LifecycleState::Active)
        .await
        .unwrap();
    manager.close(id).await.unwrap();

    assert_eq!(counters.on_sleep.load(Ordering::SeqCst), 1);
    assert!(counters.on_activate.load(Ordering::SeqCst) >= 2);
    assert_eq!(counters.on_close.load(Ordering::SeqCst), 1);

    // Brief pause to let the bus flush before snapshotting.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let events = captured.lock().clone();
    let saw_sleep = events.iter().any(|e| e.to == LifecycleState::Sleeping);
    let saw_reactivate = events
        .iter()
        .any(|e| e.from == LifecycleState::Sleeping && e.to == LifecycleState::Active);
    assert!(saw_sleep, "expected a Sleeping transition event");
    assert!(saw_reactivate, "expected a Sleeping→Active transition event");
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_lifecycle_transition_rejected() {
    let (manager, _registry, _counters, _bus) = make_manager();
    let ws = Uuid::new_v4();
    let id = manager
        .create(CreateWidgetRequest {
            type_id: "test.dummy".into(),
            workspace_id: ws,
            position: None,
            size: None,
            initial_lifecycle: None,
            config_bytes: None,
        })
        .await
        .unwrap();
    // Active -> Unloaded is not allowed.
    let err = manager
        .change_lifecycle(id, LifecycleState::Unloaded)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        orchid_widgets::WidgetError::InvalidStateForOperation(_)
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_widget_type_rejected() {
    let (manager, _registry, _counters, _bus) = make_manager();
    let err = manager
        .create(CreateWidgetRequest {
            type_id: "nonexistent".into(),
            workspace_id: Uuid::new_v4(),
            position: None,
            size: None,
            initial_lifecycle: None,
            config_bytes: None,
        })
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        orchid_widgets::WidgetError::UnknownWidgetType(_)
    ));
    // Reference EventSource to acknowledge the import in the file.
    let _ = EventSource::Subsystem("tests".into());
}
