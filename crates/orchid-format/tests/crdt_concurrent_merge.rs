//! Phase 4 DONE: concurrent two-client CRDT merge is deterministic.

use orchid_format::{
    write_sealed_file, ActorId, CrdtDocument, SealedCreateRequest, SealedFile, capability,
};

const HUMAN: ActorId = 1;
const AGENT: ActorId = 2;

#[test]
fn concurrent_two_client_merge_same_hash() {
    // Shared genesis.
    let mut base = CrdtDocument::new();
    let root = base.local_insert(HUMAN, None, "Title\n");

    // Client A (human): append body.
    let mut client_a = base.clone();
    let a1 = client_a.local_insert(HUMAN, Some(root), "Hello ");
    client_a.local_insert(HUMAN, Some(a1), "from human.");

    // Client B (agent): concurrent edit after the same root.
    let mut client_b = base.clone();
    let b1 = client_b.local_insert(AGENT, Some(root), "[agent] ");
    client_b.local_insert(AGENT, Some(b1), "suggestion.");

    // Merge both directions.
    let mut ab = client_a.clone();
    ab.merge(&client_b);
    let mut ba = client_b.clone();
    ba.merge(&client_a);

    let text_ab = ab.materialize();
    let text_ba = ba.materialize();
    assert_eq!(text_ab, text_ba, "materialized text must commute");
    assert_eq!(
        ab.materialize_hash(),
        ba.materialize_hash(),
        "BLAKE3 of materialized doc must match"
    );
    assert!(text_ab.contains("Title"));
    assert!(text_ab.contains("human") || text_ab.contains("agent"));
}

#[test]
fn sealed_crdt_roundtrip_preserves_hash() {
    let mut doc = CrdtDocument::new();
    let root = doc.local_insert(HUMAN, None, "alpha");
    doc.local_insert(AGENT, Some(root), "-beta");
    let expected = doc.materialize_hash();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crdt.orchid");
    write_sealed_file(
        &path,
        &SealedCreateRequest {
            file_uuid: Some([0x44; 16]),
            created_unix_ms: Some(1),
            raw: Vec::new(),
            raw_content_type: None,
            raw_name: None,
            clean_text: doc.materialize().into_bytes(),
            structured: Vec::new(),
            structured_content_type: None,
            structured_crdt: Some(doc.clone()),
            encrypt_with: None,
            sign_c2pa: false,
        },
    )
    .unwrap();

    let opened = SealedFile::open(&path).unwrap();
    assert_ne!(opened.header().capability_flags & capability::CRDT, 0);
    let back = opened.structured_crdt(None).unwrap();
    assert_eq!(back.materialize(), doc.materialize());
    assert_eq!(back.materialize_hash(), expected);
}
