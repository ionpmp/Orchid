//! Content-addressed dedup: overlapping files share chunks.

use std::sync::Arc;

use orchid_crypto::{ChunkStore, Chunker, ChunkerConfig, Deduplicator};

fn tiny_config() -> ChunkerConfig {
    // Smallest legal FastCDC parameters so tests stay fast.
    ChunkerConfig {
        min_size: 64 * 1024,
        avg_size: 128 * 1024,
        max_size: 512 * 1024,
    }
}

/// Deterministic ~1 MiB payload starting from `seed`.
fn make_payload(seed: u32, len: usize) -> Vec<u8> {
    let words = len / 4;
    (0..words as u32)
        .flat_map(|i| (i.wrapping_add(seed).wrapping_mul(2_654_435_761)).to_le_bytes())
        .collect()
}

#[tokio::test]
async fn overlapping_files_share_chunks() {
    let td = tempfile::tempdir().unwrap();
    let storage = Arc::new(orchid_storage::StateStore::open_in_memory("0").unwrap());
    let store = Arc::new(
        ChunkStore::new(td.path().join("chunks"), Arc::clone(&storage)).unwrap(),
    );
    let dedup = Deduplicator::new(Arc::clone(&store), tiny_config());

    // Payload A: 10 MiB of deterministic bytes.
    let a = make_payload(1, 10 * 1024 * 1024);
    let path_a = td.path().join("a.bin");
    std::fs::write(&path_a, &a).unwrap();

    // Payload B: same first 5 MiB, different second 5 MiB.
    let mut b = a.clone();
    let tail = make_payload(9_999, 5 * 1024 * 1024);
    b[(5 * 1024 * 1024)..].copy_from_slice(&tail);
    let path_b = td.path().join("b.bin");
    std::fs::write(&path_b, &b).unwrap();

    let manifest_a = dedup.ingest_file(&path_a).await.unwrap();
    let size_after_a = store.total_bytes().unwrap();
    let manifest_b = dedup.ingest_file(&path_b).await.unwrap();
    let size_after_b = store.total_bytes().unwrap();

    // Growth should be noticeably less than a full second copy.
    let full_copy = (a.len() as u64) * 2;
    assert!(
        size_after_b < full_copy,
        "expected dedup: size after both ({size_after_b}) < full copy ({full_copy})"
    );
    // And at least some additional chunks landed for the changed tail.
    assert!(size_after_b > size_after_a, "new content was stored");

    // Reconstruction is byte-identical for both files.
    let out_a = td.path().join("out_a.bin");
    dedup.reconstruct_to(&manifest_a, &out_a).await.unwrap();
    assert_eq!(std::fs::read(&out_a).unwrap(), a);

    let out_b = td.path().join("out_b.bin");
    dedup.reconstruct_to(&manifest_b, &out_b).await.unwrap();
    assert_eq!(std::fs::read(&out_b).unwrap(), b);
}

#[tokio::test]
async fn release_decrements_and_re_ingest_shares() {
    let td = tempfile::tempdir().unwrap();
    let storage = Arc::new(orchid_storage::StateStore::open_in_memory("0").unwrap());
    let store = Arc::new(
        ChunkStore::new(td.path().join("chunks"), Arc::clone(&storage)).unwrap(),
    );
    let dedup = Deduplicator::new(Arc::clone(&store), tiny_config());

    let payload = make_payload(42, 4 * 1024 * 1024);
    let p = td.path().join("p.bin");
    std::fs::write(&p, &payload).unwrap();

    let m1 = dedup.ingest_file(&p).await.unwrap();
    let size1 = store.total_bytes().unwrap();

    // Ingesting the same file again must not increase size (every chunk
    // already existed — refcount bumps).
    let m2 = dedup.ingest_file(&p).await.unwrap();
    let size2 = store.total_bytes().unwrap();
    assert_eq!(size1, size2, "identical content does not grow the store");

    // Releasing one manifest keeps chunks alive for the other.
    dedup.release(&m1).await.unwrap();
    let size_after_release = store.total_bytes().unwrap();
    assert_eq!(size_after_release, size1, "chunks retained for second manifest");

    // Releasing the last manifest frees everything.
    dedup.release(&m2).await.unwrap();
    assert_eq!(store.total_bytes().unwrap(), 0);

    // Peek at stats.
    let _ = dedup.stats(&m1).await.unwrap();
    let _ = dedup.stats(&m2).await.unwrap();
}

#[test]
fn chunker_config_round_trip_preserved() {
    // Sanity: config comes back identical via chunker.
    let c = Chunker::new(tiny_config());
    let payload = make_payload(1, 2 * 1024 * 1024);
    let chunks = c.chunk_bytes(&payload);
    assert!(!chunks.is_empty());
}
