//! Phase 1 sealed create → mmap read round-trip.

use orchid_format::{write_sealed_file, SealedCreateRequest, SealedFile};
use std::fs;

#[test]
fn sealed_clean_text_roundtrip_bit_identical() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.orchid");

    let clean = "Line one\nLine two with unicode \u{1F338}\n"
        .as_bytes()
        .to_vec();
    let structured = br#"{"schema":"orchid-doc-v1","blocks":[]}"#.to_vec();
    let raw = b"\0\x01\x02raw-bytes".to_vec();

    write_sealed_file(
        &path,
        &SealedCreateRequest {
            file_uuid: Some([0x11; 16]),
            created_unix_ms: Some(1_700_000_000_123),
            raw: raw.clone(),
            raw_content_type: Some("application/octet-stream".into()),
            raw_name: Some("attachment.bin".into()),
            clean_text: clean.clone(),
            structured: structured.clone(),
            structured_content_type: Some("application/vnd.orchid.structured+json".into()),
            encrypt_with: None,
            sign_c2pa: false,
        },
    )
    .unwrap();

    assert!(path.metadata().unwrap().len() > 4096);

    let opened = SealedFile::open(&path).unwrap();
    assert_eq!(opened.header().file_uuid, [0x11; 16]);
    assert_eq!(opened.header().created_unix_ms, 1_700_000_000_123);

    let got_clean = opened.clean_text(None).unwrap();
    assert_eq!(got_clean, clean);

    assert_eq!(opened.raw(None).unwrap(), raw);
    assert_eq!(opened.structured(None).unwrap(), structured);

    let toc = opened.toc().unwrap();
    assert_eq!(toc.generation(), 1);
    assert_eq!(toc.regions().unwrap().len(), 3);

    let copy = dir.path().join("copy.orchid");
    fs::copy(&path, &copy).unwrap();
    let again = SealedFile::open(&copy).unwrap();
    assert_eq!(again.clean_text(None).unwrap(), clean);
}
