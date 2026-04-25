//! Integration test for [`orchid_ui::OrchidApp::open_in_viewer`].

use std::io::Write;

use orchid_fs::FsPath;
use orchid_storage::OrchidPaths;
use orchid_ui::OrchidApp;
use tempfile::TempDir;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn open_text_file_in_viewer() {
    let tmp = TempDir::new().expect("temp dir");
    let paths = OrchidPaths::for_testing(tmp.path());

    let src = tmp.path().join("hello.rs");
    let mut f = std::fs::File::create(&src).expect("create file");
    writeln!(f, "fn main() {{ println!(\"Hello\"); }}").expect("write");

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

    let path = FsPath::from_local(&src).expect("fs path");
    let viewer_id = app.open_in_viewer(path).await.expect("open in viewer");

    for _ in 0..40 {
        if let Some(snap) = app.widget_manager().snapshot_cache().get(viewer_id) {
            match &snap.payload {
                orchid_widgets::WidgetPayload::Viewer(v) => {
                    use orchid_viewers::ViewerSnapshot::*;
                    match &v.snapshot {
                        Text(_) => return,
                        Loading { .. } => {}
                        other => panic!("unexpected snapshot variant: {other:?}"),
                    }
                }
                other => panic!("not a viewer payload: {other:?}"),
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("viewer did not produce a Text snapshot in time");
}
