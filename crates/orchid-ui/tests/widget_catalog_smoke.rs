//! Ensures catalog widget types can be instantiated after bootstrap.

use orchid_storage::OrchidPaths;
use orchid_ui::OrchidApp;
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn catalog_widgets_can_be_created() {
    let tmp = TempDir::new().expect("temp dir");
    let paths = OrchidPaths::for_testing(tmp.path());

    let app = OrchidApp::bootstrap(paths).await.expect("bootstrap");
    let ws_id = app
        .workspace_manager()
        .create("Main".to_string())
        .await
        .expect("create workspace");

    for type_id in ["terminal", "weather", "moon", "system"] {
        let req = orchid_widgets::CreateWidgetRequest {
            type_id: type_id.to_string(),
            workspace_id: ws_id,
            position: None,
            size: None,
            initial_lifecycle: None,
            config_bytes: None,
        };
        app.widget_manager()
            .create(req)
            .await
            .unwrap_or_else(|e| panic!("failed to create {type_id}: {e}"));
    }

    assert_eq!(app.widget_manager().list_instances().len(), 4);
}
