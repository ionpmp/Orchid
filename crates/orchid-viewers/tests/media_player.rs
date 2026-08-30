//! Media viewer unit tests (sidecars + unavailable snapshot defaults).

use orchid_viewers::{discover_sidecar_subs, is_media_file_extension, mpv_available, MediaViewer};
use orchid_viewers::{MediaSnapshot, Viewer, ViewerSnapshot};
use std::fs;
use std::sync::Arc;

#[test]
fn media_extensions_cover_common_av() {
    assert!(is_media_file_extension("mp4"));
    assert!(is_media_file_extension("mkv"));
    assert!(is_media_file_extension("mp3"));
    assert!(is_media_file_extension("flac"));
    assert!(!is_media_file_extension("png"));
}

#[test]
fn sidecar_discovery_matches_stem() {
    let dir = tempfile::tempdir().unwrap();
    let media = dir.path().join("clip.mp4");
    fs::write(&media, b"").unwrap();
    fs::write(dir.path().join("clip.srt"), b"1\n").unwrap();
    fs::write(dir.path().join("clip.en.ass"), b"").unwrap();
    fs::write(dir.path().join("unrelated.srt"), b"").unwrap();
    let found = discover_sidecar_subs(&media);
    let names: Vec<_> = found
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    assert!(names.iter().any(|n| n == "clip.srt"));
    assert!(names.iter().any(|n| n == "clip.en.ass"));
    assert!(!names.iter().any(|n| n == "unrelated.srt"));
}

#[tokio::test]
async fn media_viewer_snapshot_without_file() {
    let viewer = MediaViewer::new();
    let snap = viewer.snapshot();
    let ViewerSnapshot::Media(MediaSnapshot {
        available,
        frame_width,
        playing,
        ..
    }) = snap
    else {
        panic!("expected Media snapshot");
    };
    assert_eq!(available, mpv_available());
    assert_eq!(frame_width, 0);
    assert!(!playing);
}

#[tokio::test]
async fn media_viewer_open_missing_local_is_ok() {
    // Opening a non-existent path should not panic; engine may report error later.
    let mut viewer = MediaViewer::new();
    let path = orchid_fs::FsPath::new("local:c:/orchid-media-test-missing/nope.mp4").unwrap();
    let registry = Arc::new(orchid_fs::FsProviderRegistry::new());
    let _ = viewer.open(path, registry).await;
    let _ = viewer.snapshot();
    viewer.close().await.unwrap();
}
