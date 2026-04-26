//! Ensures catalog widget types can be instantiated after bootstrap.

use std::time::Duration;

use orchid_storage::OrchidPaths;
use orchid_ui::OrchidApp;
use tempfile::TempDir;
use tokio::time::timeout;

// Enough workers so terminal PTY `spawn_blocking` I/O, widget refresh tasks,
// and Tantivy search do not starve the runtime during sequential `create`s.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn catalog_widgets_can_be_created() {
    let tmp = TempDir::new().expect("temp dir");
    let paths = OrchidPaths::for_testing(tmp.path());

    let app = OrchidApp::bootstrap(paths).await.expect("bootstrap");
    let ws_id = app
        .workspace_manager()
        .create("Main".to_string())
        .await
        .expect("create workspace");

    // `sysinfo` + PTY on some Windows setups can wedge the runtime if the
    // system widget is created while a live terminal session already exists.
    // Creating system (and other lightweight types) before the terminal keeps
    // this integration test reliable; the dock still offers all six types.
    // Order keeps `system` before `terminal` so PTY + sysinfo do not wedge on Windows CI.
    for type_id in [
        "weather",
        "moon",
        "system",
        "terminal",
        "rss",
        "search",
        "media",
        "password",
        "viewer",
        "file-manager",
    ] {
        let req = orchid_widgets::CreateWidgetRequest {
            type_id: type_id.to_string(),
            workspace_id: ws_id,
            position: None,
            size: None,
            initial_lifecycle: None,
            config_bytes: None,
        };
        timeout(Duration::from_secs(45), app.widget_manager().create(req))
            .await
            .unwrap_or_else(|_| panic!("timed out after 45s while creating {type_id}"))
            .unwrap_or_else(|e| panic!("failed to create {type_id}: {e}"));
    }

    assert_eq!(app.widget_manager().list_instances().len(), 10);
}
