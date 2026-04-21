//! Reveal session lifecycle.

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use orchid_crypto::{
    Decryptor, Encryptor, FixedClock, Identity, RevealDuration, RevealManager,
};

fn fresh_bus() -> Arc<orchid_core::EventBus> {
    Arc::new(orchid_core::EventBus::new(orchid_core::EventBusConfig::default()))
}

async fn encrypt_sample(td: &std::path::Path) -> std::path::PathBuf {
    let plain = td.join("plain.txt");
    let encrypted = td.join("plain.txt.age");
    tokio::fs::write(&plain, b"top secret reveal payload").await.unwrap();
    let enc = Encryptor::new(Identity::passphrase("pw"));
    enc.encrypt_file(&plain, &encrypted).await.unwrap();
    encrypted
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bounded_reveal_expires_through_sweeper() {
    let td = tempfile::tempdir().unwrap();
    let encrypted = encrypt_sample(td.path()).await;
    let dec = Decryptor::new(Identity::passphrase("pw"));

    let clock = Arc::new(FixedClock::new(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()));
    let manager = RevealManager::with_clock(
        td.path().join("reveal-root"),
        fresh_bus(),
        clock.clone(),
    );

    let session = manager
        .reveal(&dec, &encrypted, RevealDuration::FiveMinutes)
        .await
        .unwrap();
    assert!(session.revealed_path.exists());
    assert_eq!(manager.list_active().len(), 1);

    // Fast-forward the clock past the expiry and drive a sweep manually.
    clock.set(Utc.with_ymd_and_hms(2026, 1, 1, 0, 10, 0).unwrap());
    manager.sweep_once().await;

    assert!(manager.list_active().is_empty(), "session should be gone");
    assert!(
        !session.revealed_path.exists(),
        "revealed file should have been wiped"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn until_closed_session_persists_across_sweeps() {
    let td = tempfile::tempdir().unwrap();
    let encrypted = encrypt_sample(td.path()).await;
    let dec = Decryptor::new(Identity::passphrase("pw"));

    let clock = Arc::new(FixedClock::new(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()));
    let manager = RevealManager::with_clock(
        td.path().join("reveal-root"),
        fresh_bus(),
        clock.clone(),
    );
    let session = manager
        .reveal(&dec, &encrypted, RevealDuration::UntilClosed)
        .await
        .unwrap();

    // Advance clock a long way; the sweep must still not close it.
    clock.set(Utc.with_ymd_and_hms(2030, 1, 1, 0, 0, 0).unwrap());
    manager.sweep_once().await;
    assert_eq!(manager.list_active().len(), 1, "until-closed survives sweep");

    // Explicit close wipes it.
    manager.close(session.id).await.unwrap();
    assert!(manager.list_active().is_empty());
    assert!(!session.revealed_path.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_closes_every_active_session() {
    let td = tempfile::tempdir().unwrap();
    let encrypted = encrypt_sample(td.path()).await;
    let dec = Decryptor::new(Identity::passphrase("pw"));

    let manager = RevealManager::new(td.path().join("reveal-root"), fresh_bus());
    let s1 = manager
        .reveal(&dec, &encrypted, RevealDuration::UntilClosed)
        .await
        .unwrap();
    let s2 = manager
        .reveal(&dec, &encrypted, RevealDuration::UntilClosed)
        .await
        .unwrap();
    assert_eq!(manager.list_active().len(), 2);

    manager.shutdown().await.unwrap();
    assert!(manager.list_active().is_empty());
    assert!(!s1.revealed_path.exists());
    assert!(!s2.revealed_path.exists());
}
