//! End-to-end tests for `orchid-storage`.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use orchid_storage::{
    state::tables::META_TABLE, CacheEntry, CacheKind, ConfigLoader, ConfigWatcher, FileTag,
    GridPosition, HistoryEntry, LifecycleState, OrchidConfig, SchemaMeta, SessionState, StateStore,
    StorageError, WidgetInstance, WidgetSize, Workspace, CURRENT_SCHEMA_VERSION,
};
use tempfile::tempdir;
use uuid::Uuid;

const TEST_ORCHID_VERSION: &str = "0.0.0-test";

#[test]
fn workspace_persists_across_reopen() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.redb");

    let ws_id = Uuid::new_v4();
    {
        let store = StateStore::open(&path, TEST_ORCHID_VERSION).unwrap();
        let ws = Workspace {
            id: ws_id,
            name: "Alpha".into(),
            ordinal: 1,
            wallpaper: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let mut w = store.write().unwrap();
        w.put_workspace(&ws).unwrap();
        w.commit().unwrap();
    }

    let store = StateStore::open(&path, TEST_ORCHID_VERSION).unwrap();
    let r = store.read().unwrap();
    let back = r.get_workspace(ws_id).unwrap().unwrap();
    assert_eq!(back.name, "Alpha");
    assert_eq!(back.ordinal, 1);
}

#[test]
fn history_is_iterated_in_timestamp_order() {
    let store = StateStore::open_in_memory(TEST_ORCHID_VERSION).unwrap();
    let base = Utc::now();

    // Insert out-of-order to prove the index does the sorting.
    for offset in [5_i64, 1, 3, 4, 2] {
        let entry = HistoryEntry {
            id: Uuid::new_v4(),
            timestamp: base + ChronoDuration::milliseconds(offset),
            action_id: format!("a.{offset}"),
            command_text: format!("cmd {offset}"),
            target: None,
            reversible_until: None,
            reverse_command: None,
            metadata: vec![],
        };
        let mut w = store.write().unwrap();
        w.put_history(&entry).unwrap();
        w.commit().unwrap();
    }

    let r = store.read().unwrap();
    let from = base + ChronoDuration::milliseconds(0);
    let to = base + ChronoDuration::milliseconds(10);
    let range = r.iter_history_range(from, to).unwrap();
    assert_eq!(range.len(), 5);
    for pair in range.windows(2) {
        assert!(pair[0].timestamp <= pair[1].timestamp, "range must be ascending");
    }

    let recent = r.iter_history_recent(3).unwrap();
    assert_eq!(recent.len(), 3);
    for pair in recent.windows(2) {
        assert!(pair[0].timestamp >= pair[1].timestamp, "recent must be descending");
    }
}

#[test]
fn widget_is_queryable_by_workspace() {
    let store = StateStore::open_in_memory(TEST_ORCHID_VERSION).unwrap();
    let ws = Workspace {
        id: Uuid::new_v4(),
        name: "W".into(),
        ordinal: 1,
        wallpaper: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    let widget = WidgetInstance {
        id: Uuid::new_v4(),
        widget_type: "rss".into(),
        workspace_id: ws.id,
        position: GridPosition { col: 2, row: 2 },
        size: WidgetSize::Large,
        lifecycle: LifecycleState::Active,
        config: vec![0xAB],
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    let mut w = store.write().unwrap();
    w.put_workspace(&ws).unwrap();
    w.put_widget(&widget).unwrap();
    w.commit().unwrap();

    let r = store.read().unwrap();
    let widgets = r.widgets_for_workspace(ws.id).unwrap();
    assert_eq!(widgets.len(), 1);
    assert_eq!(widgets[0].widget_type, "rss");

    let listed = r.list_all_widgets().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, widget.id);

    let other = r.widgets_for_workspace(Uuid::new_v4()).unwrap();
    assert!(other.is_empty());
}

#[test]
fn concurrent_writers_serialise_without_corruption() {
    // redb serialises write transactions; two threads calling `write()` at the
    // same time must both eventually succeed without interleaving.
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.redb");
    let store = StateStore::open(&path, TEST_ORCHID_VERSION).unwrap();
    let store = Arc::new(store);

    let n_per_thread = 20;
    let mut handles = Vec::new();
    for thread_idx in 0..2_u32 {
        let s = Arc::clone(&store);
        handles.push(thread::spawn(move || {
            for i in 0..n_per_thread {
                let ws = Workspace {
                    id: Uuid::new_v4(),
                    name: format!("t{thread_idx}-{i}"),
                    ordinal: 1,
                    wallpaper: None,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };
                let mut w = s.write().unwrap();
                w.put_workspace(&ws).unwrap();
                w.commit().unwrap();
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }

    let r = store.read().unwrap();
    let all = r.list_workspaces().unwrap();
    assert_eq!(all.len() as u32, n_per_thread * 2);
}

#[test]
fn evict_cache_older_than_removes_correct_rows() {
    let store = StateStore::open_in_memory(TEST_ORCHID_VERSION).unwrap();
    let cutoff = Utc::now();
    let old_ts = cutoff - ChronoDuration::hours(1);
    let fresh_ts = cutoff + ChronoDuration::hours(1);

    let mut old_keys = Vec::new();
    let mut fresh_keys = Vec::new();

    {
        let mut w = store.write().unwrap();
        for i in 0..3_u8 {
            let mut key = [0u8; 32];
            key[0] = i;
            old_keys.push(key);
            w.put_cache(&CacheEntry {
                key,
                kind: CacheKind::ThumbnailImage,
                created_at: old_ts,
                last_access_at: old_ts,
                size_bytes: 10,
                data: vec![i],
            })
            .unwrap();
        }
        for i in 0..2_u8 {
            let mut key = [0u8; 32];
            key[0] = 100 + i;
            fresh_keys.push(key);
            w.put_cache(&CacheEntry {
                key,
                kind: CacheKind::ThumbnailImage,
                created_at: fresh_ts,
                last_access_at: fresh_ts,
                size_bytes: 10,
                data: vec![i],
            })
            .unwrap();
        }
        w.commit().unwrap();
    }

    {
        let mut w = store.write().unwrap();
        let removed = w.evict_cache_older_than(cutoff).unwrap();
        assert_eq!(removed, 3);
        w.commit().unwrap();
    }

    let r = store.read().unwrap();
    for k in &old_keys {
        assert!(r.get_cache(k).unwrap().is_none(), "expired key was not evicted");
    }
    for k in &fresh_keys {
        assert!(r.get_cache(k).unwrap().is_some(), "fresh key should remain");
    }
    assert_eq!(r.total_cache_bytes().unwrap(), 20);
}

#[test]
fn opening_db_with_future_version_fails() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.redb");

    // Initialise at the current version, then forcibly bump the on-disk
    // schema version using the raw database handle.
    {
        let store = StateStore::open(&path, TEST_ORCHID_VERSION).unwrap();
        let db = store.raw_database();
        let txn = db.begin_write().unwrap();
        {
            let mut meta = txn.open_table(META_TABLE).unwrap();
            let bogus = SchemaMeta {
                version: CURRENT_SCHEMA_VERSION + 1,
                created_at: Utc::now(),
                last_opened_at: Utc::now(),
                orchid_version: "from-the-future".into(),
            };
            meta.insert("current", &bogus).unwrap();
        }
        txn.commit().unwrap();
    }

    let err = StateStore::open(&path, TEST_ORCHID_VERSION).unwrap_err();
    assert!(matches!(
        err,
        StorageError::UnsupportedSchemaVersion { .. }
    ));
}

#[test]
fn session_state_roundtrip_via_singleton_table() {
    let store = StateStore::open_in_memory(TEST_ORCHID_VERSION).unwrap();
    let session = SessionState {
        active_workspace_id: Some(Uuid::new_v4()),
        open_file_manager_tabs: vec![],
        open_terminal_sessions: vec![],
        last_saved_at: Utc::now(),
    };
    let mut w = store.write().unwrap();
    w.set_session_state(&session).unwrap();
    w.commit().unwrap();

    let r = store.read().unwrap();
    let back = r.get_session_state().unwrap().unwrap();
    assert_eq!(back.active_workspace_id, session.active_workspace_id);
}

#[test]
fn file_tag_update_is_last_write_wins() {
    let store = StateStore::open_in_memory(TEST_ORCHID_VERSION).unwrap();
    let path_key = "C:/docs/report.md";
    let mut tag = FileTag {
        path: path_key.into(),
        tags: vec!["draft".into()],
        color_label: None,
        starred: false,
        updated_at: Utc::now(),
    };
    {
        let mut w = store.write().unwrap();
        w.put_file_tag(&tag).unwrap();
        w.commit().unwrap();
    }
    tag.tags.push("final".into());
    tag.starred = true;
    {
        let mut w = store.write().unwrap();
        w.put_file_tag(&tag).unwrap();
        w.commit().unwrap();
    }

    let r = store.read().unwrap();
    let back = r.get_file_tag(path_key).unwrap().unwrap();
    assert_eq!(back.tags, vec!["draft", "final"]);
    assert!(back.starred);
}

// ------ ConfigWatcher integration tests ------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn config_watcher_emits_on_change() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    ConfigLoader::load_or_create(&path).unwrap();

    let (watcher, mut rx) = ConfigWatcher::start(path.clone()).await.unwrap();

    // Give the notify backend a beat to register the watch.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut cfg = OrchidConfig::default();
    cfg.appearance.theme = "custom".into();
    ConfigLoader::save(&cfg, &path).unwrap();

    // Debounce window is 500 ms, so give the watcher a safety margin.
    let got =
        tokio::time::timeout(Duration::from_secs(4), rx.recv()).await.expect("watcher did not fire");
    let updated = got.expect("broadcast closed");
    assert_eq!(updated.appearance.theme, "custom");

    watcher.stop().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn config_watcher_ignores_invalid_writes() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    ConfigLoader::load_or_create(&path).unwrap();

    let (watcher, mut rx) = ConfigWatcher::start(path.clone()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Write a file that fails semantic validation (font-scale out of range).
    std::fs::write(&path, "[appearance]\nfont-scale = 99.0\n").unwrap();

    // With debounce + safety margin we should have heard a valid broadcast
    // already if the watcher were going to emit. Assert none arrives.
    let outcome = tokio::time::timeout(Duration::from_millis(1500), rx.recv()).await;
    assert!(outcome.is_err(), "watcher should not broadcast invalid configs");

    watcher.stop().await.unwrap();
}
