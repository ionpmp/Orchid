//! Embedding region round-trip + CAP_EMBEDDINGS.

use orchid_format::{
    document_embedding, write_sealed_file, SealedCreateRequest, SealedFile, capability,
};

#[test]
fn sealed_embeddings_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("emb.orchid");
    let clean = b"friendly dog plays outside".to_vec();
    let emb = document_embedding(
        "orchid.stub.synonym.v1",
        clean.len() as u64,
        4,
        vec![0.25, 0.5, 0.75, 1.0],
    );

    write_sealed_file(
        &path,
        &SealedCreateRequest {
            file_uuid: Some([0x55; 16]),
            created_unix_ms: Some(7),
            raw: Vec::new(),
            raw_content_type: None,
            raw_name: None,
            clean_text: clean.clone(),
            structured: b"{}".to_vec(),
            structured_content_type: None,
            structured_crdt: None,
            encrypt_with: None,
            sign_c2pa: false,
            embeddings: Some(emb.clone()),
        },
    )
    .unwrap();

    let opened = SealedFile::open(&path).unwrap();
    assert_ne!(opened.header().capability_flags & capability::EMBEDDINGS, 0);
    let back = opened.embeddings(None).unwrap();
    assert_eq!(back, emb);
    assert_eq!(opened.clean_text(None).unwrap(), clean);
}
