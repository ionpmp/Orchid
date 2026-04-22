//! Resize flows through to both the PTY and the emulator.

#![cfg(windows)]

use std::sync::Arc;

use orchid_terminal::{BackendSpec, PtySize, SessionManager};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resize_updates_emulator_dimensions() {
    let bus = Arc::new(orchid_core::EventBus::new(orchid_core::EventBusConfig::default()));
    let storage = Arc::new(orchid_storage::StateStore::open_in_memory("0").unwrap());
    let manager = SessionManager::new(bus, storage);

    let id = manager
        .open(BackendSpec::cmd(), PtySize::default_80x24())
        .await
        .unwrap();
    let session = manager.get(id).unwrap();

    let new_size = PtySize {
        cols: 100,
        rows: 30,
        pixel_width: 0,
        pixel_height: 0,
    };
    session.resize(new_size).unwrap();

    // Give the child a beat to process the SIGWINCH equivalent on Windows.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let snap = session.emulator.snapshot();
    assert_eq!(snap.cols, 100);
    assert_eq!(snap.rows, 30);

    manager.close(id).await.unwrap();
}
