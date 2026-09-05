//! Phase 2: sealed encrypted regions round-trip with age Identity.

use orchid_format::{
    write_sealed_file, FormatError, SealedCreateRequest, SealedFile, capability,
};
use orchid_crypto::Identity;

#[test]
fn encrypted_clean_text_roundtrips_with_correct_identity() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("enc.orchid");
    let id = Identity::passphrase("correct horse battery");
    let clean = b"top secret clean text\n".repeat(8);

    write_sealed_file(
        &path,
        &SealedCreateRequest {
            file_uuid: Some([0x22; 16]),
            created_unix_ms: Some(99),
            raw: b"raw".to_vec(),
            raw_content_type: None,
            raw_name: None,
            clean_text: clean.clone(),
            structured: b"{}".to_vec(),
            structured_content_type: None,
            encrypt_with: Some(id.clone()),
        },
    )
    .unwrap();

    let opened = SealedFile::open(&path).unwrap();
    assert_ne!(opened.header().capability_flags & capability::ENCRYPTED, 0);
    let entry = opened
        .find_region(orchid_format::toc::RegionType::CleanText)
        .unwrap();
    assert!(entry.encryption().is_some());
    assert_eq!(opened.clean_text(Some(&id)).unwrap(), clean);

    let wrong = Identity::passphrase("wrong");
    let err = opened.clean_text(Some(&wrong)).unwrap_err();
    assert!(matches!(
        err,
        FormatError::Crypto(_) | FormatError::RegionDecode(_)
    ));

    assert!(matches!(
        opened.clean_text(None).unwrap_err(),
        FormatError::IdentityRequired
    ));
}
