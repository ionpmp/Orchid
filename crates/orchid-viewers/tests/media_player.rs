//! Media viewer unit + optional libmpv smoke tests.

use orchid_viewers::{discover_sidecar_subs, is_media_file_extension, mpv_available, MediaViewer};
use orchid_viewers::{MediaSnapshot, Viewer, ViewerSnapshot};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

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

#[test]
fn libmpv_is_loadable_when_bundled() {
    let bundled = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../third-party/mpv/win-x64/libmpv-2.dll");
    let alt = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../third-party/mpv/win-x64/mpv-1.dll");
    if !(bundled.is_file() || alt.is_file()) {
        eprintln!(
            "skip: no libmpv in third-party/mpv/win-x64 (see docs/BUILDING.md)"
        );
        return;
    }
    assert!(
        mpv_available(),
        "libmpv DLL present but mpv_available() returned false"
    );
}

/// Minimal mono 16-bit PCM WAV (~1s of silence at 8 kHz).
fn write_silent_wav(path: &std::path::Path, duration_ms: u32) {
    let sample_rate = 8000u32;
    let channels = 1u16;
    let bits = 16u16;
    let n_samples = sample_rate * duration_ms / 1000;
    let data_bytes = n_samples * u32::from(channels) * u32::from(bits) / 8;
    let mut out = Vec::with_capacity((44 + data_bytes) as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    let byte_rate = sample_rate * u32::from(channels) * u32::from(bits) / 8;
    out.extend_from_slice(&byte_rate.to_le_bytes());
    let block_align = channels * bits / 8;
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_bytes.to_le_bytes());
    out.resize(out.len() + data_bytes as usize, 0);
    fs::write(path, out).unwrap();
}

#[tokio::test]
async fn libmpv_plays_wav_when_bundled() {
    if !mpv_available() {
        eprintln!("skip: libmpv not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("tone.wav");
    write_silent_wav(&wav, 1500);

    let path_str = format!("local:{}", wav.display().to_string().replace('\\', "/"));
    let path = orchid_fs::FsPath::new(path_str).unwrap();
    let registry = Arc::new(orchid_fs::FsProviderRegistry::new());

    let mut viewer = MediaViewer::new();
    assert!(viewer.engine_available());
    viewer.open(path, registry).await.unwrap();

    // Give the worker time to load + start.
    let mut saw_duration = false;
    let mut saw_progress = false;
    for _ in 0..40 {
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
        if s.position_ms > 50 || s.progress > 0.02 {
            saw_progress = true;
        }
        if saw_duration && (saw_progress || s.playing) {
            break;
        }
    }

    let ViewerSnapshot::Media(s) = viewer.snapshot() else {
        panic!("expected Media");
    };
    assert!(
        saw_duration,
        "expected duration after load; got duration_ms={} playing={} err={}",
        s.duration_ms, s.playing, s.error
    );
    // Position may stay 0 if muted/paused quirks; at least playing or progress.
    assert!(
        s.playing || saw_progress || s.position_ms > 0,
        "expected playback activity; pos={} playing={} progress={}",
        s.position_ms, s.playing, s.progress
    );

    viewer.close().await.unwrap();
}

#[test]
fn audio_engine_mode_spawns() {
    use orchid_viewers::{EngineMode, MpvEngine};
    let engine = MpvEngine::spawn_with_mode(EngineMode::Audio);
    assert_eq!(engine.mode, EngineMode::Audio);
    assert_eq!(
        engine.shared.available.load(std::sync::atomic::Ordering::Relaxed),
        mpv_available()
    );
    // Drop sends Quit; no panic.
}
