//! Garbage collection removes chunk files not tracked in the refcount table.

use std::sync::Arc;

use orchid_crypto::{hash_bytes, hex, ChunkStore};

#[tokio::test]
async fn orphan_chunk_file_is_removed_by_gc() {
    let td = tempfile::tempdir().unwrap();
    let chunks_dir = td.path().join("chunks");
    let storage = Arc::new(orchid_storage::StateStore::open_in_memory("0").unwrap());
    let store = ChunkStore::new(chunks_dir.clone(), Arc::clone(&storage)).unwrap();

    // Put a legitimate chunk so that the `<first-two>` directory exists.
    let legit_hash = store.put(b"legit").await.unwrap();
    let legit_hex = hex(&legit_hash);

    // Fabricate an orphan on disk with a matching shape.
    let orphan_bytes = b"this was never registered";
    let orphan_hash = hash_bytes(orphan_bytes);
    let orphan_hex = hex(&orphan_hash);
    let (a, b) = orphan_hex.split_at(2);
    let orphan_dir = chunks_dir.join(a);
    std::fs::create_dir_all(&orphan_dir).unwrap();
    let orphan_path = orphan_dir.join(format!("{b}.bin"));
    std::fs::write(&orphan_path, orphan_bytes).unwrap();
    assert!(orphan_path.exists());

    // Also drop an obviously-unrelated file that doesn't parse as a chunk
    // (short filename) — GC should leave it alone.
    let ignored = chunks_dir.join("zz").join("not-a-chunk.txt");
    std::fs::create_dir_all(ignored.parent().unwrap()).unwrap();
    std::fs::write(&ignored, b"unrelated").unwrap();

    let stats = store.garbage_collect().await.unwrap();
    assert_eq!(stats.files_removed, 1);
    assert_eq!(stats.bytes_freed, orphan_bytes.len() as u64);
    assert!(!orphan_path.exists(), "orphan was removed");
    assert!(ignored.exists(), "unrelated file was preserved");

    // Legit chunk still retrievable.
    let got = store.get(&legit_hash).await.unwrap();
    assert_eq!(got.as_slice(), b"legit");
    let _ = legit_hex; // silence unused
}
