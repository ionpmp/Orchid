//! Universal search debouncer integration tests.
//!
//! Query pushes go through the widget debouncer and the
//! `WidgetSnapshotUpdated` → snapshot cache → frame-dirty path. The UI patches
//! those rows with `patch_workspace_frames` instead of rebuilding the whole
//! workspace on every keystroke (see `docs/universal-search-issue.md`).

mod common;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use orchid_core::{EventBus, EventBusConfig};
use orchid_storage::StateStore;
use orchid_widgets::builtin::search::{
    self, universal_search_push_query, ActionTarget, SearchAggregator, SearchCandidate,
    SearchSource, TYPE_ID,
};
use orchid_widgets::{
    CreateWidgetRequest, WidgetManager, WidgetManagerOptions, WidgetPayload, WidgetRegistry,
};
use parking_lot::RwLock;

use common::test_locale;

struct EchoSource;

#[async_trait]
impl SearchSource for EchoSource {
    fn id(&self) -> &'static str {
        "echo"
    }
    fn name_key(&self) -> &'static str {
        "echo"
    }
    fn icon(&self) -> &'static str {
        "x"
    }
    async fn search(&self, query: &str, _limit: usize) -> Vec<SearchCandidate> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        vec![SearchCandidate {
            id: format!("echo:{query}"),
            source_id: "echo",
            title: format!("hit:{query}"),
            subtitle: None,
            icon: "x",
            score: 10,
            action_hint: None,
            action_target: ActionTarget::RunCommand("noop".into()),
        }]
    }
}

async fn make_search_manager() -> (WidgetManager, uuid::Uuid) {
    let bus = Arc::new(EventBus::new(EventBusConfig::default()));
    let storage = Arc::new(StateStore::open_in_memory("search-debouncer").unwrap());
    let config = Arc::new(RwLock::new(orchid_storage::OrchidConfig::default()));
    let widget_registry = Arc::new(WidgetRegistry::new());
    let agg = Arc::new(SearchAggregator::new(vec![
        Arc::new(EchoSource) as Arc<dyn SearchSource>
    ]));
    widget_registry.register(search::descriptor(agg)).unwrap();

    let manager = WidgetManager::new(
        widget_registry,
        bus,
        storage,
        config,
        test_locale(),
        Arc::new(orchid_core::BackgroundJobQueue::new()),
        WidgetManagerOptions::default(),
    );
    manager.start().await.unwrap();

    let ws_id = uuid::Uuid::new_v4();
    let instance_id = manager
        .create(CreateWidgetRequest {
            type_id: TYPE_ID.into(),
            workspace_id: ws_id,
            position: None,
            size: None,
            initial_lifecycle: None,
            config_bytes: None,
        })
        .await
        .unwrap();

    (manager, instance_id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn search_push_query_updates_via_frame_dirty_path() {
    let (manager, id) = make_search_manager().await;

    universal_search_push_query(id, "widget".into());

    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(25)).await;
        if let Some(s) = manager.snapshot_cache().get(id).map(|x| (*x).clone()) {
            if let WidgetPayload::UniversalSearch(p) = &s.payload {
                if p.query == "widget" && p.candidates.len() == 1 {
                    let dirty = manager.drain_frame_dirty_ids();
                    assert!(
                        dirty.contains(&id),
                        "WidgetSnapshotUpdated should mark the instance frame-dirty for patch_workspace_frames"
                    );
                    assert_eq!(p.candidates[0].title, "hit:widget");
                    return;
                }
            }
        }
    }
    panic!("search snapshot not updated after push_query");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn search_rapid_query_pushes_coalesce_before_search() {
    let (manager, id) = make_search_manager().await;

    universal_search_push_query(id, "a".into());
    tokio::time::sleep(Duration::from_millis(50)).await;
    universal_search_push_query(id, "ab".into());
    tokio::time::sleep(Duration::from_millis(50)).await;
    universal_search_push_query(id, "abc".into());
    tokio::time::sleep(Duration::from_millis(300)).await;

    let snap = manager.snapshot_cache().get(id).expect("cached snapshot");
    let p = match &snap.payload {
        WidgetPayload::UniversalSearch(p) => p,
        _ => panic!("expected universal search payload"),
    };
    assert_eq!(p.query, "abc");
    assert_eq!(p.candidates.len(), 1);
    assert_eq!(p.candidates[0].title, "hit:abc");
}

#[test]
fn search_push_query_unknown_instance_increments_miss_counter() {
    let before = search::universal_search_live_miss_count();
    let unknown = uuid::Uuid::new_v4();
    universal_search_push_query(unknown, "x".into());
    assert_eq!(search::universal_search_live_miss_count(), before + 1);
}
