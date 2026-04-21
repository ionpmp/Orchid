//! End-to-end age file encryption round-trip.

use orchid_crypto::{hash_file, Decryptor, Encryptor, Identity};

#[tokio::test]
async fn passphrase_file_round_trip_is_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("plain.bin");
    let encrypted = dir.path().join("plain.bin.age");
    let recovered = dir.path().join("recovered.bin");

    // 5 MiB of deterministic content.
    let payload: Vec<u8> = (0..(5 * 1024 * 1024_u32 / 4))
        .flat_map(|i| i.wrapping_mul(2_654_435_761).to_le_bytes())
        .collect();
    tokio::fs::write(&plain, &payload).await.unwrap();

    let enc = Encryptor::new(Identity::passphrase("very-secret-passphrase"));
    let dec = Decryptor::new(Identity::passphrase("very-secret-passphrase"));

    let meta = enc.encrypt_file(&plain, &encrypted).await.unwrap();
    assert_eq!(meta.original_size, payload.len() as u64);
    assert!(encrypted.exists());
    assert!(encrypted.with_extension("age.meta").exists());

    let back_meta = dec.decrypt_file(&encrypted, &recovered).await.unwrap();
    assert_eq!(back_meta.id, meta.id);
    assert_eq!(
        hash_file(&plain).await.unwrap(),
        hash_file(&recovered).await.unwrap(),
        "byte-identical recovery"
    );
}
