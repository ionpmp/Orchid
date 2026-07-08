//! Managed-folder policy and engine integration tests.

use std::sync::Arc;

use orchid_core::EventBus;
use orchid_crypto::{ChunkStore, ChunkerConfig, Deduplicator};
use orchid_fs::{
    ManagedFolderConfig, ManagedFolderEngine, ManagedFolderPolicy, ManagedFolderStats,
};
use orchid_fs::{FileWatcher, FsPath, FsProvider, FsProviderRegistry, LocalProvider};

fn bus() -> Arc<EventBus> {
    Arc::new(EventBus::new(orchid_core::EventBusConfig::default()))
}

fn storage() -> Arc<orchid_storage::StateStore> {
    Arc::new(orchid_storage::StateStore::open_in_memory("managed-policy").unwrap())
}

fn registry_with_local() -> Arc<FsProviderRegistry> {
    let reg = Arc::new(FsProviderRegistry::new());
    reg.register(Arc::new(LocalProvider::new()) as Arc<dyn FsProvider>)
        .unwrap();
    reg
}

fn engine(storage: &Arc<orchid_storage::StateStore>) -> ManagedFolderEngine {
    let td = tempfile::tempdir().unwrap();
    let chunk_store = Arc::new(
        ChunkStore::new(td.path().join("chunks"), Arc::clone(storage)).unwrap(),
    );
    let dedup = Arc::new(Deduplicator::new(
        Arc::clone(&chunk_store),
        ChunkerConfig::default(),
    ));
    let bus = bus();
    let registry = registry_with_local();
    let watcher = Arc::new(FileWatcher::new(Arc::clone(&bus), Arc::clone(&registry)));
    ManagedFolderEngine::new(
        Arc::clone(storage),
        chunk_store,
        dedup,
        registry,
        bus,
        watcher,
    )
}

#[test]
fn managed_policy_exclude_patterns() {
    let policy = ManagedFolderPolicy {
        exclude_patterns: vec!["*.bak".into()],
        ..Default::default()
    };
    assert!(!policy.should_ingest("local:/work/data.bak"));
    assert!(policy.should_ingest("local:/work/data.txt"));
}

#[test]
fn managed_policy_quota() {
    let policy = ManagedFolderPolicy {
        max_size_bytes: Some(500),
        ..Default::default()
    };
    let under = ManagedFolderStats {
        physical_bytes: 400,
        ..Default::default()
    };
    let over = ManagedFolderStats {
        physical_bytes: 600,
        ..Default::default()
    };
    assert!(policy.check_quota(&under).is_ok());
    assert!(policy.check_quota(&over).is_err());
}

#[tokio::test]
async fn managed_engine_skips_excluded_ingest() {
    let storage = storage();
    let engine = engine(&storage);
    let td = tempfile::tempdir().unwrap();
    let root = FsPath::from_local(td.path()).unwrap();
    let file = root.join("skip.tmp");
    std::fs::write(td.path().join("skip.tmp"), b"payload").unwrap();

    engine
        .add_folder(ManagedFolderConfig {
            path: root.clone(),
            chunk_size: ChunkerConfig::default(),
            enabled: true,
            auto_ingest: true,
            policy: Some(ManagedFolderPolicy {
                exclude_patterns: vec!["*.tmp".into()],
                ..Default::default()
            }),
        })
        .await
        .unwrap();

    let err = engine.ingest(&file).await.unwrap_err();
    assert!(matches!(
        err,
        orchid_fs::FsError::ManagedIngestExcluded(_)
    ));
}

#[tokio::test]
async fn managed_engine_rejects_ingest_when_quota_exceeded() {
    let storage = storage();
    let engine = engine(&storage);
    let td = tempfile::tempdir().unwrap();
    let root = FsPath::from_local(td.path()).unwrap();
    let first = root.join("one.bin");
    let second = root.join("two.bin");
    std::fs::write(td.path().join("one.bin"), vec![0_u8; 512]).unwrap();
    std::fs::write(td.path().join("two.bin"), vec![0_u8; 512]).unwrap();

    engine
        .add_folder(ManagedFolderConfig {
            path: root.clone(),
            chunk_size: ChunkerConfig::default(),
            enabled: true,
            auto_ingest: true,
            policy: Some(ManagedFolderPolicy {
                max_size_bytes: Some(256),
                ..Default::default()
            }),
        })
        .await
        .unwrap();

    engine.ingest(&first).await.unwrap();
    let err = engine.ingest(&second).await.unwrap_err();
    assert!(matches!(
        err,
        orchid_fs::FsError::ManagedQuotaExceeded { .. }
    ));
}
