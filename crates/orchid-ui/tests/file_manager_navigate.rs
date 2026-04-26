//! File-manager widget navigation smoke test.

use std::time::Duration;

use orchid_storage::OrchidPaths;
use orchid_ui::OrchidApp;
use tempfile::TempDir;
use tokio::time::timeout;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn file_manager_lists_temp_dir_entries() {
    let tmp = TempDir::new().expect("temp dir");
    let paths = OrchidPaths::for_testing(tmp.path());
    let app = OrchidApp::bootstrap(paths).await.expect("bootstrap");
    let ws_id = app
        .workspace_manager()
        .create("Main".to_string())
        .await
        .expect("create workspace");

    let dir = tempfile::tempdir().expect("data dir");
    std::fs::write(dir.path().join("a.txt"), "a").expect("write a");
    std::fs::write(dir.path().join("b.txt"), "b").expect("write b");
    std::fs::write(dir.path().join("c.txt"), "c").expect("write c");

    let fm_id = timeout(
        Duration::from_secs(45),
        app.widget_manager().create(orchid_widgets::CreateWidgetRequest {
            type_id: "file-manager".to_string(),
            workspace_id: ws_id,
            position: None,
            size: None,
            initial_lifecycle: None,
            config_bytes: None,
        }),
    )
    .await
    .expect("create fm timed out")
    .expect("create fm");

    let fs_path = orchid_fs::FsPath::from_local(dir.path()).expect("fs path");
    orchid_widgets::builtin::file_manager::navigate(fm_id, 0, fs_path)
        .await
        .expect("navigate");

    for _ in 0..80 {
        if let Some(s) = app
            .widget_manager()
            .snapshot_cache()
            .get(fm_id)
            .and_then(|x| x.as_deref().cloned())
        {
            if let orchid_widgets::WidgetPayload::FileManager(p) = &s.payload {
                let tab = &p.panes[0].tabs[0];
                if tab.entries.len() == 3 {
                    return;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    panic!("did not observe 3 entries after navigation");
}

