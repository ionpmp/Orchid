//! Integration: create 3 widgets, snapshot to storage, drop the manager,
//! reopen against the same disk-backed store, and verify the same instances
//! come back.

mod common;

use std::sync::Arc;

use orchid_core::{EventBus, EventBusConfig};
use orchid_widgets::{
    CreateWidgetRequest, WidgetManager, WidgetManagerOptions, WidgetRegistry,
};
use parking_lot::RwLock;
use uuid::Uuid;

use common::{register_dummy, DiskStorage};

#[tokio::test(flavor = "multi_thread")]
async fn snapshot_and_restore_preserves_widgets() {
    let disk = DiskStorage::new();
    let bus = Arc::new(EventBus::new(EventBusConfig::default()));
    let config = Arc::new(RwLock::new(orchid_storage::OrchidConfig::default()));
    let workspace_id = Uuid::new_v4();

    // Persist a workspace row so restore_all walks it.
    {
        let storage = disk.open();
        let ws = orchid_storage::Workspace {
            id: workspace_id,
            name: "Test".into(),
            ordinal: 1,
            wallpaper: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let mut txn = storage.write().unwrap();
        txn.put_workspace(&ws).unwrap();
        txn.commit().unwrap();
    }

    // Phase 1: create three widgets with the first manager.
    let created_ids = {
        let registry = Arc::new(WidgetRegistry::new());
        let _counters = register_dummy(&registry);
        let storage = disk.open();
        let manager = WidgetManager::new(
            registry,
            bus.clone(),
            storage,
            config.clone(),
            WidgetManagerOptions::default(),
        );
        let mut ids = Vec::new();
        for _ in 0..3 {
            let id = manager
                .create(CreateWidgetRequest {
                    type_id: "test.dummy".into(),
                    workspace_id,
                    position: None,
                    size: None,
                    initial_lifecycle: None,
                    config_bytes: None,
                })
                .await
                .unwrap();
            ids.push(id);
        }
        manager.snapshot_to_storage().await.unwrap();
        ids
    };

    // Phase 2: fresh manager, restore from the same disk path.
    let registry = Arc::new(WidgetRegistry::new());
    let _counters = register_dummy(&registry);
    let storage = disk.open();
    let manager = WidgetManager::new(
        registry,
        bus.clone(),
        storage,
        config,
        WidgetManagerOptions::default(),
    );
    let restored = manager.restore_from_storage().await.unwrap();
    assert_eq!(restored, 3);

    let mut actual: Vec<Uuid> =
        manager.list_instances().iter().map(|i| i.id).collect();
    actual.sort();
    let mut expected = created_ids.clone();
    expected.sort();
    assert_eq!(actual, expected);
}
