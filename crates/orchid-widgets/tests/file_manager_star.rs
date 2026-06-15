//! File-manager star action persists via TagManager.

use std::sync::Arc;
use std::time::Duration;

use orchid_core::{EventBus, EventBusConfig};
use orchid_fs::{FsProvider, FsProviderRegistry, LocalProvider, TagManager};
use orchid_storage::StateStore;
use orchid_widgets::builtin::file_manager::{self, FileClipboard, FileManagerDeps};
use orchid_widgets::{CreateWidgetRequest, WidgetManager, WidgetManagerOptions, WidgetPayload, WidgetRegistry};
use parking_lot::RwLock;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn star_action_updates_snapshot_from_tag_manager() {
    let bus = Arc::new(EventBus::new(EventBusConfig::default()));
    let storage = Arc::new(StateStore::open_in_memory("fm-star").unwrap());
    let registry = Arc::new(FsProviderRegistry::new());
    registry
        .register(Arc::new(LocalProvider::new()) as Arc<dyn FsProvider>)
        .unwrap();
    let tag_manager = Arc::new(TagManager::new(storage.clone(), bus.clone()));
    let cache_dir = tempfile::tempdir().unwrap();
    let thumbnails = Arc::new(
        orchid_viewers::ThumbnailService::new(cache_dir.path().join("thumbs")).unwrap(),
    );
    let fm_deps = FileManagerDeps {
        registry: registry.clone(),
        clipboard: Arc::new(FileClipboard::new()),
        tag_manager,
        thumbnails,
        search: None,
        managed: None,
        encrypted: None,
        network_mounts: Arc::new(RwLock::new(Vec::new())),
    };

    let widget_registry = Arc::new(WidgetRegistry::new());
    widget_registry
        .register(file_manager::descriptor(fm_deps))
        .unwrap();

    let config = Arc::new(RwLock::new(orchid_storage::OrchidConfig::default()));
    let manager = WidgetManager::new(
        widget_registry,
        bus,
        storage,
        config,
        WidgetManagerOptions::default(),
    );
    manager.start().await.unwrap();

    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("marked.txt");
    std::fs::write(&file_path, "x").unwrap();
    let file_fs = orchid_fs::FsPath::from_local(&file_path).unwrap();
    let file_path_str = file_fs.as_str().to_string();

    let fm_id = manager
        .create(CreateWidgetRequest {
            type_id: file_manager::TYPE_ID.into(),
            workspace_id: uuid::Uuid::new_v4(),
            position: None,
            size: None,
            initial_lifecycle: None,
            config_bytes: None,
        })
        .await
        .unwrap();

    let parent = orchid_fs::FsPath::from_local(dir.path()).unwrap();
    file_manager::navigate(fm_id, 0, parent).await.unwrap();
    file_manager::run_action(fm_id, "fs.star", vec![file_path_str.clone()])
        .await
        .unwrap();

    for _ in 0..80 {
        manager.refresh_snapshot_cache(fm_id).await.unwrap();
        if let Some(s) = manager.snapshot_cache().get(fm_id).map(|x| (*x).clone()) {
            if let WidgetPayload::FileManager(p) = &s.payload {
                let tab = &p.panes[0].tabs[0];
                if let Some(entry) = tab.entries.iter().find(|e| e.path == file_path_str) {
                    assert!(entry.is_starred);
                    return;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    panic!("did not observe starred entry in snapshot");
}
