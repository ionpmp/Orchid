//! Floating viewer open / reuse-by-path behaviour.

use std::io::Write;

use orchid_fs::FsPath;
use orchid_storage::OrchidPaths;
use orchid_ui::OrchidApp;
use tempfile::TempDir;

async fn boot_with_workspace(tmp: &TempDir) -> OrchidApp {
    let paths = OrchidPaths::for_testing(tmp.path());
    let app = OrchidApp::bootstrap(paths).await.expect("bootstrap");
    let _ws_id = app
        .workspace_manager()
        .create("Main".to_string())
        .await
        .expect("create workspace");
    app.workspace_manager()
        .switch_to(
            app.workspace_manager()
                .list()
                .first()
                .expect("workspace")
                .id,
        )
        .await
        .expect("switch");
    app
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn open_new_document_is_floating() {
    let tmp = TempDir::new().expect("temp dir");
    let src = tmp.path().join("a.txt");
    let mut f = std::fs::File::create(&src).expect("create");
    writeln!(f, "hello").expect("write");

    let app = boot_with_workspace(&tmp).await;
    let path = FsPath::from_local(&src).expect("fs path");
    let viewer_id = app.open_in_viewer(path).await.expect("open");

    let bounds = orchid_widgets::builtin::viewer::floating_bounds(viewer_id);
    assert!(
        bounds.is_some(),
        "new document should open as a floating viewer"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reopen_same_path_reuses_viewer() {
    let tmp = TempDir::new().expect("temp dir");
    let src = tmp.path().join("reuse.txt");
    let mut f = std::fs::File::create(&src).expect("create");
    writeln!(f, "reuse me").expect("write");

    let app = boot_with_workspace(&tmp).await;
    let path = FsPath::from_local(&src).expect("fs path");
    let first = app.open_in_viewer(path.clone()).await.expect("open first");
    let second = app.open_in_viewer(path).await.expect("open second");
    assert_eq!(first, second, "same path must focus the existing viewer");

    let ws = app.workspace_manager().active().expect("active ws").id;
    let viewers: Vec<_> = app
        .widget_manager()
        .instances_for_workspace(ws)
        .into_iter()
        .filter(|i| i.type_id == orchid_widgets::builtin::viewer::TYPE_ID)
        .collect();
    assert_eq!(viewers.len(), 1, "must not create a second viewer");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dock_clears_floating_bounds() {
    let tmp = TempDir::new().expect("temp dir");
    let src = tmp.path().join("dock.txt");
    let mut f = std::fs::File::create(&src).expect("create");
    writeln!(f, "dock me").expect("write");

    let app = boot_with_workspace(&tmp).await;
    let path = FsPath::from_local(&src).expect("fs path");
    let viewer_id = app.open_in_viewer(path).await.expect("open");
    assert!(orchid_widgets::builtin::viewer::floating_bounds(viewer_id).is_some());

    orchid_widgets::builtin::viewer::set_floating_bounds(viewer_id, None)
        .expect("clear floating");
    assert!(
        orchid_widgets::builtin::viewer::floating_bounds(viewer_id).is_none(),
        "docked viewer must leave the floating layer"
    );
}
