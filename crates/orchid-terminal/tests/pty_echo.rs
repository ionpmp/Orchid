//! Spawn `cmd.exe`, send `echo hello`, wait for the output to show up in the
//! emulator snapshot, then close cleanly.

#![cfg(windows)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use orchid_terminal::{BackendSpec, PtySize, SessionManager};

fn bus() -> Arc<orchid_core::EventBus> {
    Arc::new(orchid_core::EventBus::new(orchid_core::EventBusConfig::default()))
}

fn storage() -> Arc<orchid_storage::StateStore> {
    Arc::new(orchid_storage::StateStore::open_in_memory("0").unwrap())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cmd_echo_roundtrip() {
    let manager = SessionManager::new(bus(), storage());
    // `cmd.exe /c` runs the command and exits immediately, which is more
    // reliable than driving an interactive session in tests.
    let spec = orchid_terminal::BackendSpec {
        kind: orchid_terminal::BackendKind::Custom {
            command: "cmd.exe".into(),
            args: vec!["/c".into(), "echo orchid-marker".into()],
        },
        working_directory: None,
        env: Default::default(),
        initial_command: None,
    };
    let id = manager
        .open(spec, PtySize::default_80x24())
        .await
        .expect("spawn cmd.exe");

    let session = manager.get(id).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut seen_marker = false;
    while Instant::now() < deadline {
        let snapshot = session.emulator.snapshot();
        let dump: String = snapshot
            .lines
            .iter()
            .flat_map(|l| l.cells.iter())
            .map(|c| c.ch)
            .collect();
        if dump.contains("orchid-marker") {
            seen_marker = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(seen_marker, "did not observe marker in emulator output");

    let _ = BackendSpec::cmd(); // silence unused import warnings
    manager.close(id).await.unwrap();
}
