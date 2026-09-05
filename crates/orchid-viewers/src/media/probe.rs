//! Short-lived libmpv probes (duration without opening the player UI).

use std::path::Path;
use std::time::{Duration, Instant};

use super::engine::set_opt;
use super::ffi::{self, command_args, get_double, MpvHandle};

/// Probe media duration via a temporary audio-only libmpv session.
///
/// Returns milliseconds when libmpv can open the file and report a positive
/// `duration`. Used to fill library rows that lack ID3 `TLEN`.
#[must_use]
pub fn probe_media_duration_ms(path: &Path) -> Option<u32> {
    if !path.is_file() {
        return None;
    }
    let api = ffi::api().ok()?;
    let handle = unsafe { (api.create)() };
    if handle.is_null() {
        return None;
    }
    let result = unsafe { probe_with_handle(api, handle, path) };
    unsafe {
        (api.terminate_destroy)(handle);
    }
    result
}

/// Probe many paths with one mpv handle (faster than create/destroy per file).
#[must_use]
pub fn probe_media_durations_ms(paths: &[impl AsRef<Path>]) -> Vec<Option<u32>> {
    if paths.is_empty() {
        return Vec::new();
    }
    let Ok(api) = ffi::api() else {
        return paths.iter().map(|_| None).collect();
    };
    let handle = unsafe { (api.create)() };
    if handle.is_null() {
        return paths.iter().map(|_| None).collect();
    }
    let configured = unsafe { configure_probe(api, handle) };
    let mut out = Vec::with_capacity(paths.len());
    if configured {
        for path in paths {
            let path = path.as_ref();
            out.push(if path.is_file() {
                unsafe { load_and_read_duration(api, handle, path) }
            } else {
                None
            });
        }
    } else {
        out.resize(paths.len(), None);
    }
    unsafe {
        (api.terminate_destroy)(handle);
    }
    out
}

unsafe fn probe_with_handle(api: &ffi::MpvApi, handle: MpvHandle, path: &Path) -> Option<u32> {
    if !configure_probe(api, handle) {
        return None;
    }
    load_and_read_duration(api, handle, path)
}

unsafe fn configure_probe(api: &ffi::MpvApi, handle: MpvHandle) -> bool {
    let ok = set_opt(api, handle, "vo", "null")
        .and_then(|_| set_opt(api, handle, "ao", "null"))
        .and_then(|_| set_opt(api, handle, "video", "no"))
        .and_then(|_| set_opt(api, handle, "audio-display", "no"))
        .and_then(|_| set_opt(api, handle, "terminal", "no"))
        .and_then(|_| set_opt(api, handle, "idle", "yes"))
        .and_then(|_| set_opt(api, handle, "pause", "yes"))
        .and_then(|_| set_opt(api, handle, "keep-open", "yes"))
        .and_then(|_| set_opt(api, handle, "load-scripts", "no"))
        .and_then(|_| {
            let rc = (api.initialize)(handle);
            if rc < 0 {
                Err(ffi::error_message(api, rc))
            } else {
                Ok(())
            }
        });
    ok.is_ok()
}

unsafe fn load_and_read_duration(api: &ffi::MpvApi, handle: MpvHandle, path: &Path) -> Option<u32> {
    let path_s = path.to_string_lossy();
    let rc = command_args(api, handle, &["loadfile", path_s.as_ref(), "replace"]);
    if rc < 0 {
        return None;
    }
    let _ = command_args(api, handle, &["set", "pause", "yes"]);
    let deadline = Instant::now() + Duration::from_millis(2_500);
    while Instant::now() < deadline {
        let _ = (api.wait_event)(handle, 0.05);
        if let Some(secs) = get_double(api, handle, "duration") {
            if secs.is_finite() && secs > 0.0 {
                let ms = (secs * 1000.0).round();
                if ms > 0.0 && ms < f64::from(u32::MAX) {
                    return Some(ms as u32);
                }
            }
        }
    }
    None
}
