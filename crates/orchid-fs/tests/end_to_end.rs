//! End-to-end tests covering provider + watcher + tags + archive + ops.

use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use orchid_fs::{
    copy, open_archive, CopyOptions, EncryptedFolderConfig, EncryptedFolderEngine, FileWatcher,
    FsPath, FsProvider, FsProviderRegistry, LocalProvider, TagManager,
};

fn bus() -> Arc<orchid_core::EventBus> {
    Arc::new(orchid_core::EventBus::new(orchid_core::EventBusConfig::default()))
}

fn storage() -> Arc<orchid_storage::StateStore> {
    Arc::new(orchid_storage::StateStore::open_in_memory("0").unwrap())
}

fn registry_with_local() -> Arc<FsProviderRegistry> {
    let reg = Arc::new(FsProviderRegistry::new());
    reg.register(Arc::new(LocalProvider::new()) as Arc<dyn FsProvider>).unwrap();
    reg
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_provider_list_read_write_metadata() {
    let td = tempfile::tempdir().unwrap();
    let provider = LocalProvider::new();

    let dir_path = FsPath::from_local(td.path()).unwrap();
    let file_path = dir_path.join("hello.txt");

    provider
        .write(&file_path, b"hello world")
        .await
        .unwrap();

    let listing = provider.list(&dir_path).await.unwrap();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0].name, "hello.txt");
    assert_eq!(listing[0].metadata.size, 11);

    let meta = provider.metadata(&file_path).await.unwrap();
    assert_eq!(meta.size, 11);

    let bytes = provider.read(&file_path).await.unwrap();
    assert_eq!(bytes, b"hello world");

    let renamed = dir_path.join("renamed.txt");
    provider.rename(&file_path, &renamed).await.unwrap();
    assert!(provider.exists(&renamed).await.unwrap());
    assert!(!provider.exists(&file_path).await.unwrap());

    provider.remove(&renamed, false).await.unwrap();
    assert!(!provider.exists(&renamed).await.unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn watcher_publishes_created_event() {
    let td = tempfile::tempdir().unwrap();
    let dir_path = FsPath::from_local(td.path()).unwrap();

    let bus = bus();
    let registry = registry_with_local();
    let watcher = FileWatcher::new(Arc::clone(&bus), Arc::clone(&registry));

    let (_sub, mut rx) = bus
        .subscribe(
            orchid_core::EventFilter::of_type("fs.created"),
            orchid_core::HandlerPriority::Normal,
        )
        .unwrap();

    let _watch = watcher.watch(dir_path.clone()).await.unwrap();
    // Give the OS watcher a moment to register.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Create a file inside the watched dir.
    tokio::fs::write(td.path().join("new.txt"), b"payload").await.unwrap();

    let env = tokio::time::timeout(Duration::from_secs(3), rx.recv())
        .await
        .expect("watcher should publish an event")
        .expect("bus channel open");
    assert_eq!(env.event_type, "fs.created");
}

#[tokio::test]
async fn tag_manager_round_trip() {
    let storage = storage();
    let bus = bus();
    let tags = TagManager::new(Arc::clone(&storage), Arc::clone(&bus));
    let td = tempfile::tempdir().unwrap();
    let path = FsPath::from_local(td.path()).unwrap().join("x.txt");

    tags.set_tags(&path, vec!["Urgent".into(), "work".into()]).unwrap();
    let rec = tags.get(&path).unwrap().unwrap();
    assert_eq!(rec.tags, vec!["urgent".to_string(), "work".to_string()]);

    tags.add_tag(&path, "WORK").unwrap();
    tags.add_tag(&path, "followup").unwrap();
    let rec = tags.get(&path).unwrap().unwrap();
    assert_eq!(rec.tags, vec!["followup", "urgent", "work"]);

    tags.remove_tag(&path, "work").unwrap();
    let rec = tags.get(&path).unwrap().unwrap();
    assert!(!rec.tags.contains(&"work".to_string()));

    tags.set_color(&path, Some(orchid_storage::ColorLabel::Red)).unwrap();
    tags.set_starred(&path, true).unwrap();
    let rec = tags.get(&path).unwrap().unwrap();
    assert_eq!(rec.color_label, Some(orchid_storage::ColorLabel::Red));
    assert!(rec.starred);

    let by_tag = tags.paths_with_tag("followup").unwrap();
    assert!(by_tag.iter().any(|p| p == &path));

    let starred = tags.starred_paths().unwrap();
    assert!(starred.iter().any(|p| p == &path));

    let all_tags = tags.all_tags().unwrap();
    assert!(all_tags.contains(&"followup".to_string()));
}

#[tokio::test]
async fn zip_list_and_read_with_slip_protection() {
    let td = tempfile::tempdir().unwrap();
    let zip_path = td.path().join("test.zip");

    // Build a test zip on the fly.
    {
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        let opts: zip::write::SimpleFileOptions =
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zw.start_file("inside/a.txt", opts).unwrap();
        zw.write_all(b"hello-a").unwrap();
        zw.start_file("inside/b.txt", opts).unwrap();
        zw.write_all(b"hello-b").unwrap();
        zw.finish().unwrap();
    }

    let reader = open_archive(&zip_path).unwrap();
    let entries = reader.list().await.unwrap();
    let names: Vec<String> = entries.iter().map(|e| e.path.clone()).collect();
    assert!(names.contains(&"inside/a.txt".into()));
    assert!(names.contains(&"inside/b.txt".into()));

    let bytes = reader.read_entry("inside/a.txt").await.unwrap();
    assert_eq!(bytes, b"hello-a");

    // Now add a zip-slip attempt and confirm the listing excludes it.
    {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&zip_path)
            .unwrap();
        let mut zw = zip::ZipWriter::new(file);
        let opts: zip::write::SimpleFileOptions =
            zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zw.start_file("../evil.txt", opts).unwrap();
        zw.write_all(b"pwn").unwrap();
        zw.start_file("good.txt", opts).unwrap();
        zw.write_all(b"ok").unwrap();
        zw.finish().unwrap();
    }
    let reader = open_archive(&zip_path).unwrap();
    let entries = reader.list().await.unwrap();
    assert!(entries.iter().all(|e| !e.path.contains("..")));
    assert!(entries.iter().any(|e| e.path == "good.txt"));
}

#[tokio::test]
async fn copy_with_verify_hash() {
    let td = tempfile::tempdir().unwrap();
    let registry = registry_with_local();
    let src = td.path().join("src.bin");
    let dst = td.path().join("dst.bin");
    std::fs::write(&src, b"payload to verify").unwrap();

    let from = FsPath::from_local(&src).unwrap();
    let to = FsPath::from_local(&dst).unwrap();

    copy(
        &registry,
        &from,
        &to,
        CopyOptions {
            overwrite: false,
            verify_content_hash: true,
            preserve_timestamps: false,
            follow_symlinks: true,
        },
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(&dst).unwrap(), b"payload to verify");
}

#[tokio::test]
async fn encrypt_in_place_then_reveal_round_trip() {
    use orchid_crypto::{Identity, RevealDuration, RevealManager};
    let td = tempfile::tempdir().unwrap();
    let reveal_root = td.path().join("reveal");

    let storage = storage();
    let bus = bus();
    let registry = registry_with_local();
    let watcher = Arc::new(FileWatcher::new(Arc::clone(&bus), Arc::clone(&registry)));
    let reveal = Arc::new(RevealManager::new(reveal_root, Arc::clone(&bus)));
    let engine = EncryptedFolderEngine::new(
        Arc::clone(&storage),
        Arc::clone(&registry),
        Arc::clone(&reveal),
        Arc::clone(&bus),
        Arc::clone(&watcher),
    );

    // Prepare plaintext file.
    let plain_path = td.path().join("secret.txt");
    std::fs::write(&plain_path, b"super secret contents").unwrap();
    let fs_path = FsPath::from_local(&plain_path).unwrap();

    engine
        .encrypt_in_place(&fs_path, Identity::passphrase("pw"))
        .await
        .unwrap();

    assert!(!plain_path.exists(), "plaintext was removed");
    let encrypted_os = plain_path.with_extension("txt.age");
    assert!(encrypted_os.exists(), "ciphertext exists");

    let encrypted_fs = FsPath::from_local(&encrypted_os).unwrap();
    let cfg = EncryptedFolderConfig {
        path: encrypted_fs.clone(),
        identity: Identity::passphrase("pw"),
        reveal_duration: RevealDuration::FiveMinutes,
        enabled: true,
    };
    // mark_encrypted was already called internally by encrypt_in_place, so
    // re-marking just updates the record.
    let _ = engine.mark_encrypted(cfg.clone()).await;

    let session = engine
        .reveal(&encrypted_fs, Identity::passphrase("pw"))
        .await
        .unwrap();
    let revealed = std::fs::read(&session.revealed_path).unwrap();
    assert_eq!(revealed, b"super secret contents");
}
