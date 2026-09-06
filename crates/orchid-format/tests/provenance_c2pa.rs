//! Phase 3: Provenance C2PA carrier validates via third-party Reader.

use orchid_format::{
    capability, is_c2pa_accepted, verify_provenance_carrier, write_sealed_file,
    SealedCreateRequest, SealedFile,
};

#[test]
fn provenance_payload_accepted_by_c2pa_reader() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("signed.orchid");
    let clean = b"Provenance Phase 3 clean text\n".to_vec();

    write_sealed_file(
        &path,
        &SealedCreateRequest {
            file_uuid: Some([0x33; 16]),
            created_unix_ms: Some(123),
            raw: Vec::new(),
            raw_content_type: None,
            raw_name: None,
            clean_text: clean,
            structured: b"{}".to_vec(),
            structured_content_type: None,
            structured_crdt: None,
            encrypt_with: None,
            sign_c2pa: true,
            embeddings: None,
        },
    )
    .unwrap();

    let file = SealedFile::open(&path).unwrap();
    assert_ne!(file.header().capability_flags & capability::C2PA, 0);
    let carrier = file.provenance_carrier().unwrap();
    let state = verify_provenance_carrier(&carrier).unwrap();
    assert!(is_c2pa_accepted(state), "c2pa state={state:?}");

    // Extracted payload is a standalone PNG a third-party tool can open.
    let out = dir.path().join("provenance.png");
    std::fs::write(&out, &carrier).unwrap();
    let again = verify_provenance_carrier(&std::fs::read(out).unwrap()).unwrap();
    assert!(is_c2pa_accepted(again));
}
