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

    let inst = app
        .widget_manager()
        .get_instance(viewer_id)
        .expect("viewer instance");
    assert!(
        inst.is_visible_floating(),
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
async fn viewer_created_without_undock_stays_on_grid() {
    let tmp = TempDir::new().expect("temp dir");
    let src = tmp.path().join("grid.docx");
    // Plain text is enough: catalog Document uses sample docx, but placement is independent of format.
    let mut f = std::fs::File::create(&src).expect("create");
    writeln!(f, "on grid").expect("write");

    let app = boot_with_workspace(&tmp).await;
    let ws = app.workspace_manager().active().expect("active").id;
    let path = FsPath::from_local(&src).expect("fs path");
    let id = app
        .widget_manager()
        .create(orchid_widgets::CreateWidgetRequest {
            type_id: orchid_widgets::builtin::viewer::TYPE_ID.into(),
            workspace_id: ws,
            position: None,
            size: Some(orchid_storage::WidgetSize::Medium),
            initial_lifecycle: None,
            config_bytes: None,
        })
        .await
        .expect("create viewer");
    orchid_widgets::builtin::viewer::open_path(id, path)
        .await
        .expect("open");
    assert!(
        !app.widget_manager().get_instance(id).unwrap().is_windowed(),
        "catalog Document path must leave the viewer on the canvas grid"
    );
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
    assert!(app
        .widget_manager()
        .get_instance(viewer_id)
        .unwrap()
        .is_visible_floating());

    app.widget_manager()
        .dock_to_grid(viewer_id)
        .await
        .expect("dock");
    assert!(
        !app.widget_manager()
            .get_instance(viewer_id)
            .unwrap()
            .is_windowed(),
        "docked viewer must leave the floating layer"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn undock_any_widget_and_minimize() {
    let tmp = TempDir::new().expect("temp dir");
    let app = boot_with_workspace(&tmp).await;
    let ws = app.workspace_manager().active().expect("active").id;
    let id = app
        .widget_manager()
        .create(orchid_widgets::CreateWidgetRequest {
            type_id: "notes".into(),
            workspace_id: ws,
            position: None,
            size: None,
            initial_lifecycle: None,
            config_bytes: None,
        })
        .await
        .expect("create notes");

    let bounds = orchid_widgets::PixelBounds {
        x: 40.0,
        y: 40.0,
        width: 320.0,
        height: 240.0,
    };
    app.widget_manager()
        .undock_to_floating(id, bounds)
        .await
        .expect("undock");
    assert!(app
        .widget_manager()
        .get_instance(id)
        .unwrap()
        .is_visible_floating());

    app.widget_manager()
        .minimize_window(id)
        .await
        .expect("minimize");
    assert_eq!(
        app.widget_manager()
            .get_instance(id)
            .unwrap()
            .window_state(),
        Some(orchid_storage::WindowState::Minimized)
    );

    app.widget_manager()
        .restore_window(id)
        .await
        .expect("restore");
    assert!(app
        .widget_manager()
        .get_instance(id)
        .unwrap()
        .is_visible_floating());

    let max = orchid_widgets::PixelBounds {
        x: 0.0,
        y: 0.0,
        width: 800.0,
        height: 500.0,
    };
    app.widget_manager()
        .maximize_window(id, max)
        .await
        .expect("maximize");
    assert_eq!(
        app.widget_manager()
            .get_instance(id)
            .unwrap()
            .window_state(),
        Some(orchid_storage::WindowState::Maximized)
    );
}
