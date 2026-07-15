//! Visibility-driven lifecycle: Active ↔ Sleeping follows canvas presence.

mod common;

use common::{register_dummy, test_locale};

use std::sync::Arc;

use orchid_core::{EventBus, EventBusConfig};
use orchid_storage::{GridPosition, LifecycleState, StateStore, WidgetSize};
use orchid_widgets::{
    visible_instance_ids, CreateWidgetRequest, GroupManager, WidgetManager, WidgetManagerOptions,
    WidgetRegistry, WorkspaceManager,
};
use parking_lot::RwLock;
use uuid::Uuid;

fn make_stack() -> (WidgetManager, WorkspaceManager, GroupManager) {
    let registry = Arc::new(WidgetRegistry::new());
    register_dummy(&registry);
    let bus = Arc::new(EventBus::new(EventBusConfig::default()));
    let storage = Arc::new(StateStore::open_in_memory("vis-test").unwrap());
    let config = Arc::new(RwLock::new(orchid_storage::OrchidConfig::default()));
    let jobs = Arc::new(orchid_core::BackgroundJobQueue::new());
    let wm = WidgetManager::new(
        registry,
        bus.clone(),
        storage.clone(),
        config,
        test_locale(),
        jobs,
        WidgetManagerOptions::default(),
    );
    let wsm = WorkspaceManager::new(bus.clone(), storage.clone());
    let gm = GroupManager::new(bus, storage);
    (wm, wsm, gm)
}

#[tokio::test(flavor = "multi_thread")]
async fn apply_visibility_sleeps_hidden_and_wakes_visible() {
    let (wm, _wsm, _gm) = make_stack();
    let ws = Uuid::new_v4();
    let a = wm
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
    let b = wm
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

    assert_eq!(
        *wm.get_instance(a).unwrap().lifecycle.read(),
        LifecycleState::Active
    );
    assert_eq!(
        *wm.get_instance(b).unwrap().lifecycle.read(),
        LifecycleState::Active
    );

    wm.apply_visibility(&[a]).await;
    assert_eq!(
        *wm.get_instance(a).unwrap().lifecycle.read(),
        LifecycleState::Active
    );
    assert_eq!(
        *wm.get_instance(b).unwrap().lifecycle.read(),
        LifecycleState::Sleeping
    );

    wm.apply_visibility(&[b]).await;
    assert_eq!(
        *wm.get_instance(a).unwrap().lifecycle.read(),
        LifecycleState::Sleeping
    );
    assert_eq!(
        *wm.get_instance(b).unwrap().lifecycle.read(),
        LifecycleState::Active
    );

    wm.apply_visibility(&[a, b]).await;
    assert_eq!(
        *wm.get_instance(a).unwrap().lifecycle.read(),
        LifecycleState::Active
    );
    assert_eq!(
        *wm.get_instance(b).unwrap().lifecycle.read(),
        LifecycleState::Active
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn visible_instance_ids_respects_workspace_and_group_tab() {
    let (wm, wsm, gm) = make_stack();
    let ws_a = wsm.create("A".into()).await.unwrap();
    let ws_b = wsm.create("B".into()).await.unwrap();
    wsm.switch_to(ws_a).await.unwrap();

    let on_a = wm
        .create(CreateWidgetRequest {
            type_id: "test.dummy".into(),
            workspace_id: ws_a,
            position: Some(GridPosition { col: 0, row: 0 }),
            size: Some(WidgetSize::Medium),
            initial_lifecycle: None,
            config_bytes: None,
        })
        .await
        .unwrap();
    let on_b = wm
        .create(CreateWidgetRequest {
            type_id: "test.dummy".into(),
            workspace_id: ws_b,
            position: Some(GridPosition { col: 0, row: 0 }),
            size: Some(WidgetSize::Medium),
            initial_lifecycle: None,
            config_bytes: None,
        })
        .await
        .unwrap();

    let ids = visible_instance_ids(&wm, &wsm, &gm);
    assert_eq!(ids, vec![on_a]);

    wsm.switch_to(ws_b).await.unwrap();
    let ids = visible_instance_ids(&wm, &wsm, &gm);
    assert_eq!(ids, vec![on_b]);

    wsm.switch_to(ws_a).await.unwrap();
    let tab2 = wm
        .create(CreateWidgetRequest {
            type_id: "test.dummy".into(),
            workspace_id: ws_a,
            position: Some(GridPosition { col: 1, row: 0 }),
            size: Some(WidgetSize::Medium),
            initial_lifecycle: None,
            config_bytes: None,
        })
        .await
        .unwrap();

    let gid = gm
        .create_group(
            ws_a,
            vec![on_a, tab2],
            GridPosition { col: 0, row: 0 },
            WidgetSize::Medium,
        )
        .await
        .unwrap();
    gm.switch_active(gid, on_a).await.unwrap();

    let ids = visible_instance_ids(&wm, &wsm, &gm);
    assert!(ids.contains(&on_a));
    assert!(!ids.contains(&tab2));
    assert!(!ids.contains(&on_b));

    gm.switch_active(gid, tab2).await.unwrap();
    let ids = visible_instance_ids(&wm, &wsm, &gm);
    assert!(ids.contains(&tab2));
    assert!(!ids.contains(&on_a));
}
