//! Verify every command id referenced by [`orchid_core::default_bindings`] is
//! registered when the full application command set is built.

use orchid_core::default_bindings;
use orchid_storage::OrchidPaths;
use orchid_ui::OrchidApp;
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn default_gesture_bindings_are_registered() {
    let tmp = TempDir::new().expect("temp dir");
    let paths = OrchidPaths::for_testing(tmp.path());
    let app = OrchidApp::bootstrap(paths).await.expect("bootstrap succeeds");
    let registry = app.command_registry();

    let bindings = default_bindings();
    let missing: Vec<_> = bindings
        .gesture_bindings
        .iter()
        .map(|(_, cmd_id)| cmd_id.as_str())
        .filter(|cmd_id| registry.get(cmd_id).is_none())
        .collect();

    assert!(
        missing.is_empty(),
        "default_bindings command ids missing from CommandRegistry: {missing:?}"
    );
}
