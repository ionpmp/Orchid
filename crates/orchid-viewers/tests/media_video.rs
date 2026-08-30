//! Optional video smoke: needs ffmpeg on PATH + bundled libmpv.

use orchid_viewers::{mpv_available, MediaViewer, Viewer, ViewerSnapshot};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

fn find_ffmpeg() -> Option<PathBuf> {
    if Command::new("ffmpeg").arg("-version").output().is_ok() {
        return Some(PathBuf::from("ffmpeg"));
    }
    None
}

fn make_tiny_mp4(dir: &std::path::Path) -> Option<PathBuf> {
    let ffmpeg = find_ffmpeg()?;
    let out = dir.join("smoke.mp4");
    let status = Command::new(ffmpeg)
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=320x240:d=1.5",
            "-f",
            "lavfi",
            "-i",
            "sine=f=440:d=1.5",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
            out.to_str()?,
        ])
        .output()
        .ok()?;
    if !status.status.success() || !out.is_file() {
        return None;
    }
    Some(out)
}

#[tokio::test]
async fn libmpv_plays_mp4_when_tools_available() {
    if !mpv_available() {
        eprintln!("skip: libmpv not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let Some(mp4) = make_tiny_mp4(dir.path()) else {
        eprintln!("skip: ffmpeg not available or encode failed");
        return;
    };

    let path_str = format!("local:{}", mp4.display().to_string().replace('\\', "/"));
    let path = orchid_fs::FsPath::new(path_str).unwrap();
    let registry = Arc::new(orchid_fs::FsProviderRegistry::new());
    let mut viewer = MediaViewer::new();
    viewer.open(path, registry).await.unwrap();

    let mut saw_video = false;
    let mut saw_duration = false;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let ViewerSnapshot::Media(s) = viewer.snapshot() else {
            panic!("expected Media");
        };
        if !s.error.is_empty() {
            panic!("mpv error: {}", s.error);
        }
        if s.duration_ms > 500 {
            saw_duration = true;
        }
        if s.has_video && s.frame_width > 0 && !s.frame_rgba.is_empty() {
            saw_video = true;
            break;
        }
    }

    let ViewerSnapshot::Media(s) = viewer.snapshot() else {
        panic!("expected Media");
    };
    assert!(
        saw_duration,
        "expected duration; got duration_ms={} err={}",
        s.duration_ms, s.error
    );
    assert!(
        saw_video,
        "expected video frame; has_video={} {}x{} rgba_len={}",
        s.has_video,
        s.frame_width,
        s.frame_height,
        s.frame_rgba.len()
    );

    viewer.close().await.unwrap();
}
