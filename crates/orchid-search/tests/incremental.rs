//! Live-indexing via `IndexFsSubscriber` driven by `FileWatcher` events.

use std::sync::Arc;
use std::time::Duration;

use orchid_fs::{FileWatcher, FsPath, FsProvider, FsProviderRegistry, LocalProvider, TagManager};
use orchid_search::{
    Extractor, IndexFsSubscriber, IndexScheduler, IndexScope, QueryBuilder, SearchEngine,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fs_event_triggers_indexing() {
    let td = tempfile::tempdir().unwrap();
    let index_dir = td.path().join("index");
    let watch_dir = td.path().join("watch");
    std::fs::create_dir_all(&watch_dir).unwrap();

    let bus = Arc::new(orchid_core::EventBus::new(orchid_core::EventBusConfig::default()));
    let storage = Arc::new(orchid_storage::StateStore::open_in_memory("0").unwrap());

    let registry = Arc::new(FsProviderRegistry::new());
    registry
        .register(Arc::new(LocalProvider::new()) as Arc<dyn FsProvider>)
        .unwrap();

    let watcher = FileWatcher::new(Arc::clone(&bus), Arc::clone(&registry));
    let engine = Arc::new(SearchEngine::open(&index_dir).unwrap());
    let scheduler = Arc::new(IndexScheduler::new(Arc::clone(&engine), 1));
    let extractor = Arc::new(Extractor::new());
    let tags = Arc::new(TagManager::new(Arc::clone(&storage), Arc::clone(&bus)));

    let subscriber = IndexFsSubscriber::new(
        Arc::clone(&bus),
        Arc::clone(&scheduler),
        Arc::clone(&extractor),
        Arc::clone(&registry),
        Arc::clone(&tags),
    );

    // Scope must include the watched directory for the subscriber to act.
    let watch_fs = FsPath::from_local(&watch_dir).unwrap();
    subscriber.set_scope(IndexScope {
        included_roots: vec![watch_fs.clone()],
        excluded_patterns: vec!["*.tmp".into()],
        max_file_size: 1024 * 1024,
        extract_text_content: true,
        extract_pdf_content: false,
    });
    subscriber.start().await.unwrap();

    let _watch = watcher.watch(watch_fs.clone()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Create a file that is in scope.
    let good = watch_dir.join("notes.md");
    std::fs::write(&good, "a quick brown fox jumps over the lazy dog").unwrap();

    // Create a file that the scope excludes.
    let bad = watch_dir.join("scratch.tmp");
    std::fs::write(&bad, "excluded content").unwrap();

    // Wait for debounced events + scheduler flush.
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if engine.doc_count().unwrap_or(0) > 0 {
            break;
        }
    }
    scheduler.flush().await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    engine.commit().await.unwrap();

    // Search should find the .md file content.
    let q = QueryBuilder::new().text("brown").build();
    let hits = engine.search(q).await.unwrap();
    assert!(
        hits.hits.iter().any(|h| h.name == "notes.md"),
        "expected notes.md to be indexed; got {:?}",
        hits.hits.iter().map(|h| &h.name).collect::<Vec<_>>()
    );

    // .tmp file must be absent.
    let q = QueryBuilder::new().extension("tmp").build();
    let hits = engine.search(q).await.unwrap();
    assert!(hits.hits.is_empty(), "excluded .tmp must not be indexed");

    subscriber.shutdown().await.unwrap();
    watcher.shutdown().await.unwrap();
}
