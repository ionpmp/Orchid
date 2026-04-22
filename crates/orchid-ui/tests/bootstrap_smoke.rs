//! Smoke test: `OrchidApp::bootstrap` wires every subsystem without
//! opening a window. We deliberately do **not** call `run_startup` —
//! Slint's event loop is a terminal operation unfit for unit tests.

use orchid_storage::OrchidPaths;
use orchid_ui::OrchidApp;
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bootstrap_builds_all_subsystems() {
    let tmp = TempDir::new().expect("temp dir");
    let paths = OrchidPaths::for_testing(tmp.path());

    let app = OrchidApp::bootstrap(paths).await.expect("bootstrap succeeds");

    assert!(!app.theme().current().meta.id.is_empty());
    assert!(!app.locale().current().as_str().is_empty());
    assert!(!app.bus().is_shutdown());
}
