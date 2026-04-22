//! Headless check that full bootstrap, workspace + terminal creation, and close
//! work without opening a window.

use orchid_storage::OrchidPaths;
use orchid_ui::OrchidApp;
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workspace_bootstrap_with_terminal() {
    let tmp = TempDir::new().expect("temp dir");
    let paths = OrchidPaths::for_testing(tmp.path());

    let app = OrchidApp::bootstrap(paths).await.expect("bootstrap succeeds");

    assert_eq!(app.workspace_manager().list().len(), 0);
    assert_eq!(app.widget_manager().list_instances().len(), 0);

    let ws_id = app
        .workspace_manager()
        .create("Main".to_string())
        .await
        .expect("create workspace");

    let request = orchid_widgets::CreateWidgetRequest {
        type_id: "terminal".to_string(),
        workspace_id: ws_id,
        position: None,
        size: None,
        initial_lifecycle: None,
        config_bytes: None,
    };
    let widget_id = app.widget_manager().create(request).await.expect("create widget");

    assert_eq!(app.widget_manager().list_instances().len(), 1);
    let inst = app
        .widget_manager()
        .get_instance(widget_id)
        .expect("instance");
    assert_eq!(inst.type_id, "terminal");

    app.widget_manager()
        .close(widget_id)
        .await
        .expect("close");
    assert_eq!(app.widget_manager().list_instances().len(), 0);
}
