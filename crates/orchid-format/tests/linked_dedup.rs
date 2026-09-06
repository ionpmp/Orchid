//! Phase 2 DONE: linked dedupe + private decrypt with Identity.

use std::sync::Arc;

use orchid_crypto::{ChunkStore, ChunkerConfig, Identity};
use orchid_format::toc::RegionType;
use orchid_format::{
    capability, linked_region_plaintext, write_linked_file, LinkedCreateRequest, SealedFile,
};

fn tiny_chunker() -> ChunkerConfig {
    ChunkerConfig {
        min_size: 64 * 1024,
        avg_size: 128 * 1024,
        max_size: 512 * 1024,
    }
}

fn make_payload(seed: u32, len: usize) -> Vec<u8> {
    let words = len / 4;
    (0..words as u32)
        .flat_map(|i| (i.wrapping_add(seed).wrapping_mul(2_654_435_761)).to_le_bytes())
        .collect()
}

#[tokio::test]
async fn two_linked_files_dedupe_shared_clean_text() {
    let td = tempfile::tempdir().unwrap();
    let storage = Arc::new(orchid_storage::StateStore::open_in_memory("fmt").unwrap());
    let store = ChunkStore::new(td.path().join("chunks"), Arc::clone(&storage)).unwrap();

    // Shared *unencrypted* bytes (age ciphertext is non-deterministic, so it
    // cannot dedupe across independently encrypted files).
    let shared = make_payload(7, 2 * 1024 * 1024);

    let path_a = td.path().join("a.orchid");
    write_linked_file(
        &path_a,
        &store,
        &LinkedCreateRequest {
            file_uuid: Some([0xAAu8; 16]),
            created_unix_ms: Some(1),
            generation: 1,
            parent_generation: 0,
            raw: b"a-raw".to_vec(),
            clean_text: shared.clone(),
            structured: b"{\"a\":1}".to_vec(),
            encrypt_with: None,
            chunker: tiny_chunker(),
        },
    )
    .await
    .unwrap();

    let path_b = td.path().join("b.orchid");
    write_linked_file(
        &path_b,
        &store,
        &LinkedCreateRequest {
            file_uuid: Some([0xBBu8; 16]),
            created_unix_ms: Some(2),
            generation: 1,
            parent_generation: 0,
            raw: b"b-raw".to_vec(),
            clean_text: shared.clone(),
            structured: b"{\"b\":2}".to_vec(),
            encrypt_with: None,
            chunker: tiny_chunker(),
        },
    )
    .await
    .unwrap();

    let a = SealedFile::open(&path_a).unwrap();
    assert_ne!(a.header().capability_flags & capability::LINKED, 0);

    let clean_entry = a.find_region(RegionType::CleanText).unwrap();
    assert_eq!(
        clean_entry.storage(),
        orchid_format::toc::StorageMode::Linked
    );
    let chunks = clean_entry.chunks().unwrap();
    assert!(!chunks.is_empty());

    let mut saw_shared = false;
    for i in 0..chunks.len() {
        let c = chunks.get(i);
        let mut hash = [0u8; 32];
        hash.copy_from_slice(c.blake3().unwrap().bytes());
        let info = store.info(&hash).unwrap().expect("chunk registered");
        if info.refcount >= 2 {
            saw_shared = true;
            break;
        }
    }
    assert!(
        saw_shared,
        "expected at least one Clean-Text chunk with refcount >= 2"
    );

    let got = linked_region_plaintext(&a, &store, RegionType::CleanText, None)
        .await
        .unwrap();
    assert_eq!(got, shared);
}

#[tokio::test]
async fn linked_encrypted_region_needs_correct_identity() {
    let td = tempfile::tempdir().unwrap();
    let storage = Arc::new(orchid_storage::StateStore::open_in_memory("fmt2").unwrap());
    let store = ChunkStore::new(td.path().join("chunks"), Arc::clone(&storage)).unwrap();
    let id = Identity::passphrase("linked-secret");
    let clean = make_payload(3, 256 * 1024);

    let path = td.path().join("enc.orchid");
    write_linked_file(
        &path,
        &store,
        &LinkedCreateRequest {
            file_uuid: Some([0xCCu8; 16]),
            created_unix_ms: Some(3),
            generation: 1,
            parent_generation: 0,
            raw: Vec::new(),
            clean_text: clean.clone(),
            structured: b"{}".to_vec(),
            encrypt_with: Some(id.clone()),
            chunker: tiny_chunker(),
        },
    )
    .await
    .unwrap();

    let file = SealedFile::open(&path).unwrap();
    assert_ne!(file.header().capability_flags & capability::ENCRYPTED, 0);
    assert_eq!(
        linked_region_plaintext(&file, &store, RegionType::CleanText, Some(&id))
            .await
            .unwrap(),
        clean
    );
    let wrong = Identity::passphrase("nope");
    assert!(
        linked_region_plaintext(&file, &store, RegionType::CleanText, Some(&wrong))
            .await
            .is_err()
    );
}
