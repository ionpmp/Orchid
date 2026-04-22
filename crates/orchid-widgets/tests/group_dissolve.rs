//! Integration: form a group, dissolve it, confirm members come back.

mod common;

use std::sync::Arc;

use orchid_core::{EventBus, EventBusConfig};
use orchid_storage::{GridPosition, StateStore, WidgetSize};
use orchid_widgets::GroupManager;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread")]
async fn group_dissolve_releases_members() {
    let bus = Arc::new(EventBus::new(EventBusConfig::default()));
    let storage = Arc::new(StateStore::open_in_memory("test").unwrap());
    let mgr = GroupManager::new(bus, storage);

    let ws = Uuid::new_v4();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let c = Uuid::new_v4();

    let group_id = mgr
        .create_group(
            ws,
            vec![a, b, c],
            GridPosition { col: 0, row: 0 },
            WidgetSize::Medium,
        )
        .await
        .unwrap();

    assert_eq!(mgr.get(group_id).unwrap().members.len(), 3);

    let released = mgr.dissolve_group(group_id).await.unwrap();
    assert_eq!(released.len(), 3);
    assert!(released.contains(&a));
    assert!(released.contains(&b));
    assert!(released.contains(&c));

    assert!(mgr.get(group_id).is_err());
}
